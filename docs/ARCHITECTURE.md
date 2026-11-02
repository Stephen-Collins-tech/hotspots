# Architecture

## Overview

Hotspots is a Rust workspace with two crates:

- **`hotspots-core`** — library: parsing, CFG construction, metrics, risk scoring, snapshot persistence, delta, policy, report generation
- **`hotspots-cli`** — binary: argument parsing, file collection, output formatting, command dispatch

**Technology stack:** Rust 2021 Edition (MSRV 1.75), `swc_ecma_parser` for JS/TS, `tree-sitter-*` for all other languages, `clap` v4.5 for CLI, `serde`/`serde_json` for serialization, `anyhow` for error propagation.

## Analysis Pipeline

```
Source Code
  ↓
[Parser] → Module AST (per language, per file)
  ↓
[Function Discovery] → FunctionNode[]
  ↓  (for each function)
[CFG Builder] → Control Flow Graph
  ↓
[Metric Extraction] → CC, ND, FO, NS, LOC
  ↓
[Risk Components] → R_cc, R_nd, R_fo, R_ns (log-scaled, bounded)
  ↓
[LRS + Risk Band]
  ↓
[optional: git history + call graph enrichment]
  ↓
[Activity Risk Score + Activity Tier]
  ↓
[Snapshot persistence / Delta computation / Report rendering]
  ↓
[Output: text / JSON / HTML / SARIF]
```

Stages above the enrichment line run on source code alone. Stages below require a git repository (`--mode snapshot` or `--mode delta`).

## Phase Details

### Phase 1 — Parsing and Function Discovery

Each source file is parsed into an AST by the language-specific parser module. All function types are discovered: declarations, expressions, arrow functions, methods, object literal methods, closures.

Functions are sorted by source position (byte offset) before processing to ensure deterministic output. Anonymous functions are named `<anonymous>@<file>:<line>`.

**JS/TS:** SWC parser (`swc_ecma_parser`). Decorator support enabled for all `.ts` files (Angular `@Component`, etc.). JSX enabled for `.jsx` and `.js` files (React webpack convention). **All other languages:** tree-sitter parsers.

### Phase 2 — CFG Construction

A Control Flow Graph is built for each function. The CFG models all control structures explicitly:
- `if`/`else` — condition node, then/else branches, lazy join node
- Loops (`for`, `while`, `do-while`, `for-in`, `for-of`) — loop header, back edge, lazy break target
- `switch` — switch node, case branches, lazy join (no switch→join edge to avoid CC inflation)
- `try`/`catch`/`finally` — handler edges, lazy join, fallback `try_start→finally_start` edge only when no catch handler
- Early exits (`return`, `throw`, `break`, `continue`) — edge to exit/break-target; `current_node = None` marks dead code

Key correctness rules:
- Join nodes are created **lazily** — only when needed (when at least one live path reaches them). Eager join node creation causes orphaned nodes with no predecessors, failing CFG validation.
- After a terminating statement sets `current_node = None`, subsequent `visit_*` calls return early (`let Some(from_node) = self.current_node else { return; }`) to prevent panics on dead code.
- `BreakableContext.break_target: Option<NodeId>`: `Some` for condition-guarded loops (break target pre-created); `None` (lazy) for `do-while`, infinite `for`, and `switch`.

CFG validation: all nodes must be reachable from entry; all non-exit nodes must have at least one successor.

### Phase 3 — Metric Extraction

From the validated CFG:
- **CC** = `E − N + 2` + one per `&&`/`||` short-circuit + one per `switch` case + one per `catch` clause
- **ND** = maximum nesting depth tracked during AST traversal (if, loops, switch, try; excludes bare blocks and lexical scopes)
- **FO** = count of distinct call expressions during AST traversal; each segment of a chained call counts independently (`a().b().c()` = 3)
- **NS** = count of non-tail `return`, `throw`, `break`, `continue` during traversal
- **LOC** = physical line count of the function body

### Phase 4 — Risk Scoring

Log-scale transforms and weighted sum → LRS → risk band. See [REFERENCE.md](REFERENCE.md#lrs-formula) for formulas.

### Phase 5 — Enrichment (snapshot mode)

**Git history:** one batched `git log` per file provides churn and touch counts, applied to every function in the file. Touch counts use a 365-day window (see [REFERENCE.md](REFERENCE.md#activity-risk-score-snapshot-mode)).

**Call graph:** Import resolution builds a cross-file call graph and computes fan-in, the only call graph metric still reported. PageRank, betweenness, strongly connected components and dependency depth were removed because nothing validated them (see [REFERENCE.md](REFERENCE.md#activity-risk-score-snapshot-mode)).

**Tier assignment:** touch count above the repo median or changed in the last 30 days → `active`, else `quiet`.

### Phase 6 — Snapshot Persistence

Snapshots are stored as `<repo>/.hotspots/snapshots/<commit-sha>.json.zst` (compressed). They are immutable by default — identified by commit SHA. `--force` regenerates; `--no-persist` skips writing.

Index at `.hotspots/index.json` tracks known snapshots.

Delta computation (`Delta::new(head, Some(&base))`) produces per-function status (`new`/`deleted`/`modified`/`unchanged`) and metric deltas.

## Module Structure

```
hotspots-core/src/
├── language/
│   ├── typescript/     # SWC-based parser + CFG builder
│   ├── javascript/     # same, JSX-enabled
│   ├── go/
│   ├── java/
│   ├── python/
│   ├── rust/
│   ├── c/
│   ├── csharp/
│   ├── vue/
│   └── astro/
├── cfg/
│   ├── builder.rs      # generic CFG construction traits
│   └── mod.rs          # CfgNode, CfgEdge, Cfg, validation
├── metrics.rs          # raw metric extraction
├── risk.rs             # LRS, risk components, risk bands
├── snapshot.rs         # snapshot serialization, persistence, loading
├── delta.rs            # delta computation
├── policy.rs           # policy rule evaluation
├── analysis.rs         # pipeline orchestration
├── aggregates.rs       # file_risk, co_change
├── callgraph.rs        # call graph and fan-in
├── git.rs              # git log integration, touch cache, ref resolution
├── config.rs           # config loading and resolution
├── html.rs             # HTML report rendering
├── sarif.rs            # SARIF output
└── report.rs           # JSON rendering

hotspots-cli/src/
├── main.rs             # CLI entry, Commands enum
└── cmd/
    ├── analyze.rs
    ├── diff.rs
    ├── train.rs
    ├── coordinate.rs   # coupling/ownership risk for a set of files, pre-work
    ├── prune.rs
    ├── compact.rs
    ├── config.rs
    └── init.rs
```

## Global Invariants

These are non-negotiable. Any violation is a bug.

1. **Per-function analysis** — each function analyzed independently; no cross-function state during analysis
2. **No global mutable state** — no `static mut`, no shared mutable references between functions
3. **No randomness, clocks, threads, or async** — all operations fully deterministic
4. **Deterministic traversal order** — files sorted by path; functions sorted by source position (byte offset)
5. **Formatting/whitespace invariance** — only structural AST nodes used; comments and whitespace do not affect results
6. **Identical input → byte-for-byte identical output** — all JSON key ordering and floating-point formatting are deterministic

## Testing Strategy

**Golden tests** (`tests/golden/`) — expected JSON outputs for known fixture files. Automatically verified in CI. Updated when metric behavior intentionally changes.

**Unit tests** — per-module: CFG construction, metric extraction, risk scoring, config parsing.

**Integration tests** (`integration/`, pytest-based E2E) — full pipeline tests against real fixture projects. `make test-integration` runs pytest; `make test-comprehensive` auto-detects pytest or falls back to legacy script.

**Determinism tests** — run analysis twice on identical input, assert byte-for-byte identical output.

**No manual golden path fixing needed** — golden tests normalize file paths at assertion time for cross-platform consistency.

## Design Decisions

**SWC for JS/TS, tree-sitter for everything else.** SWC is Rust-native (no Node.js dependency), fast, and supports full TypeScript syntax. Tree-sitter parsers are widely available for other languages and have a uniform API.

**Explicit CFG over implicit flow tracking.** More complex to implement, but enables formal CC calculation (`E − N + 2`) and catches edge cases (dead code after return, multiple catch clauses, finally with and without catch).

**Logarithmic scaling for CC and FO.** The marginal risk of CC 1→4 is larger than CC 40→44. Caps prevent extreme outliers from dominating the aggregate score.

**Immutable snapshots keyed by commit SHA.** Enables `hotspots diff` between any two historical refs, supports multiple in-flight branches without collision, and makes snapshot storage auditable.

**Lazy join node creation in CFG builder.** Eager creation of join nodes that never receive incoming edges (e.g., when all branches terminate) causes CFG validation failures (`"Nodes not reachable from entry"`). Lazy creation — only when at least one live path needs to merge — is the correct approach.

**No cross-function analysis in LRS.** LRS is per-function and named "Local" deliberately. Fan-in is computed at the call graph layer and is not folded into LRS, which keeps LRS a pure structural measure.

