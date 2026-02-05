# Faultline Roadmap

**Last Updated:** 2026-02-04
**Current Version:** Pre-release (approaching v1.0.0)
**Vision:** Universal complexity analysis for modern software development

---

## Table of Contents

1. [Current State](#current-state)
2. [Strategic Direction](#strategic-direction)
3. [Phase 1: Market Validation (Q1 2026)](#phase-1-market-validation-q1-2026)
4. [Phase 2: Multi-Language Experiment (Q2 2026)](#phase-2-multi-language-experiment-q2-2026)
5. [Phase 3: Scale & Enterprise (Q3-Q4 2026)](#phase-3-scale--enterprise-q3-q4-2026)
6. [Multi-Language Technical Analysis](#multi-language-technical-analysis)
7. [Decision Framework](#decision-framework)
8. [Experimental Plan](#experimental-plan)
9. [Success Metrics](#success-metrics)

---

## Current State

### ✅ Completed (as of 2026-02-04)

**Core Engine:**
- ✅ LRS (Local Risk Score) calculation for TypeScript/JavaScript
- ✅ CFG (Control Flow Graph) construction
- ✅ Policy engine with 7 built-in policies
- ✅ Suppression comments system
- ✅ HTML report generation
- ✅ Proactive warning system
- ✅ Git delta mode (snapshot + trends)
- ✅ Configuration file support

**Language Support:**
- ✅ TypeScript (.ts, .tsx, .mts, .cts)
- ✅ JavaScript (.js, .jsx, .mjs, .cjs)
- ✅ JSX/TSX (React components)

**CI/CD Integration:**
- ✅ GitHub Action (Task 2.1 - COMPLETED)
  - Automatic PR/push detection
  - Delta analysis for PRs
  - PR comments with violations
  - HTML report artifacts
  - Job summaries
  - Binary caching

**Infrastructure:**
- ✅ Automated release workflow (multi-platform binaries)
- ✅ Comprehensive test suite
- ✅ Documentation
- ✅ CLAUDE.md coding conventions

### 📊 Progress

**Overall:** 8/25 tasks completed (32%)

**Phase Breakdown:**
- **Phase 1 (Foundations):** 4/7 completed
- **Phase 2 (CI/CD):** 3/4 completed
- **Phase 3 (Governance):** 1/4 completed
- **Phase 4 (Advanced):** 0/10 started

---

## Strategic Direction

### Vision Statement

**"Make complexity regressions impossible in CI/CD pipelines"**

Faultline should be the standard tool that prevents code complexity from growing unchecked, just like linters prevent formatting issues and type checkers prevent type errors.

### Core Principles

1. **Zero Configuration** - Works out of the box
2. **Deterministic** - Byte-for-byte reproducible
3. **Fast** - Completes in <30s for most repos
4. **Actionable** - Clear violations, not just metrics
5. **Developer-Friendly** - Integrates seamlessly into existing workflows

### Strategic Bets

**Bet #1: GitHub Actions Integration is the Wedge**
- Hypothesis: Teams adopt via GitHub Actions first
- If true: Focus on GitHub Action UX, PR comments, workflow polish
- If false: Pivot to CLI-first, local development focus

**Bet #2: TypeScript/JavaScript Market is Sufficient**
- Hypothesis: TS/JS coverage captures 70%+ of potential users
- If true: Defer multi-language, deepen TS/JS features
- If false: Add Go/Python to expand addressable market

**Bet #3: Policy Engine Differentiates vs Competitors**
- Hypothesis: Automated regression blocking > manual metric review
- If true: Invest in policy sophistication, customization
- If false: Focus on reporting, visualization, trends

---

## Phase 1: Market Validation (Q1 2026)

**Timeline:** Feb - Mar 2026 (8 weeks)
**Goal:** Achieve product-market fit with TypeScript/JavaScript users

### Milestones

#### M1.1: v1.0.0 Release (Week 1)

**Deliverables:**
- ✅ Tag v1.0.0
- ✅ Trigger release workflow (build binaries)
- ✅ Test published GitHub Action
- ✅ Publish to GitHub Marketplace
- ✅ Announcement (Twitter, Reddit, HN)

**Success Criteria:**
- Binaries available for all platforms
- GitHub Action works in external repos
- Listed on GitHub Marketplace

#### M1.2: Early Adopter Feedback (Weeks 2-4)

**Activities:**
1. **Outreach:**
   - Share in TypeScript/React communities
   - Post on r/typescript, r/reactjs
   - Tweet with demo video
   - Reach out to 10 target repos

2. **Documentation:**
   - Create "Getting Started" video (5 min)
   - Write 3 blog posts:
     - "Why LRS > Cyclomatic Complexity"
     - "Blocking Complexity Regressions in CI"
     - "Case Study: Analyzing [Popular TS Repo]"

3. **Monitoring:**
   - Track GitHub stars/forks
   - Monitor GitHub Action usage (via releases)
   - Collect issues/feedback
   - Run user interviews (5-10 users)

**Success Criteria:**
- 50+ GitHub stars
- 10+ repos using the GitHub Action
- 5+ user interviews completed
- Clear feedback themes identified

#### M1.3: Iteration Based on Feedback (Weeks 5-8)

**Focus Areas:**
- Fix top 3 user pain points
- Add most-requested features (if small)
- Improve documentation gaps
- Polish GitHub Action UX

**Deliverables:**
- v1.1.0 release with improvements
- Updated docs
- Case studies from real users

**Success Criteria:**
- 100+ GitHub stars
- 25+ repos using the action
- <10% churn rate (repos stop using)
- Net Promoter Score > 30

### Decision Point 1: Language Expansion (End of Week 8)

**Question:** Should we add multi-language support?

**Collect Data:**
- Survey users: "What languages do you use alongside TypeScript?"
- Analyze user repos: What languages appear in the same repos?
- Count feature requests for specific languages

**Decision Criteria:**

| Scenario | Data | Decision |
|----------|------|----------|
| **A: TS/JS is Sufficient** | <20% users need other languages | Continue TS/JS deepening |
| **B: Go is Top Request** | >40% users have Go + TS repos | Proceed to Multi-Language Experiment |
| **C: Mixed Demand** | Multiple languages requested equally | Survey deeper, defer decision |

---

## Phase 2: Multi-Language Experiment (Q2 2026)

**Timeline:** Apr - Jun 2026 (12 weeks)
**Goal:** Validate multi-language demand and technical feasibility
**Trigger:** Decision Point 1 → Scenario B

### Experimental Approach

**Instead of full implementation, run a controlled experiment:**

#### Experiment 1: Go Prototype (4 weeks)

**Hypothesis:**
- Users will adopt Faultline for Go if it provides value comparable to TS
- Go's simpler control flow makes it a good validation case
- Go support expands addressable market by 30%+

**Experiment Design:**

**Week 1-2: Architecture Refactoring**
- Extract language-agnostic CFG
- Create `LanguageSupport` trait
- Refactor TS/JS to new architecture
- **Validation:** All existing tests pass, no regression

**Week 3-4: Go Minimal Viable Implementation**
- Integrate tree-sitter-go parser
- Implement basic CFG builder (if, loops, switch)
- Skip: defer, goroutines, select (initially)
- Generate LRS for Go functions
- **Validation:** Analyze 3 popular Go repos, compare to manual review

**Deliverables:**
- `faultline analyze --lang go main.go` works
- LRS calculation for basic Go code
- 20+ unit tests
- Analysis of 3 real Go repos

**Success Criteria:**
- LRS values make sense (validated by Go developers)
- No crashes on real Go code
- <30% variance from manual complexity assessment

#### Experiment 2: User Validation (2 weeks)

**Beta Release: v1.2.0-beta (Go support)**

**Recruit 10 Beta Testers:**
- Criteria: TypeScript + Go polyglot repos
- Provide early access
- Ask to analyze their Go code
- Collect feedback

**Questions:**
1. Does Go LRS align with your intuition about complex functions?
2. Would you use Faultline for Go in CI?
3. What Go features are missing/broken?
4. Is Go support worth it vs. TS-only?

**Success Criteria:**
- 7/10 testers say "LRS is accurate"
- 5/10 testers would use in CI
- <5 critical bugs reported
- No major architectural blockers

#### Decision Point 2: Full Go Implementation (Week 6)

**Question:** Should we complete Go support?

| Outcome | Data | Decision |
|---------|------|----------|
| **Strong Signal** | 8/10 testers positive, high demand | Invest 4 more weeks for full Go |
| **Mixed Signal** | 5/10 testers positive, some demand | Release as experimental, iterate |
| **Weak Signal** | <5/10 testers positive, low demand | Shelve Go, focus on TS/JS depth |

### Post-Experiment: Full Go Implementation (4 weeks)

**If Decision Point 2 → Strong Signal:**

**Week 7-8: Complete Go Features**
- Defer handling
- Goroutine spawn (count as fan-out)
- Select statements
- Multiple return values
- Error handling patterns

**Week 9-10: Testing & Polish**
- 50+ integration tests
- Analyze 10 popular Go repos
- Document Go-specific behavior
- Update GitHub Action

**Week 11-12: Release & Validation**
- v1.2.0 stable release
- Announce Go support
- Monitor adoption
- Collect feedback

**Success Criteria:**
- 50+ repos using Go analysis
- <5% error rate on real Go code
- Positive community feedback

---

## Phase 3: Scale & Enterprise (Q3-Q4 2026)

**Timeline:** Jul - Dec 2026 (24 weeks)
**Goal:** Enterprise-ready features, monetization, ecosystem growth

### Q3: Enterprise Features (12 weeks)

**Focus:** Features that large teams need

#### M3.1: Advanced CI/CD (4 weeks)

- GitLab CI integration
- Jenkins plugin
- CircleCI orb
- Bitbucket Pipelines support
- Azure DevOps task

#### M3.2: Team Features (4 weeks)

- Config inheritance (repo-level, org-level)
- Team-specific policies
- Centralized reporting dashboard
- Trend tracking across repos
- Email/Slack notifications

#### M3.3: Enterprise Security (4 weeks)

- SAML/SSO integration
- Audit logging
- Policy enforcement API
- Webhook support
- On-premise deployment option

### Q4: Ecosystem & Growth (12 weeks)

#### M3.4: Developer Experience (6 weeks)

- VS Code extension (inline LRS display)
- IntelliJ plugin
- CLI autocomplete
- Interactive tutorials
- Playground (web-based demo)

#### M3.5: Community & Content (6 weeks)

- Open source reference implementations
- Complexity best practices guide
- Video tutorial series
- Case studies (5+ companies)
- Conference talks

### Decision Point 3: Monetization (End of Q3)

**Question:** What's the business model?

**Options:**

**A: Open Core**
- Free: Core analysis, CLI, basic GitHub Action
- Paid: Team features, SSO, dashboard, SLA support
- Price: $99/month per team (5-50 devs)

**B: Hosted SaaS**
- Free: Open source repos
- Paid: Private repos, $49/month per org
- Enterprise: Custom pricing, on-premise

**C: Consulting/Support**
- Free: All features
- Revenue: Implementation consulting, training, custom integrations
- Price: $5K-50K per engagement

**D: Keep Free, GitHub Sponsors**
- Free: Everything
- Donations: GitHub Sponsors, Open Collective
- Sustainability: Grants, company sponsorships

---

## Multi-Language Technical Analysis

### Language Complexity Assessment

| Language | Complexity | Dev Time | Risk | ROI |
|----------|-----------|----------|------|-----|
| **Go** | 🟡 Medium | 7 weeks | Medium | ⭐⭐⭐ High |
| **Python** | 🔴 High | 9 weeks | High | ⭐⭐ Medium |
| **Rust** | 🔴🔴 Very High | 14 weeks | Very High | ⭐ Low |
| **Java** | 🟡 Medium | 6 weeks | Medium | ⭐⭐ Medium |
| **C#** | 🟡 Medium | 6 weeks | Medium | ⭐⭐ Medium |

### Go (Medium Complexity)

**Parser:** tree-sitter-go or official Go parser via FFI

**Control Flow Challenges:**
```go
// Error handling pattern (inflates CC)
result, err := function()
if err != nil {
    return err
}

// Defer (implicit finally)
defer cleanup()

// Select (multi-way branching)
select {
case msg := <-ch1:
case ch2 <- value:
default:
}

// Goroutines (concurrent execution)
go worker()
```

**Metrics Impact:**
- **CC:** `if err != nil` pattern everywhere (2x normal)
- **Fan-out:** Goroutine spawns count?
- **NS:** panic, multiple returns, defer
- **ND:** Standard nesting

**Effort Breakdown:**
- Architecture refactor: 3 weeks
- Go parser + CFG: 3 weeks
- Testing + polish: 1 week
- **Total:** 7 weeks

**Risk Factors:**
- CGO dependency if using official parser
- Error handling inflates CC (need normalization?)
- Defer semantics subtle

### Python (High Complexity)

**Parser:** tree-sitter-python or rustpython-parser

**Control Flow Challenges:**
```python
# Comprehensions (implicit loops)
result = [x for x in range(10) if x % 2 == 0]

# Context managers (implicit try/finally)
with open('file') as f:
    data = f.read()

# Generators (multiple exits)
def gen():
    yield 1
    yield 2

# Loop else clause
for x in items:
    pass
else:
    # Runs if no break
    pass

# Multiple exception types
try:
    risky()
except (TypeError, ValueError) as e:
    handle()
```

**Metrics Impact:**
- **CC:** Comprehensions = loops? (design decision)
- **Fan-out:** Comprehensions have implicit calls?
- **NS:** yield, raise, comprehension exits
- **ND:** Comprehensions nest differently

**Effort Breakdown:**
- Basic Python CFG: 4 weeks
- Comprehensions: 2 weeks
- Context managers: 1 week
- Testing + polish: 2 weeks
- **Total:** 9 weeks (without generators)

**Risk Factors:**
- Dynamic typing makes fan-out hard
- Comprehension modeling (research problem)
- Generator CFG very complex (defer to later)

### Rust (Very High Complexity)

**Parser:** syn crate (official Rust parser)

**Control Flow Challenges:**
```rust
// Pattern matching (multi-way branching)
match value {
    Some(x) if x > 10 => {}
    Some(_) => {}
    None => {}
}

// if let / while let
if let Some(value) = option {
    // ...
}

// ? operator (implicit return)
let result = func()?;

// Async/await (state machines)
async fn foo() {
    bar().await;
}

// Loop labels with break values
'outer: loop {
    break 'outer 42;
}
```

**Metrics Impact:**
- **CC:** Match arms explode CC
- **Fan-out:** Trait methods, closures
- **NS:** ?, unwrap, panic!
- **ND:** Match arms count as nesting?

**Effort Breakdown:**
- Basic Rust CFG: 5 weeks
- Match, if let: 3 weeks
- ? operator: 1 week
- Testing + polish: 3 weeks
- **Async (later):** 8 weeks
- **Total:** 12 weeks (subset), 20 weeks (full)

**Risk Factors:**
- Async/await is research-level complex
- Macro expansion changes CFG
- Trait resolution for fan-out

### Architecture Requirements

**For any new language, need:**

1. **Language-agnostic CFG representation**
   ```rust
   pub trait LanguageSupport {
       fn parse(&self, source: &str) -> Result<ParsedModule>;
       fn discover_functions(&self, module: &ParsedModule) -> Vec<Function>;
       fn build_cfg(&self, function: &Function) -> Cfg;
   }
   ```

2. **Shared metric calculation**
   - CFG → metrics should be language-agnostic
   - Risk transformation same across languages

3. **Determinism guarantees**
   - Function ordering deterministic
   - Output byte-for-byte identical

4. **Cross-language test suite**
   - Same algorithm in different languages
   - Validate LRS consistency

---

## Decision Framework

### When to Add a Language

**Required Conditions (ALL must be true):**

1. **User Demand**
   - >30% of surveyed users need this language
   - OR: 20+ GitHub issues requesting it
   - OR: Clear competitor gap (they don't support it)

2. **Market Size**
   - Language in top 10 by usage (TIOBE/Stack Overflow)
   - OR: Niche with high willingness to pay

3. **Technical Feasibility**
   - Parser available in Rust ecosystem
   - Control flow modeling is tractable
   - Estimated effort <8 weeks

4. **Strategic Fit**
   - Aligns with roadmap priorities
   - Team has capacity
   - Won't distract from core value prop

### Language Priority Matrix

```
High Demand, Low Complexity → IMPLEMENT SOON (Go)
High Demand, High Complexity → EXPERIMENTAL VALIDATION (Python)
Low Demand, Low Complexity → WAIT FOR SIGNAL (Java, C#)
Low Demand, High Complexity → DEFER INDEFINITELY (Rust async, Haskell)
```

### Go/No-Go Decision Template

For each language candidate:

**Demand Score (0-10):**
- User requests: ___ / 3 pts
- Market size: ___ / 3 pts
- Competitor gap: ___ / 2 pts
- Strategic importance: ___ / 2 pts

**Feasibility Score (0-10):**
- Parser quality: ___ / 3 pts
- Control flow complexity: ___ / 4 pts (inverse)
- Team expertise: ___ / 2 pts
- Testing burden: ___ / 1 pt (inverse)

**Formula:** `Priority = (Demand × 1.5) + Feasibility`

**Thresholds:**
- **>22:** Implement now
- **15-22:** Experimental validation
- **<15:** Defer

---

## Experimental Plan

### Experiment Framework

**For each language being considered:**

#### Phase 1: Rapid Prototype (2 weeks)

**Goal:** Prove technical feasibility

**Deliverables:**
- Parser integration
- Basic CFG for 3 control flow constructs (if, loop, return)
- LRS calculation for 1 real-world function
- 5 unit tests

**Success Criteria:**
- Parser doesn't crash on real code
- LRS values in expected range
- No obvious architectural blockers

**Budget:** 2 weeks, 1 developer

#### Phase 2: User Validation (2 weeks)

**Goal:** Validate user interest and accuracy

**Activities:**
1. Recruit 5 beta testers (users who requested this language)
2. Analyze 3-5 repos in their codebases
3. Compare LRS to manual assessment
4. Interview: "Would you use this in CI?"

**Success Criteria:**
- 4/5 testers say LRS is accurate
- 3/5 testers would use in CI
- <3 critical bugs found

**Budget:** 2 weeks, 1 developer + PM for interviews

#### Phase 3: Decision Point

**Go:** Commit to full implementation
- Allocate 6-8 weeks
- Assign dedicated developer
- Set quality bar (95% accuracy, <5% crashes)

**No-Go:** Shelve and document learnings
- Publish "Why we didn't add [Language]" blog post
- Keep prototype as proof-of-concept
- Revisit in 6 months

### Experiment Tracking

**For each experiment, track:**

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Time to prototype | 2 weeks | - | - |
| Parser crash rate | <1% | - | - |
| LRS accuracy (vs manual) | >80% | - | - |
| User interest (would use) | >60% | - | - |
| Critical bugs found | <5 | - | - |

**Decision Criteria:**
- If ALL targets met → Full implementation
- If 4/5 targets met → Iterate prototype
- If <4/5 targets met → Shelve

---

## Success Metrics

### Phase 1: Market Validation

**Adoption:**
- 100 GitHub stars (Week 4)
- 25 repos using GitHub Action (Week 8)
- 10 paying customers (if monetization starts)

**Engagement:**
- 20 issues/PRs from community
- 5 blog posts/articles about Faultline
- 10 user interviews completed

**Quality:**
- <5% crash rate on real repos
- <10% false positive rate (violations that aren't real)
- Net Promoter Score >30

### Phase 2: Multi-Language Experiment

**Technical:**
- Go prototype completes in 2 weeks
- >80% LRS accuracy on Go code
- <1% parser crash rate

**User Validation:**
- 5/10 beta testers positive
- 3/10 would use in CI
- <5 critical bugs

**Decision Quality:**
- Clear go/no-go decision made
- Documented reasoning
- No regrets 3 months later

### Phase 3: Scale & Enterprise

**Revenue (if monetized):**
- $10K MRR (Month 12)
- $50K MRR (Month 18)
- 5 enterprise customers

**Ecosystem:**
- 3 integrations (GitLab, Jenkins, etc.)
- 2 IDE plugins
- 10 case studies

**Community:**
- 1,000 GitHub stars
- 100 contributors
- 50 third-party blog posts

---

## Risks & Mitigation

### Technical Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Multi-language breaks determinism | High | Medium | Extensive testing, formal verification |
| LRS not comparable across languages | High | High | Cross-language validation, normalize metrics |
| Parser dependencies break builds | Medium | Medium | Pin versions, vendor if needed |
| CFG modeling too complex | High | Medium | Start with subset, defer edge cases |

### Market Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| No demand for multi-language | High | Low | Validate before building |
| Competitors add TS support | Medium | Medium | Move fast, differentiate on UX |
| Users don't trust LRS | High | Low | Publish methodology, case studies |
| GitHub changes Actions API | Medium | Low | Abstract GitHub-specific code |

### Execution Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Scope creep delays v1.0 | High | Medium | Ruthless prioritization, MVP focus |
| Multi-language takes 2x longer | Medium | High | Experimental validation first |
| Team burnout | High | Low | Sustainable pace, celebrate wins |
| Poor documentation blocks adoption | Medium | Medium | Invest in docs, videos, examples |

---

## Open Questions

### Product Direction

1. **Is Faultline a CLI tool or a service?**
   - CLI-first: Self-hosted, open source, community-driven
   - Service-first: Hosted analysis, dashboard, enterprise features

2. **Should LRS be comparable across languages?**
   - Yes: Massive validation effort, normalize aggressively
   - No: Language-specific thresholds, easier to implement

3. **What's the primary use case?**
   - PR blocking: Policy enforcement, regression prevention
   - Refactoring guide: Identify hotspots, track improvements
   - Code review: Show complexity in reviews

### Technical Architecture

1. **Trait-based abstraction or per-language modules?**
   - Trait-based: Shared CFG builder, complex design
   - Per-language: Code duplication, simpler implementation

2. **How to handle async/await?**
   - Model state machine CFG: Very complex
   - Ignore for now: Missing important complexity
   - Treat as black box: Simple but inaccurate

3. **Should we support language subsets initially?**
   - Yes: Ship faster, iterate based on feedback
   - No: Users expect full coverage, partial support is confusing

---

## Next Actions

### Immediate (This Week)

- [ ] Merge GitHub Action PR to main
- [ ] Tag v1.0.0 release
- [ ] Test published action in external repo
- [ ] Publish to GitHub Marketplace
- [ ] Write launch announcement

### Short Term (Next 4 Weeks)

- [ ] User outreach (10 target repos)
- [ ] Create "Getting Started" video
- [ ] Write 3 blog posts
- [ ] Collect 5+ user interviews
- [ ] Monitor GitHub Action adoption

### Medium Term (Next 8 Weeks)

- [ ] Analyze interview feedback
- [ ] Survey users on language needs
- [ ] Make Decision Point 1 (language expansion)
- [ ] If GO: Start architecture refactoring
- [ ] If NO: Plan TS/JS feature deepening

---

## Appendix: Competitor Landscape

### Complexity Analysis Tools

**SonarQube**
- Multi-language (30+ languages)
- Hosted + self-hosted
- Enterprise focus
- Heavy, slow, expensive

**CodeClimate**
- Multi-language (10+ languages)
- Hosted only
- GitHub integration
- $200+/month

**Codacy**
- Similar to CodeClimate
- Multi-language
- Hosted + self-hosted
- Enterprise focus

### Faultline Differentiation

**vs SonarQube:**
- ✅ Fast (seconds vs minutes)
- ✅ Deterministic (byte-for-byte)
- ✅ Policy-based (not just metrics)
- ❌ Fewer languages (for now)

**vs CodeClimate:**
- ✅ Self-hosted (no data leaves repo)
- ✅ Open source (transparent metrics)
- ✅ GitHub Action (zero config)
- ❌ No hosted dashboard (yet)

**vs All:**
- ✅ LRS (better than CC alone)
- ✅ Git-aware (delta analysis)
- ✅ Proactive warnings (not just blocking)
- ✅ Suppression with documentation

---

**Last Updated:** 2026-02-04
**Next Review:** 2026-03-01 (after Phase 1 completion)
**Maintained By:** Core team

**Changes to this roadmap require:**
- Data-driven decision making
- User validation
- Team consensus
- Updated success metrics
