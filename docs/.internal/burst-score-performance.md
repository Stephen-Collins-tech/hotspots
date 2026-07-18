# burst_score: performance problem and open question for research

**Status:** mitigated (escape hatch shipped), not fixed. Written for hand-off to
`hotspots-research` to re-analyze whether burst_score's lift justifies its cost.

**Related:** `TASKS.md` lines 59–61 (F63 handoff note) already flagged part of this
problem before F93 shipped; the reconciliation it called for never fully happened.

---

## What burst_score is

F93 (`benchmarks/RESULTS.md`, v1.30.0, promoted per `CHANGELOG.md`) added a
commit-timing "burstiness" signal: for each file, the sliding 30-day-window
max-commit-count-over-mean-commit-count ratio, computed from that file's full commit
history. It's a `ScoringWeights.burst` term (default weight 0.3) folded into
activity risk in `scoring.rs`, and a `burst_score: Option<f64>` field on
`FunctionSnapshot`.

Benchmarked lift (`benchmarks/RESULTS.md`): mean Spearman ρ across the 7-repo corpus
went from **+0.350** (v1.26.0, without burst_score) to **+0.387** (v1.30.0, with
burst_score) — mean P@10 unchanged at 0.54. Per-repo, the gain ranged from about
+0.01 (golang/go) to +0.05 (redis/redis, curl/curl).

## How it's computed

`Snapshot::populate_burst_score` (`hotspots-core/src/snapshot.rs:713`) runs:

```
git log --format=COMMIT %at --name-only
```

over the **entire** repository history in a single subprocess, buffers the whole
output via `Command::output()`, parses every line into a per-file list of commit
timestamps, then computes the ratio per file. There is no windowing — the ratio's
mean term is inherently a function of the file's whole commit history, not a recent
slice, so truncating the input would change the metric's meaning, not just its cost.

## The performance problem

1. **The full-history walk is unconditional and wired into every enrichment path.**
   `SnapshotEnricher::with_burst_score` (`snapshot.rs:1985`) is called from both
   `build_snapshot_via_db` and `build_enriched_snapshot`
   (`hotspots-cli/src/cmd/analyze.rs:1307`, `:1368`) — the shared path behind
   `--mode snapshot`, `--mode delta`, `--mode models`, and the `diff --auto-analyze`
   per-commit backfill (`analyze_and_persist_at_ref`). Before F93, only
   `--mode snapshot` and `hotspots train` ever paid a full-history git cost (via
   directed coupling, see below). F93 added that cost to **delta mode**, which is
   what CI runs on every PR (`action/src/*.ts:327`), and to **every commit** a
   `--auto-analyze` backfill has to reconstruct.

2. **It duplicates other full-history walks in the same pipeline run, uncoordinated.**
   Two other features independently re-walk the entire repository history to get
   file-level per-commit data, and none of the three share results:
   - `coupling::compute_directed_coupling_for_repo` (`coupling.rs:271`, via
     `load_commits` at `coupling.rs:66`) — full history, `--first-parent
     --diff-filter=ACDMRT`. Called from `--mode snapshot`
     (`analyze.rs:538`) and `hotspots train` (`trainer.rs:427`).
   - `history_signals::load_commits_with_author` (`history_signals.rs:37`) — full
     history, `--name-only --diff-filter=ACDMRT`, no `--first-parent`. Added by F63
     for `populate_history_signals` (`trainer.rs:432`), used by `hotspots train`
     and `analyze --cold-start`.
   - burst_score's own walk, described above — no `--first-parent`, no
     `--diff-filter` at all (broadest, most expensive of the three).

   A single `hotspots analyze --mode snapshot` invocation therefore pays **two**
   independent full-history traversals (burst_score + directed coupling); a single
   `hotspots train` run pays **three** (burst_score + directed coupling +
   history_signals). The GitHub Action's default pipeline calls `analyze --mode
   snapshot` twice (initial + `--force` re-persist) and `analyze --mode delta` once
   in a single run — up to **five** independent full-history git log traversals of
   the same repository in one CI run.

3. **No caching.** Touch metrics have `.hotspots/touch-cache.json.zst`, giving warm
   runs near-zero cost. burst_score (and the other two full-history walks above)
   recompute from scratch on every single invocation, even back-to-back runs on the
   same commit.

4. **The cost is inherent to git, not to how the output is consumed.** git itself
   must tree-diff every commit against its parent(s) to produce `--name-only`
   output — that's the dominant cost, and it happens before Rust reads a single
   byte. Switching `Command::output()` to a streamed `Stdio::piped()` +
   `BufReader` would reduce peak memory (currently the whole stdout is buffered,
   then `String::from_utf8_lossy` makes a second full copy) and could matter on
   memory-constrained CI runners if swapping is part of what's driving multi-minute
   stalls — but it would not reduce git's own tree-diffing work, which scales with
   total history size regardless of consumption strategy.

**Net effect reported:** CI pipelines on large repos now exceed 30 minutes,
correlated with the F93 release (`CHANGELOG.md`, v1.31.0). The benchmark corpus
that produced the ρ +0.037 lift figure above (`benchmarks/RESULTS.md`) tops out at
6,116 files (golang/go) and is not known to include a repo at the commit-history
scale where this cost becomes a 30-minute problem — the corpus has not stress-tested
burst_score's cost/benefit at that scale.

## What's shipped so far (escape hatch, not a fix)

`burst_score_skip_above` config field (`.hotspotsrc.json`,
`hotspots-core/src/config.rs`), default unbounded. A cheap `git rev-list --count
HEAD` pre-check (`snapshot.rs:480`, `commit_count()`) skips the expensive
`--name-only` traversal entirely when the repo's commit count exceeds the
threshold. This unblocks large repos but **drops the signal entirely** above the
threshold — `burst_score` is `null` and contributes nothing to activity risk for
that run. It does not address point 2 (duplicate walks) or point 3 (no caching)
above, and does nothing for repos under the threshold that still pay the cost on
every run.

## Open question for research

Is burst_score's lift (+0.037 mean ρ, flat P@10, on a corpus that doesn't include a
repo at the scale where this becomes a 30-minute problem) worth its recurring
full-history-walk cost on large repos? Things worth re-analyzing:

- Does a bounded/windowed history (e.g. last 12–24 months instead of full history)
  preserve most of the lift? This would change the formula (not just an
  implementation detail) and would need its own promotion brief if validated.
- How much of the benchmarked lift survives on repos closer to the scale where the
  cost actually bites — the current 7-repo corpus may be systematically biased
  toward repos small enough that this tradeoff never shows up.
- If burst_score's raw per-commit data were shared with directed coupling's (and,
  for `train`/`--cold-start`, history_signals') full-history load instead of each
  walking independently, burst_score's *marginal* cost in `--mode snapshot` and
  `train` would be much smaller — it would only be the sole cost-adder in
  `--mode delta` and `--auto-analyze` backfill. That changes the cost/benefit
  calculus without touching the formula at all, and may be worth prioritizing
  ahead of, or instead of, revisiting the formula itself.
