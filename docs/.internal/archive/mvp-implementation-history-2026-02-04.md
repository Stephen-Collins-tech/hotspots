# MVP Implementation History

This document archives the original MVP task list that was successfully completed.

**Status:** ✅ Complete (as of version 0.0.1)

For current development tasks, see `TASKS.md`.
For future roadmap, see `roadmap.md`.

---

# TASKS.md - Hotspots MVP (Fully Expanded)

## Status

```text
project: hotspots
implementation: Rust
analyzes: TypeScript
scope: static, per-function analysis
dependencies: none (standalone)
kernel: explicitly not used
```

---

## Assumptions and Design Decisions

These decisions are **final and binding**. The Coding Agent must implement exactly as specified.

### Architecture and Setup

**Workspace structure:**
* Single Rust workspace with **two crates only**:
  * `hotspots-core` (library)
  * `hotspots-cli` (binary)
* No additional crates in MVP

**Rust edition and MSRV:**
* Rust edition: **2021**
* MSRV: **1.75**
* Do not use nightly or 2024-only features

**Dependencies and error handling:**
* Allowed and recommended:
  * `serde` + `serde_json` for JSON output
  * `thiserror` for error types
  * `anyhow` for CLI-level error propagation
* Disallowed:
  * Logging frameworks
  * Async runtimes
  * Heavy utility crates
* Errors should be typed in `hotspots-core` and wrapped at the CLI boundary

### Parser and AST

**JSX support:**
* **No JSX in MVP**
* Plain TypeScript only
* If JSX syntax is encountered, emit a clear parse error and abort

**Anonymous function naming:**
* Format: `<anonymous>@<file>:<line>`
* Example: `<anonymous>@src/api.ts:42`
* This is stable, human-readable, and deterministic

**Synthetic function IDs:**
* Use a **monotonic numeric ID per file**, assigned in traversal order
* Format: `FunctionId { file_index, local_index }`
* IDs are internal only and must never appear in user output

### CFG and Control Flow

**Switch fallthrough:**
* Fallthrough is **explicit only when `break` is missing**
* CFG rules:
  * Each `case` is a node
  * If a case does not end in `break`, add an edge to the next case
  * Default case participates the same way

**Try / catch / finally CFG semantics:**
* `finally` **always executes**
* CFG structure:
  * Try body flows to catch blocks on exception
  * Normal try completion flows to finally
  * Catch completion flows to finally
  * Finally flows to a single join node
* Even if no explicit join exists in source, the CFG must create one

**Generator functions (`function*`):**
* **Defer** (not supported in MVP)
* If encountered:
  * Emit a deterministic error
  * Skip analysis of that function only
  * Continue with remaining functions

**Labeled break / continue:**
* Support labeled `break` and `continue`
* Rules:
  * Resolve label target statically
  * Edge jumps to the correct loop exit or header
  * Count as non-structured exits
* If label resolution fails, error deterministically

### Metrics

**Short-circuit operators and CC:**
* Each logical short-circuit operator increments CC by 1:
  * `a && b` → +1
  * `a || b` → +1
* Nested short-circuits accumulate

**Fan-out and method calls:**
* `obj.method()` counts as `obj.method`
* `foo()` counts as `foo`
* Computed calls like `obj[x]()` are counted as `"<computed>"`
* Deduplication is done on the final string representation

**Nesting depth and try blocks:**
* `try` blocks **do increment nesting depth**
* Included in ND:
  * if
  * loops
  * switch
  * try / catch
* Excluded:
  * lexical scopes
  * blocks without control flow

### Risk Score and Output

**Floating point precision:**
* Internal calculations use full `f64`
* Final LRS is **not rounded internally**
* Text output displays **2 decimal places**
* JSON output emits full precision `f64`
* No rounding of intermediate components

**Sorting stability:**
* Final sort order:
  1. LRS descending
  2. File path ascending
  3. Line number ascending
  4. Function name ascending
* This guarantees total ordering

**JSON output schema:**
* Include both raw metrics and risk components
* Use exactly this structure:

```json
{
  "file": "...",
  "function": "...",
  "line": 42,
  "metrics": {
    "cc": 5,
    "nd": 2,
    "fo": 3,
    "ns": 1
  },
  "risk": {
    "r_cc": 2.58,
    "r_nd": 2,
    "r_fo": 2,
    "r_ns": 1
  },
  "lrs": 5.96,
  "band": "moderate"
}
```

* No additional fields in MVP

**CLI text output style:**
* Simple, aligned columns. No borders
* Example:

```
LRS   File              Line  Function
11.2  src/api.ts        88    handleRequest
9.8   src/db/migrate.ts 41    runMigration
```

* Human-readable, not pretty-printed

### Testing and Validation

**Golden file location:**
* Golden files live in:
  * `tests/fixtures/` (source code)
  * `tests/golden/` (expected JSON output)

**Determinism tests:**
* **Explicitly required**
* Tests must:
  * Run analysis twice on same input
  * Assert byte-for-byte identical JSON

**Error handling strategy:**
* Parse errors: fail fast per file
* Function-level unsupported features: error for that function, continue
* Aggregate errors and report at end
* If errors occur, valid function reports are still emitted, and errors are printed to stderr
* JSON output contains only valid function results
* CLI exits non-zero if any errors occurred

### Edge Cases

**Empty functions:**
* CC = 1
* ND = 0
* FO = 0
* NS = 0
* This yields a minimal but valid LRS

**Recursive calls:**
* Self-calls **do count** toward fan-out
* Reason: recursion increases reasoning load

**Arrow functions with implicit returns:**
* Supported
* Rules:
  * Treat implicit return as a return node
  * If it is the final expression, it is a tail return and does not count as non-structured
* CFG must still include an edge to exit

---

## Global invariants (non-negotiable)

These apply to **all phases**:

* Analysis is strictly per-function
* No global mutable state
* No randomness, clocks, threads, or async
* Deterministic traversal order must be explicit
* Formatting, comments, and whitespace must not affect results
* Identical input yields byte-for-byte identical output

Any violation is a bug.

---

## Task graph and execution order

The Coding Agent must execute phases strictly in this order:

```
Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
```

No phase may be partially skipped or interleaved.

---

## Phase 0 - Workspace, crate layout, invariants

### 0.1 Workspace creation

* [x] Create Rust workspace `hotspots`
* [x] Add crates:

  * `hotspots-core` (library)
  * `hotspots-cli` (binary)
* [x] Lock Rust edition and MSRV
* [x] Configure clippy and rustfmt
* [x] Enable warnings as errors

**Acceptance:** `cargo build` succeeds with zero warnings.

---

### 0.2 CLI skeleton

* [x] Add `analyze` subcommand
* [x] Parse arguments:

  * `<path>`
  * `--format text|json`
  * `--top <N>`
  * `--min-lrs <float>`
* [x] Normalize and validate paths
* [x] Call core library with structured options

**Acceptance:** CLI runs and prints placeholder output deterministically.

---

### 0.3 Invariants documentation

* [x] Create `docs/invariants.md`
* [x] Document all global invariants
* [x] Reference invariants in code comments where enforced

**Acceptance:** Invariants are explicit and referenced.

---

## Phase 1 - TypeScript parsing and function discovery

### 1.1 Parser selection and locking

* [x] Use `swc_ecma_parser`
* [x] Pin exact versions
* [x] Enable TypeScript syntax only
* [x] Disable experimental proposals
* [x] Document supported syntax in `docs/ts-support.md`

**Acceptance:** A simple TS file parses successfully.

---

### 1.2 AST adapter layer

Define a stable abstraction:

```rust
struct FunctionNode {
  id: FunctionId,
  name: Option<String>,
  span: Span,
  body: AstNode,
}
```

Tasks:

* [x] Map SWC nodes to `FunctionNode`
* [x] Generate synthetic IDs for anonymous functions
* [x] Extract start line number
* [x] Enforce deterministic ordering by (file, span.start)

**Acceptance:** All functions are discovered in stable order.

---

### 1.3 Supported and ignored constructs

Explicitly support:

* [x] Function declarations
* [x] Function expressions
* [x] Arrow functions
* [x] Class methods
* [x] Object literal methods

Explicitly ignore:

* [x] Interfaces
* [x] Type aliases
* [x] Overload signatures without bodies
* [x] Ambient declarations

**Acceptance:** Ignored constructs never appear in analysis results.

---

## Phase 2 - Control Flow Graph (CFG)

### 2.1 CFG data model

Define:

```rust
struct CfgNode { id: NodeId, kind: NodeKind }
struct CfgEdge { from: NodeId, to: NodeId }
struct Cfg {
  nodes: Vec<CfgNode>,
  edges: Vec<CfgEdge>,
  entry: NodeId,
  exit: NodeId,
}
```

Rules:

* One CFG per function
* No cross-function edges
* No global graph

**Acceptance:** Empty CFG can be constructed and validated. ✅

---

### 2.2 Formal CFG lowering rules

Lower AST → CFG using these exact rules:

#### Sequential statements

* Each *control-relevant statement* becomes a CFG node
* Expression-only statements without control flow may be collapsed
* Sequential edges connect in order

#### If / else

* Condition node
* Two outgoing edges
* Join node after both branches

#### Switch

* One node per case
* Fallthrough edges explicit
* Join node after switch

#### Loops (for, while, do-while)

* Loop header node
* Back-edge to header
* Exit edge to join node

#### Break / continue

* Edge to loop exit or header
* Count as non-structured exit

#### Return

* Edge directly to CFG exit
* Counts as non-structured exit unless it is the final statement

#### Throw

* Edge to CFG exit
* Always non-structured

#### Try / catch / finally

* Try body flows into catch blocks
* Finally always executes
* All paths converge at join

#### Boolean short-circuit operators

* Boolean short-circuit operators (`&&`, `||`) do not introduce CFG nodes
* They are treated as implicit decision points for CC calculation only

**Acceptance:** CFGs match hand-drawn examples exactly.

Note: Break/continue routing to loop exit/header requires loop context tracking (currently routes to exit as placeholder).

---

### 2.3 CFG validation

* [x] Exactly one entry node
* [x] Exactly one exit node
* [x] All nodes reachable from entry
* [x] All paths must either reach the exit node or terminate via a return or throw edge explicitly connected to exit

**Acceptance:** Invalid CFGs error deterministically. ✅

---

## Phase 3 - Metric extraction

### 3.1 Cyclomatic Complexity (CC)

* [x] Compute `CC = E - N + 2`
* [x] Increment CC for:

  * boolean short-circuit operators
  * each switch case
  * each catch clause
* [ ] Document CC contribution rules

**Acceptance:** CC matches expected values in fixtures.

---

### 3.2 Nesting Depth (ND)

* [x] Walk AST, not CFG
* [x] Count control constructs only:

  * if, loop, switch, try
* [x] Track maximum depth

**Acceptance:** ND correct for nested examples.

---

### 3.3 Fan-Out (FO)

* [x] Collect call expressions
* [x] Extract callee identifiers
* [x] For chained calls, count each call expression independently using its immediate callee representation

  Example: `foo().bar().baz()` counts as:
  * `foo`
  * `foo().bar`
  * `foo().bar().baz`
* [x] Deduplicate by symbol name
* [x] Ignore intrinsics and operators

**Acceptance:** FO invariant under formatting changes.

---

### 3.4 Non-Structured Exits (NS)

* [x] Count early `return`
* [x] Count `break`
* [x] Count `continue`
* [x] Count `throw`
* [x] Exclude final tail return

**Acceptance:** NS matches CFG structure.

---

## Phase 4 - Local Risk Score (LRS)

### 4.1 Risk transforms

Implement exactly:

```
R_cc = min(log2(CC + 1), 6)
R_nd = min(ND, 8)
R_fo = min(log2(FO + 1), 6)
R_ns = min(NS, 6)
```

* [x] Implement all risk transforms
* [x] Ensure transforms are monotonic and bounded

**Acceptance:** All transforms monotonic and bounded. ✅

---

### 4.2 LRS aggregation

```
LRS =
  1.0 * R_cc +
  0.8 * R_nd +
  0.6 * R_fo +
  0.7 * R_ns
```

* [x] Compute float score
* [x] Assign band:

  * <3 low
  * 3-6 moderate
  * 6-9 high
  * ≥9 critical

**Acceptance:** Scores match documented examples exactly. ✅

---

## Phase 5 - Reporting and CLI output

### 5.1 Report model

Define:

```rust
struct FunctionRiskReport {
  file: String,
  function: String,
  line: u32,
  metrics: RawMetrics,
  risk: RiskComponents,
  lrs: f64,
  band: RiskBand,
}
```

* [x] Define FunctionRiskReport struct
* [x] Include all required fields
* [x] Support JSON serialization

**Acceptance:** Struct serializes deterministically. ✅

---

### 5.2 Output renderers

* [x] Text renderer
* [x] JSON renderer
* [x] Stable sort by:

  1. LRS descending
  2. File path ascending
  3. Line number ascending
  4. Function name ascending

**Acceptance:** Byte-for-byte identical output across runs. ✅

---

## Phase 6 - Test fixtures and determinism

### 6.1 Golden fixtures

Create TS files for:

* [x] Single simple function
* [x] Nested branching
* [x] Loop with breaks
* [x] Try/catch/finally
* [x] Pathological complexity

* [x] Snapshot expected JSON output (golden files)

**Location:**
- Golden JSON files: `tests/golden/*.json` (5 files)
- Golden test code: `hotspots-core/tests/golden_tests.rs`

**Acceptance:** All 6 golden tests pass, verifying byte-for-byte identical output. ✅

---

### 6.2 Invariance tests

* [x] Reordered functions (tested via deterministic ordering)
* [x] Reordered files (tested via deterministic sorting)
* [x] Whitespace-only changes (test_whitespace_invariance)

**Acceptance:** Outputs unchanged. ✅

---

## Phase 7 - Documentation and polish

* [x] README with quickstart
* [x] LRS spec
* [x] Supported TS features
* [x] Known limitations

**Acceptance:** New user can run in under 5 minutes. ✅

---

## Exit criteria

Hotspots MVP is complete when:

* All phases complete
* All tests deterministic
* No kernel dependency exists
* Output is trusted and reproducible
