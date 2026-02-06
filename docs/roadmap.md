# Hotspots Roadmap - CI/CD First

## Vision

Hotspots will be **the** go-to CI/CD tool for blocking complexity regressions in TypeScript/JavaScript projects. Success means being added to every new project's GitHub Actions workflow by default.

## Strategy

**Phase 1: CI/CD Adoption** (Next 6 months)
- Make Hotspots indispensable for PR checks
- Focus on fast, reliable, actionable feedback
- GitHub-first integration

**Phase 2: Analytics Upsell** (Future)
- Once adopted in CI, layer in historical trends
- Premium features for engineering managers
- Dashboards and long-term insights

---

## Priority Framework

**P0 (Critical)** - Blocks CI/CD adoption
**P1 (High)** - Significantly improves CI/CD experience
**P2 (Medium)** - Nice-to-have for CI/CD
**P3 (Low)** - Analytics/future features

---

## Missing Features - CI/CD Prioritized

### Language Support

#### P0: JavaScript Support
**Status:** Critical blocker for adoption

Most projects mix TypeScript and JavaScript. Without JS support, Hotspots can't analyze a significant portion of most codebases.

**Scope:**
- Plain JavaScript (ES2015+)
- CommonJS and ESM modules
- Same metrics as TypeScript (CC, ND, FO, NS)

**Out of scope (separate tasks):**
- JSX/TSX (React)
- Vue SFC
- Svelte

**Estimated effort:** Medium (parser already supports JS via SWC)

---

#### P1: JSX/TSX Support
**Status:** Needed for React projects

React is ubiquitous. JSX/TSX support is essential for frontend adoption.

**Scope:**
- JSX syntax in .jsx/.tsx files
- JSX elements don't inflate complexity metrics artificially
- Same function discovery rules

**Dependencies:** JavaScript support

**Estimated effort:** Medium

---

### CI/CD Integration

#### P0: GitHub Action
**Status:** Critical for adoption

**Scope:**
- Official `hotspots-action` repository
- Inputs:
  - `path` (default: `.`)
  - `policy` (default: `critical-introduction`)
  - `min-lrs` (optional)
  - `config` (path to config file)
- Outputs:
  - PR comment with results
  - Inline annotations on changed files
  - Job summary with top violations
  - Pass/fail based on policy
- Handles PR context automatically (merge-base comparison)
- Fast: caches hotspots binary, runs only on changed files

**Acceptance:**
```yaml
- uses: hotspots-action@v1
  with:
    policy: critical-introduction
```

**Estimated effort:** High (new repo, Actions API, PR commenting)

---

#### P1: GitLab CI Template
**Status:** Important for enterprise adoption

**Scope:**
- Official `.gitlab-ci.yml` template
- Merge request comments
- Pipeline pass/fail

**Estimated effort:** Medium

---

#### P1: Incremental Analysis
**Status:** Performance critical for large repos

**Problem:** Currently analyzes all files every time, even if unchanged.

**Scope:**
- Cache analysis results keyed by (file_path, content_hash)
- Only analyze changed files in PR context
- Reuse cached results for unchanged files
- Benchmark: 1000-file repo should analyze in <5s on cache hit

**Estimated effort:** High (caching strategy, invalidation, storage)

---

### Configuration & Customization

#### P0: Configuration File
**Status:** Critical for project-specific policies

**Scope:**
- `.hotspotsrc.json` or `hotspots.config.json`
- Schema:
  ```json
  {
    "include": ["src/**/*.ts"],
    "exclude": ["**/*.test.ts", "**/*.spec.ts"],
    "policies": {
      "critical-introduction": "error",
      "excessive-risk-regression": "warn",
      "net-repo-regression": "off"
    },
    "thresholds": {
      "low": 3,
      "moderate": 6,
      "high": 9
    },
    "riskWeights": {
      "cc": 1.0,
      "nd": 0.8,
      "fo": 0.6,
      "ns": 0.7
    }
  }
  ```
- CLI flag: `--config path/to/config.json`
- Defaults if not specified

**Acceptance:**
- Config file overrides defaults
- Invalid config fails with clear error
- Config is deterministic (no env vars, no timestamps)

**Estimated effort:** Medium

---

#### P1: Suppression Comments
**Status:** Needed for false positives

**Scope:**
- `// hotspots-ignore` above function disables analysis
- `// hotspots-ignore-next-line` for inline suppression
- Suppressed functions appear in report as "ignored" (not counted in policies)
- Reason required: `// hotspots-ignore: legacy code, scheduled for refactor`

**Example:**
```typescript
// hotspots-ignore: complex algorithm, well-tested
function legacyParser(input: string) {
  // 500 lines of spaghetti
}
```

**Estimated effort:** Low (parser already has comments in AST)

---

#### P2: Custom Policies
**Status:** Advanced customization

**Scope:**
- User-defined policies via config
- Examples:
  - Block functions with ND > 5
  - Block files with total LRS > 50
  - Warn when any function enters "high" band
- Policy language (simple boolean expressions)

**Estimated effort:** High (policy DSL, validation)

---

### Reporting & Output

#### P0: HTML Reports for PRs
**Status:** Critical for CI/CD UX

**Scope:**
- Generate `hotspots-report.html` artifact
- Upload to GitHub Actions artifacts
- Link in PR comment
- Interactive table:
  - Sort by column
  - Filter by file/risk band
  - Color-coded risk bands
  - Expandable function bodies (syntax highlighted)
- Delta view: show before/after for modified functions

**Estimated effort:** High (HTML template, JavaScript, CSS)

---

#### P1: SARIF Output
**Status:** Integration with GitHub Code Scanning

**Scope:**
- `--format sarif` output
- Maps high-risk functions to SARIF locations
- Integrates with GitHub Security tab
- Appears as "warnings" in Files Changed view

**Estimated effort:** Medium (SARIF schema implementation)

---

#### P1: GitHub PR Annotations
**Status:** In-context feedback

**Scope:**
- Inline annotations on PR diffs via GitHub API
- Show LRS and risk band directly on function definitions
- Only annotate changed functions
- Group by file for readability

**Estimated effort:** High (GitHub API, diff parsing)

---

#### P2: Terminal UI Improvements
**Status:** Local development UX

**Scope:**
- Colored output (risk bands: green/yellow/orange/red)
- Progress bar for large repos
- Interactive mode: `hotspots analyze --watch`
- Better error messages with suggestions

**Estimated effort:** Medium

---

### Performance & Scalability

#### P1: Parallel File Processing
**Status:** Performance at scale

**Problem:** Single-threaded analysis bottleneck on large repos.

**Scope:**
- Rayon for parallel file iteration
- Process independent files concurrently
- Maintain deterministic output (sort after parallel processing)
- Benchmark: 10x speedup on 8-core machine for 1000-file repo

**Estimated effort:** Medium (parallelization, determinism testing)

---

#### P2: Remote Caching
**Status:** Shared CI cache

**Scope:**
- Optional remote cache (S3, GCS, HTTP)
- Teams share analysis results across CI runs
- Authentication and security

**Dependencies:** Incremental analysis

**Estimated effort:** Very High

---

### Developer Experience

#### P1: VS Code Extension
**Status:** Real-time feedback

**Scope:**
- Inline CodeLens showing LRS above functions
- Diagnostics for functions exceeding thresholds
- Quick actions:
  - Add suppression comment
  - Show complexity breakdown
  - View historical LRS trend (if snapshots exist)
- Status bar item showing file-level LRS

**Estimated effort:** High (new repo, VS Code API, TypeScript)

---

#### P2: LSP Server
**Status:** Editor-agnostic integration

**Scope:**
- Standalone LSP server (`hotspots lsp`)
- Works in any LSP-compatible editor (Neovim, Emacs, Sublime, etc.)
- Provides diagnostics and code actions

**Dependencies:** VS Code extension (reuse logic)

**Estimated effort:** High

---

### Correctness & Completeness

#### P0: Fix Break/Continue CFG Routing
**Status:** Known correctness issue

**Problem:** Break/continue currently route to CFG exit instead of loop exit/header.

**Impact:** Slightly inflates cyclomatic complexity for loops with breaks.

**Scope:**
- Track loop context during CFG construction
- Route break to loop exit node
- Route continue to loop header node
- Update tests

**Estimated effort:** Medium (requires loop context tracking)

---

#### P2: Generator Functions Support
**Status:** Currently errors on `function*`

**Scope:**
- Analyze generator functions
- Handle `yield` expressions (treat as potential exits?)
- Async generators (`async function*`)

**Estimated effort:** Medium

---

#### P2: Async/Await Complexity
**Status:** Currently treats as normal functions

**Scope:**
- Should `await` count toward complexity?
- Research: does async increase cognitive load?
- Possibly add async-specific metrics

**Estimated effort:** Low (research + decision)

---

### Testing & Quality

#### P1: Comprehensive Test Suite
**Status:** Expand beyond golden tests

**Scope:**
- Integration tests for CI workflows
- Snapshot tests for HTML reports
- Performance regression tests
- Fuzz testing for parser edge cases

**Estimated effort:** Medium (ongoing)

---

#### P2: Benchmark Suite
**Status:** Track performance over time

**Scope:**
- Criterion.rs benchmarks
- Track analysis time per 1k LOC
- Regression detection in CI

**Estimated effort:** Low

---

### Documentation

#### P0: Getting Started Guide
**Status:** Onboarding friction

**Scope:**
- 5-minute quickstart
- GitHub Action setup walkthrough
- Common patterns (monorepos, test exclusion)
- FAQ
- Troubleshooting

**Estimated effort:** Low

---

#### P1: CI/CD Integration Cookbook
**Status:** Real-world examples

**Scope:**
- GitHub Actions recipes
- GitLab CI examples
- CircleCI, Jenkins, etc.
- Monorepo strategies
- Migration guides (from SonarQube, CodeClimate)

**Estimated effort:** Medium

---

#### P2: Metrics Rationale Documentation
**Status:** Build trust

**Scope:**
- Why these four metrics?
- Academic references
- Rationale for weights and transforms
- Comparison to other tools (Lizard, SonarQube)

**Estimated effort:** Low

---

## Feature Comparison: MVP vs CI/CD-Ready

| Feature | MVP (Current) | CI/CD-Ready (Goal) |
|---------|---------------|-------------------|
| Languages | TypeScript only | TypeScript + JavaScript + JSX/TSX |
| CI Integration | Manual CLI | GitHub Action, GitLab CI template |
| Configuration | CLI flags only | Config file + CLI overrides |
| Suppression | None | Comment-based ignore |
| Reports | Text + JSON | + HTML + SARIF + PR annotations |
| Performance | Single-threaded | Parallel + incremental + caching |
| Editor Support | None | VS Code extension + LSP |
| Policy Enforcement | 3 hardcoded policies | Customizable policies in config |
| Correctness | Break/continue workaround | Full CFG accuracy |

---

## Success Metrics

**Adoption Milestones:**
- [ ] 100 GitHub repos using hotspots-action
- [ ] 1,000 stars on GitHub
- [ ] Featured in Awesome TypeScript
- [ ] Mentioned in major dev blogs (dev.to, Smashing Magazine)

**Technical Milestones:**
- [ ] <5s analysis time for 1000-file repo (warm cache)
- [ ] <10s end-to-end for GitHub Action (checkout + analysis + comment)
- [ ] Zero CI flakes (100% deterministic)
- [ ] 95%+ user satisfaction (GitHub surveys)

---

## Anti-Goals (Not Priorities)

These are explicitly **not** priorities for CI/CD adoption:

- ❌ Historical dashboards (analytics phase 2)
- ❌ Team leaderboards (analytics phase 2)
- ❌ Code ownership attribution (analytics phase 2)
- ❌ Multi-language support (Go, Python, Java)
- ❌ Type-aware analysis (too complex for MVP+)
- ❌ Call graph analysis (scope creep)
- ❌ Web UI / standalone dashboard (SaaS product)
- ❌ Slack/Discord integrations (distraction)

These may be revisited after CI/CD adoption is proven.

---

## Next Steps

See `TASKS.md` for the actionable task breakdown.
