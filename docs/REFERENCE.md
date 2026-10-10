# Reference

## CLI Commands

### `hotspots analyze <path>`

Core analysis command. Scans source files, computes metrics, scores functions.

```
hotspots analyze <PATH> [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--format` | `text` | `text`, `json`, `jsonl`, `html`, `sarif`, `csv`, `xlsx` |
| `--mode` | — | `snapshot`, `delta`, `models` |
| `--top N` | none | Show top N functions by LRS |
| `--min-lrs F` | `0.0` | Filter functions below this LRS |
| `--config PATH` | auto | Path to config file |
| `--output PATH` | `.hotspots/report.html` | Output file (HTML/SARIF) |
| `--explain` | off | Per-function risk breakdown + phrase-table explanations for CRITICAL/HIGH when a trained ranker is active (snapshot+text only) |
| `--explain-patterns` | off | Show pattern trigger conditions |
| `--level` | — | `file` or `module` aggregate view (snapshot+text only) |
| `--policy` | off | Evaluate policies; exit 1 on blocking violations (delta only) |
| `--force` | off | Overwrite existing snapshot |
| `--no-persist` | off | Skip writing snapshot to disk |
| `--touch-mode` | `auto` | `auto`, `per-function`, `file`, `hybrid[:N]` (default N=5), or `none` — see below |
| `--per-function-touches` | off | **Deprecated**, use `--touch-mode per-function`. Use `git log -L` for precise touch counts (slow cold start) |
| `--no-per-function-touches` | off | **Deprecated**, use `--touch-mode file`. Force file-level touch batching |
| `--skip-touch-metrics` | off | **Deprecated**, use `--touch-mode none`. Skip all git log I/O (touch counts reported as 0) |
| `--hybrid-touches N` | — | **Deprecated**, use `--touch-mode hybrid:N`. File-level first, per-function for files with touch_count_30d >= N |
| `--all-functions` | off | Output flat array instead of triage buckets (snapshot JSON only) |
| `--include-models` | off | Add model risk map to JSON/HTML (snapshot only) |
| `--callgraph-skip-above N` | 50000 | Skip betweenness centrality if call graph > N edges |
| `--skip-gate` | off | Disable suppression gate P@10 check |
| — | — | Suppression gate verdicts are smoothed: `Suppressed` is only reported after 3 consecutive raw `Suppressed` readings across runs, tracked in `.hotspots/gate_history.json` |
| `--cold-start` | off | Gini/label-density-gated ranking with no trained ranker required — see [Cold-Start Ranking](#cold-start-ranking) |
| `-j N` / `--jobs N` | CPU count | Parallel worker threads |

**Notes:**
- `--explain` and `--level` are mutually exclusive
- `--force` and `--no-persist` are mutually exclusive
- Snapshot mode text output requires `--explain` or `--level`
- SARIF requires `--mode snapshot`; HTML requires `--mode snapshot` or `--mode delta`
- `--policy` requires `--mode delta`
- `--cold-start` is not compatible with `--mode` — it bypasses the trained-ranker/snapshot pipeline entirely
- `--touch-mode` conflicts with each of the four deprecated flags above; the deprecated flags still work on their own (each prints a one-line warning to stderr) and remain mutually compatible with each other, resolved with the same precedence as before
- `--format csv`/`--format xlsx` require `--mode snapshot`

#### `--format csv` / `--format xlsx`

A file-level triage/planning view for spreadsheet workflows — a tech lead or EM doing triage, ownership handoff, or sprint planning, not a raw per-function data dump (`--format json --all-functions`) or an in-IDE fix workflow (text/HTML). One row per file, always the full file list (ignores `--top`), with the risk band, quadrant, the file's highest-risk function and its line, ownership `newcomer_rate`, function/critical counts, LOC, and subsystem.

Coupling (`directed_coupling`) is not in the main table — only a minority of files have any coupling relationship, so folding it in would leave most rows reading "n/a" on that column. `--format xlsx` is a real multi-sheet workbook: "Files" (the main table) plus a "Coupling" sheet (files with a real `directed_coupling` value only). `--format csv` is single-file/single-table only — CSV has no notion of multiple tables, so coupling isn't included; use `--format xlsx` for coupling data. `--format xlsx` always writes to a file (`--output`, or `.hotspots/report.xlsx` by default); `--format csv` prints to stdout without `--output`.

Missing axis values (a file excluded from an axis, not a measured zero) are written as the literal string `n/a`, never a blank cell.

#### `--touch-mode`

Controls how touch metrics (git churn/recency) are computed:

| Value | Behavior |
|---|---|
| `auto` (default) | Use the resolved config's `per_function_touches`/`hybrid_touch_threshold`, falling back to `hybrid:5` |
| `per-function` | `git log -L` per function — accurate, but O(functions) cold-start git calls, cached in `.hotspots/touch-cache.json.zst` |
| `file` | File-level batching — fast, one `git log` call per unique file |
| `hybrid` / `hybrid:N` | File-level first; per-function only for files with `touch_count_30d >= N` (N defaults to 5) |
| `none` | Skip touch metrics, directed coupling, and burst_score entirely — no git log calls at all |

`--per-function-touches`, `--no-per-function-touches`, `--skip-touch-metrics`, and `--hybrid-touches` are deprecated aliases for `per-function`, `file`, `none`, and `hybrid:N` respectively. They still work but print a deprecation warning; use `--touch-mode` in new scripts and CI configs.

### `hotspots diff <base> <head>`

Compare snapshots between any two git refs. Both must have existing snapshots.

```
hotspots diff <BASE> <HEAD> [OPTIONS]
```

Accepts: branch names, tags, full/short SHAs, `HEAD~N` relative refs.

| Flag | Description |
|---|---|
| `--format` | `text` (default), `json`, `jsonl`, `html` |
| `--output PATH` | Write output to file |
| `--policy` | Evaluate policies; exit 1 on blocking violations |
| `--top N` | Limit to N changed functions by \|ΔLRS\| |
| `--config PATH` | Config file |
| `--auto-analyze` | Generate missing snapshots via git worktrees |

Exit codes: 0 = success, 1 = policy failure, 2 = auto-analysis failed, 3 = snapshot missing.

`--top` applies after policy evaluation — violations outside the top N are still detected.

### `hotspots coordinate [PATH]`

Coupling and ownership risk for a set of files, before starting work on them. See
[USAGE.md](USAGE.md#hotspots-coordinate) for the full JSON schema and field meanings, including
the honest caveat on `recommendation`.

```
hotspots coordinate [PATH] [OPTIONS]
```

| Flag | Description |
|---|---|
| `--files <FILES>` | Comma-separated file paths to analyze |
| `--diff` | Derive the file set from a unified diff read on stdin |
| `--staged` | Derive the file set from `git diff --cached --name-only` |

JSON output only today. `PATH` defaults to `.`.

### `hotspots train [PATH]`

Fit a ranker from fix-commit history. Model saved to `.hotspots/ranker.json` and auto-loaded by `hotspots analyze`.

Before fitting a RandomForest, `hotspots train` runs a pre-flight comparison (the **regime
screener**) between Ridge regression and a depth-2 RandomForest. If Ridge already
matches the forest's ranking quality (`Δρ < 0.03`), it fits Ridge instead and skips
RandomForest training — faster, and no less accurate on repos where the signal is
linear. See [`docs/USAGE.md`](USAGE.md#ridge-vs-randomforest-automatic-model-class-selection)
for the full verdict table. This selection is automatic; there is no flag to force one
model class over the other.

Before training, the command prints an estimate and (for repos with > 1,000 functions) prompts for confirmation:

```
hotspots train: 12914 functions · 200 trees · 365 days of git history (file-level labels) · estimated ~4m 30s
Proceed? [y/N]
  [10/200]  ~4m 5s remaining
  [100/200]  ~2m 1s remaining
  [200/200]
Model class: RandomForest (regime=STRONG, Δρ=+0.14)
Trained: 200 trees × depth 6 | 12914 samples | elapsed 4m 25s
```

When the screener selects Ridge, RandomForest training is skipped:

```
Model class: Ridge (regime=LINEAR, Δρ=+0.03) — RandomForest training skipped
Trained: 0 trees × depth 0 | 874 samples | elapsed 2s
```

| Flag | Default | Description |
|---|---|---|
| `--blame` | off | Blame-based function-level labels (slower, more precise) |
| `--label-window DAYS` | `365` | Days of history to scan |
| `--n-estimators N` | `200` | Trees in RandomForest (ignored if the screener selects Ridge) |
| `--max-depth N` | `6` | Maximum tree depth (ignored if the screener selects Ridge) |
| `--output PATH` | `.hotspots/ranker.json` | Model output path |
| `--eval` | off | Print Precision@K table after training |
| `--screen` | off | Pre-flight check; aborts when mean hotspots score is too flat |
| `--yes` / `-y` | off | Skip confirmation prompt (CI / non-interactive) |
| `--quiet` / `-q` | off | Suppress per-tree progress lines; estimate and completion still shown |

Requires: ≥ 50 functions in snapshot, ≥ 5 positive and ≥ 10 negative labels. Fix keywords: `fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`.

The trained model (`model_version 5`) uses 10 features: `lrs`, `cc`, `nd`, `loc`, `fo`, `fan_in`, `total_churn`, `authors_90d`, `directed_coupling`, `convention_bug_fix_count`. Models trained with an older version are rejected on load with a retrain message.

`hotspots analyze` prints which model class the loaded ranker uses:

```
hotspots: using trained ranker (model class: Ridge)
```

Once a model is trained, `hotspots analyze . --mode snapshot --explain` adds a `✦` phrase line below each CRITICAL/HIGH function — e.g. `✦ Churns heavily and is load-bearing.` — derived from which features rank in the top 20th percentile for that repo. No `✦` lines appear without a trained ranker.

### Cold-Start Ranking

`hotspots analyze <PATH> --cold-start` produces a ranking without a trained ranker and
without the label thresholds `hotspots train` requires (≥ 50 functions, ≥ 5 positive / ≥
10 negative labels). It skips the trained-ranker lookup and snapshot pipeline entirely —
`--mode` cannot be combined with it.

```
hotspots analyze . --cold-start
hotspots analyze . --cold-start --top 20
```

Routing is decided per-repo by two independent gates, checked in this order, and printed
before the ranked list (`cold-start route: formula|anomaly|uniform-prior`):

1. **Uniform-prior guard.** If no file stands out even by raw commit count (top decile of
   files by `commit_count` accounts for less than 20% of total commits), there's no basis
   for a ranking — prints `(uniform prior — no ranking to show; all files equally likely)`
   and returns an empty list rather than a manufactured one.
2. **Low-label-density gate.** If the repo has *some* fix-commit history but the function-level
   positive rate is low (5–30% of functions, with a Shannon-entropy floor to filter out
   near-degenerate splits) — plenty of history, not enough confirmed fixes to trust a
   supervised model — routes to the same label-free anomaly score as gate 3 below.
3. **Gini-coefficient gate** (fallback). Computes the Gini coefficient of `commit_count`
   across all functions:
   - **High concentration** (a few files dominate commit activity, Gini ≥ 0.55) → the
     existing formula score (`activity_risk`/`lrs`) is already a sufficient day-one
     ranking; no model needed.
   - **Low concentration** (Gini < 0.55) → fits a label-free `IsolationForest` on an
     8-feature history vector (`commit_count`, `author_count`, `author_entropy`,
     `burst_score`, `isolation_rate`, `age_days`, `last_touch_days`, `authors_90d`) and
     ranks by anomaly score.

Both anomaly routes (gate 2 and the low-Gini branch of gate 3) use the same
`IsolationForest` implementation — memory-bounded, streaming, no full-matrix
intermediate structure. Neither requires a `.hotspots/ranker.json` model file.

**Gini dead zone.** The Gini-coefficient gate is stateless and per-run — a repo whose
Gini sits just under the 0.55 low threshold could flip from `formula` to `anomaly` on
a marginal commit. Set `cold_start_gini_dead_zone` in `.hotspotsrc.json` (default `0.0`)
to widen the ambiguous middle zone that already defaults to `formula`: the effective
low threshold becomes `0.55 - cold_start_gini_dead_zone`, so Gini values in
`[0.55 - cold_start_gini_dead_zone, 0.55)` route to `formula` instead of `anomaly`.
The 0.55 and 0.60 constants themselves are unchanged; the dead zone only shrinks the
`anomaly` region. Must be non-negative and less than 0.55.

```json
{
  "cold_start_gini_dead_zone": 0.03
}
```

### `hotspots estimate <path>`

Projects `analyze --touch-mode per-function`'s wall-clock cost before running it, so callers can pick a touch mode instead of guessing from repo size (`size_kb` is a weak proxy — see the runtime-prediction spike this command is based on).

```
hotspots estimate <PATH> [--format text|json] [--config PATH] [--budget-seconds N] [--tier1-timeout-seconds N]
```

Two tiers:
- **Tier 0** (near-instant, no parsing): file discovery + a raw line count per file, always completes fast.
- **Tier 1** (the real `--touch-mode none` structural pass — parsing, CFG, call graph, but zero git-log calls): gives the real per-language function count. Runs on a background thread with a `--tier1-timeout-seconds` budget (default 30s) so Tier 0's report is never blocked by a repo where even Tier 1 is slow.

Per-language ms/function calibration is seeded from a 9-repo, 6-language spike (directional, not a fitted regression). `--budget-seconds` compares the projection against a caller-supplied wall-clock budget and recommends `per-function`, `hybrid`, or `file`.

### `hotspots prune`

Remove unreachable snapshots (after force-push or branch deletion).

```
hotspots prune --unreachable [--older-than DAYS] [--dry-run]
```

`--unreachable` is required. Only prunes snapshots unreachable from `refs/heads/*`, `refs/tags/*`, or `refs/remotes/*`.

### `hotspots compact`

Set compaction level for snapshot storage.

```
hotspots compact --level 0
```

Level 0 = full snapshots (current). Levels 1–2 are not yet implemented.

### `hotspots trends [PATH]`

Analyze complexity trends across snapshot history.

```
hotspots trends . [--window N] [--top K] [--format text|json|html]
```

| Flag | Default | Description |
|---|---|---|
| `--window N` | `10` | Number of snapshots to analyze |
| `--top K` | `5` | Top K functions to track |
| `--format` | `json` | Output format |

Reports: risk velocities (LRS change per snapshot), hotspot stability (consistent top-K), refactor effectiveness (sustained LRS reduction).

### `hotspots config`

```bash
hotspots config show              # show resolved config (merged defaults + file)
hotspots config show --path FILE  # show specific file
hotspots config validate          # validate auto-discovered config (exit 1 on failure)
hotspots config validate --path FILE
```

### `hotspots init`

```bash
hotspots init --hooks   # print pre-commit and CI hook templates to stdout
```

### `hotspots upgrade`

```bash
hotspots upgrade
```

Checks the latest GitHub release against the running version and prints the matching install
command (`cargo install`, `npm install -g`, or `brew upgrade`) if one is newer — reports only,
never replaces the binary. Every other command also does this check passively (a one-line
stderr notice), cached at `~/.hotspots/update_check.json`, re-queried at most once every 24
hours.

### Global flags

```bash
hotspots --help
hotspots --version
```

### Environment variables

- `NO_COLOR` — disable ANSI colors in text output
- `GIT_DIR`, `GIT_WORK_TREE` — override git repository location
- `GITHUB_EVENT_NAME=pull_request` — triggers merge-base comparison in delta mode
- `CI_MERGE_REQUEST_IID` (GitLab), `CIRCLE_PULL_REQUEST` (CircleCI), `TRAVIS_PULL_REQUEST` (Travis) — same effect

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (or warnings only) |
| 1 | Error or blocking policy failure |
| 2 | Auto-analysis failed (`hotspots diff --auto-analyze` only) |
| 3 | Snapshot missing (`hotspots diff` only) |

---

## Metrics

### The four structural metrics

**CC — Cyclomatic Complexity**
Number of independent decision paths. Counts: `if`, `else if`, `for`, `while`, `do/while`, `case`, `catch`, `&&`, `||`, ternary. A function with no branches has CC 1.

**ND — Nesting Depth**
Maximum depth of nested control structures (`if`, loops, `try`/`catch`, `switch`). Each additional level degrades readability non-linearly. ND ≥ 5 almost always warrants refactoring.

**FO — Fan-Out**
Distinct functions called from within this function. Each call segment in a chained expression counts independently (`foo().bar().baz()` = 3). High FO = high external coupling.

**NS — Non-Structured Exits**
Count of early returns, throws, breaks, and continues (excluding the final tail return). Scattered exits make control flow hard to trace and postconditions hard to reason about.

**LOC — Lines of Code**
Physical line count. Used for pattern detection only, not the LRS score.

### LRS formula

```
R_cc = min(log2(CC + 1), 6.0)     # logarithmic, capped at 6
R_nd = min(ND, 8.0)               # linear, capped at 8
R_fo = min(log2(FO + 1), 6.0)     # logarithmic, capped at 6
R_ns = min(NS, 6.0)               # linear, capped at 6

LRS = 1.0×R_cc + 0.8×R_nd + 0.6×R_fo + 0.7×R_ns
```

Logarithmic scaling for CC and FO: going from CC 1→4 matters more than CC 40→44. Linear for ND and NS: each additional level contributes uniformly. Caps prevent a single extreme value from dominating.

**Theoretical range:** 1.0 (trivial) to 20.2 (all four at cap).

**Weight rationale:** CC (1.0) = primary defect correlate; ND (0.8) = captures complexity CC can miss; NS (0.7) = implicit exit conditions; FO (0.6) = external coupling weighted lower.

### Risk bands

| Band | LRS range | Typical action |
|---|---|---|
| Critical | ≥ 9.0 | Refactor now |
| High | 6.0–8.9 | Refactor next time you touch it |
| Moderate | 3.0–5.9 | Monitor; block increases in CI |
| Low | < 3.0 | Leave alone |

Thresholds are configurable.

### Field stability: absolute vs. population-relative

Output fields fall into two categories with different stability guarantees. This
matters when diffing output commit-to-commit or gating CI on it:

| Field | Category | Behavior |
|---|---|---|
| `lrs` | Absolute | Depends only on that function's own metrics (`cc`, `nd`, `fo`, `ns`). Unchanged unless the function itself changes. |
| `activity_risk` | Absolute | Extends `lrs` with that function's own git/call-graph signals. Unchanged unless the function or its direct callers/history change. |
| `band` (risk band) | Absolute | A fixed threshold on `lrs`/`activity_risk` (see Risk bands above). Moves only when the function's own score crosses a threshold. |
| `percentile` | Population-relative | A rank against every other function in the current run. Can change when unrelated functions elsewhere in the repo are added, removed, or change score — even if this function is untouched. |
| `driver` / `driver_detail` | Population-relative | Computed from percentile thresholds (default P75, see Driver labels below). Can flip for the same reason as `percentile`. |
| `quadrant` | Population-relative | Depends on population-relative activity thresholds (median/P75, see Quadrant assignment below) as well as `band`. Can flip even when the function's own `lrs`/`activity_risk`/`band` are unchanged. |

**Implication for CI gates:** if you want a gate that only fires when a function's
*own* risk changed — not when the rest of the repo's distribution shifted — key it on
`lrs`, `activity_risk`, and `band`. Gates keyed on `percentile`, `driver`, or
`quadrant` can trip on a commit that didn't touch the flagged function at all, because
those labels are recomputed relative to the whole population on every run.

No output field is renamed or restructured by this distinction — it is purely
documentation of behavior that already exists in `compute_percentiles`,
`populate_driver_labels`, and `compute_quadrants` (`hotspots-core/src/snapshot.rs`).

### Activity Risk Score (snapshot mode)

Extends LRS with git history and call graph signals:

```
Activity Risk = LRS
  + (lines_added + lines_deleted) / 100 × 0.5   # churn
  + min(touch_count_30d / 10, 5.0) × 0.3         # touch frequency (see note below on the window)
  + max(0, 5.0 − days_since_change / 7) × 0.2    # recency
  + neighbor_churn / 500 × 0.2                    # churn in callees
```

`touch_count_30d`'s field name is kept for compatibility, but the window it measures is
**365 days by default**, not 30 — see the touch-window note below the formula.

`fan_in` and `scc` (cyclic dependency) no longer contribute to the live Activity Risk /
composite score, per hotspots-research F160 (confirmed, 17-repo pre-registered gate):
both terms individually showed "no measurable contribution" to ranking quality (fan_in 4%
Shapley share of rho, scc 1%), and dropping both is non-inferior within a pre-registered
margin. Both values are still computed and reported elsewhere — `--axes coupling`,
CSV/HTML output, and as features 5 and 8 of `hotspots train`'s 10-feature set — only the
live Activity Risk sum no longer includes them, following the same pattern already in
place for `burst_score` below. The `fan_in`/`scc` weights (`ScoringWeights.fan_in`/`.scc`,
defaults `0.4`/`0.3`) and the `RiskFactors.fan_in`/`.cyclic_dependency` fields are kept in
place, unused (both are always `0.0` in `RiskFactors`), for the same forward-compatibility
reason as `burst`.

`dependency_depth` (depth from entrypoints) no longer contributes to the live Activity
Risk / composite score, per hotspots-research F167 (scoped 6-repo pre-registered gate,
5 gate-passing): the term's Shapley share of rho was -2.1% (mean) and its leave-one-out
effect was sign-inconsistent across repos, with dropping it non-inferior within the
pre-registered margin. The underlying field is sparse by construction — `dependency_depth`
is only computed via BFS from a small, name-heuristic set of "entry points"
(`callgraph.rs::is_entry_point`), so most functions in most repos never get a real depth
value at all. `dependency_depth` is still computed, populated, and stored on the
snapshot — only the live Activity Risk sum no longer includes it, following the same
pattern as `fan_in`/`scc` above. The `ScoringWeights.depth` weight (default `0.1`) and
the `RiskFactors.depth` field are kept in place, unused (`RiskFactors.depth` is always
`0.0`), for the same forward-compatibility reason as `burst`. See
`hotspots-research/docs/findings/F167-depth-score-decomposition.md` and
`hotspots-research/docs/promotion-briefs/depth-score-remove-from-live-score.md`.

`burst_score` no longer contributes to the live Activity Risk / composite score. It is
computed and stored on the snapshot (and still used by `trainer::cold_start_features`
for offline model training), but `compute_activity_risk` in `scoring.rs` does not read
it: `burst_score` is a full-history value that is effectively monotonic non-decreasing,
so folding it into the live score made Activity Risk a one-way ratchet — a file with
any historical burst, however old, could never leave the CRITICAL tier on this term
alone even after the code had long since stabilized. See
`hotspots-research/docs/burst-score-non-decaying-issue.md` for the full diagnosis and
`hotspots-research/docs/promotion-briefs/burst-score-remove-from-live-score.md` for this
change; a trailing-window or decayed replacement for the live score is tracked as
follow-on research. The `burst` weight (`ScoringWeights.burst`, default `0.3`) and the
`RiskFactors.burst` field are both kept in place, unused (`RiskFactors.burst` is always
`0.0`), so a future replacement can reuse them without a renaming exercise.

`burst_score` is a sliding 30-day-window max/mean commit ratio per file (always ≥ 1.0;
higher means commits cluster into frantic bursts rather than steady, spread-out
changes). For each commit touching a file, it counts how many of that file's commits
fall within the following 30 days, then divides the largest such count by the mean —
a file with 5 commits crammed into one week scores higher than one with 5 commits
spread evenly across a year, even though both have `commit_count = 5`.

Computed as part of the same single full-history `git log --name-only` pass used for
the other cold-start signals (`commit_count`, `author_count`, `author_entropy`,
`isolation_rate`, `age_days`, `last_touch_days`) — one subprocess per `hotspots
analyze` run, not one per file and not a separate walk from those other six signals.
Files with fewer than 2 commits get the baseline `1.0` (no burst signal); files with
no matching commit history keep `burst_score = None`.

Validated against real-world defect data: it's one of the strongest signals in a
CVE/OSV-linked-file logistic regression (largest standardized coefficient of 5
candidate signals, positive across all leave-one-repo-out folds tested) — see
`hotspots-research` findings F67 and F93 for the full cross-repo evaluation.

**Touch window:** the git-log window behind `touch_count_30d` (and
`days_since_last_change`) is 365 days by default (`git::TOUCH_WINDOW_DAYS`), per
hotspots-research F165 (confirmed, pre-registered, 13-repo cutoff-safe gate) — 365 days
gave the strongest, monotonic gain of every window tested, with a size-confound check
ruling out "it's just counting more commits." On a high-commit-velocity repo this is a
real, measured wall-clock cost (a single git subprocess call, but scanning more history:
+16ms on a moderate-velocity repo, up to +943ms on a very active one in testing) —
override it with `touch_window_days` in `.hotspotsrc.json` (any positive integer; default
365) if that cost matters more than the ranking-quality gain for a given repo.

Activity Risk is always ≥ LRS. When no git data is available, Activity Risk = LRS.

All activity-risk weights in the formula above (`churn`, `touch`, `recency`, `depth`,
`neighbor_churn`) are overridable via the `scoring` key in `.hotspotsrc.json`. `fan_in`,
`scc`, and `burst` are also accepted for forward compatibility, but currently have no
effect since none of the three is read by the live formula:

```json
{
  "scoring": {
    "burst": 0.5
  }
}
```

Unset weights fall back to the defaults shown in the formula above. Same validation
as the LRS `weights` block: non-negative, at most 10.0.

### Call graph metrics (snapshot mode)

- **Fan-in** — functions that call this function (blast radius)
- **PageRank** — importance/centrality based on call graph topology
- **Betweenness centrality** — fraction of shortest paths that pass through this function (hub detection); exact for graphs < 2000 nodes, approximate (k=256 pivots) for larger
- **SCC size** — strongly connected component size; > 1 = part of a dependency cycle
- **Dependency depth** — longest acyclic path from entrypoints to this function
- **Neighbor churn** — sum of churn in directly-called functions

### Quadrant assignment

| | Low activity | High activity |
|---|---|---|
| **High/Critical band** | `debt` | `fire` |
| **Low/Moderate band** | `ok` | `watch` |

Activity is "high" if: touch count (365-day window by default, see `touch_window_days`
above) above population median, OR changed within that same window.

`fire` = live regression risk (refactor now). `debt` = structural debt (schedule proactively). `watch` = monitor. `ok` = no action.

`quadrant` is population-relative (activity is judged against the current run's
median/P75) — see "Field stability" above before keying a CI gate on it.

### Driver labels

Each function gets a single primary diagnosis, checked in priority order:

| Label | Condition | Action |
|---|---|---|
| `cyclic_dep` | Part of dependency cycle (SCC > 1) | Break the cycle before adding callers |
| `high_complexity` | CC above P75 | Schedule refactor; extract sub-functions |
| `deep_nesting` | ND above P75 | Flatten with early returns or guard clauses |
| `high_fanout_churning` | FO above P75 AND touches above P50 | Extract interface boundary |
| `high_fanin_complex` | Fan-in above P75 AND CC above P50 | Extract and stabilize; wide blast radius |
| `high_churn_low_cc` | Touches above P75 AND CC below P25 | Add regression tests before next change |
| `composite` | No single dimension clearly dominates | Address the highest dimension first |

Thresholds are percentile-relative (default P=75, configurable via `driver_threshold_percentile`). `cyclic_dep` is the sole absolute check.

`driver_detail` (JSON): for `composite` functions, lists up to 3 near-miss dimensions with their percentile rank (e.g. `"cc (P72), nd (P68)"` — notable but below P75 threshold). Omitted when null.

`driver` and `driver_detail` are population-relative (thresholds are recomputed from
the current run's distribution) — see "Field stability" above before keying a CI gate
on them.

### Pattern detection

Patterns are informational labels. A function can have multiple. They do not affect LRS.

**Tier 1 — structural (all modes):**

| Pattern | Trigger |
|---|---|
| `complex_branching` | CC ≥ 10 AND ND ≥ 4 |
| `deeply_nested` | ND ≥ 5 |
| `exit_heavy` | NS ≥ 5 |
| `god_function` | LOC ≥ 60 AND FO ≥ 10 |
| `long_function` | LOC ≥ 80 |

**Tier 2 — enriched (snapshot mode, requires call graph + git data):**

| Pattern | Trigger |
|---|---|
| `churn_magnet` | churn ≥ 200 lines AND CC ≥ 8 |
| `cyclic_hub` | SCC size ≥ 2 AND fan-in ≥ 6 |
| `hub_function` | fan-in ≥ 10 AND CC ≥ 8 |
| `middle_man` | fan-in ≥ 8 AND FO ≥ 8 AND CC ≤ 4 |
| `neighbor_risk` | neighbor churn ≥ 400 AND FO ≥ 8 |
| `shotgun_target` | fan-in ≥ 8 AND churn ≥ 150 lines |
| `stale_complex` | CC ≥ 10 AND LOC ≥ 60 AND days since change ≥ 180 |
| `volatile_god` | Derived: `god_function` AND `churn_magnet` |

All thresholds configurable in `.hotspotsrc.json`. Use `--explain-patterns` to see which conditions triggered each pattern.

---

## Configuration

Config file is auto-discovered from project root in this order:
1. `--config <path>` CLI flag (explicit override)
2. `.hotspotsrc.json`
3. `hotspots.config.json`
4. `"hotspots"` key in `package.json`

The project root is determined by walking up from the analyzed path to find `.git`. CLI flags take precedence over config file values.

Validate: `hotspots config validate` / Inspect resolved: `hotspots config show`

### Full schema

```json
{
  "schema_version": 1,
  "include": ["src/**/*.ts"],
  "exclude": [
    "**/*.test.ts", "**/*.spec.ts",
    "**/node_modules/**", "**/__tests__/**", "**/__mocks__/**",
    "**/dist/**", "**/build/**", "**/vendor/**",
    "**/*.pb.go", "**/zz_generated*.go"
  ],
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 0.8,
    "fo": 0.6,
    "ns": 0.7
  },
  "warning_thresholds": {
    "watch_min": 2.5,
    "watch_max": 3.0,
    "attention_min": 5.5,
    "attention_max": 6.0,
    "rapid_growth_percent": 50.0
  },
  "min_lrs": 0.0,
  "top": null,
  "co_change_window_days": 90,
  "co_change_min_count": 3,
  "driver_threshold_percentile": 75,
  "per_function_touches": true,
  "policy": {
    "critical_introduction": "warn",
    "critical_introduction_reason": "eval/ scripts are one-shot research code reviewed case-by-case, not shipped services — approved by @stephenc222 2026-07-06",
    "excessive_risk_regression": "block"
  }
}
```

**Validation rules:**
- `moderate < high < critical` (all positive)
- `watch_min < watch_max ≤ moderate < attention_min < attention_max ≤ high`
- All weights non-negative; at least one positive; none > 10.0
- `policy.*` values must be one of `"block"`, `"warn"`, `"off"`
- `policy.<name>_reason` is **required** (non-empty) whenever `policy.<name>` is not `"block"`
- Unknown fields are rejected (to catch typos)
- `schema_version` must not exceed the version this build of hotspots supports

**`schema_version`:** defaults to the current config schema version (currently `1`) when
omitted, so existing `.hotspotsrc.json` files without it keep working unchanged. Bumped
only when a breaking change to weight/threshold semantics needs migration or explicit
detection, mirroring `schema_version` on snapshot and delta files.

**`policy`:** severity overrides for the two blocking CI policies. Both default to
`"block"`. `critical-introduction` fires identically whether a function is brand-new or
an existing function that regressed to Critical — a Critical function needs review
either way, so there is no separate new-vs-regressed knob, only overall severity.
Set to `"warn"` to report without failing CI (e.g. a research repo where new
one-shot scripts are expected to score high on introduction), or `"off"` to disable
the policy entirely. Raising `thresholds.critical` instead changes what counts as
Critical repo-wide (affecting reporting too); `policy` only changes what happens
once something *is* Critical.

Downgrading a policy below `"block"` requires a `<name>_reason` string — mirroring the
`// hotspots-ignore: <reason>` convention for per-function suppression — so that anyone
reviewing a `.hotspotsrc.json` diff sees *why* a blocking gate was weakened, not just
that it was. `hotspots config validate` rejects a downgrade with a missing or
whitespace-only reason. This does not make the override unbypassable — anyone with
commit access to the config can still weaken it, the same as anyone with access to a CI
workflow file can remove a required check — but it does mean the change can't be silent.

**`driver_threshold_percentile`:** default 75 means a function must be in the top 25% of its metric to receive a specific driver label. Lower (50–60) for small/uniform repos; higher (85–90) for large repos with high median complexity.

**`co_change_window_days`:** days of git history to mine for file co-change pairs. Increase for repos with slow commit cadence.

**`per_function_touches`:** `true` = use cached `git log -L` per-function counts; `false` = file-level batching always (useful in CI without persistent cache).

---

## JSON Schema

### Schema versions

| Version | Structure | When |
|---|---|---|
| v4 (default snapshot JSON) | `fire`/`debt`/`watch`/`ok` triage buckets + per-function `action` + `architecture` aggregates | `hotspots analyze --mode snapshot` |
| v2 (full snapshot) | Flat `functions` array + enriched `aggregates` | `--all-functions` |
| v1 (delta) | `deltas` array with before/after | `--mode delta` |

Always check `schema_version` before consuming output in tooling.

`analysis.formula_version` is a separate integer that tracks the scoring
formula (default `ScoringWeights`, `LrsWeights`, `RiskThresholds`), bumped
only when a default weight or threshold changes. `analysis.tool_version`
changes on every release; `formula_version` does not — use it in `hotspots
diff`/`trends` tooling to tell a score delta caused by a tool upgrade apart
from one caused by a real code change.

**`formula_version` vs. config `schema_version` — not the same thing.**
`formula_version` tracks changes to hotspots' *built-in default* weights/thresholds
(`ScoringWeights::default()` etc.) and is unaffected by a user's own
`.hotspotsrc.json` — a repo with fully custom weights sees the same
`formula_version` as one with no config at all, because *their* scoring didn't
change even when the shipped defaults did. Config `schema_version` (above) tracks
the *shape/meaning* of the config file format itself — e.g. a field being renamed
or changing units — independent of what any particular default value is. A release
that changes a default weight's value bumps `formula_version` only; a release that
changes what a config field *means* bumps `schema_version` (and `formula_version`
too, if that also changes computed scores). Note `formula_version` today only
covers default-value changes, not changes to the scoring formula's structure
(e.g. adding/removing a term from `compute_activity_risk`) — a structural change
is not guaranteed to bump it.

### Function fields (v2 / `--all-functions`)

```json
{
  "function_id": "src/api/billing.ts::processPlanUpgrade",
  "file": "src/api/billing.ts",
  "line": 142,
  "language": "TypeScript",
  "lrs": 12.4,
  "band": "critical",
  "quadrant": "fire",
  "driver": "high_complexity",
  "driver_detail": null,
  "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 },
  "risk": { "r_cc": 4.0, "r_nd": 4.0, "r_fo": 3.0, "r_ns": 3.0 },
  "patterns": ["complex_branching", "churn_magnet"],
  "pattern_details": null,
  "suppression_reason": null,
  "churn": { "lines_added": 156, "lines_deleted": 89, "net_change": 67 },
  "touch_count_30d": 12,
  "days_since_last_change": 3,
  "activity_risk": 18.5,
  "callgraph": {
    "fan_in": 8, "fan_out": 8,
    "pagerank": 0.0042, "betweenness": 127.3,
    "scc_id": 0, "scc_size": 1, "dependency_depth": 5
  }
}
```

`pattern_details` is populated only with `--explain-patterns`. `suppression_reason` is omitted (not null) when no suppression is present.

`lrs`, `activity_risk`, and `band` are absolute — stable across runs unless the
function's own metrics/history change. `quadrant`, `driver`, `driver_detail`, and
`percentile` (`is_top_10_pct`/`is_top_5_pct`/`is_top_1_pct`, omitted from the example
above but present when populated) are population-relative — they can change between
runs purely because other functions in the repo changed, even when this function did
not. See "Field stability" under Metrics above.

### Aggregates (`--all-functions`)

**`aggregates.file_risk`** — per-file ranked by `file_risk_score`:
```
file_risk_score = max_cc×0.4 + avg_cc×0.3 + log2(fn_count+1)×0.2 + churn_factor×0.1
```

**`aggregates.co_change`** — file pairs that change together in the same commit:
```json
{
  "file_a": "hotspots-cli/src/main.rs",
  "file_b": "hotspots-core/src/aggregates.rs",
  "co_change_count": 14,
  "coupling_ratio": 0.78,
  "has_static_dep": false,
  "risk": "high"
}
```
`risk: "expected"` = a static import exists; co-change is explained.

**`aggregates.modules`** — directory-level instability:
```json
{
  "module": "hotspots-core/src",
  "afferent": 8,
  "efferent": 3,
  "instability": 0.27,
  "module_risk": "high"
}
```
Instability near 0 = everything depends on it (risky to change). Instability near 1 = depends on others (safe to change).

**`aggregates.models`** / **`architecture.models`** — present with `--include-models`:
```json
{
  "items": [{
    "name": "Snapshot", "file": "...", "line": 219,
    "kind": "struct", "score": 52.11,
    "critical": 4, "high": 15, "moderate": 17,
    "functions": [...]
  }],
  "links": [{ "source": 0, "target": 2, "shared_functions": 15, "shared_risk": 83.53 }]
}
```

### Delta output (v2)

```json
{
  "schema_version": 2,
  "commit": { "sha": "abc123", "parent": "def456" },
  "baseline": false,
  "deltas": [{
    "function_id": "src/api/billing.ts::processPlanUpgrade",
    "status": "modified",
    "before": { "lrs": 11.0, "band": "high", "metrics": { "cc": 13, "nd": 3, "fo": 7, "ns": 2 } },
    "after":  { "lrs": 12.4, "band": "critical", "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 } },
    "delta": { "cc": 2, "nd": 1, "fo": 1, "ns": 1, "lrs": 1.4 },
    "band_transition": { "from": "high", "to": "critical" }
  }],
  "policy": {
    "failed": [{ "id": "critical-introduction", "severity": "blocking", "message": "..." }],
    "warnings": []
  },
  "aggregates": {
    "pr_summary": {
      "pr_risk_score": 1.4,
      "fn_changed_lines": 23,
      "size_band": "small",
      "band": "critical",
      "new_count": 0,
      "modified_count": 1,
      "deleted_count": 0,
      "regression_count": 1,
      "improvement_count": 0,
      "band_upgrades": 1,
      "policy_blocking": true
    }
  },
  "change_risk": {
    "scope": { "kind": "range", "base": "def456", "head": "abc123" },
    "score": { "kind": "sum_positive_delta_lrs", "version": "1.0.0", "value": 1.4 },
    "components": {
      "max_delta_lrs": 1.4,
      "sum_positive_delta_lrs": 1.4,
      "new_critical_functions": 0,
      "changed_functions": 1
    },
    "inputs": { "functions_scored": 1, "noise_epsilon": 1e-9, "overlap_filter_applied": true },
    "tool_version": "1.42.0"
  }
}
```

Delta statuses: `new`, `deleted`, `modified`, `unchanged` (unchanged omitted by default).

**`change_risk`** (hotspots#202, `schema_version` 2) — a single change-level risk score,
additive to `deltas[]`. The scored set is `{new, modified, deleted}` functions whose
`|delta.lrs| > noise_epsilon` (default `1e-9`, looser than the per-function `modified`
status's own `f64::EPSILON` gate, to filter out genuinely-noise-sized float wobble) or
whose band transitioned, AND (when git diff data is available) whose line span overlaps a
changed hunk — `inputs.overlap_filter_applied` records whether that filter actually ran.
`sum_positive_delta_lrs` sums `max(0, after.lrs - before.lrs)` across the scored set (an
added function contributes its full `after.lrs`; a removed function contributes `0` — code
removal cannot increase risk). `scope.kind` is `"commit"` for `--mode delta` (parent-relative)
or `"range"` for `hotspots diff <base> <head>` (arbitrary two refs) — an open string, since a
hosted API caller may know a more specific scope (e.g. `"pull_request"`) without a CLI schema
change. Omitted entirely on a baseline delta (no base to score against).

**`change_risk.score.value` is not a validated defect predictor — do not present it as one.**
hotspots-research F161 (`docs/findings/F161-change-risk-vs-size-matched-baseline.md` in
`../hotspots-research`) benchmarked this exact formula (`net_delta_lrs`, equal to the shipped
`sum_positive_delta_lrs`/`pr_risk_score`) against a size-matched baseline (`fn_changed_lines` —
same lines, LRS weight set to 1) on 4 repos (black, iced, fiber, paperless-ngx) predicting
SZZ-attributed defects. Result: the shipped score was **the weakest candidate on all 4 repos**
(AUC −0.022 to −0.112 below the size baseline), and a wider sweep of 5 other LRS-weighted
operators (top-k mean, L2 norm, fan-in weighted, per-file/per-function normalized) also failed
to beat it (0 of 20 cells). F159 found the same pattern pre-registered (`max_delta_lrs`
indistinguishable from changed-function count, AUC 0.476 excluding fix/revert PRs). F161's
explicit CLI implication: expose the structural parts (`components`), do not ship a default
scalar as calibrated risk. `score.value` here is a structural signal only (worst-LRS-delta in
the diff) — treat it the same as `pr_risk_score` above, with stronger direct evidence against it.

**`aggregates.pr_summary`** — collapses every changed function into a single PR-wide
risk view: `pr_risk_score` is the net LRS delta summed across the whole diff (new
functions add `after.lrs`, deleted functions subtract `before.lrs`, modified
functions add `delta.lrs`); `band` is the highest risk band reached by any
new/modified function's `after` state.

**`fn_changed_lines`** is the count of diff-changed lines (`git diff -U0`) that fall inside a
touched function's line span, summed across the whole diff — hotspots-research (F132, F159,
F161) tested this directly against the real compiled binary and found it the strongest
predictor of which PRs later need a defect fix, beating `pr_risk_score` and every other
LRS-weighted score tried on every repo tested. **Read `pr_risk_score` as a structural signal
(what's the worst code in this diff), not a validated risk estimate** — `fn_changed_lines` is
the better-evidenced number for "is this PR risky." Present whenever `aggregates` is attached
(always true for `hotspots diff` output).

**`size_band`** buckets `fn_changed_lines` into `"small"` / `"medium"` / `"large"` /
`"very_large"` for a quick, human-readable comparison across PRs. **Unlike `fn_changed_lines`
itself, this is not a research-derived signal** — no finding specifies where "small" ends and
"large" begins; the cutoffs (default: <50/<200/<500/else) are an arbitrary, configurable
presentation judgment, the same category as `RiskThresholds`' own LRS band cutoffs. Override
via `.hotspotsrc.json`'s `change_size_thresholds` (`small`/`medium`/`large`, each a
`fn_changed_lines` count; must satisfy `small < medium < large`) if the defaults don't fit
your repo's typical change size:

```json
{
  "change_size_thresholds": { "small": 30, "medium": 150, "large": 600 }
}
```

---

## Supported Languages

| Language | Extensions |
|---|---|
| TypeScript | `.ts`, `.tsx`, `.mts`, `.cts`, `.mtsx`, `.ctsx` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs`, `.mjsx`, `.cjsx` |
| Go | `.go` |
| Python | `.py`, `.pyw` |
| Rust | `.rs` |
| Java | `.java` |
| C / C headers | `.c`, `.h` |
| C# | `.cs` |
| Vue | `.vue` |

All languages have full parity across all metrics and features.

**JSX note:** `.jsx` and `.tsx` files support JSX syntax. Plain `.js` files also enable JSX parsing (React webpack convention). JSX elements do not add CC; control flow in JSX (`&&`, ternary) does.

---

## Scoring Changelog

All changes to formulas, weights, thresholds, or ranking rules are tracked in git commit history. The LRS formula and default weights have been stable since v1.0. The trained ranker feature was introduced in a later release; the current model is `model_version 5` (10 features). Check `CHANGELOG.md` for version-specific details.

- The `authors_90d` and `directed_coupling` `--explain` phrases were reworded from directive to descriptive language (e.g. "no clear owner" → "no single frequent owner in recent history") after four independent tests found no outcome benefit from acting on the ownership/coupling diagnosis. See `META-12-observational-prescription-tests-fail.md` and `F103-review-hotspots-interventional.md` in `hotspots-research`.
