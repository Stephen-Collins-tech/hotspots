# Audit Tasks

**Source:** `docs/architecture/AUDIT_REPORT.md`
**Date Created:** 2026-02-17
**Principle:** Fix correctness and dead code before refactoring

---

## Summary

| ID   | Finding                                        | Severity | Effort     | Status      |
|------|------------------------------------------------|----------|------------|-------------|
| A-1  | `from_sources()` dead code in callgraph.rs     | Low      | Trivial    | [x] Done    |
| A-2  | `to_jsonl()` `.unwrap()` in production path    | Low      | Trivial    | [x] Done    |
| A-3  | lib.rs docs missing Java and Python            | Trivial  | Trivial    | [x] Done    |
| A-4  | `compact` subcommand incomplete (UX)           | Low      | Low        | [x] Done    |
| A-5  | CFG builder TODOs untracked                    | Low      | Low-Medium | [ ] Pending |
| A-6  | `policy.rs` repetitive evaluation pattern      | Medium   | Medium     | [ ] Pending |
| A-7  | `main.rs` emit_* extraction                    | Medium   | Low        | [ ] Pending |
| A-8  | `metrics.rs` TreeSitterMetricsConfig refactor  | Medium   | Medium-High| [ ] Pending |
| A-9  | CFG builder `.expect()` panic risk (~28 sites) | Medium   | Medium     | [ ] Pending |
| A-10 | Snapshot enrichment separation                 | Low      | Low-Medium | [ ] Pending |

**Audit report corrections (false positives):**
- `trends.rs:195` — NOT a panic; it is a safe `match (first(), last())` with `continue` fallback.
- `policy.rs:662` — `.unwrap()` is in a test function, not production code; acceptable.

---

## A-1: `CallGraph::from_sources()` Dead Code

**Finding:** `from_sources()` (callgraph.rs lines 459–496) extracts calls via regex on raw source.
It is never called anywhere. `build_call_graph()` in `lib.rs` uses AST-derived `callee_names`
from reports — the correct path. The regex-based function is a vestige of an earlier approach.

**Fix:** Delete `from_sources()` and its `use regex::Regex` import (which becomes unused).

**Acceptance:** `cargo check` clean; no grep hits for `from_sources`.

**Status:** [x] Done (2026-02-17)

---

## A-2: `to_jsonl()` Uses `.unwrap()` in Production Path

**Finding:** `snapshot.rs` `to_jsonl()` calls `obj.as_object_mut().unwrap()`. While logically
safe (serde_json always produces an object for a struct), the code path is a public `Result`-
returning function and should not panic — it should propagate errors via `?`.

**Fix:** Replace `.unwrap()` with `.context("serialized function is not a JSON object")?`.

**Acceptance:** `cargo check` clean; no `.unwrap()` in `to_jsonl()`.

**Status:** [x] Done (2026-02-17)

---

## A-3: `lib.rs` Doc Comments Missing Java and Python

**Finding:** Two places in `lib.rs` list supported languages but omit Java and Python:
- Line 79: inline comment lists "TypeScript, JavaScript, Go, Rust"
- Lines 137–143: `collect_source_files` doc lists 6 types but not Java (`.java`) or Python (`.py`, `.pyw`)

**Fix:** Update both to include Java and Python.

**Acceptance:** Grep for `Java` and `Python` in lib.rs confirms they appear in both locations.

**Status:** [x] Done (2026-02-17)

---

## A-4: `compact` Subcommand Incomplete — Misleading UX

**Finding:** `hotspots compact --level 1` and `--level 2` are accepted by the CLI and exit with
success, but only update index metadata. No actual compaction happens. A user running this in a
disk-constrained CI environment will think they compacted history but will see no space savings.

**Options:**
1. Return an error when level > 0 is requested: `bail!("compaction to level X is not yet implemented")`
2. Implement actual compaction (significant feature work)

**Recommendation:** Option 1 for now — fail fast with a clear message rather than silently no-op.
Track actual implementation as a separate feature task.

**Acceptance:** `hotspots compact --level 1` exits non-zero with a clear "not implemented" message.

**Status:** [x] Done (2026-02-17)

---

## A-5: CFG Builder TODOs — Untracked Accuracy Gaps

**Finding:** Production `// TODO:` comments mark known CFG accuracy gaps not tracked elsewhere:

| File | Line | TODO |
|------|------|------|
| `python/cfg_builder.rs` | 367 | "Model match statement CFG more precisely" |
| `java/cfg_builder.rs` | ~507 | "Check for conditional_expression (ternary)" |
| `java/cfg_builder.rs` | ~508 | "binary_expression with && or \|\|" |
| `java/cfg_builder.rs` | ~509 | "lambda_expression with control flow" |

**Impact:** These gaps mean CC is undercounted for Python `match` statements and Java ternary/lambda/
short-circuit expressions. Functions using these patterns will show lower-than-actual complexity.

**Fix:** Implement or convert to tracked issues. Partial fixes (e.g., Python `match` → CC bump per arm)
are self-contained and medium effort.

**Acceptance:** TODOs resolved or converted to `// NOTE: known limitation — <reason>`.

**Status:** [ ] Pending

---

## A-6: `policy.rs` Repetitive Evaluation Loop

**Finding:** Seven policy evaluators share the same structure:
```rust
for entry in active_deltas(deltas) {
    if entry.status != <expected> { continue; }
    // condition check...
    results.failed.push(...) or results.warnings.push(...)
}
```
The loop, status filter, and result-pushing logic are duplicated.

**Recommendation:** Introduce a `Policy` trait:
```rust
trait Policy {
    fn target_statuses(&self) -> &[FunctionStatus];
    fn evaluate(&self, entry: &FunctionDeltaEntry) -> Option<PolicyResult>;
}
```
Then a single dispatch loop. Reduces ~400 lines to ~200.

**Risk:** Refactor risk — changing the evaluation structure could break subtle behavior. Requires
careful test coverage validation.

**Acceptance:** All existing policy tests pass unchanged; no new failures.

**Status:** [ ] Pending

---

## A-7: `main.rs` Emit Function Extraction

**Finding:** `handle_mode_output` is ~160 lines with snapshot and delta output formatting
interleaved. The output-format dispatch (JSON/HTML/JSONL/Text) is repeated for both modes.

**Recommendation:** Extract `emit_snapshot_output()` and `emit_delta_output()` to reduce
`handle_mode_output` and clarify the control flow.

**Effort:** Low — pure extraction refactor, no logic changes.

**Acceptance:** `handle_mode_output` reduces to ~50 lines; all output tests pass.

**Status:** [ ] Pending

---

## A-8: `metrics.rs` TreeSitterMetricsConfig Refactor

**Finding:** Go, Java, and Python metrics extraction follows identical structure but with
different node-kind arrays. ~400 lines of similar code could become ~100 lines of shared
logic driven by a per-language config struct.

**Risk:** High refactor risk — metrics extraction is core correctness logic. Any breakage
would affect all golden tests. The golden tests provide a safety net, but this is a significant
change.

**Recommendation:** Do this only after adding more granular per-language metric tests (beyond
current golden file tests), so regressions can be caught precisely.

**Acceptance:** All golden tests pass; no metric value changes.

**Status:** [ ] Pending — defer until per-metric unit tests are added

---

---

## A-9: CFG Builder `.expect()` Panic Risk

**Finding:** ~28 production `.expect("Current node should exist")` calls across four CFG builder
files. If the visitor is invoked in an unexpected order or without proper initialization,
these will panic. The pattern is:

```rust
let from_node = self.current_node.expect("Current node should exist");
```

| File | Production `.expect()` instances |
|------|----------------------------------|
| `cfg/builder.rs` | ~12 (visitor methods) |
| `language/python/cfg_builder.rs` | 5 |
| `language/go/cfg_builder.rs` | 4 |
| `language/java/cfg_builder.rs` | 0 (already clean) |

**Fix options:**
1. Return `Result` from visitor methods (significant signature change, safest)
2. Use `unwrap_or_else(|| panic!(...))` with a debug assertion — no improvement
3. Validate state in the CFG builder's `build()` entry point and use a sentinel/default
   node ID to make the `.expect()` unreachable in practice

**Recommended:** Option 3 is the pragmatic fix — validate that `current_node` is always
initialized before the visitor starts, then document the invariant. The `.expect()` becomes
a programming-error guard, not a runtime failure mode.

**Acceptance:** No production `.expect("Current node should exist")` reachable from unvalidated
input; `cargo test` passes.

**Status:** [ ] Pending

---

## A-10: Snapshot Enrichment Separation

**Finding:** `Snapshot` is both a data container and partially an orchestrator. Enrichment
methods (`populate_churn`, `populate_touch_metrics`, `populate_callgraph`,
`compute_activity_risk`, `compute_percentiles`, `compute_summary`) still live on `Snapshot`
itself. The `SnapshotEnricher` builder wraps this but doesn't fully encapsulate it.

**Recommendation:** Move all enrichment mutation logic from `Snapshot` into `SnapshotEnricher`,
leaving `Snapshot` as a pure data struct with only serialization methods.

**Effort:** Low-Medium. No behavior changes — pure structural move. Risk: snapshot.rs is
1,213 lines; touching it has broad impact.

**Acceptance:** `Snapshot` has no `populate_*` or `compute_*` methods; all tests pass.

**Status:** [ ] Pending

---

## Ordering

Done: A-1, A-2, A-3, A-4
Next up (by value/risk ratio): A-7, A-5, A-9
Medium effort: A-6, A-10
Deferred: A-8

---

**Created:** 2026-02-17
**Source:** `docs/architecture/AUDIT_REPORT.md`
