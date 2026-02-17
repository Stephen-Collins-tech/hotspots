# Architecture Review Tasks

**Source:** `docs/architecture/ARCHITECTURE_REVIEW.md`
**Status:** Doc pass complete — code tasks in progress
**Principle:** Fix correctness and documentation gaps before adding features

---

## Summary

| ID   | Finding                                    | Severity   | Category           | Status      |
|------|--------------------------------------------|------------|--------------------|-------------|
| F-1  | Touch metrics are file-level               | Medium     | Scoring accuracy   | [x] Doc done / [ ] Research |
| F-2  | Fan-out double-penalized                   | Low-Medium | Score calibration  | [x] Doc done / [x] Research — no change needed |
| F-3  | Name-based call graph accuracy limits      | Medium     | Call graph         | [x] Doc done / [x] Measure  |
| F-4  | Tree-sitter re-parse is O(n×m)             | Medium     | Performance        | [x] Doc done / [x] Implement|
| F-5  | function_id is path-dependent              | Medium     | Delta accuracy     | [x] Doc done / [ ] Research |
| F-6  | Schema migration strategy undefined        | Low-Medium | Operational risk   | [x] Code done / [x] Doc done|
| F-7  | PR detection is CI-only                    | Low        | Documentation gap  | [x] Doc done                |
| F-8  | Trends module mis-documented as future     | Low        | Documentation gap  | [x] Doc done                |

---

## F-1: Touch Metrics Are File-Level

**Finding:** Touch count and days-since-last-change are computed at file granularity and applied
uniformly to every function in the file. A file with 50 functions where only one was touched
attributes the same touch score to all 50.

**Tasks:**

- [ ] **F-1a (doc):** Add explicit limitation note to `ARCHITECTURE.md` §9 (Git Integration)
  documenting that touch metrics are file-granularity approximations and what that means for
  large files with many functions.

- [ ] **F-1b (research):** Evaluate feasibility of per-function touch metrics using
  `git log -L <start>,<end>:<file>`. Prototype on hotspots repo and measure:
  - Accuracy improvement vs file-level
  - Performance cost (expected: significant, may need caching or opt-in flag)
  - Output schema changes required

- [ ] **F-1c (implement, blocked by F-1b):** If F-1b shows acceptable cost, implement
  function-level touch metrics. Gate behind a flag if expensive.

**Acceptance for F-1a:** ARCHITECTURE.md updated.
**Acceptance for F-1b:** Research note written, decision recorded here.

---

## F-2: Fan-Out Double-Penalized in Scoring Formula

**Finding:** Fan-out (FO) feeds into LRS via `R_fo`, and LRS feeds directly into `activity_risk`.
Fan-in is added on top. Because fan-in and fan-out are correlated in typical codebases, highly
connected hub functions are systematically penalized more than the formula implies individually.

**Tasks:**

- [ ] **F-2a (doc):** Add note to `ARCHITECTURE.md` §6 (Activity-Weighted Risk Scoring)
  documenting the fan-in/fan-out correlation and its effect on hub function scores.

- [x] **F-2b (research):** On the hotspots self-analysis output, identify the top 10 functions
  by fan-in+fan-out. Check whether their activity_risk scores seem calibrated or inflated
  relative to their actual maintenance burden. Record findings.

  **Results (2026-02-16, hotspots self-analysis):**

  | Function | fan_in | fan_out | hub_score | cc | lrs | activity_risk | band |
  |---|---|---|---|---|---|---|---|
  | render_json | 14 | 0 | 14 | 3 | 3.30 | 5.51 | moderate |
  | test_golden | 6 | 6 | 12 | 3 | 6.00 | 7.57 | high |
  | test_python_golden | 7 | 5 | 12 | 3 | 6.00 | 7.65 | high |
  | test_go_golden | 5 | 5 | 10 | 3 | 6.00 | 7.49 | high |
  | golden_path | 9 | 0 | 9 | 3 | 2.60 | 4.41 | low |
  | read_golden | 8 | 1 | 9 | 3 | 3.65 | 5.38 | moderate |
  | test_java_golden | 4 | 5 | 9 | 3 | 6.09 | 7.50 | high |
  | test_rust_golden | 4 | 5 | 9 | 3 | 6.00 | 7.41 | high |
  | git_command | 8 | 0 | 8 | 4 | 5.02 | 6.75 | moderate |
  | normalize_paths | 8 | 0 | 8 | 12 | 7.05 | 8.78 | high |

  **Findings:**
  - Maximum hub_score in this codebase is 14 — a modest scale that limits the multiplicative
    effect of the formula.
  - 8 of 10 top hubs are test infrastructure (`golden_tests.rs`, `git_history_tests.rs`),
    not production code.
  - Scores appear **well-calibrated**: simple high-fan-in functions (`render_json` cc=3 → moderate;
    `golden_path` cc=3 → low) score appropriately low. Complex high-fan-in functions
    (`normalize_paths` cc=12 → high) score higher, as expected.
  - No hub function was pushed into CRITICAL by fan-in/fan-out alone.
  - The fan-in boost is proportional: `render_json` (fi=14) → moderate vs `normalize_paths`
    (fi=8, cc=12) → high. The cc=3 cap on simple functions keeps scores in check.

  **Decision: No code change needed (F-2c not warranted).** The double-penalty is theoretically
  present but does not produce visible over-scoring at this codebase's connectivity scale.
  The concern is more relevant for very large codebases where hub functions routinely have
  fan_in+fan_out > 50. Document as a known formula property but do not adjust weights.

- [ ] **F-2c (optional):** ~~If F-2b shows systematic over-scoring of hubs, consider reducing
  `fan_in_factor` weight or adding a normalization term.~~ **Skipped — F-2b showed no
  systematic over-scoring at this scale.**

**Acceptance for F-2a:** ARCHITECTURE.md updated.
**Acceptance for F-2b:** Research note written with examples.

---

## F-3: Name-Based Call Graph Has Significant Accuracy Limits

**Finding:** Call graph edges are resolved by matching callee names via "prefer same-file, fall
back to first match." This cannot resolve interface dispatch, higher-order functions, closures,
or virtual methods. For Go/Java/Python with idiomatic interface usage, many edges are
unresolved or wrong.

**Tasks:**

- [ ] **F-3a (doc):** Update `ARCHITECTURE.md` §5 (Call Graph Analysis) to characterize the
  call graph as a "best-effort static approximation" that works well for direct calls but
  systematically misses dynamic/interface dispatch. Add concrete examples per language.

- [ ] **F-3b (measure):** Add a resolution coverage metric to the call graph builder: track
  `total_callee_names_found` vs `total_resolved_to_function_id`. Log or expose this so
  users can see what fraction of calls were resolved.

- [ ] **F-3c (research):** Evaluate whether type information available in the AST (e.g., SWC's
  type-aware mode, tree-sitter queries for Java's type resolver) could improve resolution
  accuracy without a major rewrite. Document findings and estimated effort.

**Acceptance for F-3a:** ARCHITECTURE.md updated.
**Acceptance for F-3b:** `cargo check` passes; coverage stat visible in `--format json` output or logs.

---

## F-4: Tree-Sitter Re-Parse Per Function Is O(n×m)

**Finding:** Go, Java, and Python CFG builders call `parser.parse(source, None)` for every
function in a file. A file with 30 functions is parsed 30 times. This is the primary performance
bottleneck for those languages on large files.

**Tasks:**

- [ ] **F-4a (doc):** Add performance note to `ARCHITECTURE.md` §1 (Language Abstraction) and
  §Performance documenting the re-parse pattern as a known bottleneck specific to tree-sitter
  languages, and why it exists (node lifetime constraints).

- [ ] **F-4b (implement):** Cache the parsed tree per (source_hash, language) within a single
  analysis run. The cached tree can be looked up by the CFG builder before re-parsing.
  Requires careful handling of tree-sitter lifetimes (likely: cache `Arc<Tree>` + source `Arc<String>`).

  Files affected:
  - `hotspots-core/src/language/go/cfg_builder.rs`
  - `hotspots-core/src/language/java/cfg_builder.rs`
  - `hotspots-core/src/language/python/cfg_builder.rs`
  - Possibly a new `ParseCache` in `language/mod.rs`

- [ ] **F-4c (verify):** After F-4b, benchmark analysis of a large Go/Java/Python file (100+
  functions) and confirm parse count drops from O(n) to O(1) per file.

**Acceptance for F-4b:** `cargo test` passes; `cargo check` clean; verified parse count reduced.

---

## F-5: function_id Is Path-Dependent — Refactoring Commits Lose History

**Finding:** `function_id` is `file_path::function_name`. File renames, directory moves, and
function renames all generate delete+add pairs in delta output, losing continuity. This is
especially problematic for refactoring commits — the ones where accurate history matters most.

**Tasks:**

- [ ] **F-5a (doc):** Update `ARCHITECTURE.md` §8 (Delta System) to explicitly document that
  `function_id` stability depends on stable file paths and function names. Note that
  refactoring commits will appear as delete+add pairs and that this is a known limitation.

- [ ] **F-5b (research):** Evaluate alternative identity schemes:
  - **Content hash:** Hash of function body (detects moves/renames but breaks on any edit)
  - **Signature hash:** Hash of function signature only (stable across body changes)
  - **Hybrid:** Fuzzy match by signature + structural similarity when exact ID not found
  Record the trade-offs. A hybrid fallback in delta matching (try exact ID first, then
  signature match) may be achievable without breaking existing snapshots.

- [ ] **F-5c (optional, blocked by F-5b):** If research shows a viable path, implement
  fuzzy delta matching as a fallback for unmatched deletes/adds. This is additive and
  backward-compatible if done as a heuristic on top of the existing exact match.

**Acceptance for F-5a:** ARCHITECTURE.md updated.
**Acceptance for F-5b:** Research note written, decision recorded here.

---

## F-6: Schema Migration Strategy Is Undefined

**Finding:** Snapshots carry `schema_version` but there is no documented or implemented policy
for what happens when the reader encounters a snapshot with a mismatched schema version.
The behavior is currently implicit (fields default to `None` if missing).

**Tasks:**

- [ ] **F-6a (doc):** Update `ARCHITECTURE.md` §7 (Snapshot System) to document the explicit
  migration policy:
  - Which schema versions are currently supported (`SNAPSHOT_SCHEMA_MIN_VERSION` to current)
  - What happens on version below min: error with clear message
  - What happens on version above current: error with "upgrade hotspots" message
  - That missing fields default to `None` (additive changes are backward-compatible)

- [x] **F-6b (already done):** `Snapshot::from_json()` already enforces an explicit version range
  check (snapshot.rs lines 646-659). Out-of-range versions are rejected with a clear error:
  `"unsupported schema version: got X, supported range 1-2"`. No code change needed.

- [ ] **F-6c (doc):** Add a short "Schema Migration" section to `docs/architecture/ARCHITECTURE.md`
  describing the versioning contract: additive changes bump the schema version, breaking
  changes require a migration guide.

**Acceptance for F-6c:** ARCHITECTURE.md updated with version range contract.

---

## F-7: PR Detection Is CI-Only

**Finding:** `detect_pr_context()` relies entirely on CI environment variables. Running
`hotspots analyze --mode snapshot` locally on a feature branch will persist a snapshot as if
it were mainline.

**Tasks:**

- [ ] **F-7a (doc):** Update `ARCHITECTURE.md` §9 (Git Integration) to explicitly state that
  PR detection works only in CI environments that set standard PR env vars, and that local
  snapshot mode on any branch (including feature branches) will persist a snapshot.

- [ ] **F-7b (optional):** Consider adding a `--no-persist` flag as an escape hatch for users
  who want to run snapshot-mode analysis locally without writing to `.hotspots/snapshots/`.
  (Note: `--force` flag already exists for the opposite case.)

**Acceptance for F-7a:** ARCHITECTURE.md updated.

---

## F-8: Trends Module Is Undocumented — and Mis-documented

**Finding:** `hotspots trends` is fully implemented and CLI-wired, but `ARCHITECTURE.md` §13
lists "Time-series analysis" under **Planned Enhancements → hotspots-cloud**. This is actively
misleading — trends analysis is a current CLI feature, not a future cloud feature.

**Tasks:**

- [ ] **F-8a (doc):** Remove "Time-series analysis" from the Planned Enhancements / hotspots-cloud
  section in `ARCHITECTURE.md`. It does not belong there.

- [ ] **F-8b (doc):** Add a proper §12 "Trends Analysis" section to `ARCHITECTURE.md` covering:
  - What it computes: risk velocity (`velocity: f64, direction: VelocityDirection`),
    hotspot stability (consistency of top-K high-risk functions), refactor detection
  - How it reads the snapshot index (window of N snapshots)
  - CLI: `hotspots trends <path> --window N --top K --format json`
  - Output format (JSON with trend entries per function)
  - Limitations (requires multiple snapshots over time)

- [ ] **F-8c (doc):** Verify the trends CLI `--help` text accurately describes what it does.
  Fix any discrepancies between help text and actual behavior.

**Acceptance for F-8b:** ARCHITECTURE.md updated with accurate trends section; cross-checked against `trends.rs`.

---

## Ordering / Dependencies

```
F-1a → F-1b → F-1c        (doc first, then research, then implement if warranted)
F-2a → F-2b → F-2c        (doc first, then calibration research)
F-3a → F-3b → F-3c        (doc first, then measure, then research)
F-4a → F-4b → F-4c        (doc first, then implement, then verify)
F-5a → F-5b → F-5c        (doc first, then research, then optional implement)
F-6a → F-6b, F-6c         (doc and impl can proceed together)
F-7a (→ F-7b optional)
F-8a → F-8b
```

Pure documentation tasks (F-1a, F-2a, F-3a, F-4a, F-5a, F-6a/c, F-7a, F-8a/b) can all be
done in a single pass as they only touch `ARCHITECTURE.md`.

---

## Suggested Order of Attack

1. **Doc pass** — Do all `(doc)` sub-tasks in one sitting: update ARCHITECTURE.md with all 8
   findings as documented limitations. Low risk, high value.

2. **F-4b** — Tree-sitter parse cache. Highest code impact per effort. Correctness-safe.

3. **F-6b** — Schema version check. Small, well-contained, adds operational safety.

4. **F-3b** — Call graph resolution coverage metric. Non-breaking, informs F-3c.

5. **F-1b, F-2b, F-5b** — Research tasks. Inform whether code changes are warranted.

6. **F-1c, F-2c, F-3c, F-5c** — Implementation tasks, only if research shows clear value.

---

**Created:** 2026-02-16
**Source findings:** `docs/architecture/ARCHITECTURE_REVIEW.md`
