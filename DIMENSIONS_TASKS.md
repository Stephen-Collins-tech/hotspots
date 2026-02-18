# Analysis Dimensions Roadmap

**Motivation:** hotspots is a function-level microscope today. Codebases have multiple levels of
hierarchy, each with distinct risk signals. This document tracks work to add higher-level
dimensions of analysis above the function level.

**Principle:** Each dimension should produce a ranked list and a risk score at its level —
parallel to what function-level analysis produces today.

---

## Hierarchy Overview

| Level        | Currently captured                         | Missing / Target                                 |
|--------------|--------------------------------------------|--------------------------------------------------|
| Function     | cc, nd, fo, churn, touch, fan-in/out, scc  | — mostly covered                                 |
| File         | churn (lines), touch count (via functions) | LOC, function density, avg/max cc, file score    |
| Module / Dir | nothing                                    | coupling, afferent/efferent, instability         |
| Codebase     | trends (velocity, refactor detection)      | co-change coupling, debt index, lang distribution|

---

## D-1: File-Level Risk View

**Summary:** Aggregate per-function data up to the file level and produce a ranked file list
alongside the function list. A file with 40 functions averaging cc=12 is a maintenance liability
even if no single function is individually the worst.

**Proposed file risk score:**

```
file_risk = max_cc × 0.4
           + avg_cc × 0.3
           + log2(function_count + 1) × 0.2
           + file_churn_factor × 0.1
```

**Output fields per file:**

| Field              | Description                                      |
|--------------------|--------------------------------------------------|
| `file`             | relative path                                    |
| `function_count`   | number of functions in file                      |
| `loc`              | total lines                                      |
| `max_cc`           | highest cyclomatic complexity in file            |
| `avg_cc`           | mean cyclomatic complexity across functions      |
| `critical_count`   | number of functions in critical band             |
| `file_churn`       | lines changed in last 30 days                    |
| `file_risk_score`  | composite score (see formula above)              |

**CLI surface:**

```
hotspots analyze . --mode snapshot --format json   # adds "files" array to output
hotspots analyze . --mode snapshot --format text --level file  # new: ranked file list
```

**Tasks:**

- [x] **D-1a:** Add `FileRiskView` struct to `hotspots-core/src/aggregates.rs` with the fields above.
- [x] **D-1b:** Add `compute_file_risk_views()` to `hotspots-core/src/aggregates.rs` — fold
  per-function data into `FileRiskView` entries, one per unique file path.
- [x] **D-1c:** Include `file_risk` array in snapshot JSON output (via `SnapshotAggregates`).
  Key is `aggregates.file_risk` in JSON output.
- [x] **D-1d:** Add `--level file` text output mode (`print_file_risk_output`) that prints
  a ranked file table. Usage: `hotspots analyze . --mode snapshot --format text --level file`.
  Supports `--top N` for limiting output. Mutually exclusive with `--explain`.
- [x] **D-1e:** Add file-level aggregates to delta output — which files got worse/better.
  Added `improvement_count` to `FileDeltaAggregates`; sort order changed to descending
  `net_lrs_delta` (worst regression first).

**Effort:** Low-Medium. Builds entirely on existing per-function data — no new git calls needed.
**Risk:** Low. Additive change; existing function output is unchanged.

---

## D-2: Co-Change Coupling

**Summary:** Mine git log to find files that frequently change *together* in the same commit.
High co-change with no static dependency = hidden implicit coupling — a classic maintenance
risk signal. This is the original "hotspot" concept from Adam Thornhill's work on code forensics.

**How it works:**
1. Walk git log for the last N commits (configurable window, default 90 days)
2. For each commit, collect the set of files changed
3. Count pairwise co-occurrence: how often did file A and file B change in the same commit?
4. Normalize by the file that changed less often (support / min(count_A, count_B))
5. Pairs above a threshold are "coupled" — flag if they have no static import dependency

**Output fields per pair:**

| Field                | Description                                              |
|----------------------|----------------------------------------------------------|
| `file_a`, `file_b`   | the two files                                            |
| `co_change_count`    | times they changed in the same commit                    |
| `coupling_ratio`     | co_change_count / min(total_changes_a, total_changes_b)  |
| `has_static_dep`     | whether a static import exists between them (best-effort)|
| `risk`               | `high` if ratio > 0.5 and no static dep; else `moderate` |

**CLI surface:**

```
hotspots analyze . --mode snapshot --format json   # adds "co_change" array to snapshot
hotspots coupling .                                 # new subcommand (optional)
```

**Tasks:**

- [x] **D-2a (research):** Implemented directly in Rust and validated on this repo.
  Findings: signal is good for source files with `min_count >= 3`; noise sources are
  (1) ghost files from renames (filtered: only emit pairs where both files currently exist),
  (2) config/workflow files in large setup commits (non-issue at min_count=3),
  (3) trivially expected test+source pairs (filtered by `is_trivial_pair`).
  Top Rust pairs are legitimate: `hotspots-cli/src/main.rs` ↔ `hotspots-core/src/aggregates.rs`,
  `cfg/builder.rs` ↔ language-specific builders. Threshold calibrated: high > 0.5, moderate > 0.25.
- [x] **D-2b:** Add `git::extract_co_change_pairs(repo, window_days, min_count)` to `git.rs`.
  Returns `Vec<CoChangePair>`. Default: 90-day window, min_count=3.
- [x] **D-2c:** Add `CoChangePair` struct and integrate into snapshot output.
  Key is `aggregates.co_change` in JSON output.
- [ ] **D-2d:** Add co-change section to `--explain` text output.
- [x] **D-2e:** Filter out trivially expected pairs (e.g., `foo.rs` + `foo_test.rs`,
  `mod.rs` + any sibling) to reduce noise. Also filters ghost files (renamed/deleted).

**Effort:** Medium. Requires new git log analysis but no AST/parsing work.
**Risk:** Low. Additive; no existing analysis is modified.

---

## D-3: Module / Directory Instability

**Summary:** Apply Robert Martin's instability metric at the directory level.
`instability = efferent_coupling / (efferent + afferent)`. A module with instability near 1.0
depends on many others but nothing depends on it — safe to change. Near 0.0 = everything
depends on it, very risky to change. The interesting hotspots are high-complexity modules with
low instability (hard to change AND everything depends on them).

**How it works:**
1. Use the existing call graph to extract inter-file (or inter-directory) import edges
2. For each directory: count efferent edges (calls out) and afferent edges (calls in)
3. Compute instability; combine with avg complexity of functions in that directory

**Output fields per module:**

| Field                 | Description                                         |
|-----------------------|-----------------------------------------------------|
| `module`              | directory path (e.g. `src/language/python`)         |
| `file_count`          | number of files                                     |
| `function_count`      | number of functions                                 |
| `avg_complexity`      | mean cc of all functions                            |
| `afferent`            | external callers depending on this module           |
| `efferent`            | external modules this one depends on                |
| `instability`         | efferent / (afferent + efferent)                    |
| `module_risk`         | high if instability < 0.3 and avg_complexity > 10  |

**Tasks:**

- [ ] **D-3a (research):** Verify the existing call graph has enough inter-file edge coverage to
  make module-level coupling meaningful. If resolution is too low (<30% at file level), this
  metric will be too noisy to be useful. Gate on D-2a research findings.
- [ ] **D-3b:** Add `compute_module_aggregates()` to `aggregates.rs` — group files by parent
  directory, sum call graph edges, compute instability.
- [ ] **D-3c:** Add `modules` array to snapshot JSON output.
- [ ] **D-3d:** Add module view to text output.

**Effort:** Medium. Depends on call graph quality (D-3a must validate first).
**Risk:** Low-Medium. Call graph resolution limits may make the output misleading if not gated.

---

## Ordering / Dependencies

```
D-1 (file view)          — no dependencies, start now
D-2a (co-change research) — no dependencies, start in parallel with D-1
D-2b–e (co-change impl)  — blocked by D-2a
D-3a (module research)   — blocked by D-2a (shares call graph quality question)
D-3b–d (module impl)     — blocked by D-3a
```

## Suggested Order of Attack

1. **D-1** — File-level view. Lowest effort, highest immediate value. Purely aggregates
   existing data. No new git calls, no new parsing.
2. **D-2a** — Co-change research. Run the prototype, validate signal quality, calibrate
   threshold. Short task, unblocks D-2 and D-3.
3. **D-2b–e** — Co-change implementation (if D-2a validates well).
4. **D-3** — Module instability (after call graph quality is understood).

---

**Created:** 2026-02-18
