# Multi-Language Support Analysis

**Date:** 2026-02-04
**Scope:** Adding Python, Rust, and Go support to Hotspots

---

## Current Architecture

### Language Coupling

Hotspots is **tightly coupled** to TypeScript/JavaScript via SWC:

```rust
// Heavy dependency on swc_ecma_ast types
use swc_ecma_ast::*;  // BlockStmt, Stmt, Expr, etc.

pub struct FunctionNode {
    pub body: BlockStmt,  // SWC-specific type
    // ...
}
```

**Key Modules:**
- `parser.rs` (104 LOC) - SWC parsing logic
- `ast.rs` (36 LOC) - SWC AST wrapper
- `cfg/builder.rs` (700 LOC) - **Heavily coupled to SWC AST types**
- `metrics.rs` (352 LOC) - Works on CFG (language-agnostic)
- `discover.rs` - Function discovery from SWC AST

**Total language-specific code:** ~1,200 lines

---

## Approach Options

### Option 1: Per-Language Implementations (Isolated)

Create separate parsers/CFG builders for each language:

```
hotspots-core/src/
├── languages/
│   ├── typescript/  (existing, refactored)
│   │   ├── parser.rs
│   │   ├── ast.rs
│   │   └── cfg_builder.rs
│   ├── python/
│   │   ├── parser.rs
│   │   ├── ast.rs
│   │   └── cfg_builder.rs
│   ├── rust/
│   └── go/
└── common/
    ├── metrics.rs    (shared)
    └── cfg.rs        (shared CFG representation)
```

**Pros:**
- Clean separation
- No language cross-contamination
- Easy to test in isolation
- Can evolve languages independently

**Cons:**
- Code duplication (CFG logic)
- Larger codebase
- More maintenance burden

### Option 2: Unified AST Abstraction (Trait-Based)

Create a language-agnostic AST trait:

```rust
trait LanguageParser {
    fn parse(&self, source: &str) -> Result<Module>;
    fn discover_functions(&self, module: &Module) -> Vec<FunctionNode>;
}

trait AstNode {
    fn kind(&self) -> NodeKind;
    fn children(&self) -> Vec<&dyn AstNode>;
}

// Implement for each language
impl LanguageParser for TypeScriptParser { ... }
impl LanguageParser for PythonParser { ... }
```

**Pros:**
- Shared CFG builder
- Less code duplication
- Unified architecture

**Cons:**
- Complex trait design
- Language quirks force compromises
- Harder to optimize per-language
- Upfront design cost

---

## Language-Specific Analysis

### Python

**Complexity: HIGH** 🔴

#### Parser Options
1. **tree-sitter-python** (via tree-sitter Rust bindings)
   - Pros: Fast, mature, well-tested
   - Cons: Lossy AST, no semantic info
2. **RustPython parser** (rustpython-parser crate)
   - Pros: Full Python AST, semantic analysis
   - Cons: Python 3.12 support, larger dependency

#### Control Flow Challenges

**Unique Python constructs:**
```python
# 1. List/dict/set comprehensions (implicit loops)
result = [x for x in range(10) if x % 2 == 0]

# 2. Context managers (enter/exit, exception handling)
with open('file') as f:
    data = f.read()

# 3. Decorators (call wrapping)
@decorator
def func():
    pass

# 4. Generators (yield suspends execution)
def generator():
    yield 1
    yield 2

# 5. Multiple exception types
except (TypeError, ValueError) as e:
    pass

# 6. Else clause on loops/try
for x in items:
    pass
else:
    # Runs if loop completes without break
    pass
```

**CFG Impact:**
- Comprehensions: Need to model implicit loops (increases CC/ND)
- Context managers: Implicit try/finally blocks
- Generators: Multiple exit points (yield = non-structured exit?)
- Loop else: Additional control flow edge

**Metrics Challenges:**
- **Fan-out:** How to count comprehensions? (each iteration is implicit call)
- **Non-structured exits:** yield, raise, break/continue in comprehensions
- **Nesting depth:** Comprehensions nest differently than loops

#### Effort Estimate

| Task | Effort | Risk |
|------|--------|------|
| Parser integration | 3 days | Low |
| Function discovery | 2 days | Low |
| CFG builder (basic) | 5 days | Medium |
| CFG builder (comprehensions) | 3 days | High |
| CFG builder (context managers) | 2 days | Medium |
| CFG builder (generators) | 3 days | High |
| Metrics validation | 2 days | Medium |
| Testing | 5 days | Medium |
| **Total** | **25 days** | **High** |

**Risk Factors:**
- Python's dynamic nature makes fan-out hard to track
- Comprehensions could inflate complexity unfairly
- Generator CFG modeling is research-level complexity

---

### Rust

**Complexity: VERY HIGH** 🔴🔴

#### Parser Options
1. **syn** (standard Rust parser crate)
   - Pros: Official, complete, well-maintained
   - Cons: Large dependency (already in project)
2. **tree-sitter-rust**
   - Pros: Fast
   - Cons: Lossy, no macro expansion

#### Control Flow Challenges

**Unique Rust constructs:**
```rust
// 1. Pattern matching (exhaustive, complex branching)
match value {
    Some(x) if x > 10 => { /* ... */ }
    Some(_) => { /* ... */ }
    None => { /* ... */ }
}

// 2. if let / while let (pattern-based conditions)
if let Some(value) = option {
    // ...
}

// 3. Loop with labels and break values
'outer: loop {
    'inner: loop {
        break 'outer 42;  // Break outer with value
    }
}

// 4. ? operator (implicit early return)
let result = func()?;  // Returns Err early

// 5. Unwrap panic (implicit panic = non-structured exit)
let value = option.unwrap();

// 6. Closures capturing environment
let closure = |x| x + captured_var;

// 7. Async/await (state machine transformation)
async fn foo() {
    bar().await;
}
```

**CFG Impact:**
- Match arms: Multi-way branching (high CC)
- if let/while let: Pattern matching in conditions
- Break with value: Data flow + control flow
- ? operator: Implicit return path
- Unwrap: Potential panic (count as non-structured exit?)
- Closures: Nested function scope?
- Async: State machine (very complex CFG)

**Metrics Challenges:**
- **Cyclomatic Complexity:** Match arms can explode CC
- **Fan-out:** Method calls through traits, closures as callbacks
- **Non-structured exits:** ?, panic!, unwrap, early returns
- **Nesting depth:** Match arms count as nesting?

#### Effort Estimate

| Task | Effort | Risk |
|------|--------|------|
| Parser integration (syn) | 2 days | Low |
| Function discovery | 3 days | Medium |
| CFG builder (basic) | 5 days | Medium |
| CFG builder (match) | 5 days | High |
| CFG builder (if let/while let) | 2 days | Medium |
| CFG builder (? operator) | 3 days | High |
| CFG builder (async/await) | 8 days | **Very High** |
| Closures handling | 3 days | High |
| Metrics validation | 3 days | High |
| Testing | 7 days | High |
| **Total** | **41 days** | **Very High** |

**Risk Factors:**
- Async/await CFG is extremely complex (state machines)
- Match exhaustiveness affects CC significantly
- Macro expansion could change CFG drastically
- Trait method resolution is non-trivial for fan-out

**Recommendation:** Start with **subset of Rust** (no async, no macros)

---

### Go

**Complexity: MEDIUM** 🟡

#### Parser Options
1. **tree-sitter-go**
   - Pros: Fast, lightweight
   - Cons: Lossy AST
2. **Go's standard library parser** (via FFI/CGO)
   - Pros: Official, complete
   - Cons: Requires CGO, platform-specific builds

#### Control Flow Challenges

**Unique Go constructs:**
```go
// 1. Defer (deferred function calls)
defer cleanup()

// 2. Goroutines (concurrent execution)
go func() {
    // runs concurrently
}()

// 3. Select (channel operations)
select {
case msg := <-ch1:
    // ...
case ch2 <- value:
    // ...
default:
    // ...
}

// 4. Multiple return values
result, err := function()
if err != nil {
    return err
}

// 5. Labeled break/continue
outer:
for {
    for {
        break outer
    }
}
```

**CFG Impact:**
- Defer: Adds implicit finally-like block
- Goroutines: Concurrent CFG (ignore or model spawn?)
- Select: Multi-way branching like switch
- Multiple returns: How to count (2 returns = 2 exits?)
- Error checking idiom: Inflates CC/NS significantly

**Metrics Challenges:**
- **Cyclomatic Complexity:** `if err != nil` pattern everywhere (high CC)
- **Fan-out:** Goroutine spawns count as function calls?
- **Non-structured exits:** Multiple returns, panic, defer
- **Nesting depth:** Defer doesn't nest but affects flow

#### Effort Estimate

| Task | Effort | Risk |
|------|--------|------|
| Parser integration | 4 days | Medium |
| Function discovery | 2 days | Low |
| CFG builder (basic) | 4 days | Medium |
| CFG builder (defer) | 3 days | Medium |
| CFG builder (select) | 2 days | Low |
| CFG builder (goroutines) | 2 days | Low |
| Multiple returns handling | 2 days | Low |
| Metrics validation | 2 days | Medium |
| Testing | 5 days | Medium |
| **Total** | **26 days** | **Medium** |

**Risk Factors:**
- Go's error handling inflates CC (every function has 2x checks)
- Defer semantics are subtle (order matters)
- Goroutines might need special handling for fan-out
- CGO dependency complicates builds if using official parser

---

## Cross-Language Concerns

### 1. Determinism

**Challenge:** Ensure byte-for-byte identical output across languages

**Risks:**
- Python: Indentation-sensitive, dictionary ordering
- Rust: Macro expansion non-determinism
- Go: Goroutine scheduling (non-deterministic by nature)

**Mitigation:** Strict ordering in function discovery, ignore runtime behavior

### 2. Metric Consistency

**Challenge:** LRS should be comparable across languages

**Example:**
```python
# Python
result = [x for x in range(10) if x % 2 == 0]  # LRS = ?

# JavaScript equivalent
result = [...Array(10).keys()].filter(x => x % 2 === 0)  # LRS = ?
```

Should comprehension have same LRS as explicit loop?

**Decision needed:**
- Option A: Language-normalized (comprehension = loop)
- Option B: Language-specific (comprehension counts differently)

### 3. Testing Strategy

**Each language needs:**
- Unit tests for parser
- Unit tests for CFG builder
- Integration tests for metrics
- Comprehensive language feature coverage
- Cross-language comparison tests (same algorithm, different languages)

**Estimated test coverage per language:**
- 50+ unit tests
- 20+ integration tests
- 10+ cross-language tests

---

## Recommended Approach

### Phase 1: Architecture Refactoring (2 weeks)

**Before adding languages, refactor current code:**

1. **Extract language-agnostic CFG**
   ```rust
   // Current: tightly coupled
   pub fn build_cfg(function: &FunctionNode) -> Cfg

   // New: trait-based
   pub trait CfgBuildable {
       fn build_cfg(&self) -> Cfg;
   }
   ```

2. **Create language abstraction layer**
   ```rust
   pub trait LanguageSupport {
       fn parse(&self, source: &str) -> Result<ParsedModule>;
       fn discover_functions(&self, module: &ParsedModule) -> Vec<Function>;
   }
   ```

3. **Refactor existing TypeScript/JavaScript**
   - Move to `languages/typescript/` module
   - Implement new traits
   - Ensure no regression (run full test suite)

### Phase 2: Add Go (Medium Complexity) (4 weeks)

**Why Go first:**
- Simpler than Python/Rust
- Good validation of architecture
- Useful for analyzing Go projects

**Deliverables:**
- Go parser integration
- Go CFG builder
- Go metrics validation
- Comprehensive test suite
- Documentation

### Phase 3: Add Python (6 weeks)

**After Go validates architecture:**
- Python parser integration (tree-sitter or rustpython)
- Handle comprehensions, context managers
- Defer generators to later phase
- Test suite

### Phase 4: Add Rust Subset (8 weeks)

**Subset first (no async/macros):**
- syn parser integration
- Match, if let, while let
- ? operator
- Basic CFG
- Test suite

### Phase 5: Rust Advanced (Future)

**Later iteration:**
- Async/await state machines
- Macro expansion
- Advanced features

---

## Risk Assessment

### High-Risk Items

1. **Metric Consistency** 🔴
   - **Risk:** LRS not comparable across languages
   - **Impact:** Users can't compare complexity
   - **Mitigation:** Extensive cross-language validation, normalize where possible

2. **Architecture Refactoring** 🔴
   - **Risk:** Breaking existing TypeScript support
   - **Impact:** Regression in core functionality
   - **Mitigation:** Comprehensive test coverage, incremental refactoring

3. **Async/Await CFG (Rust/Python)** 🔴🔴
   - **Risk:** Research-level complexity
   - **Impact:** Inaccurate metrics or project delay
   - **Mitigation:** Defer to later phase, start with subset

### Medium-Risk Items

1. **Parser Integration** 🟡
   - **Risk:** Dependencies break builds, platform issues
   - **Mitigation:** Use well-maintained parsers, extensive CI

2. **Testing Burden** 🟡
   - **Risk:** Test suite grows 4x, CI time increases
   - **Mitigation:** Parallel testing, cache artifacts

---

## Effort Summary

| Language | Complexity | Development | Testing | Total | Risk |
|----------|-----------|-------------|---------|-------|------|
| **Refactoring** | - | 2 weeks | 1 week | **3 weeks** | 🟡 |
| **Go** | Medium | 3 weeks | 1 week | **4 weeks** | 🟡 |
| **Python** | High | 4 weeks | 2 weeks | **6 weeks** | 🔴 |
| **Rust (subset)** | Very High | 6 weeks | 2 weeks | **8 weeks** | 🔴 |
| **Rust (async)** | Extreme | 4 weeks | 2 weeks | **6 weeks** | 🔴🔴 |

**Total for Python + Rust + Go:** ~21 weeks (~5 months) for full implementation

**Minimum viable (Go + Python, no Rust):** ~13 weeks (~3 months)

---

## Recommendations

### For Your Use Case

Based on your usage patterns (537 sessions, 167 commits, polyglot codebase):

**Priority 1: Go** (4 weeks)
- You have significant Go code (3008 file interactions)
- Medium complexity, good ROI
- Validates multi-language architecture

**Priority 2: Python** (6 weeks)
- You have Python code (485 file interactions)
- Higher complexity but high value
- Skip generators initially

**Priority 3: Rust subset** (8 weeks)
- Your Hotspots codebase is Rust
- "Dogfooding" - analyze your own tool
- Skip async initially

### Alternative: External Contribution

**Open source the architecture, accept PRs:**
- Publish language trait specification
- Provide TypeScript as reference implementation
- Community adds languages over time
- You review/merge

**Pros:**
- Faster time-to-market for multiple languages
- Community ownership
- Less maintenance burden

**Cons:**
- Quality variance
- Need to review complex PRs
- Slower than dedicated development

---

## Decision Framework

### Should you add multi-language support?

**YES, if:**
- You need to analyze polyglot repos holistically
- You want Hotspots to be a universal complexity tool
- You have 3-6 months for focused development

**NO (or DEFER), if:**
- TypeScript/JavaScript coverage is sufficient
- You'd rather focus on GitHub Action adoption
- You want to validate market fit before expanding

### Recommended Next Step

**Option A: Validate Demand First**
1. Release v1.0 with TypeScript/JavaScript
2. Gather user feedback
3. Survey: "What languages do you need?"
4. Add top-requested language

**Option B: Add Go Now**
1. Refactor architecture (3 weeks)
2. Add Go support (4 weeks)
3. Release v1.1 with TypeScript + Go
4. Validate multi-language approach

**My Recommendation:** **Option A**

Release the GitHub Action, get adoption, then add languages based on real user demand. This de-risks the investment.

---

## Questions to Answer

Before committing to multi-language support:

1. **Do your users need multi-language analysis?**
   - Single-language teams won't pay for it
   - Polyglot teams might

2. **Should LRS be comparable across languages?**
   - If yes: Massive validation effort
   - If no: Easier but less useful

3. **What's the 80/20?**
   - Maybe just Go + Python covers 80% of demand
   - Rust might not be high priority for users

4. **Who's the competition?**
   - Do existing tools (SonarQube, CodeClimate) do multi-language?
   - Can you differentiate?

---

**Bottom Line:**
- **Feasible:** Yes, with significant effort
- **Risk:** Medium-High (architecture, metrics consistency)
- **Effort:** 5 months for full implementation
- **Recommendation:** Release TypeScript/JS first, validate demand, then add Go
