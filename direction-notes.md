# Direction Notes

Assessment based on self-analysis run (2026-02-19) + two independent Claude sessions in agreement.

## Where we are

Phases 1–4 and 8 are essentially done. Solid analytical foundation:
- CFG, cyclomatic, nesting, fanout, PageRank, churn, touch, recency
- Co-change coupling (D-2, CC-1, CC-2, CC-3), file-level view (D-1), module instability (D-3)

The direction is right. The risk is shipping a high-complexity, low-signal instrument —
impressive internals, output that's hard to act on.

## The core problem

**Stop adding dimensions.** Feature velocity is outrunning signal quality. The composite
score is the bottleneck: file-level churn applied uniformly to all functions in a file is
the single largest source of noise. 8 of 15 top results in self-analysis have *identical*
activity scores — that's not ranking, that's listing. When half the top results are noise,
users stop trusting the real findings too.

The remaining task files (CFG_ACCURACY, TEST_COVERAGE, TYPES, OUTPUT, HISTORY) should wait
until signal quality is fixed. More dimensions built on a noisy base produce more noise.

## What to fix next

**1. Per-function git touches cache (highest ROI)**

`--per-function-touches` exists but is ~50x slower. Making it fast via caching fixes the
largest source of noise simultaneously across all dimensions — ranking, activity risk,
neighbor churn. Everything downstream improves when this is right.

**2. Dimension-specific actions in `--explain` (output-layer fix)**

The data is already in the JSON. `--explain` collapses it into one composite score with a
generic "URGENT" for everything. Tie the recommended action to the driving dimension:

| driver | action |
|---|---|
| high cc + low churn | stable debt, schedule refactor |
| high churn + low cc | add tests, watch for regression |
| high fanout + high churn | consider interface boundary |
| high nesting | flatten with early returns |
| high fan-in + high cc | extract and stabilize |

This is a presentation change, not an analysis change. Low effort, high impact.

## What's lower priority than it looks

**Splitting main.rs** matters for maintainability and for the tool's own self-reported
hotspot score, but users won't notice it. It's hygiene, not strategy — do it eventually,
not next.

**More co-change dimensions** — CC-1's `has_static_dep` was a genuine signal quality
improvement (distinguishing expected from hidden coupling), but it operates at the
aggregates layer, not at the per-function score layer. The core noise problem is unaffected.

## Task tracking note

CC_TASKS.md was updated 2026-02-19 to reflect shipped state (all CC-1 through CC-3 done).
Keep task files current — stale checkboxes make them useless as source of truth.

## Consolidation milestone

Phase 6 (testing/docs) is marked deferred throughout TASKS.md. Set a deliberate point to
stop adding features and close this gap. The analysis depth is solid; the limiting factor
is signal quality and actionability.
