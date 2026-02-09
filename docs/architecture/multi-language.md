# Multi-Language Support Architecture

**Status:** Planning / Research
**Last Updated:** 2026-02-04

---

## Overview

This document outlines the architectural considerations for adding multi-language support to Hotspots beyond TypeScript/JavaScript.

## Current State

Hotspots is currently tightly coupled to TypeScript/JavaScript via SWC (Speedy Web Compiler):

```rust
// Heavy dependency on swc_ecma_ast types
use swc_ecma_ast::*;  // BlockStmt, Stmt, Expr, etc.

pub struct FunctionNode {
    pub body: BlockStmt,  // SWC-specific type
    // ...
}
```

**Key language-specific modules:**
- `parser.rs` - SWC parsing logic
- `ast.rs` - SWC AST wrapper
- `cfg/builder.rs` - Heavily coupled to SWC AST types
- `discover.rs` - Function discovery from SWC AST

**Language-agnostic modules:**
- `metrics.rs` - Works on CFG (language-agnostic)
- `cfg.rs` - CFG representation

Total language-specific code: ~1,200 lines

---

## Architectural Approaches

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

## Language-Specific Challenges

### Python (Complexity: HIGH)

**Unique constructs:**
- List/dict/set comprehensions (implicit loops)
- Context managers (with statement)
- Generators (yield)
- Decorators
- Multiple exception types
- Else clause on loops/try

**CFG Impact:**
- Comprehensions need implicit loop modeling
- Context managers = implicit try/finally
- Generators = multiple exit points
- Loop else = additional control flow edge

### Rust (Complexity: VERY HIGH)

**Unique constructs:**
- Pattern matching (exhaustive, complex branching)
- if let / while let
- Loop labels and break with values
- ? operator (implicit early return)
- Async/await (state machine transformation)
- Closures capturing environment

**CFG Impact:**
- Match arms = multi-way branching (high CC)
- ? operator = implicit return path
- Async/await = state machine (very complex CFG)
- Pattern matching in conditions

### Go (Complexity: MEDIUM)

**Unique constructs:**
- Defer (deferred function calls)
- Goroutines (concurrent execution)
- Select (channel operations)
- Multiple return values
- Error handling idiom (if err != nil)

**CFG Impact:**
- Defer = implicit finally-like block
- Select = multi-way branching
- Error checking pattern inflates CC/NS
- Multiple returns = multiple exit points

---

## Cross-Language Concerns

### 1. Metric Consistency

Challenge: Should LRS be comparable across languages?

Example:
```python
# Python
result = [x for x in range(10) if x % 2 == 0]

# JavaScript equivalent
result = [...Array(10).keys()].filter(x => x % 2 === 0)
```

**Decision needed:**
- Option A: Language-normalized (comprehension = loop)
- Option B: Language-specific (comprehension counts differently)

### 2. Determinism

Ensure byte-for-byte identical output across languages:
- Python: Dictionary ordering
- Rust: Macro expansion
- Go: Goroutine scheduling (ignore runtime behavior)

**Mitigation:** Strict ordering in function discovery, ignore runtime behavior

### 3. Testing Strategy

Each language needs:
- 50+ unit tests for parser and CFG builder
- 20+ integration tests for metrics
- 10+ cross-language comparison tests
- Comprehensive language feature coverage

---

## Recommended Approach

### Phase 1: Architecture Refactoring (2 weeks)

Before adding languages, refactor current code:

1. Extract language-agnostic CFG
2. Create language abstraction layer
3. Refactor existing TypeScript/JavaScript to use traits
4. Ensure no regression (run full test suite)

### Phase 2: Add First Additional Language (4 weeks)

Start with Go (medium complexity) to validate architecture:
- Good validation of approach
- Simpler than Python/Rust
- Useful for analyzing Go projects

### Phase 3+: Add Other Languages Based on Demand

Priority based on user requests:
- Python (6 weeks)
- Rust subset (8 weeks, no async initially)
- Other languages as needed

---

## Effort Estimates

| Language | Complexity | Development | Testing | Total | Risk |
|----------|-----------|-------------|---------|-------|------|
| **Refactoring** | - | 2 weeks | 1 week | **3 weeks** | Medium |
| **Go** | Medium | 3 weeks | 1 week | **4 weeks** | Medium |
| **Python** | High | 4 weeks | 2 weeks | **6 weeks** | High |
| **Rust (subset)** | Very High | 6 weeks | 2 weeks | **8 weeks** | High |

Total for Go + Python + Rust: ~5 months

---

## Decision Framework

### Should we add multi-language support?

**YES, if:**
- Need to analyze polyglot repos holistically
- Want Hotspots to be a universal complexity tool
- Have 3-6 months for focused development

**NO (or DEFER), if:**
- TypeScript/JavaScript coverage is sufficient
- Want to focus on GitHub Action adoption first
- Need to validate market fit before expanding

### Recommended Next Step

**Validate demand first:**
1. Release with TypeScript/JavaScript
2. Gather user feedback
3. Survey: "What languages do you need?"
4. Add top-requested language

---

## References

- [Language Support Documentation](../reference/language-support.md)
- [Design Decisions](design-decisions.md)
- Original analysis: `docs/.internal/archive/multi-language-analysis-2026-02-04.md`
