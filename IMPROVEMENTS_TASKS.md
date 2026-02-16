# Improvements Task Tracking

Derived from `CODEBASE_IMPROVEMENTS.md`. Check off items as completed.

---

## Resolved (by PR #15 or this branch)

- [x] **#1 Fix faultline → hotspots naming** — scripts, env var, examples, comments
- [x] **#2 Remove unused params from build_call_graph** — resolved upstream (#15)
- [x] **#4 Replace manual_find allows with .find()** — resolved upstream (#15)
- [x] **#5 Handle NaN in partial_cmp sorts** — resolved upstream (#15)
- [x] **#6 Update lib.rs language list** — resolved upstream (#15)
- [x] **#7 Fix git context repo root** — resolved upstream (#15)
- [x] **#8 Replace too_many_arguments with structs** — resolved upstream (#15)
- [x] **#10 Lazy-compile regexes** — resolved upstream (#15)

---

## Remaining

- [ ] **#3 Extract shared tree-sitter helpers into tree_sitter_utils.rs**
  `find_child_by_kind` and `find_function_by_start` duplicated across 6 files.
  - Create `hotspots-core/src/language/tree_sitter_utils.rs`
  - Register in `language/mod.rs`
  - Replace copies in: `go/parser.rs`, `java/parser.rs`, `python/parser.rs`, `go/cfg_builder.rs`, `java/cfg_builder.rs`, `python/cfg_builder.rs`

- [ ] **#9 Reduce expect/unwrap panics in production paths**
  - `metrics.rs:476` — tree-sitter language setup
  - Go/Java/Python CFG builders — parse/expect on tree-sitter
  - `trends.rs:195` — `.unwrap()` on `sorted_points.last()`
  - `html.rs:636` — `.unwrap()` on `partial_cmp`
