# TASKS.md - Faultline CI/CD Adoption

**Goal:** Make Faultline the go-to CI/CD tool for blocking complexity regressions in TypeScript/JavaScript projects.

**Strategy:** CI/CD first. Analytics later.

**Status:** Post-MVP - building toward v1.0 CI/CD-ready release

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

### 1.1 JavaScript Support

**Priority:** P0 (Critical blocker)

**Problem:** Most projects mix TypeScript and JavaScript. Without JS support, Faultline is incomplete.

**Tasks:**

- [ ] Enable JavaScript parsing in SWC parser configuration
  - Support ES2015+ syntax
  - CommonJS and ESM modules
  - No JSX (separate task)
- [ ] Test that all metrics work identically for JS and TS
  - CC, ND, FO, NS should behave the same
- [ ] Add JS golden test fixtures
  - `tests/fixtures/js/` directory
  - Mirror TS fixtures in plain JS
- [ ] Update documentation
  - `docs/ts-support.md` → `docs/language-support.md`
  - List supported JS features
- [ ] Add file extension handling
  - `.js`, `.mjs`, `.cjs` alongside `.ts`, `.mts`, `.cts`

**Acceptance:**
- Analyze a mixed TS/JS project successfully
- JS and TS functions with identical structure yield identical LRS
- All existing tests still pass (determinism preserved)

**Estimated effort:** Medium (2-3 days)

---

### 1.2 JSX/TSX Support

**Priority:** P1 (High - needed for React adoption)

**Dependencies:** JavaScript support (1.1)

**Tasks:**

- [ ] Enable JSX parsing in SWC configuration
  - JSX in `.jsx`, `.tsx` files
  - JSX pragmas (`@jsx`, `@jsxFrag`)
- [ ] Decide JSX metric handling
  - JSX elements should NOT inflate complexity artificially
  - JSX expressions containing control flow (ternaries, &&) DO count
  - Document rationale
- [ ] Add JSX golden test fixtures
  - React component with conditional rendering
  - Component with map/loops
  - Complex nested JSX
- [ ] Update documentation with JSX support status

**Acceptance:**
- Analyze a React TypeScript project successfully
- JSX elements don't create false complexity signals
- Conditional rendering and loops are measured correctly

**Estimated effort:** Medium (3-4 days)

---

### 1.3 Fix Break/Continue CFG Routing

**Priority:** P0 (Correctness issue)

**Problem:** Break/continue currently route to CFG exit instead of loop exit/header. This slightly inflates CC for loops.

**Tasks:**

- [ ] Add loop context tracking to CFG builder
  - Stack of loop header/exit nodes during traversal
  - Push on loop entry, pop on loop exit
- [ ] Route `break` to loop exit node (top of stack)
- [ ] Route `continue` to loop header node (top of stack)
- [ ] Handle labeled break/continue
  - Resolve label to correct loop in stack
  - Error if label not found
- [ ] Add comprehensive loop tests
  - Nested loops with breaks
  - Labeled break across multiple levels
  - Continue in nested loops
- [ ] Update golden fixtures to reflect corrected CC

**Acceptance:**
- Break routes to loop exit, not CFG exit
- Continue routes to loop header
- CC values are accurate for complex loops
- Labeled break/continue work correctly

**Estimated effort:** Medium (3-5 days)

---

## Phase 2: CI/CD Integration

### 2.1 GitHub Action (Core)

**Priority:** P0 (Critical for adoption)

**Tasks:**

- [ ] Create `faultline-action` repository
  - Template from `actions/typescript-action`
  - Setup TypeScript + Neon (or shell out to binary)
- [ ] Implement action inputs
  - `path` (default: `.`)
  - `policy` (default: `critical-introduction`)
  - `min-lrs` (optional float)
  - `config` (optional path to config file)
  - `fail-on` (error, warn, never)
- [ ] Implement action outputs
  - `violations` (JSON array of violations)
  - `passed` (boolean)
  - `summary` (markdown summary)
- [ ] Add binary caching
  - Cache faultline binary by version
  - Download/extract on cache miss
  - Verify checksum
- [ ] Handle PR context automatically
  - Detect PR via `GITHUB_EVENT_NAME`
  - Extract merge-base from GitHub API
  - Run delta mode comparing to merge-base
  - Run snapshot mode for mainline pushes
- [ ] Basic PR comment posting
  - Use GitHub API to post comment
  - Show top violations
  - Link to full report (artifact)
  - Update existing comment (don't spam)
- [ ] Job summary output
  - Use `GITHUB_STEP_SUMMARY`
  - Markdown table of top violations
  - Pass/fail status

**Acceptance:**
- Action can be used in any repo with:
  ```yaml
  - uses: faultline-action@v1
  ```
- Automatically detects PR vs mainline
- Posts results to PR comments
- Job summary shows violations
- Passes/fails based on policy

**Estimated effort:** High (1-2 weeks)

---

### 2.2 Proactive Warning System

**Priority:** P0 (Key differentiator for CI/CD)

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
  - `// faultline-watch-ok: reason` comment
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
- Developers see warnings before functions become critical
- Warnings give actionable recommendations
- Trend detection identifies growing complexity early
- Warnings can be acknowledged/suppressed for intentional complexity
- "Time to critical" estimates help with prioritization

**Estimated effort:** High (1 week)

---

### 2.3 HTML Report Generation

**Priority:** P0 (Better UX than JSON)

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
- [ ] Generate `faultline-report.html` artifact
  - Write HTML to `.faultline/report.html` or `faultline-report.html`
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
- Running `faultline analyze --format html` generates HTML report
- Report is interactive (sorting, filtering work)
- GitHub Action uploads report as artifact
- Report is readable and actionable

**Estimated effort:** High (1 week)

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
  - Suggest adding `// faultline-ignore` comment
  - Or link to refactoring guide

**Acceptance:**
- PR diff shows inline annotations for high-risk functions
- Annotations link to specific lines
- Developers see feedback in context of their changes

**Estimated effort:** High (5-7 days)

---

## Phase 3: Configuration & Policies

### 3.1 Configuration File

**Priority:** P0 (Required for project-specific policies)

**Tasks:**

- [ ] Design config schema
  - Use JSON for simplicity (extendable to YAML later)
  - See `docs/roadmap.md` for proposed schema
- [ ] Implement config file loading
  - Search order: CLI flag, `.faultlinerc.json`, `faultline.config.json`, `package.json:faultline`
  - Parse and validate schema
  - Merge with CLI flags (CLI takes precedence)
- [ ] Add config validation
  - Schema validation (reject unknown fields)
  - Range validation (thresholds must be positive, weights 0-10)
  - Clear error messages for invalid config
- [ ] Support `include`/`exclude` patterns
  - Use glob patterns (via `globset` crate)
  - Default exclude: `**/*.test.ts`, `**/*.spec.ts`, `**/node_modules/**`
- [ ] Support custom thresholds
  - Low/moderate/high/critical boundaries
  - Per-project risk tolerance
- [ ] Support custom risk weights
  - Allow projects to prioritize different metrics
  - E.g., backend might weight FO higher (more calls = more coupling)
- [ ] Document config file format
  - Add `docs/configuration.md`
  - Examples for common scenarios (monorepo, test exclusion, strict mode)
- [ ] Add `faultline config validate` subcommand
  - Validates config file without running analysis
  - Useful for CI pre-flight checks

**Acceptance:**
- Projects can customize behavior via config file
- Invalid config fails with clear error
- Config is deterministic (no env vars, no timestamps)
- Documentation covers common use cases

**Estimated effort:** Medium (5-7 days)

---

### 3.2 Suppression Comments

**Priority:** P1 (Handle false positives)

**Tasks:**

- [ ] Parse suppression comments from source
  - `// faultline-ignore` above function
  - `// faultline-ignore: reason` with required reason
  - `// faultline-ignore-next-line` for inline suppression
- [ ] Update function discovery to mark suppressed functions
  - Add `suppressed: bool` field to `FunctionNode`
  - Extract reason from comment
- [ ] Exclude suppressed functions from policy checks
  - Still analyze and report (as "ignored")
  - Don't count toward violation thresholds
  - Show in report with suppression reason
- [ ] Add suppression report
  - Show all suppressed functions
  - Reason for each suppression
  - Warn if suppression has no reason
- [ ] Add lint rule for suppressions
  - Warn if suppression comment missing reason
  - Configurable: `require_suppression_reason: true`
- [ ] Document suppression guidelines
  - When to suppress (legacy code, algorithm complexity)
  - When NOT to suppress (laziness, "I'll fix it later")
  - Require code review for suppressions

**Example:**
```typescript
// faultline-ignore: complex algorithm, well-tested, refactor planned for Q2
function legacyParser(input: string) {
  // 500 lines of spaghetti
}
```

**Acceptance:**
- Functions with suppression comments are excluded from policy failures
- Suppressions require reasons (or emit warning)
- Suppression report shows all ignored functions
- Suppressions are auditable

**Estimated effort:** Low (2-3 days)

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
  - Cache location: `.faultline/cache/` (gitignored)
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
  - Clear cache on faultline version upgrade
  - Clear cache if config changes (thresholds, weights)
  - `faultline cache clear` subcommand
- [ ] Add cache statistics
  - Show cache hit/miss rate
  - `faultline cache stats` subcommand
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

- [ ] Create `vscode-faultline` repository
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
  - Click to open Faultline panel
- [ ] Implement Faultline panel
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
  - Tool name: "faultline"
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
- `faultline analyze --format sarif` produces valid SARIF
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
  - Faultline vs Lizard
  - Faultline vs SonarQube
  - Faultline vs CodeClimate
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

- [ ] Register domain (e.g., faultline.dev)
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
- [ ] 100 projects using faultline in CI
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
