# Design Decisions

This document captures key design decisions made during Faultline MVP development. These decisions are **final and binding** for the MVP scope.

## Architecture Decisions

### Rust Workspace Structure

**Decision:** Single workspace with two crates only.

**Rationale:**
- Separation of concerns: core library vs CLI
- Library can be reused by other tools
- CLI provides user-facing interface
- Keeps structure simple for MVP

**Implementation:**
- `faultline-core` - Library crate with all analysis logic
- `faultline-cli` - Binary crate with CLI interface

### Rust Version

**Decision:** Rust 2021 Edition, MSRV 1.75.

**Rationale:**
- Stable, widely-supported version
- Avoids nightly-only features
- Ensures compatibility
- No 2024-only features required

### Parser Selection

**Decision:** Use `swc_ecma_parser` (SWC - Speedy Web Compiler).

**Rationale:**
- Rust-native parser (no Node.js dependency)
- Fast and well-maintained
- Supports TypeScript syntax
- Actively developed (mature but evolving)
- Used by major projects (Next.js, etc.)

**Trade-offs:**
- Requires pinning specific versions for compatibility
- SWC version evolution may require updates
- TypeScript-only (no JSX in MVP)

### No Kernel Dependency

**Decision:** Explicitly avoid any kernel or external service dependencies.

**Rationale:**
- Standalone tool requirement
- Works offline
- No external API calls
- Fully deterministic analysis
- Portable and self-contained

## Analysis Design Decisions

### Per-Function Analysis

**Decision:** Analyze each function in complete isolation.

**Rationale:**
- Simpler model (no inter-function dependencies)
- Parallelizable (future enhancement)
- Clear boundaries
- Matches "local" in Local Risk Score
- Easier to reason about

**Implications:**
- No cross-function call graph analysis
- No inter-function metrics
- Each function analyzed independently

### Control Flow Graph Model

**Decision:** Build explicit CFG for each function with entry/exit nodes.

**Rationale:**
- Formal model for control flow
- Enables metric calculation (CC uses E-N+2)
- Validates program structure
- Handles complex control flow (try/catch/finally)
- Makes edge cases explicit

**Structure:**
- One CFG per function
- Entry and exit nodes
- Explicit edges for all control flow
- No global CFG (per-function only)

### Deterministic Ordering

**Decision:** Sort functions and results deterministically by (file, span.start).

**Rationale:**
- Reproducible output
- Stable test fixtures
- Predictable user experience
- Byte-for-byte identical output

**Ordering Rules:**
- Functions: sorted by `span.lo` (byte offset)
- Reports: sorted by (LRS desc, file asc, line asc, name asc)
- Files: processed in discovery order (deterministic)

### Anonymous Function Naming

**Decision:** Format: `<anonymous>@<file>:<line>`

**Rationale:**
- Stable across runs
- Human-readable
- Includes location context
- Deterministic (uses file and line)
- Example: `<anonymous>@src/api.ts:42`

**Alternative Considered:**
- Synthetic numeric IDs (too cryptic)
- Hash-based names (not human-readable)
- Context-based names (not stable)

## Metric Calculation Decisions

### Cyclomatic Complexity Formula

**Decision:** `CC = E - N + 2` with additional increments.

**Rationale:**
- Standard McCabe formula
- Accounts for decision points
- Additional increments for:
  - Short-circuit operators (implicit decisions)
  - Switch cases (explicit decisions)
  - Catch clauses (exception paths)

**Increments:**
- `&&` and `||`: +1 each
- Switch case: +1 per case
- Catch clause: +1 per catch

### Fan-Out Chained Calls

**Decision:** Count each segment of chained calls independently.

**Example:** `foo().bar().baz()` counts as:
- `foo`
- `foo().bar`
- `foo().bar().baz`

**Rationale:**
- Each call expression is a distinct dependency
- Chained calls represent multiple coupling points
- More accurate representation of complexity
- Matches actual function call sites

**Alternative Considered:**
- Count only terminal call (under-counts coupling)
- Count only root identifier (misses intermediate calls)

### Non-Structured Exits

**Decision:** Count all early exits except final tail return.

**Rationale:**
- Early exits increase complexity
- Tail return is expected control flow
- Includes: `return`, `break`, `continue`, `throw`
- Excludes: final `return` statement

**Implication:**
- Functions with multiple exit points have higher NS
- Tail recursion patterns don't inflate NS
- Exception handling increases NS appropriately

### Nesting Depth Calculation

**Decision:** Count only control constructs (if, loops, switch, try).

**Rationale:**
- Focuses on control flow complexity
- Ignores lexical scoping (less relevant)
- Maximum depth tracks worst-case path
- Excludes plain blocks

**Included:**
- `if`, `else if`
- `for`, `while`, `do-while`, `for-in`, `for-of`
- `switch`
- `try`, `catch`, `finally`

**Excluded:**
- Lexical scopes (`{ }` blocks)
- Function bodies (separate analysis)
- Object literals
- Array literals

## Risk Scoring Decisions

### Risk Transform Functions

**Decision:** Use logarithmic transforms for CC and FO, linear for ND and NS.

**Rationale:**
- Logarithmic for metrics that can grow unbounded (CC, FO)
- Linear for metrics with natural bounds (ND, NS)
- Bounded to prevent extreme scores
- Monotonic (higher metric → higher risk)

**Formulas:**
- `R_cc = min(log2(CC + 1), 6)` - Logarithmic, capped at 6
- `R_nd = min(ND, 8)` - Linear, capped at 8
- `R_fo = min(log2(FO + 1), 6)` - Logarithmic, capped at 6
- `R_ns = min(NS, 6)` - Linear, capped at 6

### LRS Weights

**Decision:** Weighted sum with CC having highest weight.

**Rationale:**
- CC is most established metric
- ND important but secondary
- FO and NS have lower but meaningful weights
- Weights chosen to balance contributions

**Weights:**
- `R_cc`: 1.0 (highest)
- `R_nd`: 0.8
- `R_ns`: 0.7
- `R_fo`: 0.6 (lowest)

### Risk Bands

**Decision:** Four bands with specific thresholds.

**Rationale:**
- Clear categorization
- Actionable thresholds
- Balanced distribution
- Intuitive ranges

**Bands:**
- **Low:** LRS < 3
- **Moderate:** 3 ≤ LRS < 6
- **High:** 6 ≤ LRS < 9
- **Critical:** LRS ≥ 9

## Output Format Decisions

### JSON Schema

**Decision:** Include both raw metrics and risk components.

**Rationale:**
- Transparency (users can see inputs)
- Debugging (verify calculations)
- Flexibility (users can recompute with different weights)
- Complete information

**Schema:**
```json
{
  "file": "...",
  "function": "...",
  "line": 42,
  "metrics": { "cc": 5, "nd": 2, "fo": 3, "ns": 1 },
  "risk": { "r_cc": 2.58, "r_nd": 2, "r_fo": 2, "r_ns": 1 },
  "lrs": 5.96,
  "band": "moderate"
}
```

### Text Output Format

**Decision:** Simple aligned columns, no borders.

**Rationale:**
- Human-readable
- Easy to scan
- Works in terminals
- Minimal formatting overhead

**Example:**
```
LRS   File              Line  Function
11.2  src/api.ts        88    handleRequest
9.8   src/db/migrate.ts 41    runMigration
```

### Precision

**Decision:** Full `f64` precision in JSON, 2 decimals in text.

**Rationale:**
- JSON: Machine-readable, preserve precision
- Text: Human-readable, round for display
- Internal calculations: Full precision
- No rounding of intermediate values

## Testing Decisions

### Golden Files

**Decision:** Snapshot expected JSON outputs in `tests/golden/`.

**Rationale:**
- Regression testing
- Verify output stability
- Easy to update when needed
- Clear expected vs actual comparison

**Location:**
- Fixtures: `tests/fixtures/*.ts`
- Golden: `tests/golden/*.json`

### Determinism Tests

**Decision:** Explicitly test byte-for-byte identical output.

**Rationale:**
- Core requirement (invariant #6)
- Catches non-deterministic bugs
- Ensures reproducible results
- Verifies stable sorting

**Implementation:**
- Run analysis twice
- Compare JSON output byte-for-byte
- Fail if any difference

## Error Handling Decisions

### Parse Errors

**Decision:** Fail fast per file, continue with other files.

**Rationale:**
- Clear error attribution
- Don't fail entire run for one bad file
- Aggregate errors at end
- Valid results still reported

**Behavior:**
- Parse error → skip file, report error
- Continue with remaining files
- Exit non-zero if any errors

### Unsupported Features

**Decision:** Emit error for unsupported function, skip it, continue.

**Rationale:**
- Graceful degradation
- Don't fail entire analysis
- Clear error messages
- Continue with supported functions

**Examples:**
- Generator functions (`function*`)
- JSX syntax (file-level error)

## Scope Limitations (Intentional)

### No JSX Support

**Decision:** Plain TypeScript only, no JSX/TSX.

**Rationale:**
- MVP scope limitation
- JSX adds complexity
- Can be added later
- Clear error message when encountered

### No Type-Aware Analysis

**Decision:** Parse types but don't use for analysis.

**Rationale:**
- Keeps MVP focused
- Types add significant complexity
- Structural analysis sufficient for MVP
- Can be enhanced later

### No Cross-Function Analysis

**Decision:** Per-function analysis only.

**Rationale:**
- Simpler model
- Matches "local" scope
- Can add later if needed
- Sufficient for function-level risk

### Break/Continue Placeholder

**Decision:** Route break/continue to exit (placeholder).

**Rationale:**
- Loop context tracking complex
- MVP placeholder works
- Documented as limitation
- Can be refined later

**Note:** Labeled break/continue support planned but loop context tracking needs refinement.

## Trade-offs Summary

### Chosen Approaches

1. **Explicit CFG over implicit flow** - More complex but more precise
2. **Deterministic over fast** - Reproducibility over performance
3. **Complete metrics over simplified** - More information for users
4. **Per-function over cross-function** - Simpler, clearer boundaries
5. **Static analysis over dynamic** - No execution required

### Future Considerations

- Incremental analysis (cache CFGs)
- Type-aware metrics (use type information)
- Cross-function analysis (call graph)
- Configuration files (custom thresholds)
- Performance optimization (parallel analysis)
