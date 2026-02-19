# Co-Change Analysis Completion

**Status: COMPLETE** (2026-02-19). All three items — `has_static_dep` (CC-1), co-change delta
output (CC-2), and configurable window/threshold (CC-3) — are shipped.

**Motivation:** The D-2 co-change coupling implementation identifies file pairs that frequently
change together. CC-1 added `has_static_dep` to distinguish *expected* coupling (explicit import
relationship) from *hidden* coupling (no static dependency — the true signal). CC-2 surfaces
new/dropped pairs in delta output. CC-3 made the analysis window and `min_count` configurable.

**Principle:** Co-change without static dependency is the high-value signal. Static
dependency detection unlocks accurate `high` vs. `moderate` risk classification and
removes false positives from the coupling list.

---

## CC-1: Static Dependency Detection (`has_static_dep`)

**Summary:** For each co-change pair `(file_a, file_b)`, check whether the import
graph (already built by D-3's `imports.rs`) shows a direct or transitive import
relationship. If one exists, mark `has_static_dep = true` — the co-change is likely
intentional and lower risk. If absent, the coupling is hidden.

**How it works:**
1. The import graph is already built in `aggregates.rs` as part of module instability
   computation (`compute_module_instability`). It maps files to their imported files.
2. For each co-change pair, check if `file_a` imports `file_b` OR `file_b` imports `file_a`
   in the import edge set.
3. Set `has_static_dep` accordingly on each `CoChangePair`.

**Proposed risk reclassification:**
```
risk = "high"     if coupling_ratio > 0.5  AND NOT has_static_dep
risk = "moderate" if coupling_ratio > 0.25 AND NOT has_static_dep
risk = "expected" if has_static_dep (any ratio)
```

**Output fields:**
| Field           | Description                                                    |
|-----------------|----------------------------------------------------------------|
| `has_static_dep`| whether a direct import exists between the two files (bool)    |
| `risk`          | `high` / `moderate` / `expected` (updated classification)     |

**Tasks:**

- [x] **CC-1a:** Add `has_static_dep: bool` field to `CoChangePair` struct in
  `hotspots-core/src/git.rs`.
- [x] **CC-1b:** In `compute_snapshot_aggregates()` (`aggregates.rs`), after building
  the import graph for module instability, pass the file import edges to a new helper
  `annotate_static_deps(pairs, import_edges)` that populates `has_static_dep` on each
  pair.
- [x] **CC-1c:** Update risk classification: add `"expected"` variant for pairs with
  a static dep regardless of ratio. Update `--explain` output to show `[expected]` tag
  for these pairs.
- [x] **CC-1d:** Update JSON schema docs (`docs/reference/json-schema.md`) and
  `DIMENSIONS_TASKS.md` D-2 field table to reflect the shipped field.

**Effort:** Low. Import graph already built for D-3; this is a lookup pass over it.
**Risk:** Low. Additive; existing fields unchanged.

---

## CC-2: Co-Change in Delta Output

**Summary:** When a new co-change pair appears (or disappears) between two snapshots,
surface it in delta output. A new `(A, B)` high-coupling pair in a PR is a meaningful
signal.

**How it works:**
1. Include `co_change` in the existing `FileDeltaAggregates` structure.
2. Diff the co-change pair sets between parent and current snapshot: new pairs, dropped
   pairs, risk changes (e.g., `moderate` → `high`).

**Output fields per delta pair:**
| Field         | Description                                             |
|---------------|---------------------------------------------------------|
| `file_a`      | first file                                              |
| `file_b`      | second file                                             |
| `status`      | `new` / `dropped` / `risk_increased` / `risk_decreased` |
| `prev_risk`   | prior risk level (if existed before)                    |
| `curr_risk`   | current risk level                                      |

**Tasks:**

- [x] **CC-2a:** Add `co_change_delta: Vec<CoChangeDeltaEntry>` to `DeltaAggregates`
  struct in `hotspots-core/src/aggregates.rs`.
- [x] **CC-2b:** Implement `diff_co_change_pairs(prev, curr)` that produces
  `Vec<CoChangeDeltaEntry>`.
- [x] **CC-2c:** Include co-change delta in JSON delta output and in `--policy`
  delta text (co-change coupling section filtered to touched files).

**Effort:** Medium. Requires a snapshot-to-snapshot pair lookup.
**Risk:** Low. Additive; existing delta output unchanged.

---

## CC-3: Configurable Co-Change Window and Threshold

**Summary:** The co-change extraction used hardcoded defaults (90-day window, `min_count = 3`).
Projects with different commit cadences need to tune these — now configurable via `.hotspotsrc.json`.

**Tasks:**

- [x] **CC-3a:** Add `co_change_window_days` and `co_change_min_count` to the config
  file schema (`hotspots-core/src/config.rs`).
- [x] **CC-3b:** Thread the config values through to `extract_co_change_pairs()` call
  in `aggregates.rs`.
- [x] **CC-3c:** Document new config fields in `docs/guide/configuration.md`.

**Effort:** Low. Config plumbing only; no algorithmic changes.
**Risk:** Low. Defaults unchanged; existing behavior is preserved.

---

## Completion Order (as shipped)

```
CC-1 (has_static_dep)     — shipped; uses import graph built by imports.rs (D-3)
CC-2 (co-change delta)    — shipped; captures new/dropped pairs and risk changes
CC-3 (configurable)       — shipped; co_change_window_days and co_change_min_count in config
```

---

**Created:** 2026-02-19
