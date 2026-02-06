# TASKS.md - Hotspots CI/CD Adoption

**Goal:** Make Hotspots the go-to CI/CD tool for blocking complexity regressions in TypeScript/JavaScript projects.

**Strategy:** CI/CD first. Analytics later.

**Status:** Post-MVP - building toward v1.0 CI/CD-ready release

## Progress Summary

**Phase 1: Language Completeness**
- ✅ Task 1.1: JavaScript Support (COMPLETED 2026-01-28)
- ✅ Task 1.2: JSX/TSX Support (COMPLETED 2026-01-28)
- ✅ Task 1.3: Fix Break/Continue CFG Routing (COMPLETED 2026-02-02)

**Phase 2: CI/CD Integration**
- ✅ Task 2.1: GitHub Action (Core) (COMPLETED 2026-02-04)
- ✅ Task 2.2: Proactive Warning System (COMPLETED 2026-02-03)
- ✅ Task 2.3: HTML Report Generation (COMPLETED 2026-02-03)
- ⏳ Task 2.4: GitHub PR Annotations

**Phase 3: Configuration & Policies**
- ✅ Task 3.1: Configuration File (COMPLETED 2026-02-02)
- ✅ Task 3.2: Suppression Comments (COMPLETED 2026-02-03)

**Phase 7: AI-First Integration** (PRIORITY - v1.0.0 Blocker)
- ✅ Task 7.1: Fix Clippy Errors (COMPLETED 2026-02-06)
- ⏳ Task 7.2: JSON Schema & Types
- ⏳ Task 7.3: Claude MCP Server
- ⏳ Task 7.4: AI Integration Documentation
- ⏳ Task 7.5: Reference Implementation Examples

**Overall Progress:** 9/30 tasks completed (30%)

**Latest Update:** 2026-02-06 - Completed Task 7.1: Fixed all clippy warnings (17 errors resolved)

---

## Reference Documents

- **Roadmap:** `docs/roadmap.md` - Full feature list with prioritization
- **MVP History:** `docs/mvp-implementation-history.md` - Completed MVP tasks
- **Design Decisions:** `docs/design-decisions.md` - Architectural choices

---

## Guiding Principles

1. **Fast feedback** - Developers shouldn't wait for analysis
2. **Actionable warnings** - Tell them exactly what to fix and why
3. **Proactive alerts** - Warn before things become problems, not after
4. **Zero friction** - One line in GitHub Actions, works immediately
5. **Deterministic** - Never flaky, always reproducible
6. **Trustworthy** - Clear rationale, no magic thresholds

---

## Task Phases

```
Phase 1: Language Completeness (P0 blockers)
Phase 2: CI/CD Integration (GitHub-first)
Phase 3: Configuration & Policies (Customization)
Phase 4: Performance (Scale to large repos)
Phase 5: Developer Experience (IDE, reports, errors)
Phase 6: Polish & Documentation
```

---

## Phase 1: Language Completeness

### 1.1 JavaScript Support ✅

**Priority:** P0 (Critical blocker)

**Status:** ✅ **COMPLETED** (2026-01-28)

**Problem:** Most projects mix TypeScript and JavaScript. Without JS support, Hotspots is incomplete.

**Tasks:**

- [x] Enable JavaScript parsing in SWC parser configuration
  - Support ES2015+ syntax
  - CommonJS and ESM modules
  - No JSX (separate task)
- [x] Test that all metrics work identically for JS and TS
  - CC, ND, FO, NS should behave the same
- [x] Add JS golden test fixtures
  - `tests/fixtures/js/` directory
  - Mirror TS fixtures in plain JS
- [x] Update documentation
  - `docs/ts-support.md` → `docs/language-support.md`
  - List supported JS features
- [x] Add file extension handling
  - `.js`, `.mjs`, `.cjs` alongside `.ts`, `.mts`, `.cts`

**Acceptance:**
- ✅ Analyze a mixed TS/JS project successfully
- ✅ JS and TS functions with identical structure yield identical LRS
- ✅ All existing tests still pass (determinism preserved)

**Actual effort:** ~4 hours (faster than estimated due to SWC built-in support)

**Commit:** d6be126

---

### 1.2 JSX/TSX Support ✅

**Priority:** P1 (High - needed for React adoption)

**Status:** ✅ **COMPLETED** (2026-01-28)

**Dependencies:** JavaScript support (1.1) ✅

**Tasks:**

- [x] Enable JSX parsing in SWC configuration
  - JSX in `.jsx`, `.tsx` files
  - JSX pragmas (`@jsx`, `@jsxFrag`)
- [x] Decide JSX metric handling
  - JSX elements should NOT inflate complexity artificially
  - JSX expressions containing control flow (ternaries, &&) DO count
  - Document rationale
- [x] Add JSX golden test fixtures
  - React component with conditional rendering
  - Component with map/loops
  - Complex nested JSX
- [x] Update documentation with JSX support status

**Acceptance:**
- ✅ Analyze a React TypeScript project successfully
- ✅ JSX elements don't create false complexity signals
- ✅ Conditional rendering and loops are measured correctly

**Actual effort:** ~1 hour (SWC already had JSX support)

**Commit:** d6be126

---

### 1.3 Fix Break/Continue CFG Routing ✅

**Priority:** P0 (Correctness issue)

**Status:** ✅ **COMPLETED** (2026-02-02)

**Problem:** Break/continue currently route to CFG exit instead of loop exit/header. This slightly inflates CC for loops.

**Tasks:**

- [x] Add loop context tracking to CFG builder
  - `BreakableContext` struct with break_target, continue_target, label
  - `breakable_stack: Vec<BreakableContext>` on CfgBuilder
  - Push on loop/switch entry, pop on exit
- [x] Route `break` to loop/switch join node (innermost breakable context)
- [x] Route `continue` to loop header node (innermost loop context)
- [x] Handle labeled break/continue
  - Resolve label to correct context in stack
  - `pending_label` field consumed by loop/switch visitors
  - Fallback to exit if no matching context (shouldn't happen in valid code)
- [x] Add comprehensive loop tests (7 new tests)
  - Break routes to loop join
  - Continue routes to loop header
  - Labeled break across multiple levels
  - Labeled continue to outer loop
  - Switch break routes to switch join
  - Nested loop break targets inner
  - For-of with break and continue
- [x] Update golden fixtures to reflect corrected CC
  - pathological.ts: CC 23→20 (3 switch breaks no longer inflate edges)

**Acceptance:**
- ✅ Break routes to loop/switch join, not CFG exit
- ✅ Continue routes to loop header
- ✅ CC values are accurate for complex loops
- ✅ Labeled break/continue work correctly
- ✅ All 86 tests pass (59 unit + 27 integration)

**Actual effort:** ~2 hours

---

## Phase 2: CI/CD Integration

### 2.1 GitHub Action (Core) ✅

**Priority:** P0 (Critical for adoption)

**Status:** ✅ **COMPLETED** (2026-02-04)

**Tasks:**

- [x] Create action in monorepo (`action/` directory)
  - TypeScript implementation with `@actions/*` packages
  - Bundled with `@vercel/ncc` for distribution
- [x] Implement action inputs
  - `path`, `policy`, `min-lrs`, `config`, `fail-on`
  - `version`, `github-token`, `post-comment`
- [x] Implement action outputs
  - `violations`, `passed`, `summary`, `report-path`
- [x] Add binary caching
  - Uses `@actions/tool-cache` for version-based caching
  - Downloads from GitHub releases
  - Fallback to building from source (for development)
- [x] Handle PR context automatically
  - Detects PR vs push via `github.context.eventName`
  - Extracts merge-base from PR payload
  - Runs delta mode for PRs, snapshot for mainline
- [x] Basic PR comment posting
  - Posts markdown summary to PR
  - Updates existing comment (no spam)
  - Links to HTML report artifact
- [x] Job summary output
  - Uses `GITHUB_STEP_SUMMARY`
  - Markdown tables with violations by severity
  - Pass/fail status

**Acceptance:**
- ✅ Action can be used with `uses: ./action` or `yourorg/hotspots@v1`
- ✅ Automatically detects PR vs mainline
- ✅ Posts results to PR comments
- ✅ Job summary shows violations
- ✅ Passes/fails based on policy
- ✅ HTML reports generated and available as artifacts

**Implementation details:**
- `action/src/main.ts`: Main action entry point (500+ lines)
- `action/action.yml`: Action metadata with inputs/outputs
- `.github/workflows/test-action.yml`: Test workflow for action
- `.github/workflows/release.yml`: Automated release builds for all platforms
- `action/README.md`: Complete documentation and examples
- `RELEASE_PROCESS.md`: Release automation and binary distribution guide

**Actual effort:** ~2 hours (initial implementation)

**Note:** Binary releases need to be created using the release workflow. Test workflow validates action functionality using local build.

---

### 2.2 Proactive Warning System

**Priority:** P0 (Key differentiator for CI/CD)

**Status:** ✅ **COMPLETED** (2026-02-03)

**Concept:** Warn developers *before* functions become problems, giving them time to plan refactoring rather than being surprised by blocking failures.

**Warning Levels:**

1. **Watch** (Info) - Function approaching moderate threshold
2. **Attention** (Warning) - Function approaching high threshold or showing rapid growth
3. **Action Required** (Error) - Function exceeds critical threshold (blocking)

**Tasks:**

- [ ] Design warning thresholds
  - Watch: LRS 2.5-3.0 (approaching moderate)
  - Attention: LRS 5.5-6.0 (approaching high) OR LRS increased by >50% in one commit
  - Action Required: LRS ≥9.0 (critical)
- [ ] Extend policy engine with warning levels
  - `warn_approaching_moderate`: boolean
  - `warn_approaching_high`: boolean
  - `warn_rapid_growth`: boolean (LRS delta >50%)
  - `warn_velocity_threshold`: float (default: 0.5 LRS increase per commit averaged over 5 commits)
- [ ] Add trend detection to delta mode
  - Compare current commit vs 5-commit rolling average
  - Flag functions with sustained upward trend
  - Calculate "time to critical" estimate (linear projection)
- [ ] Update report output to include warnings
  - Warning level (watch/attention/action)
  - Reason (approaching threshold, rapid growth, etc.)
  - Recommendation (refactor now, schedule refactor, etc.)
- [ ] Add warning suppression
  - `// hotspots-watch-ok: reason` comment
  - Suppress watch/attention warnings for specific functions
  - Action Required cannot be suppressed (by design)
- [ ] Update GitHub Action to show warnings
  - Separate sections in PR comment for each level
  - Warnings don't fail the build by default
  - Configurable: `fail-on: action-required|attention|watch`

**Example Output:**

```
⚠️ Attention (2 functions)
- handleUserRequest (src/api.ts:45) - LRS 5.8 (approaching high threshold of 6.0)
  Recommendation: Consider extracting validation logic

- processPayment (src/payment.ts:120) - LRS increased 4.2 → 6.4 (+52% in one commit)
  Recommendation: Review recent changes for unnecessary complexity

👀 Watch (3 functions)
- parseInput (src/parser.ts:78) - LRS 2.7 (approaching moderate threshold of 3.0)
- formatOutput (src/formatter.ts:34) - LRS 2.9
- validateSchema (src/validation.ts:56) - LRS 2.8
```

**Acceptance:**
- ✅ Three warning levels implemented: Watch (2.5-3.0), Attention (5.5-6.0), Rapid Growth (≥50% increase)
- ✅ Policy engine extended with new warning evaluation functions
- ✅ CLI output groups warnings by level with detailed tables
- ✅ Warning thresholds configurable via config file
- ✅ All 131 tests pass

**Implementation details:**
- Extended `PolicyId` enum with `WatchThreshold`, `AttentionThreshold`, `RapidGrowth`
- Added warning threshold configuration in `config.rs`
- Added evaluation functions in `policy.rs`
- Enhanced CLI output formatting for warnings

**Commit:** 435fd9a

**Note:** Warning suppression (Task 2.2.5) deferred to Task 3.2 (Suppression Comments)

---

### 2.3 HTML Report Generation

**Priority:** P0 (Better UX than JSON)

**Status:** ✅ **COMPLETED** (2026-02-03)

**Tasks:**

- [ ] Create HTML template
  - Responsive design (mobile-friendly)
  - Sortable table (click column headers)
  - Filterable (by file, risk band, warning level)
  - Color-coded risk bands
- [ ] Add delta view for PRs
  - Side-by-side before/after for modified functions
  - Highlight changes (LRS, metrics, risk band)
  - Show "direction" arrows (↑ worse, ↓ better, → unchanged)
- [ ] Add syntax-highlighted code snippets
  - Expandable function bodies
  - Use highlight.js or similar
  - Show surrounding context (5 lines before/after)
- [ ] Generate `hotspots-report.html` artifact
  - Write HTML to `.hotspots/report.html` or `hotspots-report.html`
  - Include inline CSS/JS (no external dependencies)
  - Self-contained (can be opened offline)
- [ ] Update GitHub Action to upload artifact
  - Use `actions/upload-artifact`
  - Link to artifact in PR comment
- [ ] Add charts (optional enhancement)
  - Histogram of LRS distribution
  - Pie chart of risk bands
  - Use lightweight charting library (Chart.js)

**Acceptance:**
- ✅ Running `hotspots analyze --mode snapshot --format html` generates HTML report
- ✅ Running `hotspots analyze --mode delta --format html` generates delta report
- ✅ Report is interactive (sorting and filtering with vanilla JavaScript)
- ✅ Report is self-contained (embedded CSS/JS, works offline)
- ✅ Report is responsive (mobile-friendly with dark mode support)
- ✅ Report is deterministic (byte-for-byte reproducible)
- ✅ Both snapshot and delta modes supported with policy violations
- ✅ All 131 tests pass

**Implementation details:**
- Created `hotspots-core/src/html.rs` module with render functions
- Added `Html` variant to `OutputFormat` enum
- Added `--output` flag for custom HTML output path
- Implemented `write_html_report()` with atomic write pattern
- Hand-crafted HTML with embedded CSS/JS (no template engine)
- Risk band color scheme: Low(#22c55e), Moderate(#eab308), High(#f97316), Critical(#ef4444)

**Commit:** 57442a5

**Note:** Syntax-highlighted code snippets and charts deferred as post-MVP enhancements

---

### 2.4 GitHub PR Annotations

**Priority:** P1 (In-context feedback)

**Tasks:**

- [ ] Implement GitHub Check Runs API integration
  - Create check run on PR
  - Add annotations to changed lines
- [ ] Annotate only changed functions
  - Compare git diff to function locations
  - Only annotate functions in PR diff
- [ ] Format annotations
  - Title: "High Risk Function: functionName"
  - Message: "LRS: 7.2 (High) - CC: 5, ND: 3, FO: 4, NS: 2"
  - Level: warning or failure based on policy
- [ ] Group annotations by file
  - Max 10 annotations per file (GitHub limit)
  - Show "X more violations" if >10
- [ ] Add suggestion for top violations
  - Use GitHub's suggestion syntax
  - Suggest adding `// hotspots-ignore` comment
  - Or link to refactoring guide

**Acceptance:**
- PR diff shows inline annotations for high-risk functions
- Annotations link to specific lines
- Developers see feedback in context of their changes

**Estimated effort:** High (5-7 days)

---

## Phase 3: Configuration & Policies

### 3.1 Configuration File ✅

**Priority:** P0 (Required for project-specific policies)

**Status:** ✅ **COMPLETED** (2026-02-02)

**Tasks:**

- [x] Design config schema
  - JSON format with `serde(deny_unknown_fields)` for strict validation
  - Fields: include, exclude, thresholds, weights, min_lrs, top
- [x] Implement config file loading
  - Search order: CLI `--config` flag, `.hotspotsrc.json`, `hotspots.config.json`, `package.json:hotspots`
  - `load_and_resolve()` main entry point
  - CLI flags (`--min-lrs`, `--top`) override config file values
- [x] Add config validation
  - Schema validation via `serde(deny_unknown_fields)` (rejects unknown fields)
  - Range validation (thresholds ordered and positive, weights 0-10)
  - Clear error messages for invalid config
- [x] Support `include`/`exclude` patterns
  - Use glob patterns via `globset` crate
  - Default exclude: `**/*.test.ts`, `**/*.spec.ts`, `**/*.test.js`, `**/*.spec.js`, `**/node_modules/**`, `**/dist/**`, `**/build/**`, `**/__tests__/**`, `**/__mocks__/**`
  - `ResolvedConfig::should_include()` method for file filtering
- [x] Support custom thresholds
  - Moderate/high/critical boundaries (configurable)
  - Per-project risk tolerance via `ThresholdConfig`
- [x] Support custom risk weights
  - `WeightConfig` with cc, nd, fo, ns fields
  - `LrsWeights` and `RiskThresholds` structs in risk.rs
  - `analyze_risk_with_config()` accepts custom weights/thresholds
- [x] Add `hotspots config validate` subcommand
  - Validates config file without running analysis
  - Exit code 1 on failure with clear error messages
- [x] Add `hotspots config show` subcommand
  - Prints resolved config as JSON for debugging

**Acceptance:**
- ✅ Projects can customize behavior via config file
- ✅ Invalid config fails with clear error
- ✅ Config is deterministic (no env vars, no timestamps)
- ✅ CLI flags override config file values
- ✅ 21 unit tests for config module
- ✅ All 112 tests pass (80 unit + 32 integration)

**Implementation details:**
- `hotspots-core/src/config.rs`: Full config module (discovery, parsing, validation, resolution)
- `hotspots-core/src/risk.rs`: Added `LrsWeights`, `RiskThresholds`, `_with_config` variants
- `hotspots-core/src/analysis.rs`: Added `analyze_file_with_config()`
- `hotspots-core/src/lib.rs`: Added `analyze_with_config()` with include/exclude filtering
- `hotspots-cli/src/main.rs`: Added `--config` flag and `Config` subcommand
- `hotspots-core/Cargo.toml`: Added `globset = "0.4"` dependency

---

### 3.2 Suppression Comments

**Priority:** P1 (Handle false positives)

**Status:** ✅ **COMPLETED** (2026-02-03)

**Tasks:**

- [x] Parse suppression comments from source
  - `// hotspots-ignore: reason` above function (immediately before)
  - Extract reason from comment or detect missing reason
- [x] Update function discovery to mark suppressed functions
  - Add `suppression_reason: Option<String>` field to FunctionNode
  - Extract and propagate through FunctionRiskReport → FunctionSnapshot → FunctionDeltaEntry
- [x] Exclude suppressed functions from policy checks
  - Skip suppressed functions in 5 function-level policies
  - Keep suppressed functions in net_repo_regression (repo-level policy)
  - Suppressed functions still appear in reports with suppression_reason field
- [x] Add suppression validation policy
  - SuppressionMissingReason policy warns when suppression lacks reason
  - Warning only (non-blocking)
- [x] Add comprehensive tests
  - Unit tests for comment parsing (8 tests)
  - Integration tests for policy filtering (6 tests)
  - All 145 tests pass

**Example:**
```typescript
// hotspots-ignore: complex algorithm, well-tested, refactor planned for Q2
function legacyParser(input: string) {
  // 500 lines of spaghetti
}
```

**Acceptance:**
- ✅ Functions with suppression comments are excluded from policy failures
- ✅ Suppressions without reasons emit warning (SuppressionMissingReason policy)
- ✅ Suppressed functions appear in reports with suppression_reason field
- ✅ Suppressions are deterministic and auditable
- ✅ Byte-for-byte determinism preserved

**Implementation details:**
- `hotspots-core/src/suppression.rs`: Comment extraction module (pure function)
- `hotspots-core/src/ast.rs`: Added suppression_reason to FunctionNode
- `hotspots-core/src/report.rs`: Added suppression_reason to FunctionRiskReport
- `hotspots-core/src/snapshot.rs`: Added suppression_reason to FunctionSnapshot
- `hotspots-core/src/delta.rs`: Added suppression_reason to FunctionDeltaEntry
- `hotspots-core/src/discover.rs`: Updated to call extract_suppression()
- `hotspots-core/src/policy.rs`: Added suppression filtering and SuppressionMissingReason policy
- `hotspots-core/tests/suppression_tests.rs`: Integration tests for end-to-end flow

**Actual effort:** ~4 hours (within estimated 2-3 day range)

---

### 3.3 Enhanced Policy Engine

**Priority:** P2 (Advanced customization)

**Tasks:**

- [ ] Extend policy DSL beyond hardcoded rules
  - Support boolean expressions
  - Example: `lrs > 8.0 AND nd > 4`
- [ ] Add file-level policies
  - Example: `file_total_lrs > 50`
  - Useful for limiting per-file complexity budget
- [ ] Add directory-level policies
  - Example: `src/api/** file_total_lrs > 30`
  - Different standards for different parts of codebase
- [ ] Add metric-specific policies
  - Example: `cc > 10` (ban high cyclomatic complexity)
  - Example: `nd > 3` (ban deep nesting)
- [ ] Support policy inheritance
  - Base config with team-wide policies
  - Project-specific overrides
- [ ] Add policy explanations
  - Each violation shows which policy failed
  - Link to policy documentation
  - Explain rationale ("High CC makes testing difficult")

**Acceptance:**
- Teams can define custom policies matching their standards
- Policies are expressive enough for real-world needs
- Policy violations are actionable and educational

**Estimated effort:** High (1-2 weeks)

---

## Phase 7: AI-First Integration

**PRIORITY: P0 - Required for v1.0.0 Release**

**Context:** Developers are using AI coding assistants (Claude, Cursor, Copilot) heavily NOW. Hotspots should be AI-first from day one, not a future phase. AI agents need:
- Structured, deterministic, machine-readable output
- Clear APIs and patterns for integration
- Direct tool access (MCP servers, SDKs)

**Timeline:** 1 week sprint before v1.0.0 release

**Marketing Message:** "Complexity guardrails for AI-assisted development"

---

### 7.1 Fix Clippy Errors

**Priority:** P0 (Blocks build)

**Status:** ✅ **COMPLETED** (2026-02-06)

**Problem:** 13 clippy errors prevent compilation with `#![deny(warnings)]`. Must fix before release.

**Tasks:**

- [x] **Fix config.rs (Line 110) - Use #[derive(Default)]**
  - [x] Read hotspots-core/src/config.rs
    - [x] Identify the struct with manual Default impl (line ~110)
    - [x] Verify struct fields are all Default-able
    - [x] Note the struct name and location
  - [x] Replace manual impl with derive
    - [x] Remove manual `impl Default for StructName { ... }` block (lines 110-121)
    - [x] Add `#[derive(Default)]` to struct definition (line ~38)
    - [x] Ensure derive appears alongside other derives
  - [x] Verify the fix
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm config.rs line 110 error is gone
    - [x] Run unit tests: `cargo test --package hotspots-core config::`
    - [x] Verify all 21 config tests pass

- [x] **Fix delta.rs - String and Option improvements**
  - [x] Read hotspots-core/src/delta.rs
    - [x] Locate line 136 with `unwrap_or_else(|| "".to_string())`
    - [x] Locate lines 134-135 with `.map(|s| s.clone())`
    - [x] Understand the context of each usage
  - [x] Fix line 136 - unwrap_or_default()
    - [x] Replace `unwrap_or_else(|| "".to_string())` with `unwrap_or_default()`
    - [x] Save file
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 136 warning is gone
  - [x] Fix lines 134-135 - cloned()
    - [x] Replace `.map(|s| s.clone())` with `.cloned()`
    - [x] Save file
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm lines 134-135 warnings are gone
  - [x] Verify delta tests
    - [x] Run `cargo test --package hotspots-core delta::`
    - [x] Ensure all delta tests pass
    - [x] Check that delta output is still deterministic

- [x] **Fix discover.rs (Lines 78, 103, 183, 214) - Remove useless clone map**
  - [x] Read hotspots-core/src/discover.rs
    - [x] Find line 78 with `.as_ref().map(|b| b.clone())`
    - [x] Find line 103 with `.as_ref().map(|b| b.clone())`
    - [x] Find line 183 with `.as_ref().map(|b| b.clone())`
    - [x] Note what's being cloned (likely BlockStmt)
  - [x] Fix line 78
    - [x] Replace `.as_ref().map(|b| b.clone())` with `.clone()`
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 78 warning is gone
  - [x] Fix line 103
    - [x] Replace `.as_ref().map(|b| b.clone())` with `.clone()`
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 103 warning is gone
  - [x] Fix line 183
    - [x] Replace `.as_ref().map(|b| b.clone())` with `.clone()`
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 183 warning is gone
  - [x] Verify discovery tests
    - [x] Run `cargo test --package hotspots-core discover::`
    - [x] Ensure all discovery tests pass (function discovery still works)
    - [x] Run integration tests: `cargo test --package hotspots-core --test '*'`

- [x] **Fix trends.rs (Lines 172, 344) - Use or_default()**
  - [x] Read hotspots-core/src/trends.rs
    - [x] Locate line 172 with `.or_insert_with(Vec::new)`
    - [x] Locate line 344 with `.or_insert_with(Vec::new)`
    - [x] Understand the HashMap/BTreeMap context
  - [x] Fix line 172
    - [x] Replace `.or_insert_with(Vec::new)` with `.or_default()`
    - [x] Save file
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 172 warning is gone
  - [x] Fix line 344
    - [x] Replace `.or_insert_with(Vec::new)` with `.or_default()`
    - [x] Save file
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm line 344 warning is gone
  - [x] Verify trends tests
    - [x] Run `cargo test --package hotspots-core trends::`
    - [x] Ensure trend calculation still works
    - [x] Verify snapshot history tracking works

- [x] **Fix test modules - Avoid module_inception**
  - [x] Fix discover/tests.rs module structure
    - [x] Read hotspots-core/src/discover/tests.rs
    - [x] Find inner `mod tests { ... }` at line 4
    - [x] Rename to `mod discover_tests { ... }` or similar
    - [x] Update any `use super::*` if needed
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm module_inception warning is gone
    - [x] Run `cargo test --package hotspots-core discover::`
  - [x] Fix parser/tests.rs module structure
    - [x] Read hotspots-core/src/parser/tests.rs
    - [x] Find inner `mod tests { ... }` at line 4
    - [x] Rename to `mod parser_tests { ... }` or similar
    - [x] Update any `use super::*` if needed
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
    - [x] Confirm module_inception warning is gone
    - [x] Run `cargo test --package hotspots-core parser::`
  - [x] Verify all unit tests still pass
    - [x] Run full test suite: `cargo test --workspace`
    - [x] Confirm 145+ tests pass
    - [x] Check no test discovery issues

- [x] **Fix build.rs warnings (optional, non-blocking)**
  - [x] Read build.rs
    - [x] Locate line 29 with `&` on array argument
    - [x] Locate lines 52-53 with string manipulation
  - [x] Fix line 29 - Remove unnecessary reference
    - [x] Remove `&` from array argument
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
  - [x] Fix lines 52-53 - Use strip_suffix
    - [x] Replace manual string slicing with `.strip_suffix("-dirty")`
    - [x] Run `cargo clippy --package hotspots-core -- -D warnings`
  - [x] Verify build script works
    - [x] Run `cargo clean`
    - [x] Run `cargo build`
    - [x] Verify version detection still works

- [x] **Final verification - Clean build**
  - [x] Run comprehensive clippy check
    - [x] Execute `cargo clippy --all-targets --all-features -- -D warnings`
    - [x] Verify exit code is 0
    - [x] Confirm zero errors, zero warnings
    - [x] Check output for "0 warnings emitted"
  - [x] Run full test suite
    - [x] Execute `cargo test --workspace`
    - [x] Verify all 145+ tests pass
    - [x] Check for no test failures or panics
    - [x] Review test summary output
  - [x] Run release build
    - [x] Execute `cargo build --release`
    - [x] Verify build completes successfully
    - [x] Check binary size is reasonable
    - [x] Test binary: `./target/release/hotspots --version`
  - [x] Run integration tests
    - [x] Execute `cargo test --package hotspots-core --test '*'`
    - [x] Verify all integration tests pass
    - [x] Check golden test fixtures still match
  - [x] Document the fixes
    - [x] Review all changed files
    - [x] Prepare commit message: "fix: resolve all clippy warnings"
    - [x] Ensure changes follow CLAUDE.md conventions

**Acceptance:**
- ✅ `cargo clippy` runs with zero errors
- ✅ `cargo test --workspace` passes (145 tests)
- ✅ `cargo build --release` completes successfully

**Estimated effort:** 2-3 hours

**Actual effort:** ~2 hours (17 clippy errors fixed across 14 files)

---

### 7.2 JSON Schema & Types

**Priority:** P0 (AI needs machine-readable specs)

**Status:** ⏳ **IN PROGRESS** (Core schemas and types complete, validation examples pending)

**Problem:** JSON output exists but schema is undocumented. AI agents can't reliably consume output without TypeScript types and JSON Schema validation.

**Completed (2026-02-06):**
- ✅ Created 4 JSON Schema files (hotspots-output, function-report, metrics, policy-result)
- ✅ Created @hotspots/types npm package with TypeScript definitions
- ✅ Added type guards and helper functions (filterByRiskBand, getHighestRiskFunctions, etc.)
- ✅ Created comprehensive docs/json-schema.md with integration examples (TypeScript, Python, Go, Rust)
- ✅ Updated action/README.md with JSON output documentation
- ⏳ **Remaining:** Create standalone validation example projects, publish to npm

**Tasks:**

- [x] **Create schemas/ directory structure** ✅
  - [x] Created `schemas/` directory with 4 JSON Schema files
    - [x] hotspots-output.schema.json (main schema)
    - [x] function-report.schema.json
    - [x] metrics.schema.json
    - [x] policy-result.schema.json
    - [x] All schemas validated with ajv-cli
    - [ ] Run hotspots in snapshot mode to get sample JSON output
      - [ ] Execute `hotspots analyze --mode snapshot --format json tests/fixtures/ > samples/snapshot.json`
      - [ ] Review output structure
    - [ ] Run hotspots in delta mode to get sample JSON output
      - [ ] Execute `hotspots analyze --mode delta --format json tests/fixtures/ > samples/delta.json`
      - [ ] Review output structure, note differences from snapshot
    - [ ] List all unique types needed: HotspotsOutput, FunctionReport, Violation, Summary, PolicyResult, etc.
  - [ ] Create hotspots-output.schema.json
    - [ ] Define root schema with $schema, $id, title, description
    - [ ] Add mode field (enum: "snapshot" | "delta")
    - [ ] Add functions array (array of FunctionReport)
    - [ ] Add violations array (array of Violation)
    - [ ] Add summary object (reference to summary.schema.json)
    - [ ] Add timestamp field (ISO 8601 string)
    - [ ] Add version field (semver string)
    - [ ] Mark required fields
    - [ ] Validate against sample output with `ajv-cli`
  - [ ] Create violation.schema.json
    - [ ] Define violation object structure
    - [ ] Add policy_id field (string)
    - [ ] Add severity field (enum: "error" | "warning" | "info")
    - [ ] Add message field (string)
    - [ ] Add file field (string)
    - [ ] Add function field (string, optional)
    - [ ] Add line field (integer, optional)
    - [ ] Add lrs field (number, optional)
    - [ ] Add details object (additional context)
    - [ ] Validate against sample violations
  - [ ] Create function-report.schema.json
    - [ ] Define function report structure
    - [ ] Add id object (file_index, local_index)
    - [ ] Add name field (string, nullable)
    - [ ] Add file field (string)
    - [ ] Add line field (integer)
    - [ ] Add metrics object (cc, nd, fo, ns)
    - [ ] Add lrs field (number)
    - [ ] Add risk_band field (enum: "low" | "moderate" | "high" | "critical")
    - [ ] Add suppression_reason field (string, nullable)
    - [ ] Add delta_type field for delta mode (enum: "added" | "modified" | "removed" | "unchanged", optional)
    - [ ] Add previous_lrs field for delta mode (number, optional)
    - [ ] Validate against sample functions
  - [ ] Create summary.schema.json
    - [ ] Define summary structure
    - [ ] Add total_functions field (integer)
    - [ ] Add functions_by_risk object (low, moderate, high, critical counts)
    - [ ] Add violations_by_severity object (error, warning, info counts)
    - [ ] Add policy_passed field (boolean)
    - [ ] Add analysis_time_ms field (integer, optional)
    - [ ] Validate against sample summary

- [x] **Generate TypeScript types package** ✅
  - [x] Set up @hotspots/types npm package
    - [x] Created `packages/types/` directory with full package structure
    - [x] Manually created TypeScript types matching JSON schemas
    - [x] Added type guards: isHotspotsOutput(), isFunctionReport(), isPolicyResult()
    - [x] Added helper functions: filterByRiskBand(), filterBySeverity(), getHighestRiskFunctions(), etc.
    - [x] Added comprehensive JSDoc comments with examples
    - [x] Created README.md with usage examples
    - [x] Package builds successfully with TypeScript
    - [ ] **PENDING:** Publish to npm (requires npm login)
    - [ ] Initialize package: `npm init --scope=@hotspots`
    - [ ] Set package name to "@hotspots/types"
    - [ ] Set version to "1.0.0"
    - [ ] Add keywords: "hotspots", "types", "typescript", "complexity", "analysis"
    - [ ] Set repository URL
    - [ ] Configure TypeScript: `tsc --init`
    - [ ] Set tsconfig.json options
      - [ ] `"declaration": true`
      - [ ] `"declarationMap": true`
      - [ ] `"outDir": "./dist"`
      - [ ] `"rootDir": "./src"`
    - [ ] Add build script: `"build": "tsc"`
    - [ ] Add prepublish script: `"prepublishOnly": "npm run build"`
  - [ ] Generate types from JSON Schema
    - [ ] Install json-schema-to-typescript: `npm install -D json-schema-to-typescript`
    - [ ] Create generation script `scripts/generate-types.ts`
    - [ ] Generate HotspotsOutput interface from hotspots-output.schema.json
    - [ ] Generate Violation interface from violation.schema.json
    - [ ] Generate FunctionReport interface from function-report.schema.json
    - [ ] Generate Summary interface from summary.schema.json
    - [ ] Output to `src/index.ts`
  - [ ] Add JSDoc comments to generated types
    - [ ] Add package-level documentation to src/index.ts
    - [ ] Add JSDoc to HotspotsOutput interface
      - [ ] Document mode field
      - [ ] Document functions array
      - [ ] Document violations array
      - [ ] Add @example showing snapshot and delta usage
    - [ ] Add JSDoc to Violation interface
      - [ ] Document policy_id, severity, message
      - [ ] Add @example showing violation structure
    - [ ] Add JSDoc to FunctionReport interface
      - [ ] Document all metrics (cc, nd, fo, ns, lrs)
      - [ ] Explain risk_band values
      - [ ] Add @example showing high-risk function
    - [ ] Add JSDoc to Summary interface
      - [ ] Document aggregation fields
      - [ ] Add @example showing summary structure
  - [ ] Add utility types and helpers
    - [ ] Create RiskBand type alias ("low" | "moderate" | "high" | "critical")
    - [ ] Create Severity type alias ("error" | "warning" | "info")
    - [ ] Create Mode type alias ("snapshot" | "delta")
    - [ ] Add type guards: `isHotspotsOutput()`, `isViolation()`, etc.
    - [ ] Add helper functions
      - [ ] `filterByRiskBand(functions, band): FunctionReport[]`
      - [ ] `filterBySeverity(violations, severity): Violation[]`
      - [ ] `getHighestRiskFunctions(functions, n): FunctionReport[]`
  - [ ] Build and test the package
    - [ ] Run `npm run build`
    - [ ] Verify dist/ contains .d.ts files
    - [ ] Create test file to import types
    - [ ] Verify types work with sample JSON
    - [ ] Test type guards with valid and invalid data
  - [ ] Prepare for npm publishing
    - [ ] Add .npmignore (exclude src/, scripts/, tests/)
    - [ ] Add LICENSE file (MIT)
    - [ ] Create comprehensive README.md
      - [ ] Installation instructions
      - [ ] Quick start example
      - [ ] API documentation for all types
      - [ ] Examples of using type guards and helpers
    - [ ] Add repository field to package.json
    - [ ] Add "types" field pointing to dist/index.d.ts
    - [ ] Add "main" field pointing to dist/index.js
    - [ ] Review package.json for completeness
  - [ ] Publish to npm
    - [ ] Login to npm: `npm login`
    - [ ] Dry run: `npm publish --dry-run`
    - [ ] Review what will be published
    - [ ] Publish: `npm publish --access public`
    - [ ] Verify on npmjs.com/@hotspots/types
    - [ ] Test installation: `npm install @hotspots/types` in fresh project

- [x] **Document output format** ✅
  - [x] Created comprehensive docs/json-schema.md
    - [x] Introduction section with overview of JSON output
    - [x] Complete schema structure documentation
    - [x] Detailed metrics explanations (CC, ND, FO, NS, LRS)
    - [x] Risk band definitions
    - [x] Integration examples for TypeScript, Python, Go, and Rust
    - [x] CI/CD integration patterns (GitHub Actions, GitLab CI)
    - [x] AI assistant integration guidance
  - [x] Updated action/README.md with JSON output format section
    - [x] Added JSON output documentation
    - [x] Added schema references
    - [x] Added example output structure
    - [x] Added link to @hotspots/types package
      - [ ] Explain purpose of JSON output
      - [ ] Link to schemas/ directory
      - [ ] Link to @hotspots/types npm package
    - [ ] Document snapshot mode output
      - [ ] Show complete example of snapshot JSON
      - [ ] Document HotspotsOutput structure
      - [ ] Document each field with type and description
      - [ ] Show example with 3+ functions in different risk bands
      - [ ] Document empty violations case (passing policy)
    - [ ] Document delta mode output
      - [ ] Show complete example of delta JSON
      - [ ] Explain differences from snapshot mode
      - [ ] Document delta_type field (added, modified, removed, unchanged)
      - [ ] Document previous_lrs field for modified functions
      - [ ] Show example with function additions, modifications, removals
    - [ ] Document policy violations format
      - [ ] Show violation structure
      - [ ] List all possible policy_id values
      - [ ] Document severity levels (error, warning, info)
      - [ ] Show examples of each violation type
        - [ ] critical_threshold violation
        - [ ] high_threshold violation
        - [ ] rapid_growth violation
        - [ ] suppression_missing_reason violation
    - [ ] Document metrics in detail
      - [ ] CC (Cyclomatic Complexity) - what it measures
      - [ ] ND (Nesting Depth) - what it measures
      - [ ] FO (Fan-Out) - what it measures
      - [ ] NS (Number of Statements) - what it measures
      - [ ] LRS (Logarithmic Risk Score) - formula and interpretation
    - [ ] Add versioning section
      - [ ] Explain schema versioning strategy
      - [ ] Document breaking vs non-breaking changes
      - [ ] List version history
  - [ ] Add examples for each schema
    - [ ] Create examples/json-output/ directory
    - [ ] Add snapshot-simple.json (1 function, no violations)
    - [ ] Add snapshot-violations.json (multiple functions, policy failures)
    - [ ] Add delta-no-changes.json (all functions unchanged)
    - [ ] Add delta-with-changes.json (additions, modifications, removals)
    - [ ] Add all-risk-bands.json (examples of low, moderate, high, critical)

- [ ] **Add schema validation examples**
  - [ ] Create TypeScript validation example
    - [ ] Create examples/validation/typescript/ directory
    - [ ] Add package.json with @hotspots/types dependency
    - [ ] Create validate.ts
      - [ ] Import types from @hotspots/types
      - [ ] Load JSON file
      - [ ] Parse and type-check with HotspotsOutput type
      - [ ] Use type guards to validate structure
      - [ ] Handle parsing errors gracefully
      - [ ] Show how to iterate through functions
      - [ ] Show how to filter violations by severity
    - [ ] Add README.md with setup and run instructions
    - [ ] Test the example to ensure it works
  - [ ] Create Python validation example
    - [ ] Create examples/validation/python/ directory
    - [ ] Add requirements.txt with jsonschema dependency
    - [ ] Create validate.py
      - [ ] Import jsonschema library
      - [ ] Load hotspots-output.schema.json
      - [ ] Load sample JSON output
      - [ ] Validate JSON against schema
      - [ ] Handle validation errors with clear messages
      - [ ] Show how to access specific fields
      - [ ] Show how to filter functions by risk band
    - [ ] Add README.md with setup and run instructions
    - [ ] Test the example to ensure it works
  - [ ] Create Go validation example
    - [ ] Create examples/validation/go/ directory
    - [ ] Create go.mod
    - [ ] Create validate.go
      - [ ] Define Go structs matching JSON schema
      - [ ] Use json.Unmarshal to parse output
      - [ ] Add validation logic
      - [ ] Show error handling patterns
    - [ ] Add README.md with setup and run instructions
    - [ ] Test the example to ensure it works
  - [ ] Create Rust validation example (bonus)
    - [ ] Create examples/validation/rust/ directory
    - [ ] Add Cargo.toml with serde, serde_json dependencies
    - [ ] Create validate.rs
      - [ ] Define Rust structs with serde(Deserialize)
      - [ ] Parse JSON with serde_json
      - [ ] Show type-safe access patterns
    - [ ] Test the example

- [x] **Update GitHub Action README** ✅
  - [x] Document outputs with schema reference
    - [x] Added "JSON Output Format" section to action/README.md
    - [x] Documented schema files and @hotspots/types package
    - [x] Added example output structure
    - [x] Added link to docs/json-schema.md
    - [ ] Add detailed documentation for each output
      - [ ] `violations` - Number of policy violations (integer)
      - [ ] `passed` - Whether policy passed (boolean)
      - [ ] `summary` - JSON summary object (string, parse as JSON)
      - [ ] `report-path` - Path to HTML report (string)
    - [ ] Link to schemas/ directory
    - [ ] Link to @hotspots/types package
    - [ ] Link to docs/json-schema.md
  - [ ] Add example of parsing action outputs
    - [ ] Create example workflow showing output usage
    - [ ] Show how to parse `summary` JSON output
    - [ ] Show how to access violations count
    - [ ] Show how to conditionally run steps based on `passed`
    - [ ] Show how to upload HTML report from `report-path`
  - [ ] Add TypeScript example for custom action
    - [ ] Show how to parse outputs in another action step
    - [ ] Use @hotspots/types for type safety
    - [ ] Show error handling

**Acceptance:**
- ✅ JSON Schema published to `schemas/` directory
- ⏳ `@hotspots/types` built (not yet published to npm)
- ✅ Types are accurate (validated against real output)
- ✅ Documentation includes examples in 4 languages (TypeScript, Python, Go, Rust)

**Estimated effort:** 1 day
**Actual effort:** ~3 hours (schemas, types package, documentation)

---

### 7.3 Claude MCP Server

**Priority:** P0 (Direct AI integration)

**Status:** ⏳ **IN PROGRESS**

**Problem:** Claude Desktop/Code users can't run Hotspots during conversations. Need Model Context Protocol (MCP) server for direct integration.

**Tasks:**

- [ ] **Create hotspots-mcp-server/ package structure**
  - [ ] Initialize TypeScript package
    - [ ] Create `packages/mcp-server/` directory
    - [ ] Initialize: `npm init --scope=@hotspots`
    - [ ] Set package name to "@hotspots/mcp-server"
    - [ ] Set version to "1.0.0"
    - [ ] Add description: "Model Context Protocol server for Hotspots complexity analysis"
    - [ ] Add keywords: "mcp", "hotspots", "claude", "complexity"
    - [ ] Set "bin" field to point to compiled server
  - [ ] Set up TypeScript configuration
    - [ ] Run `tsc --init`
    - [ ] Configure tsconfig.json
      - [ ] Set `"target": "ES2022"`
      - [ ] Set `"module": "commonjs"`
      - [ ] Set `"outDir": "./dist"`
      - [ ] Set `"rootDir": "./src"`
      - [ ] Enable `"strict": true`
      - [ ] Enable `"esModuleInterop": true`
    - [ ] Add build script to package.json: `"build": "tsc"`
    - [ ] Add dev script: `"dev": "tsc --watch"`
  - [ ] Install dependencies
    - [ ] Install MCP SDK: `npm install @modelcontextprotocol/sdk`
    - [ ] Install exec utilities: `npm install --save execa`
    - [ ] Install @hotspots/types: `npm install @hotspots/types`
    - [ ] Install dev dependencies: `npm install -D @types/node typescript`
  - [ ] Create project structure
    - [ ] Create src/index.ts (main entry point)
    - [ ] Create src/server.ts (MCP server implementation)
    - [ ] Create src/tools/ directory
    - [ ] Create src/config.ts (configuration loading)
    - [ ] Create src/utils.ts (helper functions)

- [ ] **Implement hotspots_analyze tool**
  - [ ] Define tool schema in src/tools/analyze.ts
    - [ ] Create AnalyzeInput interface
      - [ ] path: string (required) - file or directory to analyze
      - [ ] mode: "snapshot" | "delta" (optional, default "snapshot")
      - [ ] minLrs: number (optional) - minimum LRS threshold
      - [ ] config: string (optional) - path to config file
    - [ ] Define JSON Schema for MCP tool registration
      - [ ] Add tool name: "hotspots_analyze"
      - [ ] Add description: "Analyze JavaScript/TypeScript files for complexity"
      - [ ] Define input schema with all parameters
      - [ ] Mark required fields
  - [ ] Implement analyze() function
    - [ ] Accept AnalyzeInput parameters
    - [ ] Validate input parameters
      - [ ] Check path exists using fs.existsSync()
      - [ ] Validate mode is "snapshot" or "delta"
      - [ ] Validate minLrs is positive number if provided
    - [ ] Find hotspots binary
      - [ ] Check config for custom binary path
      - [ ] Fall back to `which hotspots` on PATH
      - [ ] Throw helpful error if not found
    - [ ] Build CLI arguments
      - [ ] Start with ["analyze"]
      - [ ] Add `--mode ${mode}`
      - [ ] Add `--min-lrs ${minLrs}` if provided
      - [ ] Add `--config ${config}` if provided
      - [ ] Add `--format json`
      - [ ] Add path as positional argument
    - [ ] Execute hotspots using execa
      - [ ] Run with arguments
      - [ ] Capture stdout and stderr
      - [ ] Set reasonable timeout (30 seconds)
      - [ ] Handle execution errors
    - [ ] Parse JSON output
      - [ ] Parse stdout as JSON
      - [ ] Validate against HotspotsOutput type
      - [ ] Handle parse errors gracefully
    - [ ] Format response for Claude
      - [ ] Return structured HotspotsOutput
      - [ ] Add summary text for easy reading
      - [ ] Include violation count prominently
      - [ ] List high-risk functions in summary
  - [ ] Add error handling
    - [ ] Handle "hotspots not found" error
    - [ ] Handle invalid path error
    - [ ] Handle JSON parse error
    - [ ] Handle timeout error
    - [ ] Return user-friendly error messages
  - [ ] Test analyze tool
    - [ ] Create test fixtures in tests/fixtures/
    - [ ] Test with valid TypeScript file
    - [ ] Test with directory
    - [ ] Test with snapshot mode
    - [ ] Test with delta mode
    - [ ] Test with minLrs filter
    - [ ] Test error cases

- [ ] **Implement hotspots_explain tool**
  - [ ] Define tool schema in src/tools/explain.ts
    - [ ] Create ExplainInput interface
      - [ ] file: string (required)
      - [ ] function: string (required)
      - [ ] lrs: number (required)
      - [ ] metrics: object (optional) - cc, nd, fo, ns
    - [ ] Define JSON Schema for MCP tool registration
      - [ ] Add tool name: "hotspots_explain"
      - [ ] Add description: "Explain why a function has high complexity"
      - [ ] Define input schema
  - [ ] Implement explain() function
    - [ ] Accept ExplainInput parameters
    - [ ] Validate inputs
      - [ ] Check file path is provided
      - [ ] Check function name is provided
      - [ ] Validate LRS is a number
    - [ ] Generate explanation text
      - [ ] Interpret LRS value (low/moderate/high/critical)
      - [ ] Explain what LRS means in practical terms
      - [ ] If metrics provided, break down contribution
        - [ ] Explain CC contribution ("5 decision points")
        - [ ] Explain ND contribution ("3 levels of nesting")
        - [ ] Explain FO contribution ("calls 4 other functions")
        - [ ] Explain NS contribution ("120 statements")
      - [ ] Add context about why complexity matters
        - [ ] Testing difficulty
        - [ ] Bug risk
        - [ ] Maintenance burden
    - [ ] Generate suggestions
      - [ ] If high CC: "Extract conditional logic into separate functions"
      - [ ] If high ND: "Flatten nested structures with early returns"
      - [ ] If high FO: "Consider grouping related calls"
      - [ ] If high NS: "Break function into smaller, focused functions"
      - [ ] Provide 3-5 specific, actionable suggestions
    - [ ] Format response
      - [ ] Start with summary: "This function has high complexity (LRS 7.2)"
      - [ ] Add explanation section
      - [ ] Add suggestions section
      - [ ] Add resources/links section
    - [ ] Return formatted explanation
  - [ ] Test explain tool
    - [ ] Test with low complexity function
    - [ ] Test with high complexity function
    - [ ] Test with metrics breakdown
    - [ ] Test with missing metrics (still works)
    - [ ] Verify explanation quality

- [ ] **Implement hotspots_refactor_suggestions tool**
  - [ ] Define tool schema in src/tools/refactor.ts
    - [ ] Create RefactorInput interface
      - [ ] file: string (required)
      - [ ] function: string (required)
      - [ ] code: string (optional) - function source code
      - [ ] metrics: object (optional) - current metrics
    - [ ] Define JSON Schema for MCP tool registration
      - [ ] Add tool name: "hotspots_refactor_suggestions"
      - [ ] Add description: "Get specific refactoring suggestions"
      - [ ] Define input schema
  - [ ] Implement refactor_suggestions() function
    - [ ] Accept RefactorInput parameters
    - [ ] Validate inputs
    - [ ] Read function code if not provided
      - [ ] Parse file with SWC (or read from file)
      - [ ] Extract function by name
      - [ ] Handle not found error
    - [ ] Analyze code structure (if code provided)
      - [ ] Count if/else chains
      - [ ] Count nested loops
      - [ ] Identify long blocks
      - [ ] Find repeated patterns
    - [ ] Generate targeted suggestions
      - [ ] For if/else chains: "Consider strategy pattern or lookup table"
      - [ ] For nested loops: "Extract inner loop to separate function"
      - [ ] For long blocks: "Extract logical sections into named functions"
      - [ ] For repeated patterns: "Create reusable helper functions"
      - [ ] For deep nesting: "Use early returns to reduce nesting"
    - [ ] Rank suggestions by impact
      - [ ] Estimate LRS reduction for each
      - [ ] Order by biggest impact first
    - [ ] Format response
      - [ ] List suggestions with priorities
      - [ ] Show estimated complexity reduction
      - [ ] Provide code examples where helpful
    - [ ] Return formatted suggestions
  - [ ] Test refactor tool
    - [ ] Test with if/else chain function
    - [ ] Test with nested loop function
    - [ ] Test with long function
    - [ ] Verify suggestion quality
    - [ ] Test with and without code parameter

- [ ] **Add configuration system**
  - [ ] Create src/config.ts
    - [ ] Define Config interface
      - [ ] hotspotsPath: string | null - custom binary path
      - [ ] defaultConfigFile: string | null - default config file
      - [ ] timeout: number - execution timeout in ms
      - [ ] workingDirectory: string - base directory for analysis
    - [ ] Implement loadConfig() function
      - [ ] Check for hotspots-mcp-config.json in cwd
      - [ ] Check for config in user home directory
      - [ ] Parse JSON config
      - [ ] Validate config structure
      - [ ] Apply defaults for missing fields
      - [ ] Return Config object
    - [ ] Implement getDefaultConfig() function
      - [ ] Return sensible defaults
        - [ ] hotspotsPath: null (use PATH)
        - [ ] defaultConfigFile: null (let hotspots discover)
        - [ ] timeout: 30000 (30 seconds)
        - [ ] workingDirectory: process.cwd()
  - [ ] Create example hotspots-mcp-config.json
    - [ ] Document all config options with comments (in README)
    - [ ] Show example with custom binary path
    - [ ] Show example with default config file
  - [ ] Test configuration loading
    - [ ] Test with no config (uses defaults)
    - [ ] Test with partial config (merges with defaults)
    - [ ] Test with full config
    - [ ] Test with invalid config (throws error)

- [ ] **Implement MCP server**
  - [ ] Create src/server.ts
    - [ ] Import MCP SDK
    - [ ] Import all tools (analyze, explain, refactor)
    - [ ] Create Server instance
    - [ ] Register all tools
      - [ ] Register hotspots_analyze with schema
      - [ ] Register hotspots_explain with schema
      - [ ] Register hotspots_refactor_suggestions with schema
    - [ ] Implement tool handlers
      - [ ] Route hotspots_analyze calls to analyze()
      - [ ] Route hotspots_explain calls to explain()
      - [ ] Route hotspots_refactor_suggestions calls to refactor_suggestions()
      - [ ] Wrap each handler with error handling
    - [ ] Add server metadata
      - [ ] Name: "Hotspots MCP Server"
      - [ ] Version: from package.json
      - [ ] Description
    - [ ] Start server
      - [ ] Listen on stdio transport
      - [ ] Log startup message to stderr
      - [ ] Handle shutdown gracefully
  - [ ] Create src/index.ts (entry point)
    - [ ] Import server
    - [ ] Load config
    - [ ] Start server with config
    - [ ] Handle top-level errors
    - [ ] Add --version flag support
    - [ ] Add --help flag support
  - [ ] Build and test server
    - [ ] Run `npm run build`
    - [ ] Verify dist/index.js exists
    - [ ] Test running server directly: `node dist/index.js`
    - [ ] Verify server starts and waits for input

- [ ] **Create setup documentation**
  - [ ] Create comprehensive README.md
    - [ ] Add overview section
      - [ ] What is the MCP server
      - [ ] What can it do
      - [ ] Why use it with Claude
    - [ ] Add prerequisites section
      - [ ] Node.js 18+ required
      - [ ] Hotspots CLI must be installed
      - [ ] Claude Desktop or compatible MCP client
    - [ ] Add installation section
      - [ ] Global install: `npm install -g @hotspots/mcp-server`
      - [ ] Local install in project
      - [ ] Verify installation: `hotspots-mcp-server --version`
    - [ ] Add Claude Desktop configuration
      - [ ] Show how to edit claude_desktop_config.json
      - [ ] Provide example configuration
        ```json
        {
          "mcpServers": {
            "hotspots": {
              "command": "hotspots-mcp-server",
              "args": []
            }
          }
        }
        ```
      - [ ] Show config with custom binary path
      - [ ] Show config with working directory
    - [ ] Add usage examples
      - [ ] Example conversation 1: Analyze project
      - [ ] Example conversation 2: Explain high complexity
      - [ ] Example conversation 3: Get refactoring suggestions
      - [ ] Example conversation 4: Iterative refactoring loop
    - [ ] Add API documentation
      - [ ] Document hotspots_analyze tool with all parameters
      - [ ] Document hotspots_explain tool with all parameters
      - [ ] Document hotspots_refactor_suggestions tool with all parameters
      - [ ] Show example inputs and outputs for each
    - [ ] Add troubleshooting section
      - [ ] "Server not appearing in Claude" → Check config path
      - [ ] "Hotspots not found" → Install hotspots CLI
      - [ ] "Permission denied" → Check binary permissions
      - [ ] "Timeout errors" → Increase timeout in config
    - [ ] Add configuration reference
      - [ ] Document hotspots-mcp-config.json
      - [ ] List all options with types and defaults
  - [ ] Create examples/ directory
    - [ ] Add example-conversation-1.md (basic analysis)
    - [ ] Add example-conversation-2.md (refactoring loop)
    - [ ] Add example-config.json

- [ ] **Test with Claude Desktop**
  - [ ] Set up local testing environment
    - [ ] Install Claude Desktop (if not already installed)
    - [ ] Build MCP server: `npm run build`
    - [ ] Create test config in claude_desktop_config.json
    - [ ] Point to local build for testing
  - [ ] Test tool discovery
    - [ ] Start Claude Desktop
    - [ ] Verify Hotspots tools appear in available tools
    - [ ] Check tool descriptions are clear
    - [ ] Verify tool parameters are documented
  - [ ] Test hotspots_analyze tool
    - [ ] Ask Claude to analyze a test project
    - [ ] Verify tool is called correctly
    - [ ] Verify JSON output is parsed
    - [ ] Verify Claude presents results clearly
    - [ ] Test with different modes (snapshot, delta)
    - [ ] Test with minLrs filtering
  - [ ] Test hotspots_explain tool
    - [ ] Ask Claude to explain a high-complexity function
    - [ ] Verify explanation is generated
    - [ ] Verify suggestions are helpful
    - [ ] Test with different LRS values
  - [ ] Test hotspots_refactor_suggestions tool
    - [ ] Ask Claude for refactoring suggestions
    - [ ] Verify suggestions are specific and actionable
    - [ ] Verify suggestions are ranked
  - [ ] Test analyze → refactor → re-analyze loop
    - [ ] Ask Claude to analyze code
    - [ ] Ask for refactoring suggestions
    - [ ] Apply suggestions (manually or with Claude)
    - [ ] Ask Claude to re-analyze
    - [ ] Verify LRS decreased
    - [ ] Complete full loop successfully
  - [ ] Test error handling
    - [ ] Try analyzing non-existent path
    - [ ] Try with hotspots not installed (temporarily)
    - [ ] Try with invalid parameters
    - [ ] Verify error messages are clear and actionable
  - [ ] Document test results
    - [ ] Note any issues found
    - [ ] Fix issues before publishing
    - [ ] Get sample conversation transcripts for docs

- [ ] **Publish to npm**
  - [ ] Prepare package for publishing
    - [ ] Add .npmignore
      - [ ] Exclude src/ (ship only dist/)
      - [ ] Exclude tests/
      - [ ] Exclude tsconfig.json
      - [ ] Include README.md, LICENSE, package.json
    - [ ] Add LICENSE file (MIT)
    - [ ] Ensure package.json is complete
      - [ ] Verify "bin" field points to dist/index.js
      - [ ] Verify "files" field includes dist/
      - [ ] Add repository URL
      - [ ] Add bugs URL
      - [ ] Add homepage URL
      - [ ] Add author field
    - [ ] Add shebang to dist/index.js: `#!/usr/bin/env node`
    - [ ] Make binary executable: `chmod +x dist/index.js`
  - [ ] Test package locally
    - [ ] Run `npm pack`
    - [ ] Extract tarball
    - [ ] Verify contents are correct
    - [ ] Install locally: `npm install -g ./hotspots-mcp-server-1.0.0.tgz`
    - [ ] Test running: `hotspots-mcp-server --version`
    - [ ] Test in Claude Desktop with local install
  - [ ] Publish to npm registry
    - [ ] Login to npm: `npm login`
    - [ ] Dry run: `npm publish --dry-run --access public`
    - [ ] Review what will be published
    - [ ] Publish: `npm publish --access public`
    - [ ] Verify on npmjs.com/@hotspots/mcp-server
  - [ ] Test published package
    - [ ] Uninstall local version
    - [ ] Install from npm: `npm install -g @hotspots/mcp-server`
    - [ ] Verify installation
    - [ ] Test in Claude Desktop
    - [ ] Verify all tools work
  - [ ] Add to MCP server registry
    - [ ] Visit MCP server registry submission page
    - [ ] Fill out submission form
      - [ ] Package name: @hotspots/mcp-server
      - [ ] Description
      - [ ] Category: Development Tools
      - [ ] npm URL
      - [ ] GitHub URL
    - [ ] Submit for review
    - [ ] Follow up if needed

**Example Usage:**
```
User: "Analyze the complexity of src/"
Claude: [calls hotspots_analyze]
        "I found 3 high-risk functions..."

User: "How can I reduce complexity in handleRequest?"
Claude: [calls hotspots_refactor_suggestions]
        "Here are 3 ways to refactor..."
```

**Acceptance:**
- ✅ Claude Desktop can call Hotspots as a tool
- ✅ All 3 tools work correctly (analyze, explain, refactor_suggestions)
- ✅ Error messages are helpful
- ✅ Published to npm and MCP registry

**Estimated effort:** 2 days

---

### 7.4 AI Integration Documentation

**Priority:** P1 (Essential for adoption)

**Status:** ⏳ **IN PROGRESS**

**Problem:** No guidance on how AI agents should use Hotspots. Need patterns, examples, and best practices.

**Tasks:**

- [ ] **Create docs/AI_INTEGRATION.md structure**
  - [ ] Set up document outline
    - [ ] Create docs/AI_INTEGRATION.md file
    - [ ] Add title: "AI Integration Guide"
    - [ ] Add table of contents with links
    - [ ] Add last updated date
  - [ ] Write Overview section
    - [ ] Explain "Why Hotspots is AI-First"
      - [ ] Deterministic output (same input → same output)
      - [ ] Machine-readable JSON format
      - [ ] Clear, structured schema
      - [ ] No side effects or state
      - [ ] Fast execution (suitable for tight loops)
    - [ ] Explain use cases for AI + Hotspots
      - [ ] Automated code review
      - [ ] Iterative refactoring
      - [ ] Complexity-aware code generation
      - [ ] CI/CD integration
      - [ ] Continuous quality improvement
    - [ ] List available integration methods
      - [ ] Claude MCP Server (direct tool access)
      - [ ] CLI with JSON output (any AI)
      - [ ] GitHub Action outputs (CI-based)
      - [ ] TypeScript SDK (programmatic)
  - [ ] Create Quick Start section
    - [ ] Show simplest possible example
    - [ ] 5-line code snippet for running Hotspots
    - [ ] Parse JSON output
    - [ ] Access key fields
    - [ ] Link to full workflows below

- [ ] **Document JSON Output Reference**
  - [ ] Link to docs/json-schema.md
  - [ ] Provide quick reference for common fields
    - [ ] Create table of top-level fields
      - [ ] mode, functions, violations, summary
      - [ ] Show types and descriptions
    - [ ] Create table of FunctionReport fields
      - [ ] file, name, line, lrs, risk_band, metrics
      - [ ] Show types and descriptions
    - [ ] Create table of Violation fields
      - [ ] policy_id, severity, message, file, function
      - [ ] Show types and descriptions
  - [ ] Show JSON output examples inline
    - [ ] Snapshot mode with clean code (no violations)
    - [ ] Snapshot mode with violations
    - [ ] Delta mode with changes
  - [ ] Explain snapshot vs delta modes
    - [ ] When to use snapshot (full codebase analysis)
    - [ ] When to use delta (PR/commit analysis)
    - [ ] How delta mode saves time
  - [ ] Document output fields AI should focus on
    - [ ] violations array (first thing to check)
    - [ ] functions with risk_band "high" or "critical"
    - [ ] summary.policy_passed boolean
    - [ ] delta_type "added" or "modified" (in delta mode)

- [ ] **Document AI workflows**
  - [ ] Write "Code Review" workflow
    - [ ] Overview: AI reviews PR for complexity issues
    - [ ] Step-by-step process
      - [ ] 1. Run hotspots in delta mode on PR
      - [ ] 2. Parse JSON output
      - [ ] 3. Filter for added/modified high-risk functions
      - [ ] 4. For each violation, generate review comment
      - [ ] 5. Post comments to PR
    - [ ] Code example (TypeScript pseudocode)
    - [ ] Expected output format
    - [ ] Link to reference implementation (Task 7.5)
  - [ ] Write "Refactoring Loop" workflow
    - [ ] Overview: AI iteratively refactors until LRS < threshold
    - [ ] Step-by-step process
      - [ ] 1. Run hotspots, identify high-risk function
      - [ ] 2. AI generates refactoring suggestions
      - [ ] 3. Apply suggestions (AI or human)
      - [ ] 4. Re-run hotspots to verify improvement
      - [ ] 5. Repeat until LRS < threshold or max iterations
      - [ ] 6. Validate tests still pass
    - [ ] Code example showing loop structure
    - [ ] Termination conditions
    - [ ] Safety measures (max iterations, test validation)
    - [ ] Link to reference implementation
  - [ ] Write "Complexity-Aware Generation" workflow
    - [ ] Overview: AI generates code with LRS constraint
    - [ ] Step-by-step process
      - [ ] 1. AI generates initial implementation
      - [ ] 2. Write to temp file
      - [ ] 3. Run hotspots on temp file
      - [ ] 4. If LRS > threshold, regenerate with "simpler" constraint
      - [ ] 5. Repeat until satisfactory or give up
    - [ ] Code example showing generate-check-regenerate loop
    - [ ] Prompt engineering tips (include LRS constraint)
    - [ ] Link to reference implementation
  - [ ] Write "Pre-Commit Checks" workflow
    - [ ] Overview: AI validates staged changes before commit
    - [ ] Step-by-step process
      - [ ] 1. Git hook triggers on pre-commit
      - [ ] 2. Get list of staged files
      - [ ] 3. Run hotspots in delta mode
      - [ ] 4. If violations, AI suggests fixes
      - [ ] 5. User can accept, modify, or skip commit
    - [ ] Code example for git hook
    - [ ] Integration with husky or simple-git-hooks
    - [ ] Link to reference implementation
  - [ ] Write "Automated Refactoring" workflow
    - [ ] Overview: AI proposes and applies complexity fixes
    - [ ] Step-by-step process
      - [ ] 1. Run hotspots snapshot mode
      - [ ] 2. Sort functions by LRS descending
      - [ ] 3. For top N functions, AI generates refactor
      - [ ] 4. Apply refactor, run tests
      - [ ] 5. If tests pass, keep changes; else revert
      - [ ] 6. Create PR with changes
    - [ ] Code example showing full automation
    - [ ] Safety considerations (require test passage)
    - [ ] Human review checkpoint
    - [ ] Link to reference implementation

- [ ] **Add AI assistant integration examples**
  - [ ] Document Claude integration (via MCP server)
    - [ ] Prerequisites: Claude Desktop + MCP server installed
    - [ ] Configuration steps
      - [ ] Show claude_desktop_config.json setup
      - [ ] Verify tools are loaded
    - [ ] Example conversation flow
      - [ ] User: "Analyze src/ for complexity"
      - [ ] Claude calls hotspots_analyze tool
      - [ ] Claude presents results
      - [ ] User: "Suggest refactoring for handleRequest"
      - [ ] Claude calls hotspots_refactor_suggestions
      - [ ] Claude provides specific suggestions
    - [ ] Best practices for prompts
      - [ ] Be specific about paths
      - [ ] Ask for explanations of high LRS
      - [ ] Request iterative refinement
    - [ ] Link to MCP server docs
  - [ ] Document GPT-4 integration (via API + CLI)
    - [ ] Overview: Use CLI + JSON parsing
    - [ ] Code example in Python
      - [ ] Run hotspots via subprocess
      - [ ] Parse JSON output
      - [ ] Send to GPT-4 with context
      - [ ] GPT-4 analyzes and suggests improvements
    - [ ] Code example in TypeScript/Node
      - [ ] Use execa to run hotspots
      - [ ] Parse JSON with @hotspots/types
      - [ ] Call OpenAI API
      - [ ] Format response
    - [ ] Prompt engineering tips
      - [ ] Include JSON schema in system prompt
      - [ ] Provide example outputs
      - [ ] Ask for structured responses
  - [ ] Document Cursor integration
    - [ ] Overview: In-editor AI + Hotspots CLI
    - [ ] Setup approach
      - [ ] Configure Cursor to use hotspots
      - [ ] Add keyboard shortcut for analysis
    - [ ] Usage pattern
      - [ ] Select function in editor
      - [ ] Trigger Cursor + Hotspots
      - [ ] Get inline refactoring suggestions
    - [ ] Example .cursorrules or config
    - [ ] Note: May require custom extension/script
  - [ ] Document GitHub Copilot Workspace integration
    - [ ] Overview: PR analysis in Copilot Workspace
    - [ ] Setup using GitHub Action
      - [ ] Add Hotspots action to workflow
      - [ ] Post results as PR comment
      - [ ] Copilot can read and act on comments
    - [ ] Workflow example
      - [ ] PR opened
      - [ ] Hotspots analyzes in delta mode
      - [ ] Results posted as comment
      - [ ] Copilot sees violations
      - [ ] Copilot suggests fixes in review
    - [ ] Link to GitHub Action setup docs

- [ ] **Document best practices**
  - [ ] Write "Determinism" section
    - [ ] Explain why Hotspots is reliable for AI
      - [ ] No randomness in analysis
      - [ ] Same code → same LRS (byte-for-byte)
      - [ ] No timestamps or env vars in output
      - [ ] Git-deterministic in delta mode
    - [ ] How AI can trust results
      - [ ] No flaky failures
      - [ ] Reproducible across runs
      - [ ] Suitable for automated decision-making
    - [ ] Testing tip: Run twice, compare output
  - [ ] Write "Caching" section
    - [ ] Why caching matters
      - [ ] Avoid redundant analysis
      - [ ] Speed up iterative workflows
      - [ ] Reduce CI time
    - [ ] Where to cache
      - [ ] Cache JSON output by file hash
      - [ ] Cache analysis results per commit
      - [ ] Use GitHub Actions cache for CI
    - [ ] Invalidation strategy
      - [ ] Invalidate on file change
      - [ ] Invalidate on hotspots version change
      - [ ] Invalidate on config change
    - [ ] Code example: Simple file-based cache
  - [ ] Write "Rate Limiting" section
    - [ ] Why rate limiting matters
      - [ ] Don't overwhelm CI runners
      - [ ] Respect API limits (if using AI APIs)
      - [ ] Avoid unnecessary cost
    - [ ] Strategies
      - [ ] Only analyze changed files in PR
      - [ ] Use delta mode, not snapshot
      - [ ] Batch multiple files in one run
      - [ ] Add cooldown between refactor iterations
    - [ ] Code example: Rate-limited analysis loop
  - [ ] Write "Incremental Analysis" section
    - [ ] Use delta mode for PRs
      - [ ] Only analyzes git diff
      - [ ] Much faster than full snapshot
      - [ ] Focuses AI attention on changes
    - [ ] Filter functions by delta_type
      - [ ] Focus on "added" and "modified"
      - [ ] Ignore "unchanged"
      - [ ] Note "removed" for completeness
    - [ ] Code example: Filter delta output
  - [ ] Write "Feedback Loops" section
    - [ ] Why validation is critical
      - [ ] AI changes must be verified
      - [ ] Complexity can shift, not always decrease
      - [ ] Tests must pass after refactoring
    - [ ] Validation steps
      - [ ] 1. Run tests after AI changes
      - [ ] 2. Re-run hotspots to verify LRS decreased
      - [ ] 3. Check for new violations
      - [ ] 4. Ensure overall complexity didn't shift elsewhere
    - [ ] Code example: Validation loop
    - [ ] What to do if validation fails
      - [ ] Revert changes
      - [ ] Ask AI to try different approach
      - [ ] Add constraints to prompt

- [ ] **Add troubleshooting section**
  - [ ] "AI can't parse JSON"
    - [ ] Cause: Unexpected JSON format
    - [ ] Solution 1: Check hotspots version
    - [ ] Solution 2: Use @hotspots/types for validation
    - [ ] Solution 3: Show AI the schema first
    - [ ] Code example: Robust JSON parsing with error handling
  - [ ] "Analysis is slow"
    - [ ] Cause 1: Analyzing too many files
      - [ ] Solution: Use delta mode for PRs
      - [ ] Solution: Use include/exclude patterns in config
    - [ ] Cause 2: Large codebase
      - [ ] Solution: Analyze specific directories only
      - [ ] Solution: Use parallel analysis (future feature)
    - [ ] Cause 3: Running in loop without caching
      - [ ] Solution: Implement caching (see best practices)
  - [ ] "False positives"
    - [ ] Cause: Function is complex but well-tested/intentional
    - [ ] Solution 1: Use suppression comments
      - [ ] `// hotspots-ignore: reason`
    - [ ] Solution 2: Adjust thresholds in config
      - [ ] Raise critical threshold if too strict
    - [ ] Solution 3: Use custom policy
      - [ ] Different standards for different directories
  - [ ] "AI suggests bad refactorings"
    - [ ] Cause: AI doesn't understand full context
    - [ ] Solution 1: Provide more context in prompt
      - [ ] Include surrounding code
      - [ ] Explain purpose of function
    - [ ] Solution 2: Validate with tests
      - [ ] Always run tests after AI changes
    - [ ] Solution 3: Review AI suggestions before applying
      - [ ] Don't blindly accept
  - [ ] "Violations don't make sense"
    - [ ] Cause: Misunderstanding of metrics
    - [ ] Solution: Read docs/metrics-rationale.md
    - [ ] Solution: Use hotspots_explain tool (MCP)
    - [ ] Solution: Check individual metrics (CC, ND, FO, NS)

- [ ] **Update main README.md**
  - [ ] Add "AI-First Design" section
    - [ ] Read current README.md
    - [ ] Find appropriate location (after "Features" section)
    - [ ] Write AI-First subsection
      - [ ] Title: "🤖 Built for AI Coding Assistants"
      - [ ] 2-3 sentence pitch
        - [ ] "Hotspots is designed from day one for AI-assisted development"
        - [ ] "Deterministic, machine-readable output"
        - [ ] "Direct integration with Claude, GPT-4, Cursor"
      - [ ] Bullet points
        - [ ] ✅ Claude MCP Server for direct tool access
        - [ ] ✅ Structured JSON output with TypeScript types
        - [ ] ✅ Deterministic analysis (no flaky results)
        - [ ] ✅ Fast execution for iterative workflows
      - [ ] Link to docs/AI_INTEGRATION.md for details
  - [ ] Add MCP server example
    - [ ] Create "Quick Start with Claude" subsection
    - [ ] Show installation command
      - [ ] `npm install -g @hotspots/mcp-server`
    - [ ] Show configuration snippet
      - [ ] claude_desktop_config.json example
    - [ ] Show example conversation
      - [ ] User asks Claude to analyze code
      - [ ] Claude calls hotspots_analyze
      - [ ] Claude presents results
    - [ ] Link to MCP server docs
  - [ ] Highlight deterministic output in Features
    - [ ] Find "Features" section
    - [ ] Add/emphasize determinism bullet point
      - [ ] "🎯 Deterministic - Same code always produces same results"
      - [ ] Mention "Perfect for AI-driven workflows"
  - [ ] Update badges section (if exists)
    - [ ] Add npm badge for @hotspots/mcp-server
    - [ ] Add "AI-First" badge (custom badge?)
  - [ ] Add AI workflows to Use Cases
    - [ ] Find or create "Use Cases" section
    - [ ] Add: "Automated code review with AI"
    - [ ] Add: "AI-driven iterative refactoring"
    - [ ] Add: "Complexity-aware code generation"
    - [ ] Link to AI_INTEGRATION.md for each

**Acceptance:**
- ✅ AI_INTEGRATION.md covers all common workflows
- ✅ Examples for 3+ AI assistants
- ✅ Clear, copy-pasteable code examples
- ✅ README.md prominently features AI integration

**Estimated effort:** 1 day

---

### 7.5 Reference Implementation Examples

**Priority:** P1 (Show, don't just tell)

**Status:** ⏳ **IN PROGRESS**

**Problem:** Developers learn best from working code. Need reference implementations showing AI+Hotspots patterns.

**Tasks:**

- [ ] **Set up examples/ai-agents/ directory structure**
  - [ ] Create directory structure
    - [ ] Create `examples/ai-agents/` at project root
    - [ ] Create `examples/ai-agents/shared/` for utilities
    - [ ] Create subdirectory for each example
      - [ ] `examples/ai-agents/refactor-loop/`
      - [ ] `examples/ai-agents/pre-commit-review/`
      - [ ] `examples/ai-agents/constrained-generation/`
      - [ ] `examples/ai-agents/pr-reviewer/`
  - [ ] Set up TypeScript configuration
    - [ ] Create `examples/ai-agents/tsconfig.json`
      - [ ] Set target: ES2022
      - [ ] Set module: commonjs
      - [ ] Enable strict mode
      - [ ] Set rootDir and outDir
    - [ ] Create `examples/ai-agents/package.json`
      - [ ] Set name: "hotspots-ai-examples"
      - [ ] Add scripts: build, test, clean
      - [ ] Add dependencies
        - [ ] @hotspots/types
        - [ ] execa (for running hotspots)
        - [ ] openai (for GPT examples)
        - [ ] @anthropic-ai/sdk (for Claude examples)
      - [ ] Add devDependencies
        - [ ] typescript
        - [ ] @types/node
        - [ ] tsx (for running TS directly)
  - [ ] Create main README.md
    - [ ] Title: "Hotspots AI Agent Examples"
    - [ ] Overview paragraph
    - [ ] Prerequisites section (Node.js, hotspots, API keys)
    - [ ] Directory structure explanation
    - [ ] List all examples with short descriptions
    - [ ] Installation instructions: `npm install`
    - [ ] How to run examples
    - [ ] Note about API keys (set via env vars)

- [ ] **Create shared utilities**
  - [ ] Implement hotspots-client.ts
    - [ ] Import execa and @hotspots/types
    - [ ] Create HotspotsClient class
      - [ ] Constructor accepts binary path (optional)
      - [ ] async analyze(options) method
        - [ ] Accept path, mode, minLrs, config
        - [ ] Build CLI arguments array
        - [ ] Run hotspots with execa
        - [ ] Capture stdout and stderr
        - [ ] Handle errors (binary not found, parse error)
        - [ ] Parse JSON output
        - [ ] Validate with @hotspots/types
        - [ ] Return typed HotspotsOutput
      - [ ] async analyzeFile(filePath, mode) method (convenience)
      - [ ] async analyzeDirectory(dirPath, mode) method (convenience)
    - [ ] Add error handling
      - [ ] HotspotsNotFoundError class
      - [ ] HotspotsParseError class
      - [ ] HotspotsExecutionError class
    - [ ] Add JSDoc comments
    - [ ] Export HotspotsClient and error classes
  - [ ] Implement ai-prompts.ts
    - [ ] Create prompt templates as constants
    - [ ] ANALYZE_PROMPT: Template for asking AI to analyze complexity
      - [ ] Include JSON schema reference
      - [ ] Ask for specific, actionable suggestions
    - [ ] REFACTOR_PROMPT: Template for refactoring suggestions
      - [ ] Include current LRS and target LRS
      - [ ] Ask for code changes
      - [ ] Emphasize test preservation
    - [ ] EXPLAIN_PROMPT: Template for explaining complexity
      - [ ] Include metrics breakdown
      - [ ] Ask for plain English explanation
    - [ ] GENERATE_WITH_CONSTRAINT_PROMPT: Template for constrained generation
      - [ ] Include LRS target
      - [ ] Ask to generate simple, maintainable code
    - [ ] createRefactorPrompt(functionCode, lrs, targetLrs) helper
      - [ ] Fill template with actual values
      - [ ] Return formatted prompt string
    - [ ] createExplainPrompt(functionName, lrs, metrics) helper
    - [ ] Export all templates and helpers
  - [ ] Implement result-parser.ts
    - [ ] Import @hotspots/types
    - [ ] Create parsing utilities
      - [ ] parseHotspotsOutput(json: string): HotspotsOutput
        - [ ] Try to parse JSON
        - [ ] Validate structure
        - [ ] Return typed object or throw
      - [ ] getHighRiskFunctions(output, minLrs): FunctionReport[]
        - [ ] Filter functions by LRS threshold
        - [ ] Sort by LRS descending
        - [ ] Return array
      - [ ] getViolations(output, severity?): Violation[]
        - [ ] Filter violations by severity if provided
        - [ ] Return array
      - [ ] getChangedFunctions(output): FunctionReport[]
        - [ ] Filter for delta_type = "added" or "modified"
        - [ ] Return array
      - [ ] formatFunctionSummary(fn: FunctionReport): string
        - [ ] Return human-readable string
        - [ ] Example: "handleRequest (src/api.ts:45) - LRS 7.2 (High)"
    - [ ] Add error handling for invalid JSON
    - [ ] Export all utilities

- [ ] **Implement refactor-loop example**
  - [ ] Create refactor-loop/ directory structure
    - [ ] Create `examples/ai-agents/refactor-loop/refactor-loop.ts`
    - [ ] Create `examples/ai-agents/refactor-loop/README.md`
    - [ ] Create `examples/ai-agents/refactor-loop/test-input/complex-function.ts`
  - [ ] Write refactor-loop.ts
    - [ ] Import dependencies (HotspotsClient, AI SDK, prompts, parsers)
    - [ ] Define configuration interface
      - [ ] targetLrs: number
      - [ ] maxIterations: number
      - [ ] testCommand: string (command to run tests)
    - [ ] Implement main refactorLoop function
      - [ ] Accept filePath, functionName, config
      - [ ] Initialize HotspotsClient
      - [ ] Step 1: Initial analysis
        - [ ] Run hotspots on file
        - [ ] Find target function by name
        - [ ] Log initial LRS
        - [ ] If LRS < targetLrs, exit early (already good)
      - [ ] Step 2: Refactoring loop
        - [ ] For iteration 1 to maxIterations:
          - [ ] Read current function code from file
          - [ ] Generate refactor prompt with current LRS
          - [ ] Call AI API for refactoring suggestion
          - [ ] Parse AI response (extract code)
          - [ ] Write refactored code to file
          - [ ] Run tests (execute testCommand)
          - [ ] If tests fail:
            - [ ] Log failure
            - [ ] Revert code
            - [ ] Continue to next iteration
          - [ ] Re-run hotspots analysis
          - [ ] Get new LRS
          - [ ] Log improvement
          - [ ] If new LRS < targetLrs:
            - [ ] Success! Exit loop
          - [ ] If new LRS >= old LRS (no improvement):
            - [ ] Log stagnation
            - [ ] Try different approach or exit
      - [ ] Step 3: Final report
        - [ ] Log total iterations
        - [ ] Log initial vs final LRS
        - [ ] Log whether target was reached
    - [ ] Add detailed logging throughout
    - [ ] Add error handling for AI API failures
    - [ ] Add command-line argument parsing
      - [ ] Accept file path, function name, target LRS
      - [ ] Use commander or minimist
  - [ ] Create test input file
    - [ ] Write complex-function.ts with intentionally high LRS
    - [ ] Include multiple issues: nesting, long chains, high CC
    - [ ] Add simple tests that can be run after refactoring
  - [ ] Write comprehensive README.md
    - [ ] Purpose: Iteratively refactor until complexity target met
    - [ ] Prerequisites (API key setup)
    - [ ] Installation: `npm install`
    - [ ] Configuration: Set OPENAI_API_KEY or ANTHROPIC_API_KEY
    - [ ] Usage: `npx tsx refactor-loop.ts test-input/complex-function.ts handleRequest 6.0`
    - [ ] Expected output walkthrough
    - [ ] Customization options
    - [ ] Troubleshooting
  - [ ] Test the example end-to-end
    - [ ] Run with test input
    - [ ] Verify loop executes
    - [ ] Verify tests run after each iteration
    - [ ] Verify LRS decreases
    - [ ] Fix any bugs

- [ ] **Implement pre-commit-review example**
  - [ ] Create pre-commit-review/ directory structure
    - [ ] Create `examples/ai-agents/pre-commit-review/pre-commit-review.ts`
    - [ ] Create `examples/ai-agents/pre-commit-review/README.md`
    - [ ] Create `examples/ai-agents/pre-commit-review/install-hook.sh`
  - [ ] Write pre-commit-review.ts
    - [ ] Import dependencies
    - [ ] Implement main function
      - [ ] Step 1: Get staged files
        - [ ] Run `git diff --cached --name-only --diff-filter=ACM`
        - [ ] Parse output to get list of files
        - [ ] Filter for .ts, .tsx, .js, .jsx files
      - [ ] Step 2: Run hotspots in delta mode
        - [ ] Use HotspotsClient
        - [ ] Analyze staged changes only
        - [ ] Get HotspotsOutput
      - [ ] Step 3: Check for violations
        - [ ] Use getViolations() parser
        - [ ] If no violations, exit 0 (allow commit)
      - [ ] Step 4: AI review if violations found
        - [ ] For each violation:
          - [ ] Get function context from file
          - [ ] Generate AI prompt asking for assessment
          - [ ] Call AI to explain violation
          - [ ] Format AI response
        - [ ] Log all AI explanations
      - [ ] Step 5: User decision
        - [ ] Print summary: "X violations found"
        - [ ] Show AI explanations
        - [ ] Ask: "Proceed with commit? (y/n)"
        - [ ] Read user input from stdin
        - [ ] Exit 0 if 'y', exit 1 if 'n' (block commit)
    - [ ] Add colorized output (chalk or similar)
    - [ ] Add progress indicators
    - [ ] Handle errors gracefully (AI API down, git fails, etc.)
  - [ ] Write install-hook.sh script
    - [ ] Check if .git directory exists
    - [ ] Copy pre-commit-review.ts to .git/hooks/pre-commit
    - [ ] Make executable: `chmod +x`
    - [ ] Add shebang: `#!/usr/bin/env npx tsx`
    - [ ] Print success message
  - [ ] Write comprehensive README.md
    - [ ] Purpose: Review staged changes before commit
    - [ ] Prerequisites (hotspots, Node.js, API key)
    - [ ] Installation
      - [ ] Run `npm install` in examples/ai-agents
      - [ ] Run `./install-hook.sh` to set up git hook
      - [ ] Set OPENAI_API_KEY or ANTHROPIC_API_KEY
    - [ ] Usage: Commit as normal, hook runs automatically
    - [ ] Example output walkthrough
    - [ ] How to bypass: `git commit --no-verify`
    - [ ] Customization options
  - [ ] Test the example
    - [ ] Install hook in test repo
    - [ ] Stage complex file
    - [ ] Attempt commit
    - [ ] Verify AI review runs
    - [ ] Test blocking (choose 'n')
    - [ ] Test allowing (choose 'y')

- [ ] **Implement constrained-generation example**
  - [ ] Create constrained-generation/ directory structure
    - [ ] Create `examples/ai-agents/constrained-generation/constrained-generation.ts`
    - [ ] Create `examples/ai-agents/constrained-generation/README.md`
  - [ ] Write constrained-generation.ts
    - [ ] Import dependencies
    - [ ] Define configuration interface
      - [ ] maxLrs: number (complexity target)
      - [ ] maxAttempts: number
      - [ ] outputFile: string (where to write generated code)
    - [ ] Implement main generateWithConstraint function
      - [ ] Accept prompt (what to generate), config
      - [ ] Step 1: Generate initial code
        - [ ] Create AI prompt with complexity constraint
          - [ ] Include maxLrs target
          - [ ] Ask for simple, maintainable implementation
        - [ ] Call AI API
        - [ ] Extract generated code from response
      - [ ] Step 2: Validate complexity
        - [ ] Write code to temporary file
        - [ ] Run hotspots on temp file
        - [ ] Parse output, get LRS of generated function
        - [ ] Log LRS
      - [ ] Step 3: Retry if too complex
        - [ ] If LRS > maxLrs and attempts < maxAttempts:
          - [ ] Regenerate with stronger constraint
          - [ ] Update prompt: "Previous attempt had LRS X, too complex. Simplify further."
          - [ ] Repeat validation
        - [ ] If LRS <= maxLrs:
          - [ ] Success! Write to outputFile
          - [ ] Return code
        - [ ] If max attempts reached:
          - [ ] Log failure
          - [ ] Return best attempt
      - [ ] Step 4: Report results
        - [ ] Log how many attempts needed
        - [ ] Log final LRS
        - [ ] Show generated code path
    - [ ] Add command-line argument parsing
      - [ ] Accept prompt as argument
      - [ ] Accept maxLrs, maxAttempts, outputFile as flags
    - [ ] Add detailed logging
  - [ ] Write comprehensive README.md
    - [ ] Purpose: Generate code that meets complexity target
    - [ ] Prerequisites (API key)
    - [ ] Installation: `npm install`
    - [ ] Usage: `npx tsx constrained-generation.ts "create a function to validate email" --max-lrs 3.0`
    - [ ] Example output walkthrough
    - [ ] Use cases
      - [ ] Generating simple utility functions
      - [ ] Creating boilerplate with constraints
    - [ ] Limitations (AI might not always succeed)
  - [ ] Test the example
    - [ ] Try generating simple function
    - [ ] Try generating complex function (should retry)
    - [ ] Verify LRS checking works
    - [ ] Verify retry logic works

- [ ] **Implement pr-reviewer example**
  - [ ] Create pr-reviewer/ directory structure
    - [ ] Create `examples/ai-agents/pr-reviewer/pr-reviewer.ts`
    - [ ] Create `examples/ai-agents/pr-reviewer/README.md`
  - [ ] Write pr-reviewer.ts
    - [ ] Import dependencies (Octokit, HotspotsClient, AI SDK)
    - [ ] Define configuration interface
      - [ ] githubToken: string
      - [ ] repoOwner: string
      - [ ] repoName: string
      - [ ] prNumber: number
    - [ ] Implement main reviewPR function
      - [ ] Accept config
      - [ ] Step 1: Checkout PR
        - [ ] Use git or GitHub API to get PR diff
        - [ ] Identify changed files
      - [ ] Step 2: Run hotspots in delta mode
        - [ ] Use HotspotsClient
        - [ ] Analyze PR changes
        - [ ] Get violations
      - [ ] Step 3: Generate AI review comments
        - [ ] For each violation:
          - [ ] Get function code and context
          - [ ] Generate prompt for AI review
          - [ ] Call AI to generate review comment
          - [ ] Format comment with:
            - [ ] File and line number
            - [ ] LRS and risk band
            - [ ] AI explanation
            - [ ] Suggestions for improvement
      - [ ] Step 4: Post comments to GitHub PR
        - [ ] Use Octokit to create review
        - [ ] Add inline comments for each violation
        - [ ] Add summary comment with overview
        - [ ] Set review state (REQUEST_CHANGES if critical, COMMENT otherwise)
      - [ ] Step 5: Report results
        - [ ] Log how many comments posted
        - [ ] Log PR review URL
    - [ ] Add command-line argument parsing
      - [ ] Accept repo owner, name, PR number
      - [ ] Read GitHub token from env var
    - [ ] Add error handling (API failures, rate limits)
  - [ ] Write comprehensive README.md
    - [ ] Purpose: Automated AI-powered PR complexity review
    - [ ] Prerequisites (GitHub token, API key)
    - [ ] Installation: `npm install`
    - [ ] Setup
      - [ ] Create GitHub token with repo access
      - [ ] Set GITHUB_TOKEN env var
      - [ ] Set OPENAI_API_KEY or ANTHROPIC_API_KEY
    - [ ] Usage: `npx tsx pr-reviewer.ts --owner myorg --repo myrepo --pr 123`
    - [ ] Example output
    - [ ] Integration with GitHub Actions
      - [ ] Provide example workflow YAML
      - [ ] Trigger on pull_request event
    - [ ] Customization options
  - [ ] Test the example
    - [ ] Create test PR with complexity violations
    - [ ] Run pr-reviewer against test PR
    - [ ] Verify comments are posted
    - [ ] Verify AI reviews are helpful
    - [ ] Test with PR that passes (no comments)

- [ ] **Add integration tests**
  - [ ] Set up test infrastructure
    - [ ] Create `examples/ai-agents/tests/` directory
    - [ ] Install testing dependencies
      - [ ] vitest or jest
      - [ ] @types/jest
    - [ ] Create test configuration
      - [ ] Set up test timeout (longer for AI calls)
      - [ ] Configure env var loading (.env.test)
  - [ ] Test refactor-loop
    - [ ] Create refactor-loop.test.ts
    - [ ] Test with mock AI (no API key required)
      - [ ] Mock AI responses
      - [ ] Verify loop logic
      - [ ] Verify termination conditions
    - [ ] Test with real AI (if API key available)
      - [ ] Skip if no API key: `test.skipIf(!process.env.OPENAI_API_KEY)`
      - [ ] Run on test fixture
      - [ ] Verify LRS decreases
      - [ ] Verify determinism (same result on re-run)
  - [ ] Test pre-commit-review
    - [ ] Create pre-commit-review.test.ts
    - [ ] Test git diff parsing
    - [ ] Test violation detection
    - [ ] Test user prompt (mock stdin)
    - [ ] Test with mock AI
  - [ ] Test constrained-generation
    - [ ] Create constrained-generation.test.ts
    - [ ] Test with mock AI
    - [ ] Verify retry logic
    - [ ] Verify constraint checking
  - [ ] Test pr-reviewer
    - [ ] Create pr-reviewer.test.ts
    - [ ] Test with mock GitHub API
    - [ ] Test comment generation
    - [ ] Test with mock AI
  - [ ] Test shared utilities
    - [ ] Test HotspotsClient
      - [ ] Test analyze() with real hotspots
      - [ ] Test error handling (binary not found)
      - [ ] Test JSON parsing
    - [ ] Test result-parser
      - [ ] Test parsing valid JSON
      - [ ] Test parsing invalid JSON (error)
      - [ ] Test filter functions
    - [ ] Test ai-prompts
      - [ ] Test template filling
      - [ ] Verify prompts contain necessary context
  - [ ] Run all tests in CI
    - [ ] Add test script to package.json
    - [ ] Run tests without API keys (mock tests only)
    - [ ] Optionally run real tests if secrets available

- [ ] **Polish and finalize**
  - [ ] Review all code for consistency
    - [ ] Consistent error handling
    - [ ] Consistent logging format
    - [ ] Consistent naming conventions
    - [ ] Add missing JSDoc comments
  - [ ] Review all READMEs
    - [ ] Check for typos
    - [ ] Verify all commands work
    - [ ] Ensure examples are copy-pasteable
    - [ ] Add "Next Steps" section to each
  - [ ] Create master examples README
    - [ ] Update examples/ai-agents/README.md
    - [ ] Add table of contents
    - [ ] Add comparison table (which example for which use case)
    - [ ] Add "Getting Started" quick guide
  - [ ] Add LICENSE files
    - [ ] Add MIT LICENSE to examples/ai-agents/
    - [ ] Add copyright notice to each source file
  - [ ] Test all examples end-to-end
    - [ ] Fresh npm install
    - [ ] Run each example
    - [ ] Verify they all work
    - [ ] Fix any issues
  - [ ] Record demo videos (optional)
    - [ ] Screen recording of each example
    - [ ] Add to README or link to YouTube
  - [ ] Link from main project docs
    - [ ] Update docs/AI_INTEGRATION.md
    - [ ] Link to each example with description
    - [ ] Add "Try the examples" call-to-action

**Example Structure:**
```
examples/ai-agents/
├── README.md
├── package.json
├── tsconfig.json
├── refactor-loop/
│   ├── refactor-loop.ts
│   ├── README.md
│   └── test-input/
│       └── complex-function.ts
├── pre-commit-review/
│   ├── pre-commit-review.ts
│   ├── README.md
│   └── install-hook.sh
├── constrained-generation/
│   ├── constrained-generation.ts
│   └── README.md
├── pr-reviewer/
│   ├── pr-reviewer.ts
│   └── README.md
├── shared/
│   ├── hotspots-client.ts
│   ├── ai-prompts.ts
│   └── result-parser.ts
└── tests/
    ├── refactor-loop.test.ts
    ├── pre-commit-review.test.ts
    ├── constrained-generation.test.ts
    ├── pr-reviewer.test.ts
    └── shared.test.ts
```

**Acceptance:**
- ✅ 4+ working reference implementations
- ✅ Each example has clear README
- ✅ Examples use `@hotspots/types` for type safety
- ✅ Code is production-quality (error handling, logging)
- ✅ Tests prove examples work

**Estimated effort:** 1 day

---

## Phase 7 Timeline

**Day 1 (Today):**
- ✅ Planning and requirements
- Fix clippy errors (3 hours)

**Day 2:**
- JSON Schema & Types (full day)
- Publish `@hotspots/types` to npm

**Day 3-4:**
- Claude MCP Server (2 days)
- Test with Claude Desktop
- Publish to npm and MCP registry

**Day 5:**
- AI Integration Documentation
- Update README.md
- Write AI_INTEGRATION.md

**Day 6:**
- Reference Implementation Examples
- Create 4+ working examples
- Test and validate

**Day 7:**
- Polish and review
- Update ROADMAP.md
- Prepare v1.0.0 release

---

## Phase 7 Success Metrics

**Technical:**
- ✅ Zero clippy errors
- ✅ JSON Schema validates all outputs
- ✅ TypeScript types match runtime output
- ✅ MCP server works in Claude Desktop
- ✅ All 145+ tests pass

**Documentation:**
- ✅ AI_INTEGRATION.md covers 5+ workflows
- ✅ 4+ reference implementations
- ✅ README.md highlights AI-first design

**Adoption (post-release):**
- [ ] 10+ repos using Hotspots with AI agents
- [ ] MCP server listed in official MCP registry
- [ ] Featured in AI coding assistant communities
- [ ] Blog posts from AI users showing workflows

---

## Phase 4: Performance

### 4.1 Parallel File Processing

**Priority:** P1 (Critical for large repos)

**Tasks:**

- [ ] Add Rayon for parallel iteration
  - Use `par_iter()` on file list
  - Each file analyzed independently
  - Collect results in thread-safe structure
- [ ] Ensure deterministic output despite parallelism
  - Sort final results after parallel processing
  - No race conditions in aggregation
  - Test: parallel analysis yields identical output to serial
- [ ] Add progress indicator
  - Show "Analyzing 234/1000 files..." during analysis
  - Update every 50 files
  - Only show in TTY (not CI)
- [ ] Benchmark parallelism gains
  - Measure speedup on 1000-file repo
  - Test on 1/2/4/8 core machines
  - Document expected performance
- [ ] Add `--jobs N` flag
  - Control parallelism (default: num_cpus)
  - `--jobs 1` for serial (debugging)

**Acceptance:**
- 10x speedup on 8-core machine for 1000-file repo
- Output is byte-for-byte identical to serial
- All determinism tests still pass

**Estimated effort:** Medium (3-5 days)

---

### 4.2 Incremental Analysis

**Priority:** P1 (Essential for CI performance)

**Problem:** Re-analyzing unchanged files is wasteful. CI should only analyze files changed in PR.

**Tasks:**

- [ ] Design cache key strategy
  - Key: `(file_path, content_hash)`
  - Content hash: SHA256 of file contents
  - Cache location: `.hotspots/cache/` (gitignored)
- [ ] Implement cache storage
  - Store `FunctionRiskReport[]` per file as JSON
  - Indexed by content hash
  - LRU eviction policy (max 1000 files)
- [ ] Add cache lookup logic
  - Hash file contents before parsing
  - Check if hash exists in cache
  - Return cached results if found
  - Otherwise analyze and populate cache
- [ ] Handle PR context optimally
  - In delta mode, only analyze files in `git diff`
  - Reuse cached results for unchanged files from parent snapshot
  - Benchmark: 1000-file repo, 10-file PR → <5s analysis
- [ ] Add cache invalidation
  - Clear cache on hotspots version upgrade
  - Clear cache if config changes (thresholds, weights)
  - `hotspots cache clear` subcommand
- [ ] Add cache statistics
  - Show cache hit/miss rate
  - `hotspots cache stats` subcommand
  - "Cache: 932/1000 hits (93%)" in output

**Acceptance:**
- PR analysis only processes changed files
- Cache hit rate >90% for typical PRs
- Large repo analysis completes in <5s with warm cache
- Cache invalidates correctly on version/config changes

**Estimated effort:** High (1-2 weeks)

---

### 4.3 Optimize Parser Performance

**Priority:** P2 (Polish)

**Tasks:**

- [ ] Profile parser with `perf` or `cargo flamegraph`
  - Identify hot paths
  - Look for unnecessary allocations
- [ ] Optimize SWC parser configuration
  - Disable unused features
  - Minimize AST cloning
- [ ] Use arena allocation for CFG nodes
  - Reduce allocation overhead
  - Use `typed-arena` crate
- [ ] Add parser benchmarks
  - Benchmark suite with Criterion.rs
  - Track parse time per 1k LOC
  - Detect performance regressions in CI

**Acceptance:**
- Parse time <10ms per 1k LOC on typical hardware
- No performance regressions
- Benchmarks run in CI

**Estimated effort:** Medium (5-7 days)

---

## Phase 5: Developer Experience

### 5.1 VS Code Extension

**Priority:** P1 (Real-time feedback)

**Tasks:**

- [ ] Create `vscode-hotspots` repository
  - TypeScript extension project
  - Use VS Code Extension API
- [ ] Implement CodeLens
  - Show LRS above each function
  - Format: "⚡ LRS: 7.2 (High)"
  - Click to show breakdown (CC, ND, FO, NS)
- [ ] Implement diagnostics
  - Underline functions exceeding thresholds
  - Severity: info (watch), warning (attention), error (critical)
  - Hover message shows details + recommendations
- [ ] Add quick actions
  - "Add suppression comment"
  - "Show complexity breakdown"
  - "View historical trend" (if snapshots exist)
- [ ] Add status bar item
  - Show file-level LRS summary
  - Click to open Hotspots panel
- [ ] Implement Hotspots panel
  - Tree view of functions by risk band
  - Sort by LRS
  - Click to jump to function
- [ ] Optimize performance
  - Run analysis in background
  - Debounce on file changes (500ms)
  - Cache results per file
  - Only re-analyze on save

**Acceptance:**
- Extension available in VS Code Marketplace
- Real-time feedback as developers write code
- Quick actions are convenient and fast
- No noticeable lag or stuttering

**Estimated effort:** High (2-3 weeks)

---

### 5.2 Improved Error Messages

**Priority:** P1 (Reduce frustration)

**Tasks:**

- [ ] Categorize error types
  - Parse errors (syntax issues)
  - Unsupported features (generators, etc.)
  - File not found
  - Configuration errors
- [ ] Add contextual error messages
  - Show file path and line number
  - Show snippet of problematic code
  - Suggest fixes
- [ ] Add error codes
  - E001: Parse error
  - E002: Unsupported feature
  - E003: Configuration error
  - Link to documentation for each code
- [ ] Add `--verbose` flag for debugging
  - Show full stack traces
  - Show AST dump
  - Show CFG visualization (as text graph)
- [ ] Add error recovery
  - Continue analyzing other files after parse error
  - Show partial results + errors
  - Exit code 1 if any errors, 0 if all successful

**Acceptance:**
- Errors are clear and actionable
- Users can self-debug common issues
- Errors link to relevant documentation

**Estimated effort:** Medium (3-5 days)

---

### 5.3 SARIF Output

**Priority:** P1 (GitHub Security integration)

**Tasks:**

- [ ] Implement SARIF 2.1.0 schema
  - Use `serde_json` for serialization
  - Map `FunctionRiskReport` to SARIF `result`
- [ ] Map risk bands to SARIF levels
  - Low: "note"
  - Moderate: "warning"
  - High: "warning"
  - Critical: "error"
- [ ] Add SARIF metadata
  - Tool name: "hotspots"
  - Tool version
  - Rule definitions (one per risk band)
- [ ] Add `--format sarif` CLI flag
- [ ] Document GitHub Security integration
  - How to upload SARIF to GitHub
  - Show example workflow
- [ ] Test with GitHub Code Scanning
  - Verify annotations appear in Security tab
  - Verify annotations appear in PR Files Changed

**Acceptance:**
- `hotspots analyze --format sarif` produces valid SARIF
- SARIF uploads to GitHub Security successfully
- High-risk functions appear as warnings in PR

**Estimated effort:** Medium (3-5 days)

---

## Phase 6: Polish & Documentation

### 6.1 Getting Started Guide

**Priority:** P0 (Onboarding)

**Tasks:**

- [ ] Write `docs/getting-started.md`
  - Installation (cargo install, binary download, GitHub Action)
  - 5-minute quickstart
  - First analysis
  - Interpreting results
- [ ] Add GitHub Action setup walkthrough
  - Step-by-step with screenshots
  - Common configurations
  - Troubleshooting
- [ ] Add video tutorial (optional)
  - Record 5-minute walkthrough
  - Upload to YouTube
  - Embed in docs
- [ ] Update README.md
  - Add badges (CI status, crates.io version)
  - Add "Quick Start" section at top
  - Add links to detailed docs
  - Add animated GIF of CLI output

**Acceptance:**
- New user can get started in under 5 minutes
- README is compelling and clear
- Documentation covers common questions

**Estimated effort:** Low (2-3 days)

---

### 6.2 CI/CD Integration Cookbook

**Priority:** P1 (Reduce support burden)

**Tasks:**

- [ ] Write `docs/ci-cd-cookbook.md`
  - GitHub Actions recipes
    - Basic setup
    - Monorepo setup
    - Matrix strategy (multiple Node versions)
    - Custom policies
  - GitLab CI examples
  - CircleCI examples
  - Jenkins examples (Jenkinsfile)
- [ ] Add troubleshooting section
  - "Action fails with permission error" → GITHUB_TOKEN permissions
  - "Analysis is slow" → Enable caching
  - "False positives" → Use suppression comments
- [ ] Add migration guides
  - From CodeClimate
  - From SonarQube
  - From ESLint complexity rules
- [ ] Add monorepo strategies
  - Analyzing multiple packages
  - Per-package policies
  - Aggregate reporting

**Acceptance:**
- Cookbook covers 90% of common use cases
- Users can copy-paste examples
- Troubleshooting saves support time

**Estimated effort:** Medium (3-5 days)

---

### 6.3 Metrics Rationale Documentation

**Priority:** P2 (Build trust)

**Tasks:**

- [ ] Write `docs/metrics-rationale.md`
  - Why CC, ND, FO, NS?
  - Academic references
  - Why logarithmic transforms?
  - Why these specific weights?
- [ ] Add comparison to other tools
  - Hotspots vs Lizard
  - Hotspots vs SonarQube
  - Hotspots vs CodeClimate
  - Table of metrics supported
- [ ] Add validation studies
  - Internal validation (tested on real projects)
  - Bug correlation (if available)
  - Maintainability correlation
- [ ] Add "Designing Your Own Thresholds" guide
  - How to calibrate for your team
  - How to set risk tolerance
  - Example: analyze existing codebase, set thresholds at 90th percentile

**Acceptance:**
- Users understand why metrics are chosen
- Users trust the tool's recommendations
- Users can customize intelligently

**Estimated effort:** Medium (3-5 days)

---

### 6.4 Website & Branding

**Priority:** P2 (Polish)

**Tasks:**

- [ ] Register domain (e.g., hotspots.dev)
- [ ] Create static site
  - Use Hugo, Next.js, or similar
  - Homepage with value proposition
  - Documentation section
  - Blog for release notes
- [ ] Design logo
  - Memorable, professional
  - Works in monochrome (for terminal)
- [ ] Create GitHub social preview
  - Open Graph image
  - Shows in GitHub shares
- [ ] Add analytics (privacy-respecting)
  - Plausible or similar
  - Track docs pageviews
  - Track GitHub Action installs (via badge)

**Acceptance:**
- Professional online presence
- Documentation is easy to find and read
- Branding is consistent

**Estimated effort:** High (1-2 weeks, depending on design)

---

## Success Metrics

**Phase 1 (Language Completeness):**
- [ ] Can analyze 95% of TypeScript/JavaScript projects without errors

**Phase 2 (CI/CD Integration):**
- [ ] GitHub Action works in 100 repos
- [ ] Average PR analysis time <10s
- [ ] Zero CI flakes (100% deterministic)

**Phase 3 (Configuration & Policies):**
- [ ] 50% of users customize config file
- [ ] Suppression comments used in 80% of projects

**Phase 4 (Performance):**
- [ ] <5s analysis for 1000-file repo (warm cache)
- [ ] 10x speedup from parallelism

**Phase 5 (Developer Experience):**
- [ ] VS Code extension has 1k installs
- [ ] 90% of errors self-resolvable from error messages

**Phase 6 (Polish & Documentation):**
- [ ] Time-to-first-analysis <5 minutes for new users
- [ ] Documentation covers 90% of support questions

**Overall Adoption:**
- [ ] 1,000 GitHub stars
- [ ] 100 projects using hotspots in CI
- [ ] Featured in Awesome TypeScript
- [ ] Mentioned in major dev publications

---

## Notes

- Maintain all MVP invariants (determinism, no global state, etc.)
- Preserve byte-for-byte reproducibility
- All new features must include tests
- Performance changes must include benchmarks
- Breaking changes require major version bump

**See also:**
- `docs/roadmap.md` - Full roadmap with all features
- `docs/design-decisions.md` - Architectural decisions
- `docs/invariants.md` - Non-negotiable invariants
