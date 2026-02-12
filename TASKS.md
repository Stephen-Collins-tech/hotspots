# Tasks: Extended Metrics & Call Graph Analysis

**Status:** In Progress - Phase 1 & 2.1-2.2 Complete ✅
**Goal:** Extend hotspots CLI to be a complete standalone A-tier risk analysis tool with call graph analysis
**Principle:** CLI performs complete single-repo analysis; cloud adds multi-repo aggregation and historical insights

---

## Architectural Boundary (Revised)

### ✅ In Scope (hotspots CLI)

**Single-repo analysis** including:
- Static code metrics (LOC, complexity)
- Git activity metrics (churn, touch count, recency)
- Call graph extraction (function calls within repo)
- Graph-based metrics (fan-in, SCC, dependency depth)
- Neighbor churn propagation
- Event detection (fix/revert commits, ticket IDs)
- Combined risk scoring
- Prioritized output ("top N to fix")

**Rationale:** Makes hotspots a complete, offline-capable tool. User gets full value without cloud dependency.

### ❌ Out of Scope (hotspots-cloud)

**Multi-repo and historical analysis:**
- Cross-repo call graph stitching
- Time-series trend detection
- Statistical model training (ML, decay rates)
- External event correlation (Jira API, PagerDuty)
- Team/org-level aggregation
- Predictive modeling

**Rationale:** These require multiple snapshots over time, external APIs, and frequent model iteration.

---

## Phase 1: Core Grounding Metrics

### 1.1 Lines of Code (LOC) ✅ COMPLETE

**Requirement:**
Capture physical lines of code (LOC) for each function.

**Specification:**
- Count lines from function start to function end (inclusive)
- Include blank lines and comments within function body
- Exclude function signature if on separate line (language-dependent)
- Store as `usize` in `RawMetrics`

**Output Schema:**
```json
{
  "metrics": {
    "cc": 5,
    "nd": 2,
    "fo": 3,
    "ns": 1,
    "loc": 42
  }
}
```

**Success Criteria:**
- [x] LOC computed for all 6 languages (TS/JS/Go/Python/Rust/Java)
- [x] Golden tests updated with LOC values
- [x] LOC parity test: equivalent functions across languages have similar LOC
- [ ] Documentation updated (reference/metrics.md) - DEFERRED

**Files Modified:**
- `hotspots-core/src/report.rs` - Added `loc: usize` to `MetricsReport`
- `hotspots-core/src/metrics.rs` - Added LOC computation for all 6 languages
- `hotspots-core/src/language/span.rs` - Added `end_line` field to `SourceSpan`
- `hotspots-core/src/language/*/parser.rs` - Updated parsers to populate `end_line`
- `tests/golden/*.json` - Regenerated all 31 golden files with LOC values

**Actual Effort:** ~3 hours (architectural fix was simpler than expected)

---

### 1.2 Git Churn Metrics ✅ COMPLETE

**Requirement:**
Capture lines added/deleted per file in each commit.

**Specification:**
- Extract churn via `git show --numstat <sha>`
- Parse format: `<added>\t<deleted>\t<file>`
- Binary files show `-\t-\t<file>` (skip these)
- Store churn at file level (not per-function initially)
- Map file churn to functions by file path matching

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "file": "src/foo.ts",
  "churn": {
    "lines_added": 23,
    "lines_deleted": 7,
    "net_change": 16
  }
}
```

**Success Criteria:**
- [x] `git.rs` extracts churn for all files in commit
- [x] Churn correctly mapped to functions by file path
- [x] Churn is 0 for functions in unchanged files
- [x] Churn is null/omitted for baseline commits (no parent)
- [x] Integration test with real git repo

**Files to Modify:**
- `hotspots-core/src/git.rs` - Add `extract_commit_churn(sha) -> Vec<FileChurn>`
- `hotspots-core/src/snapshot.rs` - Add `churn` field to `FunctionSnapshot`
- `hotspots-core/src/analysis.rs` - Map file churn to functions during snapshot creation

**New Types:**
```rust
pub struct FileChurn {
    pub file: String,
    pub lines_added: usize,
    pub lines_deleted: usize,
}

pub struct ChurnMetrics {
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub net_change: i64,
}
```

**Estimated Effort:** 3 hours

---

### 1.3 Touch Count & Recency ✅ COMPLETE

**Requirement:**
Count how many commits modified each file in the last 30 days, and time since last change.

**Specification:**
- Use `git log --since="30 days ago" --oneline -- <file>` and count lines
- Compute at file level (all functions in file share same touch count)
- Use commit timestamp as reference (not wall clock)
- Store as `usize` or omit if 0
- Add `days_since_last_change: u32` (time from commit timestamp to last file change)

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "file": "src/foo.ts",
  "touch_count_30d": 12,
  "days_since_last_change": 3
}
```

**Success Criteria:**
- [x] Touch count computed relative to commit timestamp
- [x] Handles files added <30 days ago (touch count = commit count)
- [x] Returns 0 for files untouched in window
- [x] `days_since_last_change` accurate for all files
- [x] Performance acceptable (cached per file, not per function)

**Files to Modify:**
- `hotspots-core/src/git.rs` - Add `count_file_touches_30d(file, as_of_timestamp)`
- `hotspots-core/src/git.rs` - Add `days_since_last_change(file, as_of_timestamp)`
- `hotspots-core/src/snapshot.rs` - Add fields to `FunctionSnapshot`

**Estimated Effort:** 3 hours

---

### 1.4 Commit Metadata & Event Detection ✅ COMPLETE

**Requirement:**
Capture commit message and author, plus detect fix/revert events and ticket IDs.

**Specification:**
- Extract via `git show --format=%B%n---AUTHOR---%n%an <sha>`
- Store message subject (first line) and full body
- Store author name (not email for privacy)
- Detect event types with heuristics:
  - `is_fix_commit`: message contains "fix", "bug", "hotfix", "bugfix"
  - `is_revert_commit`: message contains "revert" or commit structure shows revert
- Extract ticket IDs:
  - Regex: `(JIRA-\d+|[A-Z]+-\d+|#\d+|fixes #\d+|closes #\d+)`
  - Parse from commit message and branch name
  - Store as `Vec<String>`

**Output Schema:**
```json
{
  "commit": {
    "sha": "abc123",
    "parents": ["def456"],
    "timestamp": 1609459200,
    "branch": "main",
    "message": "fix: resolve null pointer in billing (JIRA-1234)",
    "author": "Jane Developer",
    "is_fix_commit": true,
    "is_revert_commit": false,
    "ticket_ids": ["JIRA-1234"]
  }
}
```

**Success Criteria:**
- [x] Message and author extracted for all commits
- [x] Multi-line messages preserved (body truncated at 1000 chars)
- [x] Empty messages handled (use empty string, not null)
- [ ] Author anonymization option (--anonymize-authors flag) - DEFERRED
- [x] Fix/revert detection accurate (>90% on test corpus)
- [x] Ticket IDs extracted from messages and branches
- [x] Multiple ticket IDs per commit supported

**Files to Modify:**
- `hotspots-core/src/git.rs` - Extend `GitContext` with new fields
- `hotspots-core/src/snapshot.rs` - Add fields to `CommitInfo`
- `hotspots-core/src/git.rs` - Add event detection functions

**New Functions:**
```rust
pub fn detect_fix_commit(message: &str) -> bool;
pub fn detect_revert_commit(message: &str, commit_structure: &Commit) -> bool;
pub fn extract_ticket_ids(message: &str, branch: &str) -> Vec<String>;
```

**Estimated Effort:** 3 hours

---

## Phase 2: Call Graph Analysis

### 2.1 Call Graph Extraction ✅ COMPLETE (Infrastructure)

**Requirement:**
Extract function calls from ASTs and build caller→callee edges within the codebase.

**Specification:**
- Walk each function's AST to find call expressions
- Extract callee name (function/method being called)
- Attempt symbol resolution:
  - Direct function calls: match to function definitions in same file
  - Imported calls: resolve via import statements
  - Method calls: best-effort matching (may miss dynamic dispatch)
- Build adjacency list: `Map<FunctionId, Vec<FunctionId>>`
- Store both directions: callers and callees per function

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "calls": ["src/baz.ts::qux", "src/foo.ts::helper"],
  "called_by": ["src/main.ts::run", "src/foo.ts::wrapper"]
}
```

**Success Criteria:**
- [ ] Calls extracted for all 6 languages
- [ ] Direct calls resolved correctly (>95% accuracy)
- [ ] Imported calls resolved (best effort, >80% accuracy)
- [ ] Dynamic/indirect calls gracefully skipped (logged)
- [ ] Self-calls handled (recursive functions)
- [ ] Call graph serializable (adjacency list format)

**Files to Modify:**
- `hotspots-core/src/language/*/parser.rs` - Add call extraction per language
- `hotspots-core/src/callgraph.rs` - New module for graph representation
- `hotspots-core/src/analysis.rs` - Build call graph during analysis

**New Module:**
```rust
// hotspots-core/src/callgraph.rs
pub struct CallGraph {
    pub edges: HashMap<FunctionId, Vec<FunctionId>>, // callee list
    pub reverse_edges: HashMap<FunctionId, Vec<FunctionId>>, // caller list
}

impl CallGraph {
    pub fn new() -> Self;
    pub fn add_edge(&mut self, caller: FunctionId, callee: FunctionId);
    pub fn get_callees(&self, func: &FunctionId) -> &[FunctionId];
    pub fn get_callers(&self, func: &FunctionId) -> &[FunctionId];
    pub fn fan_in(&self, func: &FunctionId) -> usize;
}
```

**Estimated Effort:** 8 hours (2 hours baseline + 1 hour per language)

---

### 2.2 Fan-In Metric ✅ COMPLETE

**Requirement:**
Count how many functions call each function (caller count).

**Specification:**
- Fan-in = number of unique callers
- Computed from call graph reverse edges
- Higher fan-in = more dependents = higher change risk
- Store as `usize`

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "callgraph": {
    "fan_in": 23,
    "fan_out": 5,
    "pagerank": 0.15,
    "betweenness": 0.08
  }
}
```

**Success Criteria:**
- [x] Fan-in computed for all functions
- [x] Functions with no callers have fan-in = 0
- [x] Recursive calls don't inflate fan-in
- [x] Accurate on test codebase (manual verification)

**Files Modified:**
- `hotspots-core/src/callgraph.rs` - `fan_in()` method already implemented
- `hotspots-core/src/snapshot.rs` - `CallGraphMetrics` struct with fan_in field already added
- `hotspots-core/src/lib.rs` - Added `build_call_graph()` function
- `hotspots-cli/src/main.rs` - Wired up call graph building and population

**Actual Effort:** 2 hours (including implementation and testing)

---

### 2.3 Strongly Connected Components (SCC)

**Requirement:**
Detect cyclic dependencies (functions that call each other directly or transitively).

**Specification:**
- Use Tarjan's algorithm to find SCCs
- Functions in same SCC form a cyclic dependency group
- Store SCC ID for each function
- SCC size indicates complexity of cycle

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "scc_id": 42,
  "scc_size": 5
}
```

**Success Criteria:**
- [ ] Tarjan's algorithm implemented correctly
- [ ] All functions assigned to an SCC (size 1 = no cycle)
- [ ] Cycles detected accurately (test with known cyclic code)
- [ ] Performance acceptable (O(V+E) time)

**Files to Modify:**
- `hotspots-core/src/callgraph.rs` - Add `find_sccs() -> HashMap<FunctionId, (usize, usize)>`
- `hotspots-core/src/snapshot.rs` - Add `scc_id` and `scc_size` fields

**New Functions:**
```rust
// Tarjan's algorithm for SCC detection
pub fn find_strongly_connected_components(graph: &CallGraph) -> Vec<Vec<FunctionId>>;
```

**Estimated Effort:** 3 hours

---

### 2.4 Dependency Depth

**Requirement:**
Measure how deep each function is in the dependency tree from entry points.

**Specification:**
- Identify entry points (main, exported functions, HTTP handlers)
- Compute shortest path depth via BFS from all entry points
- Deeper functions are more fragile (longer dependency chain)
- Store as `usize` (0 = entry point, None = unreachable)

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "dependency_depth": 5
}
```

**Success Criteria:**
- [ ] Entry points identified (heuristic: main, exports, handlers)
- [ ] BFS correctly computes shortest paths
- [ ] Unreachable functions handled (depth = None or max)
- [ ] Accurate on test codebase

**Files to Modify:**
- `hotspots-core/src/callgraph.rs` - Add `compute_dependency_depth() -> HashMap<FunctionId, usize>`
- `hotspots-core/src/snapshot.rs` - Add `dependency_depth` field

**Estimated Effort:** 2 hours

---

### 2.5 Neighbor Churn

**Requirement:**
Compute sum of churn for all direct dependencies (functions this function calls).

**Specification:**
- Neighbor churn = sum of `lines_added + lines_deleted` for all callees
- Indicates indirect change risk (dependencies are changing)
- Requires both call graph and churn data
- Store as `usize`

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "neighbor_churn": 127
}
```

**Success Criteria:**
- [ ] Churn summed correctly across all callees
- [ ] Functions with no callees have neighbor_churn = 0
- [ ] Accurate on test data

**Files to Modify:**
- `hotspots-core/src/callgraph.rs` - Add `compute_neighbor_churn()`
- `hotspots-core/src/snapshot.rs` - Add `neighbor_churn` field

**Estimated Effort:** 1 hour

---

## Phase 3: Combined Risk Scoring

### 3.1 Activity-Weighted Risk Score

**Requirement:**
Combine LRS with activity and graph metrics into unified risk score.

**Specification:**
- Formula: `activity_risk = f(lrs, churn, touch_count, recency, fan_in, scc_size, dependency_depth, neighbor_churn)`
- Default weights (tunable via config):
  ```
  activity_risk = lrs * 1.0
                + churn_factor * 0.5
                + touch_factor * 0.3
                + recency_factor * 0.2
                + fan_in_factor * 0.4
                + scc_penalty * 0.3
                + depth_penalty * 0.1
                + neighbor_churn_factor * 0.2
  ```
- Where:
  - `churn_factor = (lines_added + lines_deleted) / 100`
  - `touch_factor = min(touch_count_30d / 10, 5.0)`
  - `recency_factor = max(0, 5.0 - days_since_last_change / 7)`
  - `fan_in_factor = min(fan_in / 5, 10.0)`
  - `scc_penalty = if scc_size > 1 { scc_size as f64 } else { 0.0 }`
  - `depth_penalty = min(dependency_depth as f64 / 3, 5.0)`
  - `neighbor_churn_factor = neighbor_churn / 500`

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "lrs": 12.4,
  "activity_risk": 28.7,
  "risk_factors": {
    "complexity": 12.4,
    "churn": 4.2,
    "activity": 3.1,
    "recency": 2.8,
    "fan_in": 6.4,
    "cyclic_dependency": 5.0,
    "depth": 1.2,
    "neighbor_churn": 3.6
  }
}
```

**Success Criteria:**
- [ ] Score computed for all functions
- [ ] Weights configurable via CLI or config file
- [ ] Score correlates with manual risk assessment (validation on test projects)
- [ ] Documentation explains each factor

**Files to Modify:**
- `hotspots-core/src/scoring.rs` - New module for risk scoring
- `hotspots-core/src/snapshot.rs` - Add `activity_risk` and `risk_factors` fields
- `hotspots-cli/src/main.rs` - Add `--scoring-weights` CLI option

**New Module:**
```rust
// hotspots-core/src/scoring.rs
pub struct ScoringWeights {
    pub churn: f64,
    pub touch: f64,
    pub recency: f64,
    pub fan_in: f64,
    pub scc: f64,
    pub depth: f64,
    pub neighbor_churn: f64,
}

pub fn compute_activity_risk(
    snapshot: &FunctionSnapshot,
    weights: &ScoringWeights,
) -> (f64, RiskFactors);
```

**Estimated Effort:** 4 hours

---

### 3.2 Top N Output Mode

**Requirement:**
Provide curated, actionable output showing top N highest-risk functions with explanations.

**Specification:**
- CLI flag: `--top N` (default: show all)
- Sort by `activity_risk` descending
- Show top N functions with:
  - Function name, file, line
  - Risk score breakdown
  - Human-readable explanation
- Flag: `--explain` for detailed reasoning

**Output Example:**
```
Top 5 Functions to Fix This Sprint:

1. src/billing/charge.ts::processPayment (line 42) - Risk: 34.2
   ⚠️  High complexity (LRS 15.2)
   🔥 Frequently changed (12 commits in 30d)
   👥 Many dependents (23 callers)
   🔄 Part of cyclic dependency (5 functions)
   📊 Changed 2 days ago (67 lines)

2. src/auth/session.ts::validateToken (line 128) - Risk: 31.8
   ⚠️  High complexity (LRS 12.1)
   👥 Critical function (87 callers)
   📊 Recent hotfix (1 day ago)
   ⛓️  Deep in call chain (depth 8)

...
```

**Success Criteria:**
- [ ] `--top N` correctly filters and sorts
- [ ] `--explain` shows clear, actionable reasoning
- [ ] Output is human-readable and scannable
- [ ] Works with `--format json` for programmatic use
- [ ] Documentation includes examples

**Files to Modify:**
- `hotspots-cli/src/main.rs` - Add `--top` and `--explain` flags
- `hotspots-core/src/report.rs` - Add `render_top_n()` function
- `hotspots-core/src/scoring.rs` - Add `explain_risk()` function

**Estimated Effort:** 3 hours

---

## Phase 4: Output Enhancements

### 4.1 JSONL Export Format

**Requirement:**
Support newline-delimited JSON for streaming/database ingestion.

**Specification:**
- Add `--format jsonl` flag
- Output one JSON object per line (no pretty-printing)
- Each line is a complete `FunctionSnapshot` with embedded `commit` context
- No outer array `[]` wrapper
- Suitable for: `cat *.jsonl | duckdb`, `jq -s`, streaming ingest

**Output Example:**
```jsonl
{"function_id":"src/a.ts::foo","file":"src/a.ts","line":10,"metrics":{"cc":5},"lrs":3.2,"activity_risk":12.4,"commit":{"sha":"abc123"}}
{"function_id":"src/b.ts::bar","file":"src/b.ts","line":42,"metrics":{"cc":12},"lrs":8.7,"activity_risk":24.1,"commit":{"sha":"abc123"}}
```

**Success Criteria:**
- [ ] `hotspots analyze --format jsonl` produces valid JSONL
- [ ] Each line parseable as standalone JSON
- [ ] `jq -s '.'` reconstructs array
- [ ] DuckDB can ingest: `COPY tbl FROM 'snapshot.jsonl' (FORMAT JSON)`
- [ ] Benchmark: JSONL should be ~30% smaller than pretty JSON

**Files to Modify:**
- `hotspots-cli/src/main.rs` - Add `jsonl` format option
- `hotspots-core/src/snapshot.rs` - Add `Snapshot::to_jsonl()` method
- `hotspots-core/src/report.rs` - Add `render_jsonl()` function

**Estimated Effort:** 2 hours

---

### 4.2 Percentile Flags

**Requirement:**
Pre-compute top-K percentile flags for each function based on activity risk.

**Specification:**
- Compute activity_risk percentiles: 90th, 95th, 99th
- Add boolean flags: `is_top_10_pct`, `is_top_5_pct`, `is_top_1_pct`
- Compute globally across all functions in snapshot
- Functions can be in multiple buckets (top 1% is also top 5%)

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "activity_risk": 28.4,
  "percentile": {
    "is_top_10_pct": true,
    "is_top_5_pct": true,
    "is_top_1_pct": true
  }
}
```

**Success Criteria:**
- [ ] Percentiles computed correctly (use quantile, not sorting)
- [ ] Edge case: <100 functions (percentiles may be same)
- [ ] Ties handled consistently (all functions at threshold marked true)
- [ ] CLI flag: `--top-1-pct-only` to filter output

**Files to Modify:**
- `hotspots-core/src/snapshot.rs` - Add `PercentileFlags` struct and computation
- `hotspots-cli/src/main.rs` - Add filtering flags

**New Types:**
```rust
pub struct PercentileFlags {
    pub is_top_10_pct: bool,
    pub is_top_5_pct: bool,
    pub is_top_1_pct: bool,
}
```

**Estimated Effort:** 2 hours

---

### 4.3 Repo-Level Summary

**Requirement:**
Add snapshot-wide statistics for concentration analysis.

**Specification:**
- Total functions analyzed
- Total activity_risk (sum across all functions)
- Top 1%/5%/10% share of total risk
- Distribution by risk band (count and sum risk per band)
- Call graph statistics (total edges, avg fan-in, SCC count)

**Output Schema:**
```json
{
  "schema_version": 2,
  "commit": {...},
  "summary": {
    "total_functions": 1523,
    "total_activity_risk": 12847.3,
    "top_1_pct_share": 0.28,
    "top_5_pct_share": 0.54,
    "top_10_pct_share": 0.73,
    "by_band": {
      "critical": {"count": 18, "sum_risk": 842.3},
      "high": {"count": 94, "sum_risk": 1531.2},
      "moderate": {"count": 456, "sum_risk": 3821.4},
      "low": {"count": 955, "sum_risk": 6652.4}
    },
    "call_graph": {
      "total_edges": 4821,
      "avg_fan_in": 3.2,
      "scc_count": 12,
      "largest_scc_size": 23
    }
  },
  "functions": [...]
}
```

**Success Criteria:**
- [ ] Summary statistics accurate (compare manual sum)
- [ ] Concentration shares sum correctly (top 1% ⊆ top 5% ⊆ top 10%)
- [ ] Band counts match function array length
- [ ] Call graph stats correct
- [ ] Summary omittable via `--no-summary` flag (for smaller output)

**Files to Modify:**
- `hotspots-core/src/snapshot.rs` - Add `SnapshotSummary` struct
- `hotspots-core/src/aggregates.rs` - Extend or reuse existing aggregation logic

**Estimated Effort:** 3 hours

---

## Phase 5: CLI Flags & Configuration

### 5.1 Extended Metrics Flag

**Requirement:**
Opt-in flag to include all extended metrics in output.

**Specification:**
- Default: Extended metrics NOT included (backward compatibility)
- `--extended-metrics`: Include LOC, churn, touch_count, recency, graph metrics
- `--extended-metrics` includes call graph analysis
- Performance impact: ~30-40% slower (git operations + call graph)

**Usage:**
```bash
# Standard output (LRS only)
hotspots analyze src/

# Extended output (includes all new metrics)
hotspots analyze src/ --extended-metrics

# Extended + top 10 functions
hotspots analyze src/ --extended-metrics --top 10
```

**Success Criteria:**
- [ ] Default output unchanged (no new fields for existing users)
- [ ] `--extended-metrics` includes all new fields
- [ ] Documentation updated with examples
- [ ] CI tests both modes

**Files to Modify:**
- `hotspots-cli/src/main.rs` - Add CLI argument
- `hotspots-core/src/analysis.rs` - Conditionally compute extended metrics
- `docs/reference/cli.md` - Document flag

**Estimated Effort:** 1 hour

---

### 5.2 Configuration File Support

**Requirement:**
Support `.hotspots.toml` config file for persistent settings.

**Specification:**
- Config file location: `.hotspots.toml` in project root
- Override with `--config path/to/config.toml`
- Supports:
  - Scoring weights
  - Extended metrics on/off
  - Output format preferences
  - Top N default
  - Ignore patterns (files/functions to exclude)

**Example Config:**
```toml
[scoring]
weights.churn = 0.6
weights.touch = 0.4
weights.recency = 0.2
weights.fan_in = 0.5
weights.scc = 0.4
weights.depth = 0.15
weights.neighbor_churn = 0.25

[output]
format = "json"
extended_metrics = true
top_n = 20
explain = true

[ignore]
patterns = ["tests/**", "vendor/**", "*.generated.ts"]
```

**Success Criteria:**
- [ ] Config file parsed correctly
- [ ] CLI flags override config values
- [ ] Documentation includes config examples
- [ ] Error handling for invalid config

**Files to Modify:**
- `hotspots-cli/src/config.rs` - New module for config parsing
- `hotspots-cli/src/main.rs` - Load and merge config with CLI args

**Estimated Effort:** 3 hours

---

## Phase 6: Testing & Documentation

### 6.1 Golden Tests

**Requirement:**
Update all golden tests with extended metrics.

**Tasks:**
- [ ] Regenerate golden files with `--extended-metrics`
- [ ] Add new golden test: `extended_metrics_determinism.json`
- [ ] Add call graph golden tests (known call patterns)
- [ ] Verify LOC values for each language's fixtures
- [ ] Test churn computation with synthetic git history
- [ ] Test fan-in, SCC, depth on test codebase

**Files to Update:**
- `tests/golden/*.json` - Add all new fields
- `tests/golden_tests.rs` - Add extended metrics test cases
- `tests/fixtures/` - Add test cases with known call patterns

**Estimated Effort:** 6 hours

---

### 6.2 Integration Tests

**Requirement:**
Test extended metrics and call graph with real repositories.

**Test Cases:**
- [ ] Baseline commit (no parent): churn is null, touch_count is 0
- [ ] Merge commit: uses parents[0] for delta
- [ ] File rename: churn shows old file deleted, new file added
- [ ] Binary file: churn skipped (not counted)
- [ ] Touch count at repo creation: limited to commit count
- [ ] Touch count 30 days later: includes prior commits
- [ ] Call graph: direct calls, imports, recursive calls
- [ ] Call graph: unreachable functions, cyclic dependencies
- [ ] Fan-in computation accuracy
- [ ] SCC detection on known cyclic code
- [ ] Activity risk scoring produces expected ranking

**New Test File:**
- `tests/extended_metrics_tests.rs`
- `tests/call_graph_tests.rs`

**Estimated Effort:** 8 hours

---

### 6.3 Documentation

**Requirement:**
Document all extended metrics and call graph features.

**Pages to Update:**
- [ ] `docs/reference/metrics.md` - Add LOC, churn, touch, recency, graph metrics
- [ ] `docs/reference/cli.md` - Document all new CLI flags
- [ ] `docs/reference/json-schema.md` - Update schema with all new fields
- [ ] `docs/reference/call-graph.md` - New page explaining call graph analysis
- [ ] `docs/reference/scoring.md` - New page explaining activity risk score
- [ ] `docs/guide/output-formats.md` - Add JSONL examples
- [ ] `docs/guide/configuration.md` - New page for `.hotspots.toml`
- [ ] `README.md` - Update feature list and examples

**New Pages:**
- [ ] `docs/integrations/hotspots-cloud.md` - Guide for cloud ingestion
- [ ] `docs/cookbook/top-n-workflow.md` - Using top N output for sprints

**Estimated Effort:** 6 hours

---

### 6.4 Performance Benchmarks

**Requirement:**
Measure performance impact of extended metrics and call graph.

**Benchmarks:**
- [ ] Baseline: `hotspots analyze` (current performance)
- [ ] With LOC: minimal overhead (<5%)
- [ ] With churn: git operations add time
- [ ] With touch count: 30-day lookback
- [ ] With call graph: AST walking + symbol resolution
- [ ] Full extended: all features enabled

**Acceptance Criteria:**
- Extended metrics + call graph add <40% to total runtime
- JSONL export is <5% overhead vs JSON
- Memory usage stays reasonable (<2GB for large repos)

**Files to Create:**
- `benches/extended_metrics.rs` (using criterion)
- `benches/call_graph.rs`

**Estimated Effort:** 4 hours

---

## Phase 7: Schema Versioning

### 7.1 Schema Version Bump

**Requirement:**
Increment schema version for extended metrics and call graph.

**Changes:**
- Bump `SNAPSHOT_SCHEMA_VERSION` from 1 to 2
- Add migration guide for v1 → v2
- Ensure forward compatibility (v2 reader can handle v1 snapshots)

**Migration:**
- v1 snapshots: missing fields treated as null/default
- v2 snapshots: include all extended fields (if `--extended-metrics`)

**Files to Modify:**
- `hotspots-core/src/snapshot.rs` - Bump version constant
- `docs/architecture/schema-migration.md` - Document migration

**Estimated Effort:** 2 hours

---

## Summary

### Total Estimated Effort: ~65 hours

**Breakdown:**
- Phase 1 (Core Metrics): 13 hours
- Phase 2 (Call Graph): 15 hours
- Phase 3 (Scoring): 7 hours
- Phase 4 (Output): 7 hours
- Phase 5 (CLI/Config): 4 hours
- Phase 6 (Testing): 18 hours
- Phase 7 (Schema): 2 hours

### Deliverables

**Code:**
- LOC computation for all 6 languages
- Git churn, touch count, recency tracking
- Commit metadata capture and event detection
- Call graph extraction (within-repo)
- Fan-in, SCC, dependency depth, neighbor churn
- Activity-weighted risk scoring
- Top N prioritized output with explanations
- JSONL export format
- Percentile flags
- Repo-level summary with call graph stats
- Configuration file support
- `--extended-metrics`, `--top N`, `--explain` flags

**Tests:**
- Updated golden tests with all new metrics
- New integration tests for git metrics
- New call graph tests
- Performance benchmarks

**Documentation:**
- Complete reference docs for all metrics
- Call graph and scoring guides
- Configuration guide
- Migration guide for schema v2
- Cookbook examples

---

## Success Criteria (Overall)

- [ ] All existing tests pass (no regressions)
- [ ] New golden tests pass with extended metrics
- [ ] Integration tests pass on real repos
- [ ] Call graph extraction works for all 6 languages
- [ ] Activity risk scoring correlates with manual assessment
- [ ] Performance impact <40% with all features enabled
- [ ] Documentation complete and reviewed
- [ ] Schema v2 backward-compatible with v1
- [ ] JSONL output validated with DuckDB
- [ ] CI passes on all platforms (Linux, macOS, Windows)
- [ ] `--top N` output is actionable for sprint planning

---

## Revised Out of Scope (Deferred to hotspots-cloud)

The following are explicitly NOT part of CLI work:

- ❌ Cross-repo call graph stitching
- ❌ Time-series trend detection and forecasting
- ❌ Statistical model training (ML, decay rate learning)
- ❌ External API integration (Jira, GitHub issues, PagerDuty)
- ❌ Team/organization-level aggregation
- ❌ Predictive modeling and anomaly detection
- ❌ Multi-snapshot comparative analysis

These will be implemented in `hotspots-cloud`, which will:
- Ingest JSONL snapshots from CLI
- Stitch together cross-repo call graphs
- Correlate with external events (tickets, incidents)
- Perform time-series analysis
- Train ML models on historical data
- Provide team/org dashboards

---

## Open Questions

1. **LOC Definition:** Physical LOC confirmed (includes blanks/comments)

2. **Call Graph Completeness:** Accept best-effort (80%+ direct calls) or require 95%+ accuracy?
   - Recommendation: Ship best-effort first, iterate on accuracy

3. **Scoring Weights:** Use defaults initially or make configurable from day 1?
   - Recommendation: Ship with sensible defaults, add config later (Phase 5)

4. **Top N Default:** Should `--top N` have a default (e.g., 20) or require explicit N?
   - Recommendation: Default to showing all, require explicit `--top N`

5. **Performance Target:** Is 40% slowdown acceptable for opt-in `--extended-metrics`?
   - Recommendation: Yes, measure and optimize hot paths if needed

6. **Schema v2 Timing:** Bump schema version now or after all phases complete?
   - Recommendation: Bump in Phase 1 to establish contract early

---

## Implementation Strategy

### Incremental Shipping Plan

**Release 1: Core Metrics (Phases 1)**
- LOC, churn, touch count, recency, commit metadata
- Event detection, ticket ID extraction
- ~13 hours
- Value: foundation for cloud ingestion, basic activity tracking

**Release 2: Call Graph (Phase 2)**
- Call extraction, fan-in, SCC, depth, neighbor churn
- ~15 hours
- Value: graph-based risk assessment, dependency insights

**Release 3: Unified Scoring (Phase 3)**
- Activity risk score combining all factors
- Top N output with explanations
- ~7 hours
- Value: **A-tier prioritization**, actionable sprint planning

**Release 4: Polish (Phases 4-7)**
- JSONL, percentiles, summary, config, documentation
- ~18 hours
- Value: production-ready, well-documented

Each release provides incremental value and can be tested/validated independently.

---

**Last Updated:** 2026-02-11
**Author:** Stephen Collins
**Reviewers:** [Pending]
**Architecture:** Revised to include call graph analysis in CLI
