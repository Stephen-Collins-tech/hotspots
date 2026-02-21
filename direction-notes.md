# Direction Notes

Assessment based on self-analysis run (2026-02-19) + two independent Claude sessions in agreement.
Updated 2026-02-19 to reflect completion of signal quality work.

## Where we are

Phases 1–4 and 8 are essentially done. Solid analytical foundation:
- CFG, cyclomatic, nesting, fanout, PageRank, churn, touch, recency
- Co-change coupling (D-2, CC-1, CC-2, CC-3), file-level view (D-1), module instability (D-3)
- Per-function touch cache (SQ-1): warm runs match file-level speed (~230 ms vs ~268 ms)
- Dimension-specific actions in `--explain` (SQ-2): `driver` field in JSON + label in CLI output

Signal quality is no longer the bottleneck.

## The core problem (resolved)

~~Stop adding dimensions.~~ The noise problem was: file-level churn applied uniformly to all
functions in a file, causing identical activity scores across functions in the same file.

**Fixed by SQ-1:** Per-function touches now cached on disk. Warm runs are as fast as file-level.
Default is on (`per_function_touches: true` in config). First run per commit is slow (~6 s for
~200 functions); subsequent runs hit the cache (~230 ms).

**Fixed by SQ-2:** `--explain` now identifies the driving dimension (`cyclic_dep`,
`high_complexity`, `high_churn_low_cc`, etc.) and shows a specific action. The `driver` field
is present in JSON output for programmatic consumers.

## What's next

**Phase 6: Testing & Documentation** — the deliberate consolidation milestone.

Signal quality is fixed. The remaining task files (CFG_ACCURACY, TEST_COVERAGE, TYPES, OUTPUT,
HISTORY) remain lower priority. Phase 6 docs are now unblocked:

1. `docs/reference/cli.md` — document current flags (per_function_touches default, --level module,
   --explain driver output)
2. `docs/reference/scoring.md` — explain activity risk, driver labels, lrs vs activity_risk
3. `docs/reference/metrics.md` — LOC, churn, touch, recency, graph metrics
4. `README.md` — update feature list to reflect current state

## What's lower priority than it looks

**Splitting main.rs** matters for maintainability and for the tool's own self-reported hotspot
score, but users won't notice it. It's hygiene, not strategy — do it eventually, not next.

**More co-change dimensions** — CC-1's `has_static_dep` was a genuine signal quality improvement
(distinguishing expected from hidden coupling), but it operates at the aggregates layer, not at
the per-function score layer. The core noise problem is now resolved.

**CFG_ACCURACY, TEST_COVERAGE, TYPES, OUTPUT, HISTORY task files** — still valid work, but
lower priority than closing the Phase 6 documentation gap. More dimensions built on a solid
base are still more dimensions.

## Task tracking note

Keep task files current — stale checkboxes make them useless as source of truth.
- CC_TASKS.md: all CC-1 through CC-3 done ✅
- SIGNAL_QUALITY_TASKS.md: all SQ-1 and SQ-2 done ✅
- DIMENSIONS_TASKS.md: D-1, D-2, D-3 done ✅
- TASKS.md Phase 2.1: call graph infrastructure done ✅
