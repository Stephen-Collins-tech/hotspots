# Hotspots Codebase Audit Report

**Date:** 2026-02-17 (Revised)  
**Scope:** Architectural issues and code smells across the Rust codebase  
**Codebase Size:** ~18,090 LOC (production + tests)

---

## Executive Summary

Hotspots is a well-structured Rust project with strong invariants (determinism, immutability, per-function isolation). The codebase shows good patterns: `SnapshotEnricher` builder, shared `tree_sitter_utils`, AST-based call graph, parameter structs. Several issues remain: structural duplication in metrics, repetitive policy evaluation, panic-prone CFG visitor state, dead code, and oversized modules. None are critical; addressing them would improve maintainability and robustness.

---

## 1. Architectural Issues

### 1.1 metrics.rs — Large Monolith with Structural Duplication (~1,708 lines)

**Severity:** High  
**Status:** Partially addressed

The largest file in the codebase. Tree-sitter metrics (Go, Java, Python) share helpers (`ts_with_function_body`, `ts_find_function_by_start`, `ts_find_child_by_kind`, `ts_nesting_depth`), but still have duplication:

- **Callee extraction:** `go_extract_callees`, `java_extract_callees`, `python_extract_callees` follow the same pattern (collect nodes, extract identifiers, dedupe)
- **Non-structured exits:** `go_non_structured_exits`, `java_non_structured_exits`, `python_non_structured_exits` traverse AST counting exits
- **CC extras:** `go_count_cc_extras`, `java_count_cc_extras`, `python_count_cc_extras` count language-specific complexity contributors

**What's good:** Shared helpers reduce duplication. The macro `impl_nesting_visitor!` eliminates ECMAScript visitor duplication.

**Remaining duplication:** ~300-400 lines could be unified with a generic `TreeSitterMetricsConfig` that parameterizes node kinds per metric.

**Ref:** ANALYSIS.md §1, CODEBASE_IMPROVEMENTS §2.2

---

### 1.2 Snapshot — Data Struct with Orchestration Logic

**Severity:** Medium  
**Status:** Partially addressed

`Snapshot` is both a data container and an orchestrator. It exposes:
- `populate_churn()`, `populate_touch_metrics()`, `populate_callgraph()`
- `compute_activity_risk()`, `compute_percentiles()`, `compute_summary()`

**Status:** `SnapshotEnricher` builder exists and makes ordering explicit. However, the mutation methods still live on `Snapshot`; enrichment could be moved entirely into the enricher for a pure data struct.

**Recommendation:** Move all enrichment logic into `SnapshotEnricher`, leaving `Snapshot` as a pure data container.

**Ref:** ANALYSIS.md §4

---

### 1.3 policy.rs — Repetitive Evaluation Pattern (~1,077 lines)

**Severity:** Medium

Seven policy evaluators share this pattern:

```rust
for entry in active_deltas(deltas) {
    if entry.status != <expected> { continue; }
    // condition check...
    results.failed.push(...) or results.warnings.push(...);
}
```

The loop and status filter are duplicated. `active_deltas()` centralizes suppression filtering, but status checks and result pushing are repeated.

**Recommendation:** Introduce a `Policy` trait:
```rust
trait Policy {
    fn id(&self) -> PolicyId;
    fn severity(&self) -> PolicySeverity;
    fn target_statuses(&self) -> &[FunctionStatus];
    fn evaluate(&self, entry: &FunctionDeltaEntry, config: &Config) -> Option<PolicyResult>;
}
```

Then a single evaluation loop dispatches to all policies.

**Ref:** ANALYSIS.md §3

---

### 1.4 main.rs — Long CLI Module (~1,401 lines)

**Severity:** Medium
**Status:** Partially addressed (2026-02-18)

`main()` reduced from cc=50 to ~cc=6 by extracting `handle_analyze`, `handle_prune`,
`handle_compact`, `handle_config`, `handle_trends`. `build_enriched_snapshot` also extracted.

Remaining: `handle_mode_output` (cc=30, ~170 lines) still interleaves snapshot/delta output
formatting. Output formatting (JSON/HTML/JSONL/Text) and aggregates computation are duplicated
between the two modes.

**Recommendation:** Extract `emit_snapshot_output()` and `emit_delta_output()` to shrink
`handle_mode_output` to ~50 lines and clarify control flow.

---

### 1.5 html.rs — String-Based Templating (~1,034 lines)

**Severity:** Low

HTML is built with `format!()` and inline CSS/JS strings. Drawbacks:
- No compile-time validation of HTML
- No syntax checking for CSS/JS
- Potential XSS if user content is ever interpolated

**Mitigation:** Data is internal; no user input is currently rendered. Consider `askama` or `include_str!` for CSS/JS if reports grow.

**Ref:** ANALYSIS.md §5

---

### 1.6 callgraph.rs — Dead Code Path

**Severity:** Low
**Status:** ✅ Fixed (2026-02-17)

`CallGraph::from_sources()` has been deleted along with its `use regex::Regex` import.
`build_call_graph` in `lib.rs` uses AST-derived `callee_names` from reports — the correct path.

---

## 2. Code Smells

### 2.1 Panic-Prone expect/unwrap in Production

**Severity:** Medium

| Location | Context | Risk |
|----------|---------|------|
| `metrics.rs:518` | `.unwrap_or(RawMetrics{...})` | ✅ Safe fallback |
| `go/java/python cfg_builder.rs` | `self.current_node.expect("Current node should exist")` | ⚠️ Can panic if visitor state is wrong |
| `cfg/builder.rs` | `self.current_node.expect("Current node should exist")` (8 instances) | ⚠️ Same risk |
| `snapshot.rs:650` | `.unwrap_or(0)` | ✅ Safe fallback |
| `callgraph.rs:371` | `current_depth.unwrap()` after `is_none()` check | ✅ Safe (checked first) |
| `html.rs:639` | `.unwrap_or(Ordering::Equal)` | ✅ Safe fallback |
| `trends.rs:195` | `match (first(), last())` with `_ => continue` | ✅ Handles empty case |
| `git.rs:22,26` | `Regex::new(...).unwrap()` in `OnceLock` | ✅ Compile-time regex, safe |
| `policy.rs:662` | `.unwrap()` in test | ✅ Test-only |

**Production panic risks:** CFG builder visitor state (15 instances). If visitor callbacks are invoked out of order or without proper state, these will panic.

**Recommendation:** Replace CFG builder `.expect()` with `?` or explicit error handling. Consider `Option<&Node>` return type from visitor methods.

---

### 2.2 TODOs in Production Code

**Severity:** Low

| File | Line | TODO |
|------|------|------|
| `python/cfg_builder.rs` | 367 | "Model match statement CFG more precisely" |
| `java/cfg_builder.rs` | 507-509 | "Check for conditional_expression (ternary)", "binary_expression with && or ||", "lambda_expression with control flow" |

**Recommendation:** Implement or add tracking in TASKS.md. Remove or clarify if deferred.

**Ref:** CODEBASE_IMPROVEMENTS §5.2

---

### 2.3 Documentation Gaps

**Severity:** Trivial  
**Status:** ✅ Fixed

- ✅ `lib.rs:79`: Comment correctly lists "TypeScript, JavaScript, Go, Java, Python, Rust"
- ✅ `lib.rs:137-143`: `collect_source_files` doc correctly lists all languages including Java (.java) and Python (.py, .pyw)

**Previous issue resolved.**

---

### 2.4 Incomplete Features

**Severity:** Low
**Status:** ✅ Fixed (2026-02-17)

- `hotspots compact --level 1|2`: Now exits non-zero with a clear "not yet implemented" error
  instead of silently updating metadata. Prevents misleading UX.

---

## 3. Module Size / Complexity

| File | Lines | Notes |
|------|-------|-------|
| metrics.rs | 1,708 | Largest; tree-sitter duplication |
| main.rs | 1,401 | Long; could split output handling |
| snapshot.rs | 1,213 | Mix of data and orchestration |
| policy.rs | 1,077 | Repetitive evaluators |
| html.rs | 1,034 | String-based templating |
| config.rs | 930 | Config loading and resolution |
| git.rs | 856 | Git operations |
| cfg/builder.rs | 732 | ECMAScript CFG builder |
| trends.rs | 723 | Trend analysis |

Files over ~800 lines are candidates for splitting or refactoring.

---

## 4. Error Handling Patterns

- **Strengths:** `anyhow::Result` and `.context()` used consistently; graceful handling for shallow clones and missing parents.
- **Concerns:** CFG builder visitor state uses `.expect()` (15 instances); `eprintln!` for touch-metrics failure instead of structured logging.

---

## 5. Test Code Quality

- **Tests use `.unwrap()` and `.expect()`:** Acceptable in tests where failure should be immediate.
- **Integration tests:** Use temp dirs, real git, real filesystem.
- **Golden tests:** Good coverage across languages and patterns.

---

## 6. Positive Patterns

1. **Determinism:** Sorted outputs, no HashMap iteration leaks.
2. **SnapshotEnricher:** Explicit enrichment ordering (builder pattern).
3. **`tree_sitter_utils`:** Shared helpers for parsers and CFG builders.
4. **AST-based call graph:** Uses callee names from metrics; no regex in main path.
5. **Parameter structs:** `ModeOutputOptions`, `ActivityRiskInput`, `TarjanState` reduce argument count.
6. **No `#[allow(...)]` in production code:** Clippy compliance.
7. **Safe fallbacks:** Most `.unwrap()` uses have safe fallbacks (`unwrap_or`, `unwrap_or_else`).

---

## 7. Priority Recommendations

| Priority | Issue | Effort | Impact |
|----------|-------|--------|--------|
| 1 | Replace CFG builder `.expect()` with error handling | Medium | Robustness (prevents panics) |
| 2 | Extract policy trait / table-driven evaluation | Medium | Maintainability (reduces duplication) |
| 3 | Remove or document `CallGraph::from_sources` | Low | Cleanliness (dead code) |
| 4 | Extract TreeSitterMetricsConfig (generic tree-sitter metrics) | Medium | Less duplication (~300-400 lines) |
| 5 | Extract emit_snapshot_output / emit_delta_output from main | Low | Readability (shrink main.rs) |
| 6 | Move enrichment logic entirely into SnapshotEnricher | Low | Separation of concerns |
| 7 | Address CFG builder TODOs or track in TASKS | Low–Medium | Completeness |
| 8 | Document or implement compact subcommand | Low | UX |

---

## 8. Summary

| Category | Count | Notes |
|----------|-------|-------|
| Architectural issues | 6 | Duplication, dead code, oversized modules |
| Code smells | 4 | Panic risks, TODOs, incomplete features |
| Oversized modules (>800 LOC) | 9 | Candidates for splitting |
| Panic-prone production paths | 15 | CFG builder visitor state |
| TODOs in production | 4 | Python match CFG, Java ternary/binary/lambda |
| Dead code | 1 | `CallGraph::from_sources` |

**Overall Assessment:** The codebase is in good shape with strong patterns. Highest-value improvements: reduce CFG builder panic risk, simplify policy evaluation, remove dead code, and extract generic tree-sitter metrics.

---

## 9. Changes Since Last Audit

**Fixed:**
- ✅ `lib.rs` documentation now correctly lists all 6 languages (A-3)
- ✅ `html.rs` uses `unwrap_or(Ordering::Equal)` for safe fallback
- ✅ `trends.rs` handles empty slices correctly with `match`
- ✅ `callgraph.rs` — `from_sources()` dead code deleted (A-1)
- ✅ `snapshot.rs` — `to_jsonl()` `.unwrap()` replaced with `.context()` (A-2)
- ✅ `compact --level 1|2` now bails with clear "not implemented" error (A-4)
- ✅ `main()` cc reduced from 50 to ~6 via command handler extraction (A-7 partial)

**Still Open:**
- ⚠️ CFG builder visitor state panic risk (~28 `.expect()` instances across 4 files)
- ⚠️ Policy evaluation duplication (A-6)
- ⚠️ `handle_mode_output` emit extraction — cc=30 still high (A-7 remainder)
- ⚠️ Tree-sitter metrics duplication (A-8, deferred)
- ⚠️ CFG builder TODOs — Python match CC, Java ternary/lambda (A-5)

---

**References:**
- [ANALYSIS.md](../../ANALYSIS.md)
- [CODEBASE_IMPROVEMENTS.md](../../CODEBASE_IMPROVEMENTS.md)
- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [IMPROVEMENTS.md](./IMPROVEMENTS.md)
