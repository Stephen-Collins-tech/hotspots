# Signal Quality: Empirical Record

Three runs on this codebase tracking the impact of SQ-1, SQ-2, and SQ-3.

---

## Run 1 — before SQ fixes (2026-02-19)

`hotspots analyze . --mode snapshot --explain --no-persist`
Call graph: 168/1546 resolved (11% internal).

**The problem:** 8 of 15 top results were `main.rs` functions with identical activity scores
(78 lines changed, 10 commits, recency=1.0). Every result: `URGENT: Reduce complexity`.

Real findings buried in the noise:
- `analyze` (mcp-server): cc=19, 3 dependents
- `process_dir_entry`: nesting=4
- `compute_summary`: fanout=11, nesting=4

---

## Run 2 — after SQ-1 (touch cache) + SQ-2 (dimension-specific actions)

Call graph: 180/1594 resolved (11% internal).

Activity scores within `main.rs` now distinct:
`handle_mode_output` 11 commits / `handle_analyze` 10 / `emit_snapshot_output` 5 / `emit_delta_output` 1

Driver labels firing but 12 of 15 still fell through to `[composite]` — absolute thresholds
(cc>15) too high for this codebase's median complexity of ~3.

`compute_summary` (cc=6, nesting=4) correctly got `[deep_nesting]` — driver detection
looked past the low cc.

---

## Run 3 — after SQ-3 (percentile-relative thresholds)

Call graph: 180/1594 resolved (11% internal). 533 functions.

Thresholds now derived from the snapshot's own P75 distribution:
- cc_high = 8 (P75), nd_high = 2 (P75), touch_high = 2 (P75)

Label distribution:
| label | count | % |
|---|---|---|
| composite | 365 | 68% |
| high_complexity | 105 | 20% |
| deep_nesting | 32 | 6% |
| high_fanout_churning | 29 | 5% |
| high_fanin_complex | 2 | <1% |

Before: 12 of 15 top results `[composite]`. After: 14 of 15 specific labels.

`driving_dimension_label` (#5, cc=12, nesting=6) correctly flagged as a refactor candidate —
the function that classifies other functions prescribed its own refactor.

---

## Remaining limits

- Recency inflation on active branches (circular, not yet addressed)
- 11% call graph resolution caps fan-in/fan-out signal (data collection problem, not scoring)
