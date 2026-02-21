# Test Coverage Gaps

**Motivation:** Several language fixture files exist but have no corresponding golden
tests. JS, JSX, and TSX have fixtures but zero golden JSON files and zero test functions.
Go and Rust have golden JSON files for some fixtures that are not wired up in
`golden_tests.rs`. These gaps mean regressions in JS/JSX/TSX output can go undetected.

**Ground truth:** Golden tests are in `hotspots-core/tests/golden_tests.rs`. Fixtures
are in `hotspots-core/tests/fixtures/`. Golden JSON files are in
`hotspots-core/tests/golden/`.

---

## TC-1: JavaScript Golden Tests

**Problem:** `tests/fixtures/js/` contains fixture files (`simple.js`,
`nested-branching.js`, `loop-breaks.js`, `try-catch-finally.js`, `pathological.js`)
but there are no golden JSON files for any of them and no test functions in
`golden_tests.rs`.

**Tasks:**
- [ ] **TC-1a:** Run the hotspots analyzer on each JS fixture and capture the output as
  golden JSON files in `tests/golden/js/`. Use the same pattern as existing TypeScript
  goldens.
- [ ] **TC-1b:** Add `test_golden_js_simple`, `test_golden_js_nested_branching`,
  `test_golden_js_loop_breaks`, `test_golden_js_try_catch_finally`,
  `test_golden_js_pathological` in `golden_tests.rs` following the existing pattern.
- [ ] **TC-1c:** Run `cargo test` to verify all pass.

**Note:** JS and TS share the `ecmascript.rs` parser, so JS coverage also indirectly
validates the TS path and is especially important.

**Effort:** Low. Pattern is fully established; this is mechanical.

---

## TC-2: JSX Golden Tests

**Problem:** `tests/fixtures/jsx/` contains `simple-component.jsx`,
`complex-component.jsx`, `conditional-rendering.jsx` but no golden JSON files and no
test functions.

**Tasks:**
- [ ] **TC-2a:** Generate golden JSON files for all three JSX fixtures.
- [ ] **TC-2b:** Add corresponding test functions in `golden_tests.rs`.
- [ ] **TC-2c:** Run `cargo test` to verify.

**Note:** JSX is the most unique — it exercises the JSX element parsing path. Capturing
a golden ensures JSX elements (which produce no functions themselves) don't interfere
with CC/ND counting for the surrounding code.

**Effort:** Low.

---

## TC-3: TSX Golden Tests

**Problem:** `tests/fixtures/tsx/` contains `simple-component.tsx`,
`complex-component.tsx`, `conditional-rendering.tsx` but no golden JSON files and no
test functions.

**Tasks:**
- [ ] **TC-3a:** Generate golden JSON files for all three TSX fixtures.
- [ ] **TC-3b:** Add corresponding test functions in `golden_tests.rs`.
- [ ] **TC-3c:** Run `cargo test` to verify.

**Effort:** Low.

---

## TC-4: Go Missing Test Functions

**Problem:** `tests/golden/go/boolean_ops.json` and `tests/golden/go/methods.json`
exist but there are no corresponding test functions in `golden_tests.rs`.

**Tasks:**
- [ ] **TC-4a:** Add `test_go_golden_boolean_ops` and `test_go_golden_methods` to
  `golden_tests.rs` following the existing `test_go_golden_simple` pattern.
- [ ] **TC-4b:** Run `cargo test` to verify both pass against the existing golden files.

**Effort:** Very low. Two function additions in one file.

---

## TC-5: Rust Missing Test Functions

**Problem:** `tests/golden/rust/boolean_ops.json` and `tests/golden/rust/methods.json`
exist but there are no corresponding test functions in `golden_tests.rs`.

**Tasks:**
- [ ] **TC-5a:** Add `test_rust_golden_boolean_ops` and `test_rust_golden_methods` to
  `golden_tests.rs` following the existing `test_rust_golden_simple` pattern.
- [ ] **TC-5b:** Run `cargo test` to verify both pass.

**Effort:** Very low.

---

## TC-6: Contributing Documentation Stub

**Problem:** `docs/contributing/index.md` currently contains only placeholder text
(`"This page is being written. Content coming soon."`). It has no actual content —
no build instructions, no test-running guide, no PR conventions.

**Tasks:**
- [ ] **TC-6a:** Write a minimal contributing guide covering:
  - How to build (`cargo build`)
  - How to run tests (`cargo test`)
  - How to add a golden test (pattern from `golden_tests.rs`)
  - How to update a golden JSON when output intentionally changes
  - PR conventions (from `CLAUDE.md`: single-line commit messages, etc.)
- [ ] **TC-6b:** Link to `CLAUDE.md` for code style rules rather than duplicating them.

**Effort:** Low. Content already exists in `CLAUDE.md` and `README.md`; this is
mostly reorganization and cross-linking.

---

## Ordering / Dependencies

```
TC-4 (Go missing tests)     — fastest, no setup needed; do first
TC-5 (Rust missing tests)   — fastest, no setup needed; do in parallel with TC-4
TC-1 (JS golden tests)      — low effort, high value; do next
TC-2 (JSX golden tests)     — parallel with TC-1
TC-3 (TSX golden tests)     — parallel with TC-1
TC-6 (contributing docs)    — independent; low priority
```

TC-4 and TC-5 are the lowest-effort items (2 functions each) and should be done
immediately. TC-1 through TC-3 follow as a batch since they share the same
"generate golden, add test" pattern.

---

**Created:** 2026-02-19
