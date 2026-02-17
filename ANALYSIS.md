# Hotspots Codebase Analysis

An independent analysis of the hotspots codebase — its current state, architecture quality, and areas that could benefit from refactoring.

## Executive Summary

Hotspots is a well-architected Rust workspace that computes per-function risk scores across six languages. The core invariants (determinism, immutability, per-function isolation) are consistently enforced. The codebase is functional and shipped, but several structural patterns have accumulated that increase maintenance cost and make the code harder to extend. None are urgent, but addressing them would improve long-term velocity.

---

## What's Working Well

- **Strong invariants.** Deterministic output, no global mutable state, explicit ordering — these are documented and enforced.
- **Clean module boundaries.** The pipeline (parse → discover → CFG → metrics → risk → report) is clear and linear.
- **Multi-language support.** Adding a new language follows a consistent pattern (parser, CFG builder, metric extractor).
- **Comprehensive testing.** Golden tests, unit tests, and an integration harness (`test_comprehensive.py`) cover the critical paths.
- **Good error handling.** `anyhow::Result` with `.context()` throughout. Graceful fallbacks for shallow clones and missing parents.

---

## Refactoring Opportunities

### 1. `metrics.rs` — Duplicated Tree-Sitter Metric Extraction (~1,450 lines)

**File:** `hotspots-core/src/metrics.rs`

This is the largest file in the codebase and the most significant duplication issue. The Go, Java, and Python metric extractors are structurally identical — each reimplements the same four operations using tree-sitter:

| Function Pattern | Go | Java | Python |
|---|---|---|---|
| `find_*_function_by_start` | Lines 480–501 | Lines 721–742 | Lines 933–954 |
| `find_*_child_by_kind` | Lines 505–516 | Lines 746–757 | Lines 958–969 |
| `*_nesting_depth` | Lines 519–550 | Lines 760–794 | Lines 972–1003 |
| `*_fan_out` | Lines 553–584 | Lines 797–820 | Lines 1006–1032 |
| `*_non_structured_exits` | Lines 587–627 | Lines 823–842 | Lines 1035–1054 |
| `*_count_cc_extras` | Lines 630–658 | Lines 846–874 | Lines 1058–1096 |

**The only differences between languages are the node kind strings** (e.g., `"if_statement"` vs `"if_statement"`, `"for_statement"` vs `"enhanced_for_statement"`). The tree traversal logic, depth tracking, and counting are identical.

**Recommendation:** Extract a generic `TreeSitterMetrics` struct that takes a configuration of node-kind sets per metric, and a single set of traversal functions parameterized by those sets. This would reduce ~600 lines of near-identical code to ~150 lines of shared logic plus ~50 lines of per-language configuration. It would also make adding C, C++, or Ruby support trivial.

---

### 2. `main.rs` — Snapshot/Delta Code Duplication (~1,350 lines)

**File:** `hotspots-cli/src/main.rs`

The `handle_mode_output` function (lines 442–722) contains two nearly identical flows for snapshot and delta modes:

- Both extract git context (lines 472 and 591)
- Both build call graphs (lines 476–477 and 595–596)
- Both create snapshots (lines 480 and 599)
- Both extract and populate churn (lines 483–501 and 602–619)
- Both populate touch metrics (lines 504–507 and 622–625)
- Both populate call graph (lines 510–512 and 628–630)
- Both compute activity risk (lines 515 and 633)

This shared setup logic (~60 lines) is copy-pasted between the two branches. If a new enrichment step is added, it must be duplicated.

**Recommendation:** Extract a `build_enriched_snapshot(path, repo_root, resolved_config) -> Snapshot` function that handles the full enrichment pipeline. Both modes would call it, then diverge only for their mode-specific logic (persistence vs delta computation).

---

### 3. `policy.rs` — Repetitive Policy Evaluation Pattern

**File:** `hotspots-core/src/policy.rs`

Six of the seven policy evaluators follow the exact same pattern:

```
for entry in deltas {
    if entry.suppression_reason.is_some() { continue; }
    if entry.status != <expected_status> { continue; }
    // ... check condition ...
    results.<failed|warnings>.push(PolicyResult { ... });
}
```

The loop, suppression check, and status filter are duplicated across all six functions. The `compare_policy_results` function (lines 120–158) manually encodes enum ordering through an exhaustive match — this could use `#[derive(PartialOrd, Ord)]` or a simple integer mapping instead of 30+ match arms.

**Recommendation:** Consider a policy trait or a table-driven approach where each policy declares its target statuses and condition, and a single evaluator loop dispatches to them. This isn't critical but would reduce the boilerplate and make adding new policies easier.

---

### 4. `snapshot.rs` — Orchestration Methods on a Data Struct (~1,025 lines)

**File:** `hotspots-core/src/snapshot.rs`

The `Snapshot` struct is a serializable data container, but it also has seven orchestration methods that perform complex operations:

- `populate_churn()` — maps file churns to functions
- `populate_touch_metrics()` — shells out to git for touch counts
- `populate_callgraph()` — computes PageRank, betweenness, SCCs
- `compute_activity_risk()` — combines all metrics into a unified score
- `compute_percentiles()` — ranks functions by activity risk
- `compute_summary()` — builds repo-level statistics

These methods mutate the snapshot in-place and must be called in a specific order (churn → touch → callgraph → activity risk → percentiles → summary). This implicit ordering is documented in comments but not enforced by the type system.

**Recommendation:** Extract enrichment into a builder or pipeline pattern. A `SnapshotEnricher` that takes a `Snapshot` and produces an enriched one would make the ordering explicit and testable. The `Snapshot` struct itself would remain a pure data container.

---

### 5. `html.rs` — String-Based HTML Templating (~1,030 lines)

**File:** `hotspots-core/src/html.rs`

HTML is built via `format!()` with large inline string literals containing CSS and JavaScript. This works but has several drawbacks:

- No compile-time validation of the HTML structure
- CSS and JS are embedded as string literals — no syntax highlighting, no linting
- XSS injection risk if any user-controlled data is interpolated without escaping (currently mitigated since all data is internal, but fragile)

**Recommendation:** This is low priority since the current approach works. If reports become more complex, consider a lightweight template engine (e.g., `askama` for compile-time templates) or extract the CSS/JS to `include_str!()` files for better tooling support.

---

### 6. `NestingDepthVisitor` — Identical Increment Pattern (8 visit methods)

**File:** `hotspots-core/src/metrics.rs`, lines 208–285

The ECMAScript `NestingDepthVisitor` has eight `visit_*` methods that are character-for-character identical except for the type signature:

```rust
fn visit_if_stmt(&mut self, if_stmt: &IfStmt) {
    self.current_depth += 1;
    if self.current_depth > self.max_depth {
        self.max_depth = self.current_depth;
    }
    if_stmt.visit_children_with(self);
    self.current_depth -= 1;
}
```

This is repeated for `while`, `do_while`, `for`, `for_in`, `for_of`, `switch`, and `try`.

**Recommendation:** A macro `impl_nesting_visitor!(visit_if_stmt, IfStmt, if_stmt; ...)` would reduce this from 80 lines to ~10. The SWC visitor pattern makes this a natural fit for declarative macro generation.

---

### 7. `callgraph.rs` / `lib.rs` — Regex-Based Call Graph Extraction

**File:** `hotspots-core/src/lib.rs`, lines 217–336

The call graph is built by re-reading source files and using `([a-zA-Z_][a-zA-Z0-9_]*)\s*\(` to find function calls. This has known limitations:

- String literals containing `foo(` will produce false edges
- Method calls like `obj.method()` are partially handled
- Function range estimation uses "start of next function" as end boundary, which is approximate

The tool already parses every file into a full AST during analysis. The discovered `FunctionNode` objects and their bodies contain the exact call information needed.

**Recommendation:** Reuse the AST-based fan-out data that `metrics.rs` already computes (the `FanOutVisitor` for ECMAScript, `go_fan_out`, etc.). These visitors already identify function calls with full AST context. Wiring them into the call graph builder would eliminate the regex pass, the second file read, and the approximation errors.

---

### 8. Minor Items

| Item | Location | Notes |
|---|---|---|
| `go_count_cc_extras` checks `op_text.contains("&&")` on the full binary expression text, which can double-count nested operators | `metrics.rs:640–643` | Should check the operator node directly, not the full text span |
| `go_non_structured_exits` counts every `expression_statement` with a `call_expression` as an exit, not just `panic()` | `metrics.rs:592–601` | Comment acknowledges this ("Would need source to check if it's panic") but the code increments unconditionally |
| `CommitInfo` and `GitContext` have overlapping fields but no `From` conversion | `snapshot.rs:30–46`, `git.rs:18–29` | The manual field-by-field copy in `Snapshot::new()` (lines 247–256) is error-prone |
| Default excludes list doesn't include Python test patterns (`test_*.py`, `*_test.py`) or Go test files (`*_test.go`) | `config.rs:19–33` | Only JS/TS test patterns are excluded by default |
| `hotspots-cli/src/main.rs` uses `#[allow(clippy::too_many_arguments)]` on `handle_mode_output` | `main.rs:441` | The 9-parameter function is a symptom of the duplication in point 2 above |

---

## Architecture Strengths Worth Preserving

1. **Determinism-first design.** Every function sorts its outputs. No HashMap iteration order leaks. This is rare and valuable.
2. **Graceful degradation.** Shallow clones, missing parents, parse errors — all handled without panics.
3. **Schema versioning.** Snapshots carry `schema_version`, enabling future format changes.
4. **Suppression system.** The `// hotspots-ignore: reason` pattern with policy-level enforcement is well-designed.
5. **Atomic writes.** Snapshot persistence uses temp-file-plus-rename, preventing corruption.

---

## Suggested Priority Order

| Priority | Item | Impact | Effort |
|---|---|---|---|
| 1 | Extract shared tree-sitter metric logic | High — eliminates ~600 lines of duplication, unblocks new languages | Medium |
| 2 | Extract snapshot enrichment pipeline in CLI | Medium — removes copy-paste, prevents drift | Low |
| 3 | Reuse AST-based fan-out for call graph | Medium — eliminates regex pass and false edges | Medium |
| 4 | Fix Go CC/NS metric edge cases | Low — correctness improvement | Low |
| 5 | Add `From<GitContext>` for `CommitInfo` | Low — ergonomic improvement | Low |
| 6 | Add Python/Go test file default excludes | Low — completeness | Low |
| 7 | Macro for nesting depth visitors | Low — readability improvement | Low |
| 8 | Template engine for HTML reports | Low — only matters if reports grow | High |
