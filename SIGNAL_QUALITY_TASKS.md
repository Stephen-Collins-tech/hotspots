# Signal Quality Tasks

**Motivation:** The analysis engine is solid. The limiting factor is signal quality and
actionability. Two problems dominate:

1. **File-level churn smeared across functions.** The default path applies one file-level
   touch count to every function in that file. On an active feature branch, 8 of 15 top
   results can have identical activity scores — that's not ranking, that's listing.
   `--per-function-touches` exists but runs one `git log -L` subprocess per function,
   making it ~50× slower than file-level batching. It needs a cache to be practical.

2. **One composite score, one generic action.** `--explain` collapses cc, nesting, fanout,
   churn, fan-in into a single activity_risk and emits "URGENT: Reduce complexity" for
   most functions. The data to show dimension-specific actions already exists in the JSON
   output — this is a presentation fix, not an analysis change.

Fix these two things before adding any new dimensions. Both have higher ROI than anything
in the remaining task files.

---

## SQ-1: Per-Function Touch Cache

**Summary:** Cache `git log -L` results to disk so `--per-function-touches` is fast enough
to be the default (or at least practical for CI). The cache key is
`(commit_sha, file, start_line, end_line)`. A cache hit skips the subprocess entirely.

### How it works

Each `function_touch_metrics_at` call currently spawns two git subprocesses (one for the
window count, one for `days_since` fallback). With 400 functions, that's 800 subprocesses.

A simple on-disk cache stores results keyed by `(sha, file, start, end)`. On a second run
of the same commit (CI re-runs, `--no-persist` reruns) every key hits. On a new commit,
only functions whose line ranges changed miss — the rest carry over from the parent
snapshot's cache entries.

### Cache format

`$repo_root/.hotspots/touch-cache.json.zst` — a flat JSON object:

```json
{
  "<sha>:<file>:<start>:<end>": [<touch_count_30d>, <days_since_or_null>],
  ...
}
```

Compressed with zstd (same as snapshots). Load once at the start of analysis, write back
at the end only if new entries were added. Bounded: evict entries whose SHA is not in the
snapshot index (old commits that are no longer referenced).

### Tasks

- [ ] **SQ-1a:** Add `read_touch_cache` / `write_touch_cache` in a new
  `hotspots-core/src/touch_cache.rs`. Key: `"{sha}:{file}:{start}:{end}"`. Value:
  `(usize, Option<u32>)`. Serialize as `HashMap<String, (usize, Option<u32>)>`,
  compress/decompress with zstd. Return `None` on missing file (cold start).

- [ ] **SQ-1b:** In `populate_per_function_touch_metrics` (`snapshot.rs`), load the cache
  before the loop, check each key before spawning a subprocess, write the cache back after
  the loop if any new entries were added. The commit SHA is available on `self`.

- [ ] **SQ-1c:** Add cache eviction: after writing, drop entries whose SHA prefix is not
  present in the snapshot index (`load_index`). Keep at most the last 50 distinct SHAs to
  bound file size.

- [ ] **SQ-1d:** Benchmark: run `--per-function-touches` on this repo cold vs. warm.
  Document the warm speedup in a comment in `touch_cache.rs`. Target: warm run ≤ 2× the
  file-level baseline.

- [ ] **SQ-1e:** Make `--per-function-touches` the default once the warm path is fast
  enough (or add a config flag `per_function_touches: true`). Update docs.

**Effort:** Medium. New module, cache I/O, eviction logic.
**Risk:** Low. Purely additive; file-level path is unchanged as fallback.

---

## SQ-2: Dimension-Specific Actions in `--explain`

**Summary:** Replace the current `get_recommendation()` function (which looks at composite
score + two callgraph flags) with logic that identifies the *primary driving dimension* and
returns a specific, actionable recommendation tied to it.

The data is already in `FunctionSnapshot`. This is a pure output-layer change — no new
analysis, no new JSON fields.

### Driving dimension detection

Identify the single largest contributor to `activity_risk`. Approximate by ranking:

| condition | driver label | action |
|---|---|---|
| `metrics.cc > 15` or `cc` is top-3 percentile | `high_complexity` | "Stable debt: schedule a refactor. Extract sub-functions to reduce CC." |
| `touch_count_30d > 10` and `metrics.cc < 8` | `high_churn_low_cc` | "Churning but simple: add regression tests before next change." |
| `callgraph.fan_out > 8` and `touch_count_30d > 5` | `high_fanout_churning` | "High coupling + active change: consider extracting an interface boundary." |
| `metrics.nd > 4` | `deep_nesting` | "Deep nesting: flatten with early returns or guard clauses." |
| `callgraph.fan_in > 10` and `metrics.cc > 8` | `high_fanin_complex` | "Many callers + complex: extract and stabilize. Any bug here has wide blast radius." |
| `callgraph.scc_size > 1` | `cyclic_dep` | "Cyclic dependency: break the cycle before adding more callers." |
| default | `composite` | (current generic message) |

### Tasks

- [ ] **SQ-2a:** Add `fn driving_dimension(func: &FunctionSnapshot) -> (&'static str, &'static str)`
  to `main.rs` (or a new `explain.rs` output module). Returns `(driver_label, action_text)`.
  Implement the priority table above. Driver label is for the `--explain` table column;
  action text replaces the current recommendation string.

- [ ] **SQ-2b:** Update `print_explain_output` to show a `driver` column in the per-function
  table (e.g., `high_complexity`, `high_churn`, `cyclic_dep`). Keep the existing columns,
  add `driver` after `band`.

- [ ] **SQ-2c:** Update the per-function action line in `--explain` to use the new
  dimension-specific text. Currently: `  → {recommendation}`. No format change needed,
  just better content.

- [ ] **SQ-2d:** Add `driver: String` field to `FunctionSnapshot` JSON output so consumers
  can use it programmatically. Populate it in the snapshot enricher using the same logic
  (extracted to `hotspots-core`). Update JSON schema docs.

**Effort:** Low–medium. SQ-2a through SQ-2c are pure CLI output changes (~1–2 hours).
SQ-2d requires adding a field to the snapshot struct and updating all constructors.
**Risk:** Low. Existing output shape unchanged for SQ-2a–2c. SQ-2d is additive.

---

## Ordering

```
SQ-1 (touch cache)      — highest ROI, fixes ranking quality
SQ-2 (action specificity) — independent of SQ-1, can be done in parallel or after
```

SQ-2a–2c can be done immediately (no deps). SQ-2d should come after to avoid struct churn.
SQ-1 is more impactful but more work. Start with SQ-2a–2c for a quick win, then SQ-1.

---

**Created:** 2026-02-19
