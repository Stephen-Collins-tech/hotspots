# Hotspots Architecture

## Overview

Hotspots is a static analysis tool that computes **Local Risk Scores (LRS)** for TypeScript functions. It analyzes individual functions in isolation, extracting four key metrics and transforming them into a unified risk assessment.

## System Architecture

### Workspace Structure

Hotspots is implemented as a Rust workspace with two crates:

1. **`hotspots-core`** - Library crate containing all analysis logic
   - TypeScript parsing and AST traversal
   - Function discovery
   - Control Flow Graph (CFG) construction
   - Metric extraction
   - Risk score calculation
   - Report generation

2. **`hotspots-cli`** - Binary crate providing command-line interface
   - Argument parsing
   - File collection and traversal
   - Output formatting (text/JSON)
   - Error handling and reporting

### Technology Stack

- **Language:** Rust 2021 Edition (MSRV 1.75)
- **Parser:** `swc_ecma_parser` v33.0.0 (SWC - Speedy Web Compiler)
- **AST Libraries:** `swc_ecma_ast` v20.0.0, `swc_ecma_visit` v20.0.0
- **Serialization:** `serde` + `serde_json` for JSON output
- **CLI:** `clap` v4.5 for argument parsing
- **Error Handling:** `anyhow` for error propagation

### Design Principles

1. **Per-Function Analysis:** Each function is analyzed in complete isolation
2. **Determinism:** Identical input produces byte-for-byte identical output
3. **No Global State:** Stateless analysis with no shared mutable state
4. **Explicit Control Flow:** All control flow constructs are explicitly modeled in the CFG
5. **Stable Ordering:** Results are sorted deterministically for reproducible output

## Analysis Pipeline

The analysis follows a strict pipeline:

```
TypeScript Source
    ↓
[Parser] → Module AST
    ↓
[Function Discovery] → FunctionNode[]
    ↓ (for each function)
[CFG Builder] → Control Flow Graph
    ↓
[Metric Extraction] → RawMetrics (CC, ND, FO, NS)
    ↓
[Risk Calculation] → RiskComponents + LRS + RiskBand
    ↓
[Report Generation] → FunctionRiskReport[]
    ↓
[Sorting & Filtering] → Final Reports
    ↓
[Output Rendering] → Text or JSON
```

### Phase Breakdown

#### Phase 1: Parsing and Discovery
- Parse TypeScript source into SWC AST
- Discover all functions (declarations, expressions, arrows, methods)
- Extract function metadata (name, line number, span)
- Assign deterministic function IDs

#### Phase 2: CFG Construction
- Build control flow graph for each function
- Model all control structures (if/else, loops, switch, try/catch/finally)
- Handle early exits (return, throw, break, continue)
- Validate CFG structure (entry/exit, reachability)

#### Phase 3: Metric Extraction
- **Cyclomatic Complexity (CC):** `E - N + 2` + short-circuit operators + switch cases + catch clauses
- **Nesting Depth (ND):** Maximum depth of control constructs
- **Fan-Out (FO):** Distinct function call sites (including chained calls)
- **Non-Structured Exits (NS):** Early returns, breaks, continues, throws

#### Phase 4: Risk Scoring
- Transform each metric into risk components:
  - `R_cc = min(log2(CC + 1), 6)`
  - `R_nd = min(ND, 8)`
  - `R_fo = min(log2(FO + 1), 6)`
  - `R_ns = min(NS, 6)`
- Aggregate: `LRS = 1.0*R_cc + 0.8*R_nd + 0.6*R_fo + 0.7*R_ns`
- Assign risk band: Low (<3), Moderate (3-6), High (6-9), Critical (≥9)

#### Phase 5: Reporting
- Generate structured reports with all metrics and risk data
- Sort by LRS (descending), file, line, function name
- Support text and JSON output formats

## Data Models

### FunctionNode
Represents a discovered function with:
- `id: FunctionId` - Unique identifier (file_index, local_index)
- `name: Option<String>` - Function name (or None for anonymous)
- `span: Span` - Source location
- `body: BlockStmt` - Function body AST node

### CFG Components
- `CfgNode` - Graph node with ID and kind (Statement, Condition, LoopHeader, Join, etc.)
- `CfgEdge` - Directed edge connecting nodes
- `Cfg` - Complete graph with entry/exit nodes

### Metrics
- `RawMetrics` - CC, ND, FO, NS values
- `RiskComponents` - Transformed risk values (R_cc, R_nd, R_fo, R_ns)
- `RiskBand` - Enum (Low, Moderate, High, Critical)

### Reports
- `FunctionRiskReport` - Complete report including:
  - File path, function name, line number
  - Raw metrics and risk components
  - LRS score and risk band

## Global Invariants

These invariants are enforced throughout the system:

1. **Per-function analysis:** No cross-function dependencies
2. **No global mutable state:** All analysis is stateless
3. **No randomness/clocks/threads/async:** Fully deterministic
4. **Deterministic traversal:** Explicit ordering by (file, span.start)
5. **Formatting invariance:** Whitespace and formatting don't affect results
6. **Output determinism:** Identical input → identical output

See [`invariants.md`](./invariants.md) for detailed documentation.

## Supported Features

### TypeScript Syntax
- Function declarations and expressions
- Arrow functions (with expression and block bodies)
- Class methods
- Object literal methods
- All control flow structures
- Type annotations (parsed but not analyzed)

### Explicitly Unsupported
- JSX/TSX syntax
- Generator functions (`function*`)
- Experimental decorators
- Async/await analysis (parsed but not modeled in CFG)

See [`ts-support.md`](./ts-support.md) for complete details.

## Testing Strategy

### Unit Tests
- Parser tests (syntax validation, error handling)
- Function discovery tests (ordering, anonymous functions)
- CFG tests (construction, validation)
- Metric calculation tests

### Integration Tests
- End-to-end analysis of fixture files
- Determinism verification (identical outputs)
- Whitespace invariance testing
- Golden file comparisons

### Golden Files
- Expected JSON outputs for known fixtures
- Automatically verified in CI
- Location: `tests/golden/*.json`

## Performance Characteristics

- **Static analysis:** No execution required
- **Per-file parsing:** Files analyzed independently
- **Deterministic algorithms:** O(n) for AST traversal
- **CFG construction:** Linear in function size
- **No caching:** Each run is independent (by design)

## Limitations

See [`limitations.md`](./limitations.md) for detailed known limitations, including:
- Break/continue target resolution
- Labeled break/continue
- Generator functions
- Async function CFG modeling
- Type-aware analysis

## Future Considerations

Potential enhancements beyond MVP:
- Incremental analysis
- Type-aware metrics
- Cross-function dependency tracking
- Configuration file support
- Custom risk thresholds
- Export to various formats (SARIF, etc.)
