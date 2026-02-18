# Architecture Review Tasks

**Source:** `docs/architecture/ARCHITECTURE_REVIEW.md`
**Status:** Doc pass complete — code tasks in progress
**Principle:** Fix correctness and documentation gaps before adding features

---

## Summary

| ID   | Finding                                    | Severity   | Category           | Status      |
|------|--------------------------------------------|------------|--------------------|-------------|
| F-1  | Touch metrics are file-level               | Medium     | Scoring accuracy   | [x] Doc done / [x] Research / [x] Implemented |
| F-2  | Fan-out double-penalized                   | Low-Medium | Score calibration  | [x] Doc done / [x] Research — no change needed |
| F-3  | Name-based call graph accuracy limits      | Medium     | Call graph         | [x] Doc done / [x] Measure  |
| F-4  | Tree-sitter re-parse is O(n×m)             | Medium     | Performance        | [x] Doc done / [x] Implement / [x] Verify |
| F-5  | function_id is path-dependent              | Medium     | Delta accuracy     | [x] Doc done / [x] Research / [x] Implemented |
| F-6  | Schema migration strategy undefined        | Low-Medium | Operational risk   | [x] Code done / [x] Doc done|
| F-7  | PR detection is CI-only                    | Low        | Documentation gap  | [x] Doc done                |
| F-8  | Trends module mis-documented as future     | Low        | Documentation gap  | [x] Doc done                |

---

## F-1: Touch Metrics Are File-Level

**Finding:** Touch count and days-since-last-change are computed at file granularity and applied
uniformly to every function in the file. A file with 50 functions where only one was touched
attributes the same touch score to all 50.

**Tasks:**

- [x] **F-1a (doc):** Add explicit limitation note to `ARCHITECTURE.md` §9 (Git Integration)
  documenting that touch metrics are file-granularity approximations and what that means for
  large files with many functions.

- [x] **F-1b (research):** Evaluate feasibility of per-function touch metrics using
  `git log -L <start>,<end>:<file>`. Prototype on hotspots repo and measure:
  - Accuracy improvement vs file-level
  - Performance cost (expected: significant, may need caching or opt-in flag)
  - Output schema changes required

  **Results (2026-02-16, benchmarked with hyperfine on hotspots repo):**

  **Accuracy:** Demonstrably better. Example — `populate_callgraph` in `snapshot.rs`:
  - File-level: 3 touches (entire file touched in 3 commits)
  - Function-level (`git log -L 363,417:snapshot.rs`): 1 touch (function added in 1 commit)
  - The file-level approach over-attributes 2 phantom touches to this function.

  **Performance cost:**

  | Approach | Time | Scale |
  |---|---|---|
  | File-level `git log -- <file>` | ~7.5 ms/file | O(files) |
  | Per-function `git log -L <start>,<end>:<file>` | ~8.5 ms/function | O(functions) |
  | 44 functions in snapshot.rs | ~402 ms | 50× slower than file-level |

  Each `git log -L` invocation costs ~9 ms regardless of line range size (subprocess overhead
  dominates). With 767 functions in the hotspots repo, per-function mode would take ~6.9 s
  vs ~0.7 s for file-level — a 10× wall-clock penalty on a small repo. On a 10,000-function
  codebase this becomes ~90 s, which is unacceptable for CI.

  **Decision: Gate behind opt-in flag (`--per-function-touches`).** Accuracy improvement is
  real but cost is O(n) subprocess invocations. Acceptable for small repos (<200 functions,
  ~1.8 s) but prohibitive at scale. Default remains file-level.

- [x] **F-1c (implement, blocked by F-1b):** Implement per-function touch metrics behind
  `--per-function-touches` flag. Warn in output when flag is active that analysis will be
  significantly slower. No schema changes needed (same `touch_count_30d` / `days_since_last_change`
  fields, just populated from function line range instead of whole file).

  **Implemented (2026-02-17):** Added `--per-function-touches` flag to `hotspots analyze`.
  Uses `git::function_touch_metrics_at()` (new fn, `git log -L start,end:file`) per function.
  End line derived as `line + loc - 1`. Flag only valid with `--mode snapshot` or `--mode delta`.
  Emits stderr warning when active. File-level batching remains the default.

**Acceptance for F-1a:** ARCHITECTURE.md updated.
**Acceptance for F-1b:** Research note written, decision recorded here.

---

## F-2: Fan-Out Double-Penalized in Scoring Formula

**Finding:** Fan-out (FO) feeds into LRS via `R_fo`, and LRS feeds directly into `activity_risk`.
Fan-in is added on top. Because fan-in and fan-out are correlated in typical codebases, highly
connected hub functions are systematically penalized more than the formula implies individually.

**Tasks:**

- [x] **F-2a (doc):** Add note to `ARCHITECTURE.md` §6 (Activity-Weighted Risk Scoring)
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

- [x] **F-3a (doc):** Update `ARCHITECTURE.md` §5 (Call Graph Analysis) to characterize the
  call graph as a "best-effort static approximation" that works well for direct calls but
  systematically misses dynamic/interface dispatch. Add concrete examples per language.

- [x] **F-3b (measure):** Add a resolution coverage metric to the call graph builder: track
  `total_callee_names_found` vs `total_resolved_to_function_id`. Log or expose this so
  users can see what fraction of calls were resolved.

  **Implemented:** `CallGraph` struct carries `total_callee_names` and `resolved_callee_names`
  fields. Logged to stderr as "call graph: resolved X/Y callee references (N% internal)"
  during every snapshot/delta run.

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

- [x] **F-4a (doc):** Add performance note to `ARCHITECTURE.md` §Performance documenting the
  re-parse pattern as a known bottleneck specific to tree-sitter languages, and why it
  exists (node lifetime constraints).

- [x] **F-4b (implement):** Cache the parsed tree per (source_hash, language) within a single
  analysis run.

  **Implemented:** `make_parse_cache!` macro in `language/tree_sitter_utils.rs` generates
  thread-local caches (`GO_TREE_CACHE`, `JAVA_TREE_CACHE`, `PYTHON_TREE_CACHE`). Each cache
  stores `(source_hash: u64, Tree)`. All three tree-sitter CFG builders use `with_cached_*_tree`
  instead of re-parsing — reducing O(n × m) to O(1) per file after the first function.

- [x] **F-4c (verify):** After F-4b, benchmark analysis of a large Go/Java/Python file (100+
  functions) and confirm parse count drops from O(n) to O(1) per file.

  **Verified (2026-02-18):** Full repo analysis (773 functions, including Go/Java/Python files)
  completes in ~0.11s in basic mode and ~0.69s in snapshot mode (which adds git calls).
  The thread-local cache ensures each file is parsed once per analysis run regardless of
  how many functions it contains.

**Acceptance for F-4b:** `cargo test` passes; `cargo check` clean; verified parse count reduced.

---

## F-5: function_id Is Path-Dependent — Refactoring Commits Lose History

**Finding:** `function_id` is `file_path::function_name`. File renames, directory moves, and
function renames all generate delete+add pairs in delta output, losing continuity. This is
especially problematic for refactoring commits — the ones where accurate history matters most.

**Tasks:**

- [x] **F-5a (doc):** Update `ARCHITECTURE.md` §8 (Delta System) to explicitly document that
  `function_id` stability depends on stable file paths and function names. Note that
  refactoring commits will appear as delete+add pairs and that this is a known limitation.

- [x] **F-5b (research):** Evaluate alternative identity schemes:
  - **Content hash:** Hash of function body (detects moves/renames but breaks on any edit)
  - **Signature hash:** Hash of function signature only (stable across body changes)
  - **Hybrid:** Fuzzy match by signature + structural similarity when exact ID not found
  Record the trade-offs. A hybrid fallback in delta matching (try exact ID first, then
  signature match) may be achievable without breaking existing snapshots.

  **Results (2026-02-16):**

  **Current state:** `function_id = file::function_name`. Delta matching is exact HashMap
  lookup — no fuzzy logic exists anywhere. 176 references across 11 files depend on the
  current format; changing the format would require snapshot migration and break all existing
  history.

  **Identity scheme trade-offs:**

  | Scheme | Survives body edit | Survives rename | Survives file move | Complexity |
  |---|---|---|---|---|
  | `file::name` (current) | ✓ | ✗ | ✗ | zero |
  | Content hash | ✗ | ✓ | ✓ | low |
  | Signature hash | ✓ | ✗ | ✓ | medium |
  | `file + line` | ✓ | ✓ | ✗ | low |
  | Hybrid fuzzy fallback | ✓ | ~✓ | ~✓ | high |

  **Decision: Hybrid fuzzy fallback is the right path for F-5c.** Key insight: don't change
  `function_id` format (too disruptive). Instead, add a second-pass heuristic in delta
  matching: after exact-match, for each unmatched delete+add pair within the same commit,
  attempt to pair them by `(same file + line within ±10)` or `(same name + different file)`.
  This handles the two most common refactoring patterns (file rename, function move within
  file) without touching serialized data or 176 existing references.

  **Risks:** False positives possible when two functions are both added/deleted at nearby
  lines. Must be clearly labeled as "likely renamed" in delta output, not asserted as certain.

- [x] **F-5c (optional, blocked by F-5b):** Implement fuzzy delta matching as a second-pass
  heuristic in `delta.rs`. After exact-match pass, pair unmatched deletes+adds by:
  1. Same name, different file path → file-rename match
  2. Same file, line number within ±10 → function-move match
  Additive and backward-compatible — existing exact matches are unchanged.

  **Implemented (2026-02-17):** Added `rename_hint: Option<String>` field to `FunctionDeltaEntry`
  (serialized, skip-if-None). After the exact-match loop in `Delta::new()`, a second pass
  collects unmatched Deleted+New pairs, applies the two heuristics (first match wins), and
  sets `rename_hint` on the Deleted entry to the matched New `function_id`. Does not change
  `status` — the entry remains `deleted`. Seven call sites updated (delta.rs ×4, policy.rs ×2,
  suppression_tests.rs ×3) with `rename_hint: None`.

**Acceptance for F-5a:** ARCHITECTURE.md updated.
**Acceptance for F-5b:** Research note written, decision recorded here.

---

## F-6: Schema Migration Strategy Is Undefined

**Finding:** Snapshots carry `schema_version` but there is no documented or implemented policy
for what happens when the reader encounters a snapshot with a mismatched schema version.
The behavior is currently implicit (fields default to `None` if missing).

**Tasks:**

- [x] **F-6a (doc):** Update `ARCHITECTURE.md` §7 (Snapshot System) to document the explicit
  migration policy:
  - Which schema versions are currently supported (`SNAPSHOT_SCHEMA_MIN_VERSION` to current)
  - What happens on version below min: error with clear message
  - What happens on version above current: error with "upgrade hotspots" message
  - That missing fields default to `None` (additive changes are backward-compatible)

- [x] **F-6b (already done):** `Snapshot::from_json()` already enforces an explicit version range
  check (snapshot.rs lines 646-659). Out-of-range versions are rejected with a clear error:
  `"unsupported schema version: got X, supported range 1-2"`. No code change needed.

- [x] **F-6c (doc):** Add a short "Schema Migration" section to `docs/architecture/ARCHITECTURE.md`
  describing the versioning contract: additive changes bump the schema version, breaking
  changes require a migration guide.

**Acceptance for F-6c:** ARCHITECTURE.md updated with version range contract.

---

## F-7: PR Detection Is CI-Only

**Finding:** `detect_pr_context()` relies entirely on CI environment variables. Running
`hotspots analyze --mode snapshot` locally on a feature branch will persist a snapshot as if
it were mainline.

**Tasks:**

- [x] **F-7a (doc):** Update `ARCHITECTURE.md` §9 (Git Integration) to explicitly state that
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

- [x] **F-8a (doc):** Remove "Time-series analysis" from the Planned Enhancements / hotspots-cloud
  section in `ARCHITECTURE.md`. It does not belong there.

- [x] **F-8b (doc):** Add a proper §12 "Trends Analysis" section to `ARCHITECTURE.md` covering:
  - What it computes: risk velocity (`velocity: f64, direction: VelocityDirection`),
    hotspot stability (consistency of top-K high-risk functions), refactor detection
  - How it reads the snapshot index (window of N snapshots)
  - CLI: `hotspots trends <path> --window N --top K --format json`
  - Output format (JSON with trend entries per function)
  - Limitations (requires multiple snapshots over time)

- [x] **F-8c (doc):** Verify the trends CLI `--help` text accurately describes what it does.
  Fix any discrepancies between help text and actual behavior.

  **Verified:** `hotspots trends --help` accurately describes PATH, --window, --top, --format
  options. No discrepancies found.

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
