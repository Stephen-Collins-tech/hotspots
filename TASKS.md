# TASKS.md – Faultline High-Leverage Extensions (Detailed)

## Status

```text
status: completed
scope: policy enforcement + trend semantics + aggregation views + visualization
non-goals: new metrics, mutable history, config DSLs, heuristics
depends_on:
  - snapshot mode (stable)
  - delta mode (stable)
  - deterministic output invariants
```

---

## Global Constraints (Must Hold)

* [x] No new complexity metrics introduced
* [x] Snapshot format remains backward compatible
* [x] No snapshot mutation or rewriting
* [x] No non-deterministic ordering or floating behavior
* [x] No user-configurable policy files
* [x] No impact on analyze behavior when new flags are omitted

---

## Phase 1 – Minimal Policy Engine (CI Enforcement)

### Objective

Enable Faultline to **block or warn in CI** using a small, fixed set of built-in policies applied to delta output.

---

### 1.1 Policy Definitions

* [x] Define `PolicyId` enum

  * [x] `CriticalIntroduction`
  * [x] `ExcessiveRiskRegression`
  * [x] `NetRepoRegression`

* [x] Define `PolicyResult` struct

  * [x] `id: PolicyId`
  * [x] `severity: Blocking | Warning`
  * [x] `function_id: Option<String>`
  * [x] `message: String`
  * [x] `metadata: Option<PolicyMetadata>`

* [x] Define `PolicyMetadata` struct

  * [x] `delta_lrs: Option<f64>` (for Excessive Risk Regression)
  * [x] `total_delta: Option<f64>` (for Net Repo Regression)
  * [x] **Note**: `function_id` is stored in `PolicyResult` (top-level), not metadata. Metadata is for numeric values only.

---

### 1.2 Policy Evaluation Logic

* [x] Implement Critical Introduction policy

  * [x] Trigger when `after.band == Critical AND (before.band != Critical OR before is None)`
  * [x] `before is None` means no matching `function_id` in the parent snapshot (delta status `new`)
  * [x] Covers: Modified functions crossing into Critical, New functions introduced as Critical, Re-added functions as Critical
  * [x] Emit blocking failure
  * [x] Attach `function_id` in `PolicyResult` (top-level field, not metadata)

* [x] Implement Excessive Risk Regression policy

  * [x] Trigger when `status == Modified && delta.lrs >= 1.0`
  * [x] Threshold is fixed at 1.0 LRS (absolute, not relative)
  * [x] Emit blocking failure
  * [x] Attach `function_id` and `delta_lrs` in metadata

* [x] Implement Net Repo Regression policy

  * [x] Compute `Σ(all after.lrs) - Σ(all before.lrs)` by loading full snapshots (not reconstructing from delta entries)
  * [x] Load parent snapshot and current snapshot using existing snapshot loading logic
  * [x] Sum `lrs` across all `functions[]` in each snapshot
  * [x] Trigger when result > 0
  * [x] Emit warning only (non-blocking)
  * [x] Attach `total_delta` in metadata
  * [x] **Rationale**: Repo-level policies operate on snapshots, not delta internals. This ensures correctness, avoids edge cases, and remains stable if delta semantics evolve.

---

### 1.3 Policy Engine Integration

* [x] Add policy evaluation stage after delta computation
* [x] Evaluation order: Function-level policies first (Critical Introduction, Excessive Risk Regression), then repo-level (Net Repo Regression)
* [x] Collect all violations before exit (do not short-circuit)
* [x] Skip policy evaluation entirely if `baseline == true` (no violations, exit successfully)
* [x] A function may trigger multiple policies - report all violations

---

### 1.4 CLI Integration

* [x] Add `--policy` flag to `faultline analyze`

* [x] Flag only valid in `--mode delta`

* [x] When omitted:

  * [x] No policy evaluation
  * [x] No behavior change

* [x] When present:

  * [x] Blocking failures cause exit code 1
  * [x] Warnings printed but do not fail (exit code 0 if warnings only)
  * [x] Exit code 0 if no violations

---

### 1.5 Output Extensions

#### JSON

* [x] Extend delta JSON schema with optional `policy` field (only present when `--policy` flag used)

  * [x] `failed: []` (blocking violations)
  * [x] `warnings: []` (non-blocking violations)
* [x] Preserve deterministic ordering: Primary sort by `id` (enum discriminant order), secondary sort by `function_id` ASCII (None last)
* [x] Keep `schema_version = 1` (backward compatible - policy field is optional)

#### Text

* [x] Print policy summary header
* [x] Print blocking failures first (list function_ids and policy types)
* [x] Print warnings second (summary only, not in table)
* [x] Follow with filtered function table showing **only functions that triggered blocking failures**
* [x] Table columns: Function, Before, After, ΔLRS, Policy

---

### Phase 1 Completion Criteria

* [x] CI can fail on Critical introductions
* [x] CI can fail on excessive LRS regressions
* [x] Net repo regression emits warning
* [x] No behavior change without `--policy`

---

## Phase 2 – Trend Semantics (Meaning from History)

### Objective

Extract high-signal trends from existing snapshots without new metrics or prediction.

---

### 2.1 History Windowing

* [x] Define sliding window abstraction
* [x] Default window size = 10 snapshots (configurable via `--window`)
* [x] If fewer than N snapshots exist, use all available
* [x] Ignore gaps where function does not exist (skip commits where function missing)
* [x] Maintain deterministic snapshot ordering

---

### 2.2 Risk Velocity

* [x] Compute per-function `ΔLRS / Δcommits` using simple formula: `(LRS_last - LRS_first) / (commit_count - 1)`
* [x] Require at least 2 data points (skip if insufficient)
* [x] Skip commits where function does not exist
* [x] Track:

  * [x] Latest velocity (numeric value)
  * [x] Direction (positive, negative, flat)
* [x] Flat direction defined as `abs(velocity) < 1e-9` (deterministic epsilon tolerance)
* [x] Exclude baseline-only functions (functions that only appear in first snapshot)

---

### 2.3 Hotspot Stability

* [x] Identify top K functions per snapshot by LRS
* [x] Compute overlap ratio across window
* [x] Classify:

  * [x] Stable hotspots
  * [x] Emerging hotspots
  * [x] Volatile hotspots

---

### 2.4 Refactor Effectiveness Detection

* [x] Detect significant negative LRS deltas (threshold: `delta.lrs <= -1.0`)
* [x] Require sustainment across ≥ 2 commits
* [x] Detect rebound within window (threshold: `delta.lrs >= +0.5` after improvement)
* [x] Classify refactor outcome:

  * [x] Successful (improvement sustained, no rebound)
  * [x] Partial (improvement sustained, but rebound detected)
  * [x] Cosmetic (improvement not sustained across ≥ 2 commits)

---

### 2.5 CLI Command

* [x] Add `faultline trends` command
* [x] Flags:

  * [x] `--top K`
  * [x] `--window N`
* [x] JSON-first output
* [x] Text output as summary only

---

### Phase 2 Completion Criteria

* [x] Risk velocity surfaced per function
* [x] Stable vs volatile hotspots identifiable
* [x] Refactor effectiveness classified
* [x] No snapshot format changes

---

## Phase 3 – Aggregation Views (Derived Only)

### Objective

Expose architectural concentration without contaminating core metrics.

---

### 3.1 Snapshot Aggregates

* [x] Aggregate per file:

  * [x] Sum LRS (sum of all functions in file)
  * [x] Max LRS (highest LRS function in file)
  * [x] Count High+ functions (band == "high" OR band == "critical")

* [x] Aggregate per directory:

  * [x] Recursive rollup
  * [x] Same metrics as files

---

### 3.2 Delta Aggregates

* [x] Compute per-file net LRS delta
* [x] Count regressions per file (regression = function with `delta.lrs > 0` in that file)
* [x] Preserve deterministic ordering

---

### 3.3 Output Structure

* [x] Add `aggregates` namespace to snapshot JSON and delta JSON only
* [x] Trends command has its own schema (does not include aggregates)
* [x] Ensure aggregates are strictly derived
* [x] Do not modify existing function data

---

### Phase 3 Completion Criteria

* [x] Repo risk concentration visible
* [x] File-level regressions identifiable
* [x] No change to LRS or bands

---

## Phase 4 – Persuasive Visualization

### Objective

Add one visualization that communicates risk evolution instantly.

---

### 4.1 Data Preparation

* [x] Extend `update-report` to compute:

  * [x] Top K functions by LRS per snapshot
  * [x] Remaining functions as “Other”
* [x] Preserve stable function ordering
* [x] Ensure backward compatibility with existing charts

---

### 4.2 Chart Implementation

* [x] Stacked area chart
* [x] X-axis: commit order (deterministic ordering: sort by `commit.timestamp` ascending, tie-break by `commit.sha` ASCII)
* [x] Y-axis: cumulative LRS (stacked sum of Top K + Other)
* [x] Series:

  * [x] Each of Top K functions as separate stack layer
  * [x] "Other" bucket as final stack layer
* [x] Shows total risk concentration and how it evolves

---

### 4.3 Integration

* [x] Add chart to `index.html`
* [x] Document interpretation in README or docs
* [x] Do not add additional charts in this phase

---

### Phase 4 Completion Criteria

* [x] Risk concentration over time visible
* [x] Refactors clearly reflected
* [x] Chart renders from existing snapshot data only

---

## Final Success Criteria

* [x] Faultline can block CI on meaningful regressions
* [x] Trend direction is visible and actionable
* [x] Architectural risk concentration is obvious
* [x] Visualization tells a story in under 10 seconds
* [x] All determinism and git-native guarantees preserved

---

## Explicitly Deferred

* [ ] User-configurable policies
* [ ] Language plugins
* [ ] SARIF export
* [ ] IDE integration
* [ ] New metrics or scoring logic

---

**This task set converts Faultline from a passive observer into a system that actively shapes engineering behavior while preserving its core rigor.**
