# TypeScript Types Completion (`@hotspots/types`)

**Motivation:** The `packages/types/src/index.ts` package exports TypeScript types for
hotspots JSON output, but it is significantly incomplete relative to the actual JSON
shapes emitted by the CLI. Large sections of the output — delta mode, file risk views,
module instability, co-change pairs, call graph metrics, risk factors — are either
missing entirely or typed with the wrong shape. This makes the package unreliable for
consumers who depend on the full output.

**Ground truth:** The authoritative Rust types are in `hotspots-core/src/`:
- `report.rs` — `FunctionRiskReport`, `Metrics`
- `aggregates.rs` — `FileRiskView`, `ModuleInstability`, `CoChangePair`, `SnapshotAggregates`
- `delta.rs` — `Delta`, `FunctionDeltaEntry`, `DeltaAggregates`, `FileDeltaAggregates`
- `scoring.rs` — `RiskFactors`, `ChurnMetrics`, `CallGraphMetrics`, `PercentileFlags`
- `snapshot.rs` — `Snapshot`, `SnapshotSummary`, `BandStats`, `CallGraphStats`

---

## TY-1: Fix `Aggregates` Interface Shape

**Problem:** `Aggregates.files` is typed as `FileAggregate[]` (with `sum_lrs`,
`max_lrs`, `high_plus_count`), but the actual JSON emits `FileRiskView[]` (with
`file`, `function_count`, `loc`, `max_cc`, `avg_cc`, `critical_count`,
`file_churn`, `file_risk_score`). These are completely different shapes.

**Tasks:**
- [ ] **TY-1a:** Rename/replace `FileAggregate` with `FileRiskView` matching the Rust
  struct. Fields: `file`, `function_count`, `loc`, `max_cc`, `avg_cc`,
  `avg_lrs`, `critical_count`, `file_churn`, `file_risk_score`, `risk_band`.
- [ ] **TY-1b:** Update `Aggregates.files` to `FileRiskView[]`.
- [ ] **TY-1c:** If `FileAggregate` (old shape) was used anywhere in the package
  exports, deprecate or remove it.

**Effort:** Low. Field list comes directly from `aggregates.rs::FileRiskView`.

---

## TY-2: Add Missing Metric Sub-types

**Problem:** `FunctionReport` has fields (`churn`, `callgraph`, `risk_factors`,
`percentiles`) that are typed as `any` or absent. The actual sub-shapes are:

```typescript
// From scoring.rs ChurnMetrics
interface ChurnMetrics {
  lines_added: number;
  lines_deleted: number;
}

// From scoring.rs CallGraphMetrics
interface CallGraphMetrics {
  fan_in: number;
  fan_out: number;
  scc_size: number;
  dependency_depth: number;
  neighbor_churn: number;
}

// From scoring.rs RiskFactors
interface RiskFactors {
  complexity: number;
  churn: number;
  activity: number;
  recency: number;
  fan_in: number;
  cyclic_dependency: number;
  depth: number;
  neighbor_churn: number;
}

// From scoring.rs PercentileFlags
interface PercentileFlags {
  complexity_p90: boolean;
  churn_p90: boolean;
  fan_in_p90: boolean;
  // (verify exact field names against Rust struct)
}
```

**Tasks:**
- [ ] **TY-2a:** Read `hotspots-core/src/scoring.rs` and capture exact field names and
  types for `ChurnMetrics`, `CallGraphMetrics`, `RiskFactors`, `PercentileFlags`.
- [ ] **TY-2b:** Define these interfaces in `index.ts` and update `FunctionReport` to
  reference them instead of optional loose fields.
- [ ] **TY-2c:** Update `HotspotsOutput.functions` type if needed.

**Effort:** Low. Reading Rust structs is mechanical; TypeScript mapping is direct.

---

## TY-3: Add `SnapshotSummary`, `BandStats`, `CallGraphStats`

**Problem:** `HotspotsOutput` does not expose `summary` (present in snapshot JSON),
which includes aggregate statistics like band counts, call graph stats, and total
activity risk.

**Tasks:**
- [ ] **TY-3a:** Define `BandStats`, `CallGraphStats`, `SnapshotSummary` interfaces
  from `snapshot.rs`.
- [ ] **TY-3b:** Add `summary?: SnapshotSummary` to `HotspotsOutput`.

**Effort:** Low.

---

## TY-4: Add `ModuleInstability` and `CoChangePair` to `Aggregates`

**Problem:** The `Aggregates` interface has `files` and `directories` but is missing:
- `modules: ModuleInstability[]` (D-3)
- `co_change: CoChangePair[]` (D-2)

**Tasks:**
- [ ] **TY-4a:** Define `ModuleInstability` from `aggregates.rs`. Fields: `module`,
  `file_count`, `function_count`, `avg_complexity`, `afferent`, `efferent`,
  `instability`, `module_risk`.
- [ ] **TY-4b:** Define `CoChangePair` from `aggregates.rs`. Fields: `file_a`,
  `file_b`, `co_change_count`, `coupling_ratio`, `risk`.
  *(Note: `has_static_dep` is not yet in the Rust struct — do not add it yet.)*
- [ ] **TY-4c:** Add both to `Aggregates` as optional arrays.

**Effort:** Low.

---

## TY-5: Add Delta Output Types

**Problem:** Delta mode (`hotspots analyze . --mode delta`) produces a completely
different JSON shape, but `@hotspots/types` has zero coverage for it. Consumers
using delta output in CI scripts have no TypeScript types.

**Output shape (from `delta.rs`):**
```typescript
interface FunctionDeltaEntry {
  function_id: string;
  change_type: "added" | "removed" | "modified" | "unchanged";
  before?: FunctionReport;        // present if modified or removed
  after?: FunctionReport;         // present if modified or added
  rename_hint?: string;           // present if rename detected
  delta?: {
    lrs: number;
    cc: number;
    loc: number;
    // (verify field names in delta.rs)
  };
}

interface FileDeltaAggregates {
  // (verify in aggregates.rs)
}

interface DeltaOutput {
  from_commit: string;
  to_commit: string;
  analysis: AnalysisInfo;
  deltas: FunctionDeltaEntry[];
  policy?: PolicyResults;
  aggregates?: FileDeltaAggregates;
}
```

**Tasks:**
- [ ] **TY-5a (research):** Read `hotspots-core/src/delta.rs` and map all serialized
  fields in `Delta`, `FunctionDeltaEntry`, `DeltaMetrics`, and `DeltaAggregates`.
- [ ] **TY-5b:** Define `FunctionDeltaEntry`, `DeltaMetrics`, `FileDeltaAggregates`,
  `DeltaOutput` in `index.ts`.
- [ ] **TY-5c:** Export `DeltaOutput` as a named export from the package.
- [ ] **TY-5d:** Update the package README/jsdoc to mention delta types.

**Effort:** Medium. Delta output is the most complex shape; requires careful reading
of the Rust serialization.

---

## TY-6: Fix `PolicyId` Union and `PolicyResult.metadata`

**Problem:**
- `PolicyResult.metadata` is missing `growth_percent?: number` which is present in
  the Rust `PolicyMetadata` struct.
- `PolicyId` union type may be missing `"net_repo_regression"` (verify exact string
  value in the Rust code — check whether it's `net_repo_regression` or
  `net-repo-regression` in serialized form).

**Tasks:**
- [ ] **TY-6a:** Read `hotspots-core/src/delta.rs` or `policy.rs` for the exact
  `PolicyId` string values emitted in JSON.
- [ ] **TY-6b:** Ensure all `PolicyId` variants are in the union type.
- [ ] **TY-6c:** Add `growth_percent?: number` to `PolicyResult.metadata`.

**Effort:** Very low. One field addition + verification.

---

## TY-7: Generate Golden JSON and Derive Types

**Long-term approach:** Rather than maintaining types by hand, consider generating
them from the JSON schemas in `schemas/`. The schemas are the authoritative source
of truth for the output format and are already present.

**Tasks:**
- [ ] **TY-7a:** Evaluate `json-schema-to-typescript` or `quicktype` for automatic
  type generation from `schemas/hotspots-output.schema.json` and
  `schemas/policy-result.schema.json`.
- [ ] **TY-7b:** If viable, replace hand-written types with generated ones and add
  schema generation to the build step.

**Effort:** Medium. If the schemas are complete and accurate, generation is fast.
If schemas are also incomplete, they need updating first.

---

## Ordering / Dependencies

```
TY-1 (fix Aggregates.files)     — no dependencies; highest-priority bug fix
TY-6 (PolicyId/metadata)        — no dependencies; very small
TY-2 (metric sub-types)         — no dependencies; can run in parallel with TY-1
TY-3 (SnapshotSummary)          — no dependencies
TY-4 (ModuleInstability/CoChange) — no hard dependency; wait for CC-1 for has_static_dep
TY-5 (delta types)              — TY-5a research first; most complex
TY-7 (schema generation)        — evaluate after TY-1 through TY-6 are done
```

TY-1 is the most urgent: the existing `FileAggregate` type is actively wrong and
will produce runtime type errors for any consumer using `aggregates.files`.

---

**Created:** 2026-02-19
