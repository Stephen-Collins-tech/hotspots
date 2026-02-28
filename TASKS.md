# Pattern Detection — Implementation Tasks

Branch: `feature/pattern-detection`

This file tracks the implementation of pattern detection as specified in `docs/patterns.md`. Work through tasks in order — each task depends on the ones before it.

---

## Context

Patterns are informational labels derived from existing metrics. They do not affect LRS or risk bands. The authoritative spec is `docs/patterns.md`.

**Files that will be modified:**

| File | Change |
|---|---|
| `hotspots-core/src/patterns.rs` | NEW — pattern engine |
| `hotspots-core/src/lib.rs` | expose `pub mod patterns` |
| `hotspots-core/src/callgraph.rs` | make `is_entry_point` pub |
| `hotspots-core/src/config.rs` | add `PatternThresholdsConfig` to `HotspotsConfig`; add `pattern_thresholds` to `ResolvedConfig` |
| `hotspots-core/src/report.rs` | add `patterns` and `pattern_details` to `FunctionRiskReport` |
| `hotspots-core/src/analysis.rs` | compute Tier 1 patterns, pass to report |
| `hotspots-core/src/snapshot.rs` | add `neighbor_churn` to `CallGraphMetrics`; add `patterns` and `pattern_details` to `FunctionSnapshot`; compute neighbor_churn and Tier 2 patterns in enrichment |
| `hotspots-cli/src/main.rs` | add `PATTERNS` column to tabular output; add `--explain-patterns` flag |
| `hotspots-core/src/html.rs` | add `Patterns` column to HTML report |
| `hotspots-core/tests/golden_tests.rs` | add golden assertions for pattern output |
| `hotspots-core/tests/fixtures/` | add synthetic fixture for pattern golden tests |
| `hotspots-core/tests/golden/` | add expected output file for pattern golden tests |

---

## Task 1 — Create `patterns.rs`: pure pattern engine

**File:** `hotspots-core/src/patterns.rs` (new file)

Implement the entire pattern classification engine as a pure, stateless module. No I/O, no global state.

### String types, not `&'static str`

All string fields in `PatternDetail` and `TriggeredBy` must be `String`, not `&'static str`. `&'static str` cannot be used in a `#[derive(Serialize)]` struct without a custom impl. The allocation cost is negligible — patterns are computed once per function per analysis run.

### Types to define

```rust
// Input for Tier 1 classification
pub struct Tier1Input {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
    pub ns: usize,
    pub loc: usize,
}

// Input for Tier 2 classification (all Option — absent outside snapshot mode)
pub struct Tier2Input {
    pub fan_in: Option<u32>,
    pub scc_size: Option<u32>,
    pub churn_lines: Option<u64>,
    pub days_since_last_change: Option<u32>,
    pub neighbor_churn: Option<u64>,
    pub is_entrypoint: bool,  // suppresses middle_man and neighbor_risk when true
}

// All thresholds in one struct — defaults match docs/patterns.md.
// Passed by reference into classify(); callers use Thresholds::default() unless
// overridden by config. This keeps all threshold logic in one place and makes
// per-project overrides (Task 9) a thin config-loading layer.
#[derive(Debug, Clone)]
pub struct Thresholds {
    pub complex_branching_cc: usize,
    pub complex_branching_nd: usize,
    pub deeply_nested_nd: usize,
    pub exit_heavy_ns: usize,
    pub god_function_loc: usize,
    pub god_function_fo: usize,
    pub long_function_loc: usize,
    pub churn_magnet_churn: u64,
    pub churn_magnet_cc: usize,
    pub cyclic_hub_scc: u32,
    pub cyclic_hub_fan_in: u32,
    pub hub_function_fan_in: u32,
    pub hub_function_cc: usize,
    pub middle_man_fan_in: u32,
    pub middle_man_fo: usize,
    pub middle_man_cc_max: usize,
    pub neighbor_risk_churn: u64,
    pub neighbor_risk_fo: usize,
    pub shotgun_target_fan_in: u32,
    pub shotgun_target_churn: u64,
    pub stale_complex_cc: usize,
    pub stale_complex_loc: usize,
    pub stale_complex_days: u32,
}

impl Default for Thresholds { ... }  // all values from docs/patterns.md

// A single triggered condition — used by --explain-patterns.
// All fields are String (not &'static str) for serde compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggeredBy {
    pub metric: String,     // e.g. "LOC", "CC"
    pub op: String,         // ">=" or "<="
    pub value: u64,         // observed value
    pub threshold: u64,     // threshold compared against
}

// Full detail for one fired pattern — used by --explain-patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDetail {
    pub id: String,
    pub tier: u8,               // 1 or 2
    pub kind: String,           // "primitive" or "derived"
    pub triggered_by: Vec<TriggeredBy>,
}
```

### Functions to implement

```rust
// Returns sorted pattern IDs only (Tier 1 alphabetical, then Tier 2 alphabetical).
// Implemented by calling classify_detailed() and extracting ids — single source of
// threshold logic, no divergence possible.
pub fn classify(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Vec<String>

// Returns full pattern detail for --explain-patterns.
// This is the canonical implementation — classify() delegates to this.
pub fn classify_detailed(
    t1: &Tier1Input,
    t2: &Tier2Input,
    th: &Thresholds,
) -> Vec<PatternDetail>

// Internal helpers — one per primitive pattern.
// Return Some(PatternDetail) if fired, None if not.
// Each helper constructs triggered_by from the metrics that crossed threshold.
fn check_complex_branching(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail>
fn check_deeply_nested(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail>
fn check_exit_heavy(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail>
fn check_god_function(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail>
fn check_long_function(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail>
fn check_churn_magnet(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_cyclic_hub(t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_hub_function(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_middle_man(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_neighbor_risk(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_shotgun_target(t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
fn check_stale_complex(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail>
```

**Entrypoint suppression:** `check_middle_man` and `check_neighbor_risk` return `None` immediately when `t2.is_entrypoint` is `true`. This is the complete implementation of entrypoint exclusion.

**`volatile_god` (derived):** computed inside `classify_detailed()` after all primitives are checked. If both `god_function` and `churn_magnet` details are present in the result, append a `volatile_god` entry whose `triggered_by` is the union of both primitives' `triggered_by` lists. Do not re-evaluate raw thresholds. `kind: "derived"`.

**Ordering:** Tier 1 results first (alphabetical), then Tier 2 (alphabetical). `volatile_god` sorts among Tier 2.

### Unit tests (required in the same file, under `#[cfg(test)]`)

For **every primitive pattern**, write four tests using `Thresholds::default()`:
1. Just below threshold — must not fire
2. Exactly at threshold — must fire
3. Well above threshold — must fire
4. Compound pattern: one condition met, other not — must not fire

For **`volatile_god`** (derived):
1. Only `god_function` conditions met — `volatile_god` must not appear
2. Only `churn_magnet` conditions met — `volatile_god` must not appear
3. Both met — must contain all three: `god_function`, `churn_magnet`, `volatile_god`

For **entrypoint suppression**:
1. `middle_man` conditions met, `is_entrypoint: false` — must fire
2. Same conditions, `is_entrypoint: true` — must not fire
3. Same pair of tests for `neighbor_risk`

**Ordering test:** inputs that trigger all 5 Tier 1 patterns. Output must be exactly:
`["complex_branching", "deeply_nested", "exit_heavy", "god_function", "long_function"]`

**Detail test:** trigger `god_function`. Call `classify_detailed()`. Assert the returned `PatternDetail` has `tier: 1`, `kind: "primitive"`, and `triggered_by` contains entries for `LOC` and `FO` with correct `value` and `threshold`.

---

## Task 2 — `is_entry_point` pub + `neighbor_churn` on `CallGraphMetrics`

### 2a — Make `is_entry_point` pub in `callgraph.rs`

**File:** `hotspots-core/src/callgraph.rs`

`is_entry_point` is currently `fn is_entry_point` (private). Change to `pub fn is_entry_point`. No other changes to this file.

This method uses name-based heuristics (main, handler patterns, etc.) and is cheap to call. It does not need to be stored per-function — snapshot enrichment (Task 6) calls it directly.

### 2b — Add `neighbor_churn` to `CallGraphMetrics`

**File:** `hotspots-core/src/snapshot.rs`

`CallGraphMetrics` is the struct stored in `FunctionSnapshot.callgraph`. Add:

```rust
pub neighbor_churn: Option<u64>,
```

Initialize to `None` in all constructors. The value is computed during enrichment (Task 6) once churn data is available for all functions.

**Grep first:** search for all `CallGraphMetrics { ` construction sites in `snapshot.rs` and add `neighbor_churn: None`.

---

## Task 3 — Add `patterns` and `pattern_details` to `FunctionRiskReport`

**File:** `hotspots-core/src/report.rs`

Add to `FunctionRiskReport`:

```rust
pub patterns: Vec<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub pattern_details: Option<Vec<patterns::PatternDetail>>,
```

Initialize `patterns` to `vec![]` and `pattern_details` to `None` in the constructor. Task 5 wires in real values.

`pattern_details` is absent from JSON output by default. It is only populated when `--explain-patterns` is passed (Task 10).

**Grep first:** search for all `FunctionRiskReport` construction sites and add the new fields.

---

## Task 4 — Add `patterns` and `pattern_details` to `FunctionSnapshot`

**File:** `hotspots-core/src/snapshot.rs`

Add to `FunctionSnapshot`:

```rust
pub patterns: Vec<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub pattern_details: Option<Vec<patterns::PatternDetail>>,
```

Initialize both to empty. Task 6 wires in real values.

**Grep first:** search for all `FunctionSnapshot` construction sites and add the new fields.

---

## Task 5 — Wire Tier 1 patterns in analysis mode

**File:** `hotspots-core/src/analysis.rs`

After `extract_metrics()` returns `RawMetrics`, compute patterns. Use `config.pattern_thresholds` (added in Task 9; use `Thresholds::default()` until then):

```rust
let t1 = patterns::Tier1Input {
    cc: raw.cc, nd: raw.nd, fo: raw.fo, ns: raw.ns, loc: raw.loc,
};
let t2 = patterns::Tier2Input {
    fan_in: None, scc_size: None, churn_lines: None,
    days_since_last_change: None, neighbor_churn: None,
    is_entrypoint: false,
};
let function_patterns = patterns::classify(&t1, &t2, &config.pattern_thresholds);
```

Pass `function_patterns` to `FunctionRiskReport`. When `--explain-patterns` is active (Task 10), call `classify_detailed()` instead and populate `pattern_details`.

---

## Task 6 — Compute `neighbor_churn` and Tier 2 patterns in snapshot enrichment

**File:** `hotspots-core/src/snapshot.rs`

Runs after call graph and git churn data are both available for all functions.

### 6a — Compute `neighbor_churn` per function

For each function snapshot, sum `churn_lines` across all direct callees (1-hop outgoing edges). Look up each callee's churn total from the already-built churn map; contribute 0 for callees with no entry. Store in `snap.callgraph.neighbor_churn`.

### 6b — Compute Tier 2 patterns

```rust
let t1 = patterns::Tier1Input {
    cc: snap.metrics.cc, nd: snap.metrics.nd,
    fo: snap.metrics.fo, ns: snap.metrics.ns, loc: snap.metrics.loc,
};
let t2 = patterns::Tier2Input {
    fan_in: snap.callgraph.as_ref().map(|cg| cg.fan_in as u32),
    scc_size: snap.callgraph.as_ref().map(|cg| cg.scc_size as u32),
    churn_lines: snap.churn.as_ref().map(|c| c.lines_added + c.lines_deleted),
    days_since_last_change: snap.days_since_last_change,
    neighbor_churn: snap.callgraph.as_ref().and_then(|cg| cg.neighbor_churn),
    is_entrypoint: call_graph.is_entry_point(&snap.function_id),
};
snap.patterns = patterns::classify(&t1, &t2, &config.pattern_thresholds);
// if --explain-patterns (Task 10):
// snap.pattern_details = Some(patterns::classify_detailed(&t1, &t2, &config.pattern_thresholds));
```

---

## Task 7 — Tabular output for `patterns`

**File:** `hotspots-cli/src/main.rs`

Add a `PATTERNS` column to tabular output:
- Value: comma-separated pattern IDs, or `-` if empty
- Only render the column when at least one function in the result has patterns
- Column appears after `BAND`

JSON output requires no changes — `patterns: Vec<String>` serialises as a JSON array automatically.

---

## Task 8 — HTML report patterns column

**File:** `hotspots-core/src/html.rs`

Add a `Patterns` column to the HTML function table:
- Each pattern ID rendered as `<span class="pattern pattern-{id}">` — allows per-pattern CSS
- Show `-` for empty lists
- Add minimal CSS: `.pattern { font-family: monospace; font-size: 0.9em; margin-right: 4px; }`

No new data pipeline work — `FunctionSnapshot.patterns` is populated by Task 6.

---

## Task 9 — Per-project threshold overrides via `.hotspotsrc.json`

**Files:** `hotspots-core/src/config.rs`

Follow the exact same pattern as `ScoringWeightsConfig` → `ScoringWeights` → `ResolvedConfig.scoring_weights`.

### 9a — Add `PatternThresholdsConfig` to `HotspotsConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternThresholdsConfig {
    pub complex_branching_cc: Option<usize>,
    pub complex_branching_nd: Option<usize>,
    pub deeply_nested_nd: Option<usize>,
    pub exit_heavy_ns: Option<usize>,
    pub god_function_loc: Option<usize>,
    pub god_function_fo: Option<usize>,
    pub long_function_loc: Option<usize>,
    pub churn_magnet_churn: Option<u64>,
    pub churn_magnet_cc: Option<usize>,
    pub cyclic_hub_scc: Option<u32>,
    pub cyclic_hub_fan_in: Option<u32>,
    pub hub_function_fan_in: Option<u32>,
    pub hub_function_cc: Option<usize>,
    pub middle_man_fan_in: Option<u32>,
    pub middle_man_fo: Option<usize>,
    pub middle_man_cc_max: Option<usize>,
    pub neighbor_risk_churn: Option<u64>,
    pub neighbor_risk_fo: Option<usize>,
    pub shotgun_target_fan_in: Option<u32>,
    pub shotgun_target_churn: Option<u64>,
    pub stale_complex_cc: Option<usize>,
    pub stale_complex_loc: Option<usize>,
    pub stale_complex_days: Option<u32>,
}
```

Add to `HotspotsConfig` (note: `deny_unknown_fields` is on `HotspotsConfig` — the new field must be added here or config parsing will reject the `patterns` key):

```rust
#[serde(default)]
pub patterns: Option<PatternThresholdsConfig>,
```

### 9b — Add `pattern_thresholds` to `ResolvedConfig`

```rust
pub pattern_thresholds: patterns::Thresholds,
```

### 9c — Merge in `resolve()`

Follow the `scoring_weights` pattern: start from `patterns::Thresholds::default()`, override field by field for any `Some` value in `PatternThresholdsConfig`. Each field falls back to the default independently — partial configs work.

### 9d — Validation

Add a `validate_pattern_thresholds()` helper called from `HotspotsConfig::validate()`. Rules:
- All usize/u32/u64 thresholds must be > 0
- `middle_man_cc_max` must be < `hub_function_cc` (prevents middle_man and hub_function from being simultaneously impossible to distinguish — warn, not error)

### 9e — Wire through call sites

Update Tasks 5 and 6's `classify()` calls to use `&config.pattern_thresholds` instead of `&Thresholds::default()`. This is a one-line change at each call site once `ResolvedConfig` has the field.

---

## Task 10 — `--explain-patterns` flag

**File:** `hotspots-cli/src/main.rs`

Add `--explain-patterns` boolean flag to the `analyze` and `snapshot` subcommands.

When set:
- Call `patterns::classify_detailed()` instead of `patterns::classify()` in analysis (Task 5) and snapshot enrichment (Task 6)
- Store result in `pattern_details` on the report/snapshot struct
- JSON output: `pattern_details` serialises automatically (absent by default via `skip_serializing_if`)
- Tabular output: after the main row for a function, print one indented line per pattern:
  ```
    god_function: LOC=85 (≥60), FO=12 (≥10)
  ```
- HTML output: expand each `<span class="pattern">` into a `<span title="LOC=85 (≥60), FO=12 (≥10)">` tooltip

`pattern_details` must remain `None` when `--explain-patterns` is not passed. Do not call `classify_detailed()` in the hot path.

---

## Task 11 — Golden tests for Tier 1 patterns

**Files:**
- `hotspots-core/tests/fixtures/patterns_tier1.ts` (new)
- `hotspots-core/tests/golden/patterns_tier1.json` (new)
- `hotspots-core/tests/golden_tests.rs` (add test case)

### Fixture requirements

Write a TypeScript fixture with five named functions, each engineered to trigger specific Tier 1 patterns:

1. `godAndLong` — triggers `god_function` and `long_function` (LOC ≥ 80, FO ≥ 10)
2. `complexBranching` — triggers `complex_branching` (CC ≥ 10, ND ≥ 4) but not `deeply_nested`
3. `deeplyNested` — triggers `deeply_nested` alone (ND ≥ 5, CC < 10)
4. `exitHeavy` — triggers `exit_heavy` (NS ≥ 5)
5. `allFiveTier1` — triggers all five Tier 1 patterns simultaneously

After running analysis on the fixture, capture output and commit it as the golden file. The golden file locks the `patterns` array per function.

### Test assertion

```rust
// In golden_tests.rs:
// Run analysis, load golden JSON, assert patterns match for each function by name.
```

---

## Task 12 — Full test suite pass

```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

**Anticipated failure modes:**

- **`deny_unknown_fields`** on `HotspotsConfig` — if Task 9 is not complete, any test that serialises/deserialises a config with the `patterns` key will fail. Complete Task 9 before running tests.
- **Struct exhaustiveness** — any `..Default::default()` or struct literal missing new fields will fail to compile. Grep for all construction sites before compiling.
- **Golden file drift** — `patterns: []` will appear in JSON output for all existing golden files. Update them: an empty `patterns` array is valid and expected for existing fixtures that don't trigger any patterns.
- **Clippy `needless_pass_by_value`** — `classify()` takes `&Thresholds` by reference; ensure no call site passes by value.

---

## Out of scope for this branch

- **Percentile-based thresholds** — requires a two-pass analysis (collect all metric values repo-wide, then classify per-function). The current per-function stateless engine cannot support this without an architectural change to the analysis pipeline. Documented as a planned extension in `docs/patterns.md`.

---

## Definition of done

- [ ] All 13 patterns implemented in `patterns.rs` with full unit test coverage
- [ ] `Thresholds` struct with `Default` impl; all `classify()` calls accept `&Thresholds`
- [ ] Entrypoint suppression for `middle_man` and `neighbor_risk`
- [ ] `FunctionRiskReport.patterns` populated in analyze mode (Tier 1)
- [ ] `FunctionSnapshot.patterns` populated in snapshot mode (Tier 1 + Tier 2)
- [ ] `neighbor_churn` computed and stored in snapshot enrichment
- [ ] Tabular output shows `PATTERNS` column when non-empty
- [ ] HTML output shows `Patterns` column
- [ ] JSON output includes `patterns` array; `pattern_details` absent unless `--explain-patterns`
- [ ] `--explain-patterns` populates `pattern_details` with triggered conditions
- [ ] `.hotspotsrc.json` `patterns` section overrides thresholds per-field; partial configs work
- [ ] Golden test fixture and expected output committed
- [ ] `cargo fmt`, `cargo clippy`, `cargo test` all pass clean
