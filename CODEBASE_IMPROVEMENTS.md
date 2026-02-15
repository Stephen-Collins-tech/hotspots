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
| **1.1 FAULTLINE typo** | Still open |
| **1.3 Unused build_call_graph params** | Still open |
| **6.3 Git context CWD** | Still open — `build_enriched_snapshot` uses `extract_git_context()` and `extract_commit_churn()` (CWD); `extract_git_context_at(repo_root)` and `extract_commit_churn_at(repo_root, sha)` exist but are not used there. |

---

## Critical / High Priority

### 1.1 Fix FAULTLINE → HOTSPOTS Version Env Typo

**Priority:** 1 — Critical, trivial fix

**Problem:**
The project is named "hotspots" but the version environment variable is `FAULTLINE_VERSION`. This is a copy-paste error; the env var may not exist, causing `env!("FAULTLINE_VERSION")` to fail at compile time.

**Specification:**
- Rename `FAULTLINE_VERSION` to `HOTSPOTS_VERSION` everywhere
- Update build script output: `cargo:rustc-env=HOTSPOTS_VERSION={}`
- Update CLI: `env!("HOTSPOTS_VERSION")`

**Success Criteria:**
- [ ] `hotspots --version` displays correctly
- [ ] Build succeeds in clean environment
- [ ] `cargo build` passes

**Files to Modify:**
- `hotspots-cli/build.rs`
- `hotspots-cli/src/main.rs`

**Estimated Effort:** 5 minutes

---

### 1.2 ~~Restore or Document ECMAScript/Rust Call Graph Edges~~ ✅ RESOLVED

**Status:** Addressed. All 6 languages now have AST-based callee extraction. `ecmascript_extract_callees` and `rust_extract_callees` in `metrics.rs` populate `callee_names`; `build_call_graph` in `lib.rs` uses these for edges. No further action.

---

### 1.3 Remove Unused Parameters from build_call_graph

**Priority:** 3 — Low impact, API hygiene

**Problem:**
`build_call_graph(path, reports, resolved_config)` has `path` and `resolved_config` prefixed with `_` — they are unused. These were used for the regex fallback and file filtering; now dead.

**Specification:**
- Remove `_path` and `_resolved_config` parameters from `build_call_graph`
- Update all call sites (e.g. `hotspots-cli/src/main.rs`, `build_enriched_snapshot`)

**Success Criteria:**
- [ ] Function signature simplified to `build_call_graph(reports) -> Result<CallGraph>`
- [ ] All call sites updated
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/lib.rs`
- `hotspots-cli/src/main.rs`

**Estimated Effort:** 15 minutes

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

### 3.1 Replace too_many_arguments with Parameter Structs

**Priority:** 6 — Low impact, readability

**Problem:**
Four functions use `#[allow(clippy::too_many_arguments)]`:
- `hotspots-core/src/scoring.rs:74`
- `hotspots-core/src/report.rs:55`
- `hotspots-core/src/callgraph.rs:272`
- `hotspots-core/src/language/rust/parser.rs:135`

**Specification:**
- Introduce parameter structs (e.g. `ScoringParams`, `ReportParams`) for each
- Replace multiple parameters with a single struct
- Remove `#[allow(clippy::too_many_arguments)]`

**Success Criteria:**
- [ ] No `too_many_arguments` allows in codebase
- [ ] Behavior unchanged
- [ ] `cargo clippy` passes

**Files to Modify:**
- `hotspots-core/src/scoring.rs`
- `hotspots-core/src/report.rs`
- `hotspots-core/src/callgraph.rs`
- `hotspots-core/src/language/rust/parser.rs`

**Estimated Effort:** 1–2 hours

---

### 3.2 Replace manual_find with .find()

**Priority:** 7 — Low impact, trivial

**Problem:**
Seven functions use `#[allow(clippy::manual_find)]` — they use manual for-loops instead of `.find()`:
- `metrics.rs:392`
- `python/cfg_builder.rs:499`
- `go/cfg_builder.rs:361`
- `java/cfg_builder.rs:558`
- `go/parser.rs:150`
- `python/parser.rs:148`
- `java/parser.rs:153`

**Specification:**
- Refactor each to use iterator `.find()` pattern
- Remove `#[allow(clippy::manual_find)]`

**Success Criteria:**
- [ ] No `manual_find` allows in codebase
- [ ] Logic equivalent
- [ ] `cargo clippy` passes

**Files to Modify:**
- `hotspots-core/src/metrics.rs`
- `hotspots-core/src/language/python/cfg_builder.rs`
- `hotspots-core/src/language/go/cfg_builder.rs`
- `hotspots-core/src/language/java/cfg_builder.rs`
- `hotspots-core/src/language/go/parser.rs`
- `hotspots-core/src/language/python/parser.rs`
- `hotspots-core/src/language/java/parser.rs`

**Estimated Effort:** 30 minutes

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

### 4.2 Handle NaN in partial_cmp for activity_risk

**Priority:** 9 — Medium impact, prevents panic

**Problem:**
`activity_risk` is `f64` and can be NaN. Using `.partial_cmp().unwrap()` on f64 can panic. `html.rs:636` and `main.rs:559` sort by activity_risk.

**Specification:**
- Define explicit sort key that treats NaN as lowest (or highest) to avoid panic
- Use `unwrap_or(Ordering::Equal)` or custom comparator that handles NaN
- Consider `PartialOrd` wrapper or `OrderedFloat` if this pattern recurs

**Success Criteria:**
- [ ] No panic when activity_risk is NaN
- [ ] Deterministic sort order with NaN handled
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/html.rs`
- `hotspots-cli/src/main.rs`

**Estimated Effort:** 30 minutes

---

## Documentation / Consistency

### 5.1 Update lib.rs Crate Description

**Priority:** 10 — Trivial

**Problem:**
`hotspots-core/src/lib.rs:1` says "static analysis for TypeScript, JavaScript, Go, and Rust" — Java and Python are missing.

**Specification:**
- Update to: "static analysis for TypeScript, JavaScript, Go, Java, Python, and Rust"

**Success Criteria:**
- [ ] Crate docstring lists all 6 supported languages

**Files to Modify:**
- `hotspots-core/src/lib.rs`

**Estimated Effort:** 2 minutes

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

### 6.2 Avoid Snapshot Clone for Aggregates

**Priority:** 13 — Low impact

**Problem:**
In `handle_mode_output`, snapshot mode clones the full snapshot twice (JSON and HTML branches) to attach aggregates for output. Aggregates are derived data; cloning is wasteful. *Note: SnapshotEnricher now handles the enrichment pipeline; this item concerns only the output formatting step.*

**Specification:**
- Compute aggregates without mutating a full clone
- Pass `(&snapshot, &repo_root)` to output formatters; compute aggregates inline for JSON/HTML
- Or use a lightweight output struct that references snapshot + computed aggregates

**Success Criteria:**
- [ ] No full snapshot clone solely for aggregate attachment
- [ ] Output identical to current
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-cli/src/main.rs`

**Estimated Effort:** 1 hour

---

### 6.3 Verify Git Context Uses Correct Repo Root

**Priority:** 14 — Low impact

**Problem:**
`build_enriched_snapshot` calls `git::extract_git_context()` and `git::extract_commit_churn()`, which use CWD. The function receives `repo_root` from `find_repo_root(path)`, but git operations ignore it. `extract_git_context_at(repo_root)` and `extract_commit_churn_at(repo_root, sha)` exist. If the user runs from a subdirectory, CWD may differ from `repo_root`.

**Specification:**
- In `build_enriched_snapshot`, replace `extract_git_context()` with `extract_git_context_at(repo_root)`
- Replace `extract_commit_churn(&sha)` with `extract_commit_churn_at(repo_root, &sha)`
- Or document that the CLI must be run from repo root (and add a check)

**Success Criteria:**
- [ ] Git operations use correct repository root
- [ ] Behavior correct when run from subdirectory
- [ ] Documented or tests added

**Files to Modify:**
- `hotspots-core/src/git.rs`
- `hotspots-cli/src/main.rs`
- Possibly tests

**Estimated Effort:** 1 hour

---

## Minor

### 7.1 Clean Up examples/export_visualization.rs

**Priority:** 15 — Trivial

**Problem:**
`examples/export_visualization.rs` has multiple `#[allow(dead_code)]` on struct fields. Either use them or remove them.

**Specification:**
- Remove unused fields or implement the visualization that uses them
- Remove `#[allow(dead_code)]` where no longer needed

**Success Criteria:**
- [ ] No dead code allows in examples
- [ ] Example compiles and runs (or is clearly a stub)

**Files to Modify:**
- `examples/export_visualization.rs`

**Estimated Effort:** 15 minutes

---

### 7.2 Lazy-Compile Regex in Hot Paths

**Priority:** 16 — Low impact

**Problem:**
- `callgraph.rs` — `Regex::new(...).unwrap()` in alternate build path (not used by main `lib.rs::build_call_graph`)
- `git.rs` — Regex compiled on each call for ticket-ID extraction

**Specification:**
- Use `once_cell::sync::Lazy` or `lazy_static` to compile regexes once
- Reduces alloc and CPU in hot paths

**Success Criteria:**
- [ ] Regexes compiled once per process
- [ ] Behavior unchanged
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/callgraph.rs` (if regex still used)
- `hotspots-core/src/git.rs`
- `hotspots-core/Cargo.toml` (add `once_cell` or `lazy_static` if not present)

**Estimated Effort:** 30 minutes

---

## Priority Summary

| # | Task | Severity | Effort |
|---|------|----------|--------|
| 1.1 | Fix FAULTLINE → HOTSPOTS version env | Critical | Trivial |
| 1.2 | ~~ECMAScript/Rust call graph edges~~ | ✅ Resolved | — |
| 1.3 | Remove unused build_call_graph params | Low | Trivial |
| 2.1 | Extract shared tree-sitter helpers | Medium | Low |
| 2.2 | Tree-sitter CFG builder boilerplate | Medium | Medium |
| 3.1 | Replace too_many_arguments with structs | Low | Low |
| 3.2 | Replace manual_find with .find() | Low | Trivial |
| 4.1 | Reduce expect/unwrap in production | Medium | Low–Medium |
| 4.2 | Handle NaN in partial_cmp | Medium | Trivial |
| 5.1 | Update lib.rs crate description | Trivial | Trivial |
| 5.2 | Address CFG builder TODOs | Low | Low–Medium |
| 6.1 | Implement or document Compact | Low | Medium |
| 6.2 | Avoid snapshot clone for aggregates | Low | Low |
| 6.3 | Verify git context repo root | Low | Low |
| 7.1 | Clean up export_visualization.rs | Trivial | Trivial |
| 7.2 | Lazy-compile regex in hot paths | Low | Trivial |

---

**Last Updated:** 2026-02-15  
**Last Reassessment:** 2026-02-15
