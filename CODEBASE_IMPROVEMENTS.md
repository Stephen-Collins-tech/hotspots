# Codebase Improvements

**Status:** Pending  
**Source:** Independent codebase review (2026-02-15)  
**Last Reassessment:** 2026-02-15  
**Goal:** Address bugs, duplication, code smells, and incomplete features identified across the codebase

---

## Reassessment Summary

| Change | Status |
|--------|--------|
| **1.2 ECMAScript/Rust call graph** | ✅ **RESOLVED** — All 6 languages now have AST-based callee extraction (`ecmascript_extract_callees`, `rust_extract_callees` in `metrics.rs`). Call graph edges work for TS/JS/Go/Java/Python/Rust. |
| **SnapshotEnricher (TASKS 8.10)** | ✅ **IMPLEMENTED** — `SnapshotEnricher` builder pattern in place. 6.2 (snapshot clone for aggregates) still applies to JSON/HTML output formatting. |
| **1.1 FAULTLINE typo** | ✅ **RESOLVED** — `build.rs` and `main.rs` both use `HOTSPOTS_VERSION`. No action needed. |
| **1.3 Unused build_call_graph params** | ✅ **RESOLVED** — `_path` and `_resolved_config` params removed; signature is now `build_call_graph(reports)`. |
| **6.3 Git context CWD** | ✅ **RESOLVED** — `build_enriched_snapshot` now uses `extract_git_context_at(repo_root)` and `extract_commit_churn_at(repo_root, sha)`. |

---

## Critical / High Priority

### 1.1 ~~Fix FAULTLINE → HOTSPOTS Version Env Typo~~ ✅ RESOLVED

`build.rs` already emits `cargo:rustc-env=HOTSPOTS_VERSION={}` and `main.rs` uses `env!("HOTSPOTS_VERSION")`. No action needed.

---

### 1.2 ~~Restore or Document ECMAScript/Rust Call Graph Edges~~ ✅ RESOLVED

**Status:** Addressed. All 6 languages now have AST-based callee extraction. `ecmascript_extract_callees` and `rust_extract_callees` in `metrics.rs` populate `callee_names`; `build_call_graph` in `lib.rs` uses these for edges. No further action.

---

### 1.3 ~~Remove Unused Parameters from build_call_graph~~ ✅ RESOLVED

Signature simplified to `build_call_graph(reports) -> Result<CallGraph>`. The `build_enriched_snapshot` `path` parameter (only passed to `build_call_graph`) was also removed. `cargo test` passes.

---

## Duplication

### 2.1 Extract Shared Tree-Sitter Helpers (find_child_by_kind, find_function_by_start)

**Priority:** 4 — Medium impact, reduces duplication

**Problem:**
`find_child_by_kind` and `find_function_by_start` are duplicated across six files:
- `go/parser.rs`, `java/parser.rs`, `python/parser.rs`
- `go/cfg_builder.rs`, `java/cfg_builder.rs`, `python/cfg_builder.rs`

Each is nearly identical; only node kind strings differ.

**Specification:**
- Create `hotspots-core/src/language/tree_sitter_utils.rs`
- Add `pub fn find_child_by_kind(node: Node, kind: &str) -> Option<Node>`
- Add `pub fn find_function_by_start(root: Node, start_byte: usize, func_kinds: &[&str]) -> Option<Node>` (or per-language wrappers that pass their kinds)
- Replace all 6+ copies with calls to shared module

**Success Criteria:**
- [ ] Single implementation of each helper
- [ ] All language parsers and CFG builders use shared module
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/language/mod.rs` (add `pub mod tree_sitter_utils`)
- `hotspots-core/src/language/tree_sitter_utils.rs` (new)
- `hotspots-core/src/language/go/parser.rs`
- `hotspots-core/src/language/go/cfg_builder.rs`
- `hotspots-core/src/language/java/parser.rs`
- `hotspots-core/src/language/java/cfg_builder.rs`
- `hotspots-core/src/language/python/parser.rs`
- `hotspots-core/src/language/python/cfg_builder.rs`

**Estimated Effort:** 1–2 hours

---

### 2.2 Tree-Sitter CFG Builder Boilerplate

**Priority:** 5 — Medium impact, related to ANALYSIS.md item 1

**Problem:**
Go, Java, and Python CFG builders each re-parse source, find the function node by start byte, and find the body block. This duplicates the pattern in metrics.rs.

**Specification:**
- Consider shared `TreeSitterCfgBuilder` trait or module that handles re-parse + function/body lookup
- Per-language builders provide only node kind config and block-handling logic
- May be addressed as part of TASKS.md 8.1 (TreeSitterMetrics extraction) if metrics and CFG are unified

**Success Criteria:**
- [ ] Re-parse and node-finding logic defined once
- [ ] Per-language CFG builders simplified
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/language/go/cfg_builder.rs`
- `hotspots-core/src/language/java/cfg_builder.rs`
- `hotspots-core/src/language/python/cfg_builder.rs`
- Possibly new `language/tree_sitter_cfg.rs`

**Estimated Effort:** 3–4 hours

---

## Code Smells / Clippy

### 3.1 ~~Replace too_many_arguments with Parameter Structs~~ ✅ RESOLVED

- `scoring.rs`: Introduced `ActivityRiskInput` struct; `compute_activity_risk` now takes `(&ActivityRiskInput, &ScoringWeights)`.
- `callgraph.rs`: Introduced `TarjanState` struct; `tarjan_strongconnect` now takes `(v: &str, state: &mut TarjanState)`.
- `report.rs`: Introduced `FunctionAnalysis` struct; `FunctionRiskReport::new` now takes 5 args.
- `rust/parser.rs`: Removed unused `_block: &Block` parameter from `extract_function_common` (reduced to 6 args, under the threshold).
- `cargo clippy` passes with zero warnings.

---

### 3.2 ~~Replace manual_find with .find()~~ ✅ RESOLVED

All 7 `find_child_by_kind` functions now use `let result = node.children(&mut cursor).find(...); result`. The explicit `let result` binding is required to satisfy Rust's borrow checker (the iterator must drop before the cursor). `#[allow(clippy::manual_find)]` removed from all 7 files.

---

## Robustness / Error Handling

### 4.1 Reduce expect/unwrap in Production Code

**Priority:** 8 — Medium impact, prevents panics

**Problem:**
Several production paths use `.expect()` or `.unwrap()`:
- `metrics.rs:476` — tree-sitter language setup
- Go/Java/Python CFG builders — parse/expect on tree-sitter
- `trends.rs:195` — `.unwrap()` on `sorted_points.last()`
- `html.rs:636` — `.unwrap()` on `partial_cmp`
- `main.rs:559` — `.unwrap_or(Ordering::Equal)` on `partial_cmp` (acceptable fallback but worth verifying)

**Specification:**
- Replace `.expect()` with `?` and proper error propagation where feasible
- For tree-sitter: return `Result` from metric extraction, propagate to caller
- For `sorted_points.last()`: handle empty slice explicitly
- For `partial_cmp`: use `unwrap_or(Ordering::Equal)` or handle NaN explicitly

**Success Criteria:**
- [ ] No panic-prone `expect`/`unwrap` in hot paths
- [ ] Graceful handling of malformed input
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/metrics.rs`
- `hotspots-core/src/language/go/cfg_builder.rs`
- `hotspots-core/src/language/java/cfg_builder.rs`
- `hotspots-core/src/language/python/cfg_builder.rs`
- `hotspots-core/src/trends.rs`
- `hotspots-core/src/html.rs`
- `hotspots-cli/src/main.rs`

**Estimated Effort:** 2–3 hours

---

### 4.2 ~~Handle NaN in partial_cmp for activity_risk~~ ✅ RESOLVED

`html.rs` `files.sort_by` now uses `.unwrap_or(std::cmp::Ordering::Equal)`. `main.rs` already used `unwrap_or(Ordering::Equal)` for activity_risk sorting.

---

## Documentation / Consistency

### 5.1 ~~Update lib.rs Crate Description~~ ✅ RESOLVED

`hotspots-core/src/lib.rs` crate description updated to list all 6 supported languages: TypeScript, JavaScript, Go, Java, Python, and Rust.

---

### 5.2 Address CFG Builder TODOs

**Priority:** 11 — Low impact

**Problem:**
- `python/cfg_builder.rs:373` — "TODO: Model match statement CFG more precisely"
- `java/cfg_builder.rs:514–516` — "TODO: Check for conditional_expression (ternary)", "binary_expression with && or ||", "lambda_expression with control flow"

**Specification:**
- Implement TODOs or add tracking tasks to TASKS.md with acceptance criteria
- Remove stale TODO comments if deferred

**Success Criteria:**
- [ ] All CFG TODOs either implemented or explicitly tracked
- [ ] No orphaned TODO comments

**Files to Modify:**
- `hotspots-core/src/language/python/cfg_builder.rs`
- `hotspots-core/src/language/java/cfg_builder.rs`
- `TASKS.md` (if deferring)

**Estimated Effort:** 1–4 hours depending on implementation

---

## Incomplete Features

### 6.1 Implement or Document Compact Subcommand

**Priority:** 12 — Low impact

**Problem:**
`hotspots compact --level N` only updates index metadata; compaction to levels 1 and 2 is not implemented. User sees "Note: Compaction to level X is not yet implemented."

**Specification:**
Option A: Implement compaction (levels 1 = deltas only, 2 = band transitions only).
Option B: Document as known limitation, consider removing or hiding the command until implemented.

**Success Criteria:**
- [ ] Either compaction implemented for levels 1–2, or clearly documented as not yet implemented
- [ ] No misleading UX

**Files to Modify:**
- `hotspots-cli/src/main.rs`
- `hotspots-core/src/snapshot.rs` (if implementing)
- `docs/reference/cli.md`
- `docs/guide/usage.md`

**Estimated Effort:** 4–8 hours (if implementing)

---

## Performance / Architecture

### 6.2 ~~Avoid Snapshot Clone for Aggregates~~ ✅ RESOLVED

In `handle_mode_output`, snapshot mode no longer clones the snapshot to attach aggregates. Aggregates are computed first, then set directly on the `mut snapshot` before JSON/HTML rendering.

---

### 6.3 ~~Verify Git Context Uses Correct Repo Root~~ ✅ RESOLVED

`build_enriched_snapshot` now uses `extract_git_context_at(repo_root)` and `extract_commit_churn_at(repo_root, sha)`. CLI always uses the correct repo root regardless of CWD.

---

## Minor

### 7.1 ~~Clean Up examples/export_visualization.rs~~ ✅ RESOLVED

Removed unused struct fields (`schema_version`, `analysis`, `AnalysisInfo` struct, `parents`, `branch`) from the deserialization types. `#[allow(dead_code)]` annotations eliminated. Serde silently ignores unknown JSON fields, so the example continues to work correctly.

---

### 7.2 ~~Lazy-Compile Regex in Hot Paths~~ ✅ RESOLVED

`git.rs` now uses `std::sync::OnceLock<Regex>` for `JIRA_RE` and `GITHUB_RE` — compiled once per process. The `callgraph.rs` regex in `from_sources` is unused (dead code path) and can be removed in a future cleanup.

---

## Priority Summary

| # | Task | Severity | Effort |
|---|------|----------|--------|
| 1.1 | ~~Fix FAULTLINE → HOTSPOTS version env~~ | ✅ Resolved | — |
| 1.2 | ~~ECMAScript/Rust call graph edges~~ | ✅ Resolved | — |
| 1.3 | ~~Remove unused build_call_graph params~~ | ✅ Resolved | — |
| 2.1 | Extract shared tree-sitter helpers | Medium | Low |
| 2.2 | Tree-sitter CFG builder boilerplate | Medium | Medium |
| 3.1 | ~~Replace too_many_arguments with structs~~ | ✅ Resolved | — |
| 3.2 | ~~Replace manual_find with .find()~~ | ✅ Resolved | — |
| 4.1 | Reduce expect/unwrap in production | Medium | Low–Medium |
| 4.2 | ~~Handle NaN in partial_cmp~~ | ✅ Resolved | — |
| 5.1 | ~~Update lib.rs crate description~~ | ✅ Resolved | — |
| 5.2 | Address CFG builder TODOs | Low | Low–Medium |
| 6.1 | Implement or document Compact | Low | Medium |
| 6.2 | ~~Avoid snapshot clone for aggregates~~ | ✅ Resolved | — |
| 6.3 | ~~Verify git context repo root~~ | ✅ Resolved | — |
| 7.1 | ~~Clean up export_visualization.rs~~ | ✅ Resolved | — |
| 7.2 | ~~Lazy-compile regex in hot paths~~ | ✅ Resolved | — |

---

**Last Updated:** 2026-02-15
**Last Reassessment:** 2026-02-15
