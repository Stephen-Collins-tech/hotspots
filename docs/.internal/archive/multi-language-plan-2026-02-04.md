# Multi-Language Support Implementation Plan

## Goal
Add Go and Rust language support to Hotspots while maintaining existing TypeScript/JavaScript functionality.

## Current Architecture

**Pipeline:**
```
File → parse_source() → discover_functions() → build_cfg() → extract_metrics() → analyze_risk()
         ↓ SWC AST        ↓ swc_ecma_visit      ↓ swc types    ↓ Language-agnostic
```

**Current dependencies:**
- `swc_ecma_parser` - TypeScript/JavaScript parser
- `swc_ecma_ast` - AST types (language-specific)
- `swc_ecma_visit` - AST visitor pattern
- `swc_common` - SourceMap, Span tracking

**Problem:** Entire codebase is tightly coupled to SWC AST types. Need abstraction layer.

---

## Proposed Architecture

### Language Abstraction Layer

```rust
// New: hotspots-core/src/language/mod.rs

pub enum Language {
    TypeScript,
    JavaScript,
    Go,
    Rust,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext {
            "ts" | "tsx" | "mts" | "cts" => Some(Language::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "go" => Some(Language::Go),
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }
}

pub trait LanguageParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>>;
}

pub trait ParsedModule {
    fn discover_functions(&self, file_index: usize) -> Vec<FunctionNode>;
}
```

### Unified Function Representation

```rust
// Refactor: hotspots-core/src/ast.rs

pub struct FunctionNode {
    pub id: FunctionId,
    pub name: Option<String>,
    pub span: SourceSpan,  // Language-agnostic span
    pub body: FunctionBody,  // Language-agnostic body
    pub suppression_reason: Option<String>,
    pub language: Language,  // NEW
}

pub enum FunctionBody {
    TypeScript(swc_ecma_ast::BlockStmt),
    Go(GoBlockStmt),      // NEW
    Rust(RustBlock),      // NEW
}

pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub start_line: u32,
    pub start_col: u32,
}
```

### CFG Builder Abstraction

```rust
// Refactor: hotspots-core/src/cfg/builder.rs

pub trait CfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg;
}

pub struct TypeScriptCfgBuilder;
pub struct GoCfgBuilder;
pub struct RustCfgBuilder;

impl CfgBuilder for TypeScriptCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        // Current implementation
    }
}
```

---

## Implementation Phases

## Phase 1: Architecture Refactoring (Breaking Changes)

**Goal:** Decouple from SWC types, introduce language abstraction

### Task 1.1: Create Language Module
- Create `src/language/mod.rs`
- Define `Language` enum
- Implement `from_extension()` and `from_file()`
- Add language detection tests

### Task 1.2: Abstract SourceSpan
- Create `SourceSpan` struct (language-agnostic)
- Convert all `swc_common::Span` → `SourceSpan`
- Update `FunctionNode` to use `SourceSpan`
- Update metrics extraction to work with `SourceSpan`

### Task 1.3: Abstract FunctionBody
- Create `FunctionBody` enum with language variants
- Update `FunctionNode` to use `FunctionBody`
- Wrap existing `BlockStmt` in `FunctionBody::TypeScript`

### Task 1.4: Create Parser Trait
- Define `LanguageParser` trait
- Define `ParsedModule` trait
- Implement for TypeScript/JavaScript (wraps SWC)
- Update `analyze_file()` to use trait

### Task 1.5: Create CfgBuilder Trait
- Define `CfgBuilder` trait
- Extract current logic to `TypeScriptCfgBuilder`
- Update `build_cfg()` to dispatch by language

### Task 1.6: Update Analysis Pipeline
- Modify `analyze_file()` to detect language
- Select appropriate parser based on language
- Select appropriate CFG builder based on language
- Ensure all existing tests pass

**Deliverables:**
- ✅ Language detection from file extensions
- ✅ Trait-based parser abstraction
- ✅ Trait-based CFG builder abstraction
- ✅ All TypeScript/JavaScript tests passing
- ✅ No functional regressions

**Effort:** 3-4 days

---

## Phase 2: Go Language Support

**Goal:** Full Go language analysis capability

### Task 2.1: Add Go Parser Dependency
- Evaluate options:
  - Option A: `tree-sitter-go` (tree-sitter)
  - Option B: `go/parser` via FFI
  - Option C: Pure Rust Go parser (if exists)
- Add chosen dependency to Cargo.toml
- Document decision in `docs/ARCHITECTURE.md`

### Task 2.2: Implement Go Parser
- Create `src/language/go/parser.rs`
- Implement `LanguageParser` for Go
- Parse Go source to AST
- Extract source spans
- Handle parse errors gracefully

### Task 2.3: Implement Go Function Discovery
- Create `src/language/go/discover.rs`
- Implement `ParsedModule` for Go AST
- Discover function declarations
- Discover methods (receiver functions)
- Handle anonymous functions
- Extract function names and spans

### Task 2.4: Define Go AST Types
- Create `src/language/go/ast.rs`
- Define `GoBlockStmt` and related types
- Map Go AST → Hotspots IR
- Handle Go-specific constructs:
  - Defer statements
  - Go statements (goroutines)
  - Select statements
  - Type switches

### Task 2.5: Implement Go CFG Builder
- Create `src/language/go/cfg_builder.rs`
- Implement `CfgBuilder` for Go
- Handle Go control flow:
  - If/else
  - For loops (various forms)
  - Switch/select
  - Defer (track as non-structured exit)
  - Go statements (concurrent, count as call)
  - Panic/recover
- Calculate metrics:
  - CC: if, for, case, &&, ||, etc.
  - ND: nesting depth
  - FO: function calls + go statements
  - NS: return, panic, defer

### Task 2.6: Add Go Test Suite
- Create test fixtures in `tests/fixtures/go/`
- Test simple functions
- Test control flow (if, for, switch, select)
- Test Go-specific features (defer, go, panic)
- Test methods vs functions
- Golden file tests for determinism

### Task 2.7: Update Documentation
- Add Go to supported languages list
- Document Go-specific metric calculations
- Add Go examples to docs/USAGE.md
- Update README.md language support section

**Deliverables:**
- ✅ Go files can be analyzed
- ✅ Accurate metrics for Go functions
- ✅ Defer counted as non-structured exit
- ✅ Go statements counted in fan-out
- ✅ Select/switch cases counted in CC
- ✅ Comprehensive test coverage

**Effort:** 5-7 days

---

## Phase 3: Rust Language Support

**Goal:** Full Rust language analysis capability

### Task 3.1: Add Rust Parser Dependency
- Add `syn` crate (same parser rustc uses)
- Add `quote` for AST manipulation (if needed)
- Configure for full parsing (not just derive macros)

### Task 3.2: Implement Rust Parser
- Create `src/language/rust/parser.rs`
- Implement `LanguageParser` for Rust
- Parse Rust source to `syn::File`
- Extract source spans
- Handle parse errors gracefully

### Task 3.3: Implement Rust Function Discovery
- Create `src/language/rust/discover.rs`
- Implement `ParsedModule` for Rust
- Discover:
  - Free functions
  - Methods in impl blocks
  - Associated functions
  - Closures (as separate functions)
  - Async functions
- Extract function names and spans

### Task 3.4: Define Rust AST Types
- Create `src/language/rust/ast.rs`
- Define `RustBlock` and related types
- Map Rust AST → Hotspots IR
- Handle Rust-specific constructs:
  - Match expressions
  - If let / while let
  - Loop / while / for
  - Result/Option unwrapping (?, unwrap())
  - Closures
  - Async/await

### Task 3.5: Implement Rust CFG Builder
- Create `src/language/rust/cfg_builder.rs`
- Implement `CfgBuilder` for Rust
- Handle Rust control flow:
  - If/else/if let
  - Match expressions
  - Loop/while/for/while let
  - Return, break, continue
  - Panic, unwrap, expect
  - ? operator (early return)
- Calculate metrics:
  - CC: if, match arms, loop, &&, ||, ?, etc.
  - ND: nesting depth (match counts as 1 level)
  - FO: function/method calls (exclude macros?)
  - NS: return, ?, panic, unwrap, expect, break, continue

### Task 3.6: Handle Rust-Specific Challenges
- Macros: Should they be expanded or counted?
  - Decision: Count macro invocations as calls, don't expand
- Closures: Separate functions or inline?
  - Decision: Treat as separate functions
- Async: How to handle .await?
  - Decision: .await is not a decision point (no branching)
- Pattern matching: How to count complexity?
  - Decision: Each match arm is a decision point

### Task 3.7: Add Rust Test Suite
- Create test fixtures in `tests/fixtures/rust/`
- Test simple functions
- Test control flow (if, match, loop)
- Test Rust-specific features (?, unwrap, if let)
- Test methods vs functions vs closures
- Test async functions
- Golden file tests for determinism

### Task 3.8: Update Documentation
- Add Rust to supported languages list
- Document Rust-specific metric calculations
- Add Rust examples to docs/USAGE.md
- Document macro handling approach
- Document closure treatment

**Deliverables:**
- ✅ Rust files can be analyzed
- ✅ Accurate metrics for Rust functions
- ✅ Match arms counted in CC
- ✅ ? operator counted as non-structured exit
- ✅ Closures analyzed as separate functions
- ✅ Comprehensive test coverage

**Effort:** 6-8 days

---

## Phase 4: Integration & Polish

### Task 4.1: Multi-Language File Discovery
- Update file discovery to handle multiple extensions
- Detect language from extension
- Skip unsupported files gracefully
- Add `--language` CLI flag to force language

### Task 4.2: Mixed-Language Repository Support
- Analyze multi-language repos (e.g., Go + JS)
- Aggregate metrics across languages
- Language breakdown in HTML reports
- Per-language filtering

### Task 4.3: Update GitHub Action
- Support all languages in CI
- Auto-detect languages in repo
- Update action README with language support
- Add language-specific examples

### Task 4.4: Update MCP Server
- Support all languages in `hotspots_analyze` tool
- Add language parameter to tool schema
- Update tool description

### Task 4.5: Update TypeScript Types
- Add `language` field to `FunctionReport`
- Update `@hotspots/types` package
- Publish new version

### Task 4.6: Update AI Examples
- Update examples to handle multi-language
- Add Go and Rust examples to `examples/ai-agents/`
- Update AI prompts for language-specific advice

### Task 4.7: Documentation Updates
- Update main README with all supported languages
- Update docs/USAGE.md with language sections
- Create docs/LANGUAGE_SUPPORT.md with details
- Update AI integration guide
- Update JSON schema docs

### Task 4.8: Performance Testing
- Benchmark analysis speed for each language
- Ensure no regressions in TS/JS performance
- Document performance characteristics

**Deliverables:**
- ✅ All languages work in all contexts (CLI, Action, MCP)
- ✅ Documentation is comprehensive and accurate
- ✅ Performance is acceptable for all languages
- ✅ Examples cover all languages

**Effort:** 3-4 days

---

## Technical Decisions to Make

### 1. Parser Libraries

**Go:**
- **tree-sitter-go**: Incremental, fast, but less Go-idiomatic
- **go/parser via FFI**: Official Go parser, but FFI complexity
- **Recommendation:** tree-sitter-go for pure Rust, easier integration

**Rust:**
- **syn**: Official, used by rustc, full-featured
- **Recommendation:** syn (no alternatives)

### 2. Metric Definitions

Need to define how language-specific features map to metrics:

**Go:**
- Defer → NS (non-structured exit)
- Go statement → FO (function call)
- Select case → CC (decision point)
- Type switch case → CC (decision point)
- Panic → NS (non-structured exit)
- Recover → Not counted (defensive, not control flow)

**Rust:**
- Match arm → CC (decision point per arm)
- ? operator → NS (early return)
- unwrap/expect → NS (potential panic)
- break/continue → NS (non-structured exit)
- Closure → Separate function (analyzed independently)
- Macro invocation → FO (function call)

### 3. Backwards Compatibility

**Breaking changes:**
- JSON schema will add `language` field
- Internal API changes (traits, structs)

**Non-breaking:**
- CLI interface remains the same
- Output format unchanged (except new field)
- Existing TS/JS code continues to work

**Versioning:**
- Requires major version bump (v2.0.0)
- Publish migration guide
- Update all integrations

---

## Testing Strategy

### Unit Tests
- Parser tests (per language)
- Function discovery tests (per language)
- CFG builder tests (per language)
- Metrics extraction tests (per language)

### Integration Tests
- End-to-end analysis (per language)
- Mixed-language repositories
- Error handling (unsupported files, parse errors)

### Golden File Tests
- Deterministic output validation
- Snapshot comparison
- Cross-language consistency

### Performance Tests
- Benchmark suite (per language)
- Large file handling
- Memory usage profiling

---

## Risks & Mitigations

### Risk 1: Parser Maintenance Burden
**Risk:** Multiple parser dependencies to maintain
**Mitigation:** Use well-maintained, stable parsers (syn, tree-sitter)

### Risk 2: Metric Definition Inconsistencies
**Risk:** Metrics mean different things across languages
**Mitigation:** Document clearly, provide language-specific guidance

### Risk 3: Performance Regression
**Risk:** Abstraction layers slow down analysis
**Mitigation:** Benchmark early, optimize hot paths, use zero-cost abstractions

### Risk 4: Complexity Explosion
**Risk:** Codebase becomes too complex to maintain
**Mitigation:** Clear abstractions, comprehensive docs, refactor iteratively

---

## Timeline Estimate

- **Phase 1 (Architecture):** 3-4 days
- **Phase 2 (Go):** 5-7 days
- **Phase 3 (Rust):** 6-8 days
- **Phase 4 (Integration):** 3-4 days

**Total:** 17-23 days (3-4 weeks)

---

## Success Criteria

- ✅ All three languages (TS/JS, Go, Rust) fully supported
- ✅ Accurate complexity metrics for each language
- ✅ No regressions in existing TS/JS functionality
- ✅ Comprehensive test coverage (>80%)
- ✅ Performance within 2x of current TS/JS speed
- ✅ Documentation covers all languages
- ✅ All integrations updated (CLI, Action, MCP, examples)
- ✅ v2.0.0 released with migration guide

---

## Future Extensions

After Go and Rust:
- **Python:** tree-sitter-python
- **Java:** tree-sitter-java
- **C/C++:** tree-sitter-c / tree-sitter-cpp
- **C#:** tree-sitter-c-sharp

Each new language follows the same pattern:
1. Add parser
2. Implement discovery
3. Implement CFG builder
4. Add tests
5. Update docs
