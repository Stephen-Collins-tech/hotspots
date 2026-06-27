# REQ-003 — Ranker Explanation Layer (✦ phrases)

**Source finding:** Ranker track + F10/F54/F22 convergence  
**Research brief:** `hotspots-research/docs/promotion-briefs/f55-jepa-ranker-explanation.md`  
**Status:** ready to implement  

---

## Problem

`hotspots analyze` reports risk scores and structural tags (e.g. `[god_function, churn_magnet]`)
but gives no human-readable rationale for *why* a specific function is ranked high. Developers
must infer the reason from raw metrics. An AI coding agent receiving this output must do the
same. The explanation layer closes this gap with a deterministic, phrase-table-based diagnosis
line — no LLM, no network call, no SHAP at runtime.

## What to build

When `--explain` is passed, render a `✦` line below each CRITICAL and HIGH function row in
text output. The line is a short English phrase naming the 1–3 features most elevated relative
to the rest of the repo, with a severity suffix.

The phrase is computed from per-feature percentile ranks within the current snapshot — not
from the model itself. This means it works with or without a trained ranker, and model
retraining does not change phrase output.

---

## Feature set to explain

The 9 features currently in `FEATURE_NAMES` (after REQ-002 ships, 10):

| Feature | Plain-English label |
|---|---|
| `total_churn` | high lifetime churn |
| `lrs` | structurally complex |
| `fan_in` | depended on by many callers |
| `authors_90d` | no clear owner |
| `directed_coupling` | tightly coupled to other hotspots |
| `cc` | high cyclomatic complexity |
| `nd` | deeply nested |
| `fo` | high fan-out |
| `loc` | very long function |

Use these 9 (or 10 after REQ-002) for percentile ranking. `cc`, `nd`, `fo`, `loc` are
structural — only include them in phrases when they are in the top 2 percentile-ranked
features (they are usually dominated by activity signals in practice).

---

## Architecture

### New file: `hotspots-core/src/phrases.rs`

```rust
pub struct FeaturePercentiles {
    pub lrs: f32,
    pub cc: f32,
    pub nd: f32,
    pub loc: f32,
    pub fo: f32,
    pub fan_in: f32,
    pub total_churn: f32,
    pub authors_90d: f32,
    pub directed_coupling: f32,
}

/// Threshold above which a feature is considered "elevated" within this repo.
const ELEVATED: f32 = 0.80;

/// Returns a short English phrase naming the top 1–3 elevated features.
/// Falls back to the single highest feature if nothing crosses ELEVATED.
pub fn top_phrases(p: &FeaturePercentiles) -> String { ... }
```

`top_phrases` logic:
1. Build a list of `(percentile, label_string)` for each field.
2. Filter to those ≥ `ELEVATED` (80th percentile within repo).
3. Check the co-occurrence phrase table (pairs, highest two by percentile) first.
4. If a pair matches, return the pair phrase + severity suffix (appended by caller).
5. Otherwise join the top 1–2 single-feature phrases with ", and ".
6. If nothing is elevated (all < 0.80), use the single highest feature regardless.

### Phrase table (co-occurrence pairs take priority)

| Condition | Phrase |
|---|---|
| `total_churn` + `fan_in` elevated | "churns heavily and is load-bearing" |
| `total_churn` + `authors_90d` elevated | "high churn with no clear owner" |
| `lrs` + `fan_in` elevated | "structurally complex and widely depended on" |
| `lrs` + `total_churn` elevated | "complex and frequently changed" |
| `authors_90d` + `fan_in` elevated | "no clear owner and called from many places" |
| `directed_coupling` + `total_churn` elevated | "coupled to hotspots and frequently changed" |

### Single-feature phrases

| Feature | Phrase |
|---|---|
| `total_churn` | "high lifetime churn" |
| `lrs` | "structurally complex" |
| `fan_in` | "depended on by many callers" |
| `authors_90d` | "no clear owner" |
| `directed_coupling` | "tightly coupled to other hotspots" |
| `cc` | "high cyclomatic complexity" |
| `nd` | "deeply nested" |
| `fo` | "high fan-out" |
| `loc` | "very long function" |

### Severity suffix (appended after phrase by the renderer)

| Band | Suffix |
|---|---|
| Critical | `"Multiple independent signals agree."` |
| High | `"Worth prioritising before next release."` |

---

## Computing percentiles

After `snapshot.functions` is fully populated (after `compute_activity_risk`), compute
per-feature percentile ranks across all functions in the snapshot:

```rust
fn compute_feature_percentiles(snapshot: &Snapshot) -> Vec<FeaturePercentiles> {
    // For each of the 9 features, collect all values, sort, then assign percentile
    // rank = position / (n - 1) for each function.
    // Return one FeaturePercentiles per function, aligned to snapshot.functions order.
}
```

This is an O(n log n) pass over the snapshot — negligible cost.

---

## Changes required

| File | Change |
|---|---|
| `hotspots-core/src/phrases.rs` | **New file** — `FeaturePercentiles`, `top_phrases()`, phrase table, `ELEVATED` const |
| `hotspots-core/src/lib.rs` | Add `pub mod phrases;` |
| `hotspots-core/src/snapshot.rs` | Add `explanation: Option<String>` to `FunctionSnapshot` with `#[serde(skip_serializing_if = "Option::is_none")]` |
| `hotspots-core/src/analysis.rs` | After `compute_activity_risk()`, call `compute_feature_percentiles()`, then `phrases::top_phrases()` per function, write to `func.explanation` — only when `--explain` requested |
| `hotspots-cli/src/output/explain.rs` | In `print_explain_output`, after each CRITICAL/HIGH function line, emit `"         ✦ {explanation}\n           {severity_suffix}\n"` when `explanation` is `Some` |
| `hotspots-core/src/aggregates.rs` | Add `explanation: Option<String>` to `AgentFunctionView`; populate from `FunctionSnapshot.explanation` in `to_agent_views()` |

---

## Example output

Without `--explain` (unchanged):
```
CRITICAL (1)
  11.88  src/billing.rs::process_upgrade  [god_function, churn_magnet]
```

With `--explain`:
```
CRITICAL (1)
  11.88  src/billing.rs::process_upgrade  [god_function, churn_magnet]
         ✦ High lifetime churn, structurally complex, and depended on by many callers.
           Multiple independent signals agree.

HIGH (1)
   6.36  src/auth.rs::validate_token  [complex_branching]
         ✦ No clear owner and called from many places.
           Worth prioritising before next release.
```

JSON (`--format json`, with or without `--explain`):
```json
{
  "explanation": "High lifetime churn, structurally complex, and depended on by many callers."
}
```
`explanation` is `null` when `--explain` is not active or ranker is absent.

---

## Acceptance criteria

1. `cargo test` passes in both crates.
2. `hotspots analyze` default output (no `--explain`) is byte-identical to current.
3. `hotspots analyze --explain` emits a `✦` line for every CRITICAL and HIGH function
   when `snapshot.functions` is non-empty; no `✦` line for Moderate/Low functions.
4. Phrase output is deterministic — identical snapshot always produces identical phrase string.
5. No network call, no subprocess, no LLM at any point.
6. `hotspots analyze --explain` on a 500-function repo adds < 50ms wall time.
7. `--format json` includes `explanation` field in `AgentFunctionView`; `null` when not active.
8. Works with or without a trained `ranker.json` — percentile computation uses raw feature
   values from the snapshot, not model scores.

---

## Do not

- Do not call any external API or LLM at runtime.
- Do not generate free-form text — phrase table lookups only.
- Do not show raw percentile numbers in terminal output.
- Do not make `--explain` default to `true` in this PR — it remains opt-in.
- Do not touch `render_json` in `report.rs` for non-snapshot default-mode JSON — out of scope.
- Do not add SHAP computation — the brief mentions it as a JSON field but it is deferred
  pending a separate research gate. Omit `shap` from this implementation entirely.

---

## Supporting evidence

- F22: XGBoost beats hotspots formula on 7/9 repos — establishes ranker is worth explaining
- F10: history depth context — establishes that output annotation aids interpretation
- F67: OSV eval — `total_churn`, `fan_in`, `authors_90d` are the top predictors of CVE files
- `hotspots-research/docs/north-star.md` — "Explainability Contract" section
