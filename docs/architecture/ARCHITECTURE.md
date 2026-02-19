# Hotspots Architecture

**Version:** 1.0  
**Last Updated:** 2026-02-15  
**Status:** Current

---

## Overview

Hotspots is a multi-language static analysis tool that identifies high-risk functions by combining code complexity metrics with git activity data. It analyzes TypeScript, JavaScript, Go, Java, Python, and Rust codebases to produce prioritized risk scores that help teams focus refactoring efforts on code that's both complex and frequently changed.

### Core Value Proposition

Traditional complexity tools only measure code structure. Hotspots combines:
- **Static metrics** (cyclomatic complexity, nesting depth, fan-out, non-structured exits)
- **Git activity** (churn, touch count, recency)
- **Call graph analysis** (fan-in, PageRank, strongly connected components, dependency depth)

This produces an **Activity-Weighted Risk Score** that identifies the 20% of functions causing 80% of production issues.

---

## Core Principles & Invariants

Hotspots enforces strict invariants to ensure deterministic, reproducible analysis:

### Determinism
- **Identical input yields byte-for-byte identical output** — same source code produces identical results across runs
- **Deterministic traversal order** — functions sorted by `(file_index, span.start)` before analysis
- **No randomness** — no use of random number generators, hash map iteration order, or non-deterministic algorithms
- **No clocks** — analysis results don't depend on wall-clock time (uses commit timestamps)

### Per-Function Isolation
- **One function = one analysis** — each function analyzed independently
- **No cross-function edges in CFG** — control flow graphs are per-function only
- **No global mutable state** — all analysis is pure functional transformations

### Formatting Independence
- **Whitespace/formatting don't affect metrics** — code style changes don't change risk scores
- **Comments ignored** — comments don't affect complexity calculations
- **AST-based analysis** — metrics derived from abstract syntax trees, not text patterns

### Immutability
- **Snapshots are immutable** — once written, snapshots are never modified (identified by commit SHA)
- **Atomic writes** — snapshot persistence uses temp-file-plus-rename to prevent corruption
- **Schema versioning** — snapshots carry version numbers for forward compatibility

---

## High-Level Architecture

Hotspots follows a **pipeline architecture** with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI Entry Point                          │
│                    (hotspots-cli/src/main.rs)                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Configuration Loading                         │
│  (.hotspotsrc.json, hotspots.config.json, package.json)        │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      File Discovery                              │
│  Recursive traversal, include/exclude filtering, sorting       │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Per-File Analysis Pipeline                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │  Parse   │→ │ Discover │→ │ Build    │→ │ Extract  │       │
│  │          │  │ Functions │  │   CFG    │  │ Metrics  │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
│       │              │              │              │             │
│       └──────────────┴──────────────┴──────────────┘             │
│                            │                                     │
│                            ▼                                     │
│                    ┌──────────────┐                              │
│                    │ Calculate    │                              │
│                    │ LRS & Band  │                              │
│                    └──────────────┘                              │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Snapshot Enrichment                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │   Git    │→ │  Churn   │→ │  Touch   │→ │   Call  │         │
│  │ Context  │  │ Metrics   │  │ Metrics  │  │  Graph  │         │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘         │
│       │              │              │              │             │
│       └──────────────┴──────────────┴──────────────┘             │
│                            │                                     │
│                            ▼                                     │
│              ┌──────────────────────────┐                        │
│              │ Activity Risk Scoring    │                        │
│              │ (LRS + activity + graph)  │                        │
│              └──────────────────────────┘                        │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Output Generation                           │
│  Snapshot Mode: Persist + JSON/HTML/JSONL/Text                  │
│  Delta Mode: Compare vs parent + Policy evaluation               │
└─────────────────────────────────────────────────────────────────┘
```

---

## Component Architecture

### 1. Language Abstraction Layer (`hotspots-core/src/language/`)

Hotspots supports 6 languages through a unified abstraction:

#### Language Detection
- **File extension mapping** — `.ts` → TypeScript, `.go` → Go, `.py` → Python, etc.
- **Language enum** — `TypeScript`, `TypeScriptReact`, `JavaScript`, `JavaScriptReact`, `Go`, `Java`, `Python`, `Rust`

#### Parser Trait (`LanguageParser`)
```rust
trait LanguageParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>>;
}
```

Each language implements:
- **ECMAScript** (TS/JS) — Uses SWC parser (same as TypeScript compiler)
- **Go** — Uses tree-sitter-go
- **Java** — Uses tree-sitter-java
- **Python** — Uses tree-sitter-python
- **Rust** — Uses syn (same parser as rustc)

#### ParsedModule Trait
```rust
trait ParsedModule {
    fn discover_functions(&self, file_index: usize, source: &str) -> Vec<FunctionNode>;
}
```

Returns language-agnostic `FunctionNode` objects with:
- `FunctionId` (file_index, local_index)
- Function name (or None for anonymous)
- `SourceSpan` (start/end byte, line, column)
- `FunctionBody` (language-specific AST representation)
- Suppression reason (if `// hotspots-ignore: reason` comment present)

#### CFG Builder Trait (`CfgBuilder`)
```rust
trait CfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg;
}
```

Each language builds a control flow graph from its AST:
- **ECMAScript** — Visits SWC AST, builds CFG nodes for if/switch/loop/break/continue
- **Go/Java/Python** — Re-parse with tree-sitter, traverse AST to build CFG
- **Rust** — Uses syn AST, handles match/if/loop/break/continue

#### FunctionBody Enum
Wraps language-specific AST representations:
- `ECMAScript(BlockStmt)` — SWC block statement
- `Go { body_node, source }` — Tree-sitter node ID + source
- `Java { body_node, source }` — Tree-sitter node ID + source
- `Python { body_node, source }` — Tree-sitter node ID + source
- `Rust { source }` — Full function source (re-parsed on demand)

**Why this design?**
- Tree-sitter nodes are tied to tree lifetime → store source + node ID, re-parse when needed
- SWC and syn provide owned ASTs → can store directly
- Unified interface allows language-agnostic metric extraction

---

### 2. Analysis Pipeline (`hotspots-core/src/analysis.rs`)

The per-file analysis pipeline:

1. **Read source file** — `std::fs::read_to_string()`
2. **Detect language** — `Language::from_path(path)`
3. **Get parser** — Match language to parser implementation
4. **Parse** — `parser.parse(&src, filename)` → `ParsedModule`
5. **Discover functions** — `module.discover_functions(file_index, &src)` → `Vec<FunctionNode>`
6. **For each function:**
   - **Build CFG** — `get_builder_for_function(function).build(function)` → `Cfg`
   - **Validate CFG** — Ensure entry/exit nodes, no cycles, reachability
   - **Extract metrics** — `metrics::extract_metrics(function, &cfg)` → `RawMetrics`
   - **Calculate risk** — `risk::analyze_risk_with_config(&metrics, weights, thresholds)` → `(RiskComponents, LRS, RiskBand)`
   - **Create report** — `FunctionRiskReport::new(...)` → `FunctionRiskReport`

**Output:** `Vec<FunctionRiskReport>` per file, aggregated across all files

---

### 3. Metrics Extraction (`hotspots-core/src/metrics.rs`)

Extracts 5 core metrics from AST + CFG:

#### Cyclomatic Complexity (CC)
- **Formula:** `CC = E - N + 2` (edges - nodes + 2)
- **Computed from:** CFG structure (number of decision points)
- **Language-specific extras:**
  - Go: Switch/select cases, boolean operators (`&&`, `||`)
  - Java: Switch cases, ternary operators, boolean operators
  - Python: Match cases, boolean operators
  - ECMAScript/Rust: CFG-based only

#### Nesting Depth (ND)
- **Definition:** Maximum depth of nested control structures (if/loop/switch/try)
- **Computed from:** AST traversal, tracking depth on entry/exit of control nodes
- **ECMAScript:** Visitor pattern with macro-generated visit methods
- **Tree-sitter languages:** Recursive traversal, depth tracking

#### Fan-Out (FO)
- **Definition:** Number of unique functions called by this function
- **Computed from:** AST traversal, extracting callee names from call expressions
- **ECMAScript:** `FanOutVisitor` walks `CallExpr`, extracts identifier/member names
- **Go/Java/Python:** Tree-sitter traversal, extracts `call_expression` → `identifier`/`selector_expression`
- **Rust:** Syn AST traversal, extracts function/method/macro calls
- **Stored in:** `RawMetrics.callee_names` (used later for call graph)

#### Non-Structured Exits (NS)
- **Definition:** Number of early returns, exceptions, panics, or other non-structured exits
- **Computed from:** AST traversal, counting:
  - `return` statements (excluding final tail return)
  - `throw` / `raise` / `panic()` calls
  - `defer` statements (Go)
  - `?` operator (Rust)
  - `unwrap()` / `expect()` calls (Rust)

#### Lines of Code (LOC)
- **Definition:** Physical lines from function start to end (inclusive)
- **Computed from:** `SourceSpan.end_line - SourceSpan.start_line + 1`
- **Includes:** Blank lines and comments within function body
- **Excludes:** Function signature if on separate line (language-dependent)

---

### 4. Risk Scoring (`hotspots-core/src/risk.rs`)

Transforms raw metrics into risk scores:

#### Risk Component Transforms
- **R_cc** = `min(log2(CC + 1), 6.0)` — Logarithmic scaling caps at 6
- **R_nd** = `min(ND, 8.0)` — Linear, capped at 8
- **R_fo** = `min(log2(FO + 1), 6.0)` — Logarithmic scaling caps at 6
- **R_ns** = `min(NS, 6.0)` — Linear, capped at 6

#### Local Risk Score (LRS)
Weighted sum of risk components:
```
LRS = w_cc * R_cc + w_nd * R_nd + w_fo * R_fo + w_ns * R_ns
```

**Default weights:**
- `w_cc = 1.0` (cyclomatic complexity)
- `w_nd = 0.8` (nesting depth)
- `w_fo = 0.6` (fan-out)
- `w_ns = 0.7` (non-structured exits)

**Configurable:** Weights and thresholds can be overridden via config file or CLI flags.

#### Risk Bands
- **Low:** LRS < 3.0
- **Moderate:** 3.0 ≤ LRS < 6.0
- **High:** 6.0 ≤ LRS < 9.0
- **Critical:** LRS ≥ 9.0

**Configurable:** Thresholds can be customized per project.

---

### 5. Call Graph Analysis (`hotspots-core/src/callgraph.rs`)

Builds a directed graph of function calls and computes graph metrics:

#### Graph Construction
1. **Add nodes** — All functions become graph nodes (ID: `file::function`)
2. **Add edges** — For each function, add edges to all callees in `RawMetrics.callee_names`
3. **Resolve calls** — Match callee names to function IDs:
   - Prefer same-file matches
   - Fall back to first match if no same-file match
   - Handle name collisions (multiple functions with same name)

> **Known limitation — best-effort static approximation:** Name-based resolution works well
> for direct function calls but cannot resolve interface dispatch, virtual methods, higher-order
> functions, or closures. In Go/Java/Python codebases that use idiomatic interface patterns,
> a meaningful fraction of call edges may be unresolved or resolved to the wrong target. All
> graph metrics derived from the call graph (PageRank, betweenness, fan-in, neighbor churn)
> are proportionally affected.

#### Graph Metrics

**Fan-In** — Number of functions calling this function
- Higher fan-in = more dependents = higher change risk

**Fan-Out** — Number of functions this function calls
- Already computed during metric extraction (reused here)

**PageRank** — Importance/centrality score
- Iterative algorithm (20-50 iterations, damping factor 0.85)
- Functions called by important functions get higher scores
- Identifies architectural hubs

**Betweenness Centrality** — Criticality on shortest paths
- Counts how many shortest paths between other functions pass through this one
- High betweenness = architectural bottleneck

**Strongly Connected Components (SCC)** — Tarjan's algorithm
- Detects cyclic dependencies (functions that call each other)
- `scc_id` and `scc_size` identify cycles
- Functions in larger cycles are riskier

**Dependency Depth** — Shortest path from entry points
- Entry points: `main`, exported functions, HTTP handlers (heuristic)
- BFS from all entry points computes shortest depth
- Deeper functions = longer dependency chain = more fragile

**Neighbor Churn** — Sum of churn in all callees
- Indirect change risk (dependencies are changing)
- High neighbor churn = function is affected by volatile dependencies

---

### 6. Activity-Weighted Risk Scoring (`hotspots-core/src/scoring.rs`)

Combines LRS with activity and graph metrics:

#### Formula
```
activity_risk = LRS * 1.0
              + churn_factor * 0.5
              + touch_factor * 0.3
              + recency_factor * 0.2
              + fan_in_factor * 0.4
              + scc_penalty * 0.3
              + depth_penalty * 0.1
              + neighbor_churn_factor * 0.2
```

#### Factor Calculations
- **churn_factor** = `(lines_added + lines_deleted) / 100`
- **touch_factor** = `min(touch_count_30d / 10, 5.0)`
- **recency_factor** = `max(0, 5.0 - days_since_last_change / 7)`
- **fan_in_factor** = `min(fan_in / 5, 10.0)`
- **scc_penalty** = `if scc_size > 1 { scc_size } else { 0 }`
- **depth_penalty** = `min(dependency_depth / 3, 5.0)`
- **neighbor_churn_factor** = `neighbor_churn / 500`

**Configurable:** All weights can be customized via config file.

> **Known limitation — fan-in/fan-out correlation:** Fan-out enters LRS via `R_fo`, and LRS
> feeds directly into `activity_risk`. Fan-in is added separately via `fan_in_factor`. Because
> fan-in and fan-out are positively correlated in most codebases (hub functions that call many
> things tend to also be called by many things), highly connected functions are penalized more
> than either metric implies in isolation. This is intentional but worth being aware of when
> interpreting scores for architectural hub functions.

---

### 7. Snapshot System (`hotspots-core/src/snapshot.rs`)

Immutable, commit-scoped snapshots of analysis results:

#### Snapshot Structure
```rust
struct Snapshot {
    schema_version: u32,           // v2 (current)
    commit: CommitInfo,             // SHA, parents, timestamp, branch, message, author
    analysis: AnalysisInfo,         // Scope, tool version
    functions: Vec<FunctionSnapshot>, // Per-function metrics + risk scores
    summary: Option<SnapshotSummary>, // Repo-level statistics (computed on output)
    aggregates: Option<Aggregates>,   // File risk, co-change, module instability (computed on output)
}
```

#### FunctionSnapshot
Contains:
- **Static metrics:** CC, ND, FO, NS, LOC, LRS, band
- **Git activity:** Churn (lines added/deleted), touch_count_30d, days_since_last_change
- **Call graph:** Fan-in, fan-out, PageRank, betweenness, SCC, dependency_depth, neighbor_churn
- **Activity risk:** Unified risk score + risk factor breakdown
- **Percentiles:** `is_top_10_pct`, `is_top_5_pct`, `is_top_1_pct` flags

#### SnapshotEnricher (Builder Pattern)
Enrichment pipeline with explicit ordering:
```rust
SnapshotEnricher::new(snapshot)
    .with_churn(&churn_map)
    .with_touch_metrics(repo_root)
    .with_callgraph(&call_graph)
    .enrich(&scoring_weights)
    .build()
```

**Why builder pattern?**
- Makes enrichment order explicit and testable
- `Snapshot` remains a pure data container
- Prevents accidental mutation or wrong ordering

#### Persistence
- **Location:** `.hotspots/snapshots/<commit_sha>.json`
- **Index:** `.hotspots/index.json` — tracks all snapshots, commit order, compaction level
- **Atomic writes:** Temp file + rename to prevent corruption
- **Immutable:** Snapshots never overwritten (commit SHA is identity; use `--force` to override)

#### Schema Versioning
- **Current version:** `SNAPSHOT_SCHEMA_VERSION = 2`
- **Minimum supported:** `SNAPSHOT_SCHEMA_MIN_VERSION = 1` (v1 snapshots load with missing fields defaulting to `None`)
- **Version range check:** `Snapshot::from_json()` explicitly rejects snapshots outside the supported range with a clear error: `"unsupported schema version: got X, supported range 1-2"`
- **Versioning contract:** Additive field changes increment the schema version and remain backward-compatible. Structural/breaking changes require a new minimum version and a migration guide.

---

### 8. Delta System (`hotspots-core/src/delta.rs`)

Compares current snapshot vs parent to identify changes:

#### Delta Structure
```rust
struct Delta {
    schema_version: u32,
    commit: DeltaCommitInfo,        // Current SHA, parent SHA
    baseline: bool,                  // true if no parent
    deltas: Vec<FunctionDeltaEntry>,
    policy: Option<PolicyResults>,   // Policy evaluation results
    aggregates: Option<DeltaAggregates>,
}
```

#### FunctionDeltaEntry
Tracks function changes:
- **Status:** `New`, `Deleted`, `Modified`, `Unchanged`
- **Before/After:** Function state (metrics, LRS, band) before and after
- **Delta:** Numeric changes (ΔCC, ΔND, ΔFO, ΔNS, ΔLRS)
- **Band transition:** e.g., `low` → `high` (risk band changed)
- **Suppression reason:** If function is ignored

#### Matching Logic
- **By function_id:** `file::function` (not by file path or line number)
- **File moves:** Treated as delete + add (function_id changes)
- **Line changes:** Don't affect matching (function_id unchanged)

> **Known limitation — function_id stability:** `function_id` depends on both the file path
> and the function name. File renames, directory moves, and function renames all generate a
> delete+add pair in delta output, losing continuity. This is especially noticeable in
> refactoring commits. Suppression annotations also cannot follow a renamed function since the
> ID changes. A content-hash or signature-based matching fallback is a planned improvement.

#### Baseline Handling
If no parent snapshot exists:
- All functions marked as `New`
- `baseline = true`
- No `before` state or deltas

---

### 9. Git Integration (`hotspots-core/src/git.rs`)

Extracts git metadata and activity metrics:

#### GitContext
- **Commit info:** SHA, parent SHAs, timestamp, branch, message, author
- **Event detection:** `is_fix_commit`, `is_revert_commit` (heuristic-based)
- **Ticket IDs:** Extracted from commit message and branch name (JIRA-123, #456, etc.)

#### Churn Metrics
- **Extraction:** `git show --numstat <sha>` → lines added/deleted per file
- **Mapping:** File-level churn mapped to functions by file path
- **Format:** `<added>\t<deleted>\t<file>` (binary files skipped)

#### Touch Metrics
- **Touch count:** `git log --since="30 days ago" --oneline -- <file>` → commit count
- **Days since last change:** Time from commit timestamp to last file modification
- **Computed at:** File level (all functions in file share same touch metrics)

> **Known limitation — file-level granularity:** Touch count and days-since-last-change are
> computed once per file and applied uniformly to every function in that file. A file with 50
> functions where only one was recently touched will report the same `touch_count_30d` for all
> 50. This means `touch_factor` in the activity risk score is a file-level approximation, not
> a per-function signal. Large files with many functions of varying activity levels will have
> noisier touch scores. Function-level touch metrics via `git log -L` are a planned improvement.

#### PR Context Detection
- **Mechanism:** CI environment variables only (`GITHUB_BASE_REF`, `CI`, `PULL_REQUEST`, etc.)
- **Behavior in CI:** PR commits are detected and snapshot persistence is suppressed
- **Behavior locally:** Running `--mode snapshot` on any branch (including feature branches)
  is treated as mainline — a snapshot will be persisted. There is no local branch detection.

#### Co-Change Pairs
- **`extract_co_change_pairs(repo, window_days, min_count)`** — Walks git log for
  the last N days, counts pairwise file co-occurrences, normalizes by minimum file
  commit count, and returns `Vec<CoChangePair>`
- **Default:** 90-day window, `min_count = 3`
- **Filtering:** Ghost files (renamed/deleted) and trivially expected pairs
  (e.g., `foo.rs` + `foo_test.rs`, `mod.rs` + sibling) are excluded

#### Repository-Aware Operations
- **`extract_git_context_at(repo_path)`** — Uses explicit repo root (not CWD)
- **`extract_commit_churn_at(repo_path, sha)`** — Same
- **Graceful degradation:** Shallow clones, missing parents handled without errors

---

### 10. Policy Evaluation (`hotspots-core/src/policy.rs`)

Evaluates policies on deltas to enforce quality gates:

#### Policy Types
1. **Critical Introduction** — Block new functions with critical risk
2. **Critical Regression** — Block functions that moved to critical band
3. **High Regression** — Warn on functions that moved to high band
4. **LRS Increase** — Warn on functions with LRS increase > threshold
5. **Metric Regression** — Warn on significant metric increases
6. **Band Transition** — Warn on any band transition (low→moderate, etc.)

#### Evaluation Flow
1. Filter suppressed functions (`suppression_reason.is_some()`)
2. Check function status (`New`, `Modified`, etc.)
3. Evaluate policy condition (LRS threshold, band change, etc.)
4. Collect failures and warnings
5. Return `PolicyResults` with blocking failures and warnings

#### CI Integration
- **Blocking failures** → CI fails
- **Warnings** → CI passes but reports issues
- **Suppressed functions** → Ignored (explicit opt-out)

---

### 11. Output Formats

#### Text Format

In basic mode (`hotspots analyze src/`) the text output is a simple ranked table
(LRS, File, Line, Function, Risk). In snapshot mode (`--mode snapshot --format text`)
the text format requires one of three sub-modes:

- **`--explain`** — Per-function human-readable breakdown: metric contributions, activity
  signals (churn, touch count, fan-in, SCC, depth), plus a co-change coupling section
  showing the top 10 high/moderate source-file pairs.
- **`--level file`** — Ranked file risk table (one row per file): max CC, avg CC,
  function count, LOC, critical-band count, file churn, composite `file_risk_score`.
- **`--level module`** — Ranked module instability table (one row per directory):
  file count, function count, avg CC, afferent/efferent coupling, instability, risk.

#### JSON Format
- Complete snapshot/delta structure
- Pretty-printed for readability
- Includes all metrics, risk scores, and metadata

#### JSONL Format
- One JSON object per line (newline-delimited)
- Each line is a complete `FunctionSnapshot`
- Suitable for streaming, database ingestion, DuckDB

#### HTML Format
- Interactive report with:
  - Sortable tables
  - Risk band color coding
  - Expandable function details
  - Call graph visualization (future)
- Written to `.hotspots/report.html` by default

---

### 12. Trend Analysis (`hotspots-core/src/trends.rs`)

Analyzes the accumulated snapshot history to surface how risk has evolved over time.
Requires multiple snapshots in `.hotspots/snapshots/` (collected by CI or `hotspots replay`).

**CLI:** `hotspots trends <path> [--window N] [--top K] [--format json|text]`

#### What It Computes

**Risk Velocity (`Vec<RiskVelocity>`)** — Rate and direction of LRS change per function over
the analysis window. Each entry includes `velocity: f64`, `direction` (Positive/Negative/Flat),
`first_lrs`, `last_lrs`, and `commit_count`.

**Hotspot Stability (`Vec<HotspotAnalysis>`)** — Consistency of a function appearing in the
top-K highest-risk functions across snapshots. Classified as:
- `Stable` — consistently in top-K (chronic hotspot)
- `Emerging` — recently appeared in top-K (rising risk)
- `Volatile` — intermittently in top-K (inconsistent)

**Refactor Effectiveness (`Vec<RefactorAnalysis>`)** — Detects functions that had a significant
LRS drop and tracks whether the improvement held. Classified as:
- `Successful` — improvement of ≥ 1.0 LRS, sustained, no rebound
- `Partial` — improvement occurred but partially rebounded
- `Cosmetic` — improvement below significance threshold

#### Limitations
- Requires at least 2 snapshots to compute velocity; more snapshots yield more meaningful trends
- Operates on LRS (complexity-based score), not `activity_risk`, as the stable historical signal
- Snapshots must be on the same branch/mainline for meaningful comparison
- `--format html` not yet implemented (planned)

---

### 13. Aggregate Analysis (`hotspots-core/src/aggregates.rs`, `hotspots-core/src/imports.rs`)

Computes higher-level risk views from per-function data and git history. All three
aggregates are computed at output time and included in snapshot JSON under `aggregates`.

#### File Risk (D-1) — `compute_file_risk_views()`

Folds per-function data into one `FileRiskView` per unique file. No new git calls needed;
all inputs come from the enriched `FunctionSnapshot` list.

```
file_risk_score = max_cc × 0.4
               + avg_cc × 0.3
               + log2(function_count + 1) × 0.2
               + file_churn_factor × 0.1
```

Ranked by `file_risk_score` descending. Accessible via `--level file` text output or
`aggregates.file_risk` in JSON.

#### Co-Change Coupling (D-2) — `git::extract_co_change_pairs()`

Mined from git log in `git.rs`, surfaced in aggregates. See section 9 (git.rs) for the
extraction details. Pairs are stored in `aggregates.co_change`. Shown in `--explain`
output as a coupling section below the per-function list.

#### Module Instability (D-3) — `compute_module_instability()`

Parses `use`/`import` statements per language (via `imports.rs`) to build a file-level
import graph, then aggregates to directory level:

- **Afferent coupling** — number of external directories that import from this directory
- **Efferent coupling** — number of external directories this directory imports from
- **Instability** = `efferent / (afferent + efferent)` (0.0 = depended on by all; 1.0 = depends on others only)
- **`module_risk`** = `high` if `instability < 0.3` and `avg_complexity > 10`

Accessible via `--level module` text output or `aggregates.modules` in JSON.

> **Resolution quality note:** Import-based resolution is used for D-3 (not name-based
> call graph resolution). This gives better coverage than the function-level call graph
> but is still best-effort — re-exports, conditional imports, and generated code may
> produce inaccurate edge counts.

---

## Data Flow Examples

### Example 1: Basic Analysis (No Snapshot Mode)

```
1. CLI: hotspots analyze src/
2. Load config (.hotspotsrc.json or defaults)
3. Collect source files (recursive, filtered by include/exclude)
4. For each file:
   a. Parse → ParsedModule
   b. Discover functions → Vec<FunctionNode>
   c. For each function:
      - Build CFG
      - Extract metrics (CC, ND, FO, NS, LOC)
      - Calculate LRS and risk band
5. Aggregate reports across all files
6. Sort by LRS descending
7. Output: Text table or JSON
```

### Example 2: Snapshot Mode

```
1. CLI: hotspots analyze src/ --mode snapshot
2. Run basic analysis (steps 1-4 above)
3. Extract git context (SHA, parents, timestamp, etc.)
4. Build call graph from callee_names
5. Extract churn metrics (if parent exists)
6. Extract touch metrics (30-day commit count)
7. Enrich snapshot:
   - Populate churn → functions
   - Populate touch metrics → functions
   - Populate call graph metrics → functions
   - Compute activity risk → functions
   - Compute percentiles → functions
   - Compute summary → snapshot
8. Persist snapshot: .hotspots/snapshots/<sha>.json
9. Update index: .hotspots/index.json
10. Output: JSON/HTML/JSONL
```

### Example 3: Delta Mode

```
1. CLI: hotspots analyze src/ --mode delta
2. Run snapshot enrichment (steps 1-7 above)
3. Load parent snapshot: .hotspots/snapshots/<parent_sha>.json
4. Compute delta:
   - Match functions by function_id
   - Compare metrics/LRS/band
   - Classify: New/Deleted/Modified/Unchanged
   - Calculate numeric deltas
   - Detect band transitions
5. Evaluate policies (if --policy flag):
   - Check each policy condition
   - Collect failures and warnings
6. Output: Delta JSON (with policy results if requested)
```

---

## Language-Specific Details

### ECMAScript (TypeScript/JavaScript)

**Parser:** SWC (same as TypeScript compiler)
- **Advantages:** Full TypeScript support, JSX, decorators, type-aware
- **AST:** Owned `swc_ecma_ast::Module` (can store directly)
- **CFG Builder:** Visitor pattern, handles if/switch/loop/break/continue
- **Metrics:** AST-based for all metrics
- **Callee extraction:** `FanOutVisitor` walks `CallExpr`, extracts identifiers/members

### Go

**Parser:** tree-sitter-go
- **AST:** Tree-sitter nodes (tied to tree lifetime) → store source + node ID
- **CFG Builder:** Re-parse with tree-sitter, traverse AST
- **Metrics:** Tree-sitter AST traversal
- **Callee extraction:** Extract `call_expression` → `identifier`/`selector_expression`
- **Special handling:** `defer`, `go` statements, `panic()`, `os.Exit()`, `log.Fatal*`

### Java

**Parser:** tree-sitter-java
- **AST:** Tree-sitter nodes → store source + node ID
- **CFG Builder:** Re-parse with tree-sitter, handles if/switch/loop/try-catch
- **Metrics:** Tree-sitter AST traversal
- **Callee extraction:** Extract method calls, constructor calls
- **Special handling:** Ternary operators, lambda expressions (partial)

### Python

**Parser:** tree-sitter-python
- **AST:** Tree-sitter nodes → store source + node ID
- **CFG Builder:** Re-parse with tree-sitter, handles if/elif/else/for/while/try-except-finally
- **Metrics:** Tree-sitter AST traversal
- **Callee extraction:** Extract function calls
- **Special handling:** Match statements (partial CFG support)

### Rust

**Parser:** syn (same as rustc)
- **AST:** Owned `syn::ItemFn` (can store directly)
- **CFG Builder:** Traverse syn AST, handles if/match/loop/break/continue
- **Metrics:** Syn AST traversal
- **Callee extraction:** Extract function/method/macro calls
- **Special handling:** `?` operator, `unwrap()`/`expect()`, `panic!()`, `unreachable!()`

---

## Configuration System (`hotspots-core/src/config.rs`)

### Config File Discovery
1. Explicit path (`--config path/to/config.json`)
2. `.hotspotsrc.json` in project root
3. `hotspots.config.json` in project root
4. `package.json` under `"hotspots"` key

### Config Structure
```json
{
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts", "**/node_modules/**"],
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 0.8,
    "fo": 0.6,
    "ns": 0.7
  },
  "scoring_weights": {
    "churn": 0.5,
    "touch": 0.3,
    "recency": 0.2,
    "fan_in": 0.4,
    "scc": 0.3,
    "depth": 0.1,
    "neighbor_churn": 0.2
  },
  "min_lrs": 0.0,
  "top": null
}
```

### ResolvedConfig
Merges user config with defaults, compiles glob patterns for fast matching.

---

## Suppression System (`hotspots-core/src/suppression.rs`)

Functions can be suppressed with inline comments:

```typescript
// hotspots-ignore: This is a legacy function, will be removed in v2
function oldFunction() {
  // ...
}
```

**Behavior:**
- Suppressed functions are still analyzed and included in snapshots
- They are excluded from policy evaluation
- Suppression reason is stored in snapshot/delta
- Useful for documenting intentional tech debt

---

## Performance Characteristics

### Time Complexity
- **Parsing:** O(n) per file (n = file size)
- **Function discovery:** O(n) per file (AST traversal)
- **CFG construction:** O(n) per function (n = function size)
- **Metric extraction:** O(n) per function (AST/CFG traversal)
- **Call graph:** O(V + E) where V = functions, E = call edges
- **PageRank:** O(V * E * iterations) ≈ O(V * E * 20)
- **Betweenness:** O(V * E) (Brandes algorithm)
- **SCC:** O(V + E) (Tarjan's algorithm)

### Space Complexity
- **AST storage:** O(n) per file
- **CFG:** O(n) per function (nodes + edges)
- **Call graph:** O(V + E)
- **Snapshots:** O(V) per snapshot (one entry per function)

### Typical Performance
- **Small repo** (< 1000 functions): < 1 second
- **Medium repo** (1000-10000 functions): 1-10 seconds
- **Large repo** (> 10000 functions): 10-60 seconds

**Bottlenecks:**
- Git operations (churn, touch metrics) — can be slow on large repos
- Call graph PageRank — O(V * E) can be expensive for very large graphs
- File I/O — reading many small files
- **Tree-sitter re-parse (Go/Java/Python):** CFG builders for Go, Java, and Python re-parse
  the full source file for every function in that file (O(n × m) where n = functions, m = file
  size). This is a design trade-off: tree-sitter nodes are tied to the tree's lifetime, so
  the source must be re-parsed to find the target function node. ECMAScript and Rust are
  unaffected (SWC/syn provide owned ASTs). A parse-result cache scoped per analysis run is
  a planned fix.

---

## Extensibility

### Adding a New Language

1. **Implement `LanguageParser`:**
   - Parse source → `ParsedModule`
   - Implement `discover_functions()` → `Vec<FunctionNode>`

2. **Implement `CfgBuilder`:**
   - Build CFG from `FunctionNode`
   - Handle control flow (if/loop/switch/break/continue)

3. **Add metric extraction:**
   - Implement `extract_*_metrics()` in `metrics.rs`
   - Extract CC, ND, FO, NS, LOC from AST/CFG
   - Extract callee names for call graph

4. **Update language enum:**
   - Add language variant
   - Add extension mapping
   - Add parser/CFG builder dispatch

5. **Add tests:**
   - Golden tests with known outputs
   - Language parity tests (equivalent functions across languages)

**Estimated effort:** 8-16 hours per language

---

## Testing Strategy

### Unit Tests
- Per-module tests for parsing, CFG building, metric extraction
- Language-specific test fixtures

### Golden Tests
- Deterministic output comparison
- 31+ golden test files covering various code patterns
- Regenerated when metrics change

### Integration Tests
- End-to-end analysis of real repositories
- Git history tests (churn, touch metrics)
- Snapshot persistence and delta computation

### Invariant Tests
- Determinism tests (identical input → identical output)
- Formatting independence tests
- Suppression system tests

---

## Future Architecture Considerations

### Planned Enhancements
1. **Cross-repo call graph** — Stitch call graphs across multiple repositories (hotspots-cloud)
2. **ML-based scoring** — Learn risk weights from historical incident data (hotspots-cloud)
3. **External event correlation** — Jira tickets, PagerDuty incidents (hotspots-cloud)

### Architectural Boundaries

**hotspots CLI (current scope):**
- Single-repo analysis
- Static metrics + git activity
- Call graph within repo
- Snapshot persistence
- Trend analysis from accumulated snapshots (`hotspots trends`)

**hotspots-cloud (future scope):**
- Multi-repo aggregation
- Time-series analysis
- ML model training
- External API integration
- Team/org dashboards

---

## Key Design Decisions

### Why Per-Function Analysis?
- **Isolation** — Each function analyzed independently
- **Parallelization** — Functions can be analyzed in parallel (future)
- **Incremental** — Only changed functions need re-analysis
- **Clarity** — Results map directly to code units developers understand

### Why Deterministic Ordering?
- **Reproducibility** — Same code → same results
- **Testing** — Golden tests can compare byte-for-byte
- **Debugging** — Consistent ordering makes issues easier to track

### Why Immutable Snapshots?
- **History** — Complete analysis history preserved
- **Auditability** — Can verify past analysis results
- **Delta computation** — Compare any two snapshots
- **No corruption** — Atomic writes prevent partial snapshots

### Why AST-Based Metrics?
- **Accuracy** — Understands code structure, not text patterns
- **Language-aware** — Handles language-specific constructs correctly
- **Formatting-independent** — Whitespace doesn't affect metrics
- **Extensible** — Easy to add new metrics or languages

### Why Activity-Weighted Scoring?
- **Actionable** — Identifies code that's both complex AND changing
- **Prioritization** — Focuses effort on highest-impact refactoring
- **Evidence-based** — Uses git history, not gut feeling

---

## Glossary

- **LRS** — Local Risk Score (complexity-based risk, 0-20+)
- **Activity Risk** — Unified risk score combining LRS + activity + graph metrics
- **CFG** — Control Flow Graph (representation of function control flow)
- **SCC** — Strongly Connected Component (cyclic dependency group)
- **Fan-In** — Number of functions calling this function
- **Fan-Out** — Number of functions this function calls
- **Churn** — Lines added/deleted in a commit
- **Touch Count** — Number of commits modifying a file in last 30 days
- **Snapshot** — Immutable analysis result for a specific commit
- **Delta** — Comparison between two snapshots
- **Function ID** — Unique identifier: `file_path::function_name`

---

**Document Status:** Current as of 2026-02-19
**Maintainer:** Stephen Collins  
**Questions?** Open an issue or see `docs/` for more details.
