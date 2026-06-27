# REQ-001 — History Depth Tier on Ranked Output

**Source finding:** F10 — History Depth Predicts Ranking Quality  
**Research brief:** `hotspots-research/docs/promotion-briefs/f10-history-depth-context.md`  
**Status:** ready to implement  

---

## Problem

The trained ranker's prediction confidence varies by how much git history a function has.
Functions with 51–200 lifetime touches (Rich bucket) yield FT ρ=+0.635 vs ARS +0.131 —
a +0.504 delta. Functions with 1–10 touches (Sparse) yield only +0.301. Users currently
have no way to know whether a high risk score is backed by 200 data points or 3.

## What to build

Annotate each function in ranked output with a `history_depth` tier derived from
`total_churn` (lifetime lines added + deleted, already present in `FunctionSnapshot`).
This is a display annotation only — it does not affect sort order, training, or scoring.

## Exact specification

### Enum

```rust
// hotspots-core/src/trainer.rs  (or snapshot.rs — wherever most natural)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryDepth {
    Sparse,    // 0–10 lifetime touches
    Moderate,  // 11–50
    Rich,      // 51–200
    VeryRich,  // 201+
}

pub fn history_depth_tier(total_churn: u32) -> HistoryDepth {
    match total_churn {
        0..=10   => HistoryDepth::Sparse,
        11..=50  => HistoryDepth::Moderate,
        51..=200 => HistoryDepth::Rich,
        _        => HistoryDepth::VeryRich,
    }
}
```

`total_churn` for the purposes of this tier is `churn.lines_added + churn.lines_deleted`
from `FunctionSnapshot.churn` (same value used as the `total_churn` training feature).
When `churn` is `None`, treat as `0` → `Sparse`.

### Output field

Add `history_depth: Option<HistoryDepth>` to `FunctionSnapshot`. Set it after git
enrichment (same phase as `activity_risk`). When no git data is available (no churn),
leave as `None` — do not fabricate a tier from zero.

### JSON serialisation

`"history_depth": "rich"` — lowercase snake_case via `serde(rename_all = "snake_case")`.
`None` serialises as absent (use `#[serde(skip_serializing_if = "Option::is_none")]`).

### Text output

Only emit when `--explain` is active. Append `[rich history]` / `[sparse history]` /
`[moderate history]` / `[very rich history]` after the driver label on each function line.
Omit when `history_depth` is `None`.

---

## Files to change

| File | Change |
|---|---|
| `hotspots-core/src/snapshot.rs` | Add `history_depth: Option<HistoryDepth>` to `FunctionSnapshot`; add `HistoryDepth` enum + `history_depth_tier()` fn (or put in `trainer.rs` — pick one) |
| `hotspots-core/src/analysis.rs` | Populate `history_depth` after churn is resolved, in the same pass as `activity_risk` |
| `hotspots-cli/src/output/explain.rs` | Append tier label after driver in `--explain` text output |
| `hotspots-core/src/aggregates.rs` | Add `history_depth: Option<HistoryDepth>` to `AgentFunctionView` |

---

## Acceptance criteria

1. `cargo test` passes.
2. `history_depth_tier(0)` → `Sparse`. `history_depth_tier(10)` → `Sparse`. `history_depth_tier(11)` → `Moderate`. `history_depth_tier(50)` → `Moderate`. `history_depth_tier(51)` → `Rich`. `history_depth_tier(200)` → `Rich`. `history_depth_tier(201)` → `VeryRich`.
3. Unit test covers all four bucket boundaries and the zero case.
4. JSON output includes `"history_depth": "<tier>"` for functions with churn data; field absent for functions without.
5. `hotspots analyze --explain` text output shows the tier label on each function line.
6. `hotspots analyze` (no `--explain`) output is byte-identical to current — no new text.
7. All existing golden tests pass.

---

## Do not

- Do not use `history_depth` as a training feature or ranking input.
- Do not add a `--min-history` CLI flag to suppress sparse results (future enhancement, not this brief).
- Do not change bucket boundaries — they come from F10's holdout analysis.
- Do not emit the tier in non-snapshot modes (delta, LRS-only).

---

## Supporting evidence

| Bucket | N (django) | ARS ρ | FT ρ | Δ |
|---|---|---|---|---|
| Sparse (1–10) | 217 | +0.182 | +0.301 | +0.119 |
| Moderate (11–50) | 720 | +0.181 | +0.334 | +0.153 |
| **Rich (51–200)** | **1,428** | **+0.131** | **+0.635** | **+0.504** |
| Very Rich (201+) | 190 | +0.309 | +0.529 | +0.220 |

Source: `hotspots-research/docs/findings/10-history-depth-and-signal-quality.md`
