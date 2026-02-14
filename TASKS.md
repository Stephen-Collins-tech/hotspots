# Tasks: Extended Metrics & Call Graph Analysis

**Status:** Phase 1, 2, 3, & 4 Complete ✅ | Phase 5+ Pending
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

### 2.3 Strongly Connected Components (SCC) ✅ COMPLETE

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
  "callgraph": {
    "fan_in": 23,
    "fan_out": 5,
    "pagerank": 0.15,
    "betweenness": 0.08,
    "scc_id": 42,
    "scc_size": 5
  }
}
```

**Success Criteria:**
- [x] Tarjan's algorithm implemented correctly
- [x] All functions assigned to an SCC (size 1 = no cycle)
- [x] Cycles detected accurately (test with known cyclic code)
- [x] Performance acceptable (O(V+E) time)

**Files Modified:**
- `hotspots-core/src/callgraph.rs` - Added `find_strongly_connected_components()` and `tarjan_strongconnect()` methods
- `hotspots-core/src/snapshot.rs` - Added `scc_id` and `scc_size` fields to `CallGraphMetrics`
- Updated `populate_callgraph()` to compute and populate SCC metrics

**Actual Effort:** 1.5 hours (implementation and testing)

---

### 2.4 Dependency Depth ✅ COMPLETE

**Requirement:**
Measure how deep each function is in the dependency tree from entry points.

**Specification:**
- Identify entry points (main, exported functions, HTTP handlers)
- Compute shortest path depth via BFS from all entry points
- Deeper functions are more fragile (longer dependency chain)
- Store as `Option<usize>` (0 = entry point, None = unreachable)

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "callgraph": {
    "fan_in": 23,
    "fan_out": 5,
    "pagerank": 0.15,
    "betweenness": 0.08,
    "scc_id": 42,
    "scc_size": 5,
    "dependency_depth": 5
  }
}
```

**Success Criteria:**
- [x] Entry points identified (heuristic: main, exports, handlers)
- [x] BFS correctly computes shortest paths
- [x] Unreachable functions handled (depth = None)
- [x] Accurate on test codebase

**Files Modified:**
- `hotspots-core/src/callgraph.rs` - Added `compute_dependency_depth()` and `is_entry_point()` methods
- `hotspots-core/src/snapshot.rs` - Added `dependency_depth` field to `CallGraphMetrics`
- Updated `populate_callgraph()` to compute and populate dependency depth

**Actual Effort:** 1.5 hours (implementation and testing)

---

### 2.5 Neighbor Churn ✅ COMPLETE

**Requirement:**
Compute sum of churn for all direct dependencies (functions this function calls).

**Specification:**
- Neighbor churn = sum of `lines_added + lines_deleted` for all callees
- Indicates indirect change risk (dependencies are changing)
- Requires both call graph and churn data
- Store as `Option<usize>` (None if no callees have churn)

**Output Schema:**
```json
{
  "function_id": "src/foo.ts::bar",
  "callgraph": {
    "fan_in": 23,
    "fan_out": 5,
    "pagerank": 0.15,
    "betweenness": 0.08,
    "scc_id": 42,
    "scc_size": 5,
    "dependency_depth": 5,
    "neighbor_churn": 127
  }
}
```

**Success Criteria:**
- [x] Churn summed correctly across all callees
- [x] Functions with no callees or no churn have neighbor_churn = None
- [x] Implementation verified with tests

**Files Modified:**
- `hotspots-core/src/snapshot.rs` - Added `neighbor_churn` field to `CallGraphMetrics`
- Updated `populate_callgraph()` to compute neighbor churn from callees' churn data

**Actual Effort:** 0.5 hours (implementation and testing)

---

## Phase 3: Combined Risk Scoring

### 3.1 Activity-Weighted Risk Score ✅ COMPLETE

**Requirement:**
Combine LRS with activity and graph metrics into unified risk score.

**Specification:**
- Formula: `activity_risk = f(lrs, churn, touch_count, recency, fan_in, scc_size, dependency_depth, neighbor_churn)`
- Default weights:
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
- [x] Score computed for all functions
- [ ] Weights configurable via CLI or config file (deferred - uses defaults for now)
- [x] Score validated with test data
- [x] Implementation includes all specified factors

**Files Modified:**
- `hotspots-core/src/scoring.rs` - Created new module with `compute_activity_risk()` and `ScoringWeights`
- `hotspots-core/src/snapshot.rs` - Added `activity_risk` and `risk_factors` fields, added `compute_activity_risk()` method
- `hotspots-cli/src/main.rs` - Wired up activity risk computation after all metrics are populated
- `hotspots-core/src/trends.rs` - Fixed test code to include new fields
- `hotspots-core/src/aggregates.rs` - Fixed test code to include new fields

**Actual Effort:** 2 hours (implementation and testing)

---

### 3.2 Top N Output Mode ✅ COMPLETE

**Requirement:**
Provide curated, actionable output showing top N highest-risk functions with explanations.

**Specification:**
- CLI flag: `--top N` (default: show all) - sorts by `activity_risk` descending
- CLI flag: `--explain` (requires `--mode snapshot`) - human-readable breakdown
- Sort by `activity_risk` descending (falls back to LRS when no activity data)

**Output Example:**
```
Top 3 Functions by Activity Risk
================================================================================

#1 processOrder [HIGH]
   File: /home/user/hotspots/src/billing.ts:2
   Risk Score: 34.2 (complexity base: 15.2)
   Risk Breakdown:
     • Complexity:        15.20  (cyclomatic=12, nesting=2, fanout=0)
     • Churn:              4.20  (420 lines changed recently)
     • Activity:           3.00  (30 commits in last 30 days)
     • Fan-in:             2.00  (25 functions depend on this)
   Action: URGENT: Reduce complexity - extract sub-functions

#2 validateToken [HIGH]
   File: /home/user/hotspots/src/auth.ts:128
   Risk Score: 31.8 (complexity base: 12.1)
   ...

--------------------------------------------------------------------------------
Showing 3/47 functions  |  Critical: 1  High: 2
```

**Success Criteria:**
- [x] `--top N` correctly filters and sorts by activity_risk
- [x] `--explain` shows clear, actionable reasoning with factor breakdown
- [x] Output is human-readable and scannable
- [x] `--format json` with `--top N` also sorts by activity_risk
- [x] Unreachable functions shown as null depth

**Files Modified:**
- `hotspots-cli/src/main.rs` - Added `--explain` flag, sorting by activity_risk, `print_explain_output()` and `get_recommendation()` functions

**Actual Effort:** 2.5 hours (implementation and testing)

---

## Phase 4: Output Enhancements

### 4.1 JSONL Export Format ✅ COMPLETE

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
- [x] `hotspots analyze --format jsonl` produces valid JSONL
- [x] Each line parseable as standalone JSON
- [x] `jq -s '.'` reconstructs array
- [x] DuckDB can ingest: `COPY tbl FROM 'snapshot.jsonl' (FORMAT JSON)`
- [ ] Benchmark: JSONL should be ~30% smaller than pretty JSON

**Files Modified:**
- `hotspots-cli/src/main.rs` - Added `Jsonl` to `OutputFormat` enum, handled in snapshot mode
- `hotspots-core/src/snapshot.rs` - Added `Snapshot::to_jsonl()` method

**Actual Effort:** 1 hour

---

### 4.2 Percentile Flags ✅ COMPLETE

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
- [x] Percentiles computed correctly (quantile index from sorted scores)
- [x] Edge case: <100 functions (percentiles may be same threshold)
- [x] Ties handled consistently (all functions at threshold marked true)
- [ ] CLI flag: `--top-1-pct-only` to filter output - DEFERRED

**Files Modified:**
- `hotspots-core/src/snapshot.rs` - Added `PercentileFlags` struct, `percentile` field on `FunctionSnapshot`, `compute_percentiles()` method
- `hotspots-cli/src/main.rs` - Wired up `compute_percentiles()` call after activity risk scoring

**Actual Effort:** 1 hour

---

### 4.3 Repo-Level Summary ✅ COMPLETE

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
  "schema_version": 1,
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
- [x] Summary statistics accurate (total risk, band breakdown, call graph stats)
- [x] Concentration shares computed (top 1%/5%/10% of risk)
- [x] Band counts match function array length
- [x] Call graph stats correct (omitted when no call graph data)
- [ ] Summary omittable via `--no-summary` flag - DEFERRED

**Files Modified:**
- `hotspots-core/src/snapshot.rs` - Added `BandStats`, `CallGraphStats`, `SnapshotSummary` structs; `summary` field on `Snapshot`; `compute_summary()` method
- `hotspots-cli/src/main.rs` - Wired up `compute_summary()` call after activity risk scoring

**Actual Effort:** 1.5 hours

---

## Phase 5: CLI Flags & Configuration

### 5.1 Extended Metrics Flag ✅ REMOVED (by design)

**Decision:** All extended metrics (LOC, churn, touch count, recency, call graph, activity risk) are always computed. No opt-in flag.

**Rationale:**
- `build_enriched_snapshot()` already unconditionally computes all metrics with graceful fallbacks
- `--top N` and `--explain` both depend on `activity_risk`, which requires extended metrics — gating them behind a flag would break these features
- New JSON fields are backward-compatible (consumers ignore unknown fields)
- Maintaining two code paths adds complexity without meaningful benefit
- Performance concerns are better addressed by optimizing git calls than adding a mode switch

---

### 5.2 Configuration File Support

**Requirement:**
Support `.hotspots.toml` config file for persistent settings.

**Specification:**
- Config file location: `.hotspots.toml` in project root
- Override with `--config path/to/config.toml`
- Supports:
  - Scoring weights
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
- v2 snapshots: always include all extended fields

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
- `--top N`, `--explain` flags

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
- [ ] Performance acceptable (all metrics always computed)
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

5. **Performance Target:** All metrics always computed — no opt-in flag. Monitor and optimize git calls if needed.

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

## Phase 8: Code Quality Refactoring

*Tasks derived from the independent codebase analysis in ANALYSIS.md. These address structural duplication and maintenance burden without changing observable behavior.*

**Status:** Pending
**Goal:** Reduce duplication, improve maintainability, and make the codebase easier to extend

---

### 8.1 Extract Shared Tree-Sitter Metric Logic

**Priority:** 1 — High impact, unblocks new language support

**Problem:**
`hotspots-core/src/metrics.rs` (~1,450 lines) contains ~600 lines of near-identical code duplicated across Go, Java, and Python metric extractors. Each language reimplements the same six operations using tree-sitter; the only differences are node kind strings.

| Function Pattern | Go | Java | Python |
|---|---|---|---|
| `find_*_function_by_start` | Lines 480–501 | Lines 721–742 | Lines 933–954 |
| `find_*_child_by_kind` | Lines 505–516 | Lines 746–757 | Lines 958–969 |
| `*_nesting_depth` | Lines 519–550 | Lines 760–794 | Lines 972–1003 |
| `*_fan_out` | Lines 553–584 | Lines 797–820 | Lines 1006–1032 |
| `*_non_structured_exits` | Lines 587–627 | Lines 823–842 | Lines 1035–1054 |
| `*_count_cc_extras` | Lines 630–658 | Lines 846–874 | Lines 1058–1096 |

**Specification:**
- Extract a generic `TreeSitterMetrics` struct (or module-level functions) parameterized by a language config of node-kind sets
- Each language provides a `TreeSitterConfig` declaring its node kinds for: if-branches, loops, logical operators, exception handlers, exit calls, function call nodes
- Shared traversal functions handle depth tracking, counting, and tree walking
- Per-language logic reduced to ~50 lines of configuration

**Success Criteria:**
- [ ] ~600 lines of duplicated code replaced by ~150 lines of shared logic + ~50 lines per language config
- [ ] All existing golden tests pass unchanged
- [ ] Adding a new language (e.g., C, Ruby) requires only a config struct, not new traversal logic
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-core/src/metrics.rs`

---

### 8.2 Extract Snapshot Enrichment Pipeline in CLI

**Priority:** 2 — Medium impact, low effort

**Problem:**
`hotspots-cli/src/main.rs` `handle_mode_output` (lines 442–722) copy-pastes ~60 lines of snapshot enrichment logic between snapshot and delta modes. Both modes identically perform: git context extraction, call graph building, snapshot creation, churn population, touch metrics population, call graph population, and activity risk computation. Any new enrichment step must be added in two places.

**Specification:**
- Extract `build_enriched_snapshot(path, repo_root, resolved_config) -> anyhow::Result<Snapshot>` function
- This function handles the full enrichment pipeline in one place
- Both snapshot and delta modes call it, then diverge only for mode-specific logic (persistence vs delta computation)
- Remove `#[allow(clippy::too_many_arguments)]` on `handle_mode_output` (line 441) as a side effect

**Success Criteria:**
- [ ] Enrichment pipeline defined once, called from both modes
- [ ] `#[allow(clippy::too_many_arguments)]` removed from `handle_mode_output`
- [ ] Behavior unchanged; all integration tests pass
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-cli/src/main.rs`

---

### 8.3 Reuse AST-Based Fan-Out for Call Graph Extraction

**Priority:** 3 — Medium impact, eliminates correctness issues

**Problem:**
`hotspots-core/src/lib.rs` (lines 217–336) builds the call graph by re-reading source files with a regex `([a-zA-Z_][a-zA-Z0-9_]*)\s*\(`. This approach has known correctness issues:
- String literals containing `foo(` produce false edges
- Method calls like `obj.method()` are only partially handled
- Function range estimation uses "start of next function" as end boundary (approximate)

The tool already parses every file into a full AST during analysis. The `FanOutVisitor` (ECMAScript), `go_fan_out`, `java_fan_out`, and equivalent functions already identify function calls with full AST context.

**Specification:**
- Wire the existing AST-based fan-out data from `metrics.rs` into the call graph builder in `lib.rs` / `callgraph.rs`
- Eliminate the regex pass and the second file read in `lib.rs:217–336`
- Eliminate false edges from string literals and approximate range estimation

**Success Criteria:**
- [ ] Call graph built from AST fan-out data, not regex
- [ ] No second file read pass during call graph construction
- [ ] Fewer false edges (no string-literal false positives)
- [ ] All existing golden tests pass
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-core/src/lib.rs`
- `hotspots-core/src/callgraph.rs`
- `hotspots-core/src/metrics.rs` (expose fan-out data)

---

### 8.4 Fix Go CC/NS Metric Edge Cases

**Priority:** 4 — Low impact, correctness improvement

**Problem:**
Two correctness issues in Go metric extraction in `metrics.rs`:

1. `go_count_cc_extras` (lines 640–643) checks `op_text.contains("&&")` on the full binary expression text, which can double-count nested logical operators. Should check the operator node directly, not the full text span.

2. `go_non_structured_exits` (lines 592–601) counts every `expression_statement` containing a `call_expression` as a non-structured exit, not just `panic()`. A comment acknowledges this ("Would need source to check if it's panic") but the code increments unconditionally.

**Specification:**
- In `go_count_cc_extras`: check the `binary_expression`'s operator child node kind instead of calling `.contains("&&")` on the full expression text
- In `go_non_structured_exits`: verify the called function name is `panic` (or `log.Fatal`, `os.Exit`) before counting as a non-structured exit

**Success Criteria:**
- [ ] `go_count_cc_extras` does not double-count nested `&&`/`||` operators
- [ ] `go_non_structured_exits` only counts `panic()`, `os.Exit()`, and equivalent exit calls
- [ ] Relevant Go golden tests updated to reflect corrected values
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/metrics.rs`
- `tests/golden/*.json` (Go fixtures may need updating)

---

### 8.5 Add `From<GitContext>` for `CommitInfo`

**Priority:** 5 — Low impact, ergonomic improvement

**Problem:**
`CommitInfo` (`snapshot.rs:30–46`) and `GitContext` (`git.rs:18–29`) have overlapping fields but no `From` conversion. The manual field-by-field copy in `Snapshot::new()` (lines 247–256) is error-prone and will silently miss new fields added to either struct.

**Specification:**
- Implement `From<GitContext> for CommitInfo` (or `From<&GitContext>`)
- Replace the manual field-by-field copy in `Snapshot::new()` with the `From` conversion

**Success Criteria:**
- [ ] `From<GitContext> for CommitInfo` implemented
- [ ] `Snapshot::new()` uses the conversion instead of manual field copy
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-core/src/snapshot.rs`
- `hotspots-core/src/git.rs`

---

### 8.6 Add Python and Go Test File Default Excludes

**Priority:** 6 — Low impact, completeness

**Problem:**
`config.rs` (lines 19–33) default excludes list only includes JS/TS test patterns. Python test files (`test_*.py`, `*_test.py`) and Go test files (`*_test.go`) are not excluded by default, so they are analyzed as production code and can inflate risk scores.

**Specification:**
- Add `"test_*.py"`, `"*_test.py"` to the default excludes list
- Add `"*_test.go"` to the default excludes list
- Verify these patterns use the same glob matching logic as existing JS/TS patterns

**Success Criteria:**
- [ ] Python test files excluded by default
- [ ] Go test files excluded by default
- [ ] Existing JS/TS exclusion behavior unchanged
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/config.rs`

---

### 8.7 Macro for ECMAScript Nesting Depth Visitors

**Priority:** 7 — Low impact, readability improvement

**Problem:**
`hotspots-core/src/metrics.rs` (lines 208–285) has eight `visit_*` methods on `NestingDepthVisitor` that are character-for-character identical except for the type signature:

```rust
fn visit_if_stmt(&mut self, if_stmt: &IfStmt) {
    self.current_depth += 1;
    if self.current_depth > self.max_depth {
        self.max_depth = self.current_depth;
    }
    if_stmt.visit_children_with(self);
    self.current_depth -= 1;
}
```

This pattern repeats for `while`, `do_while`, `for`, `for_in`, `for_of`, `switch`, and `try` — 80 lines of identical boilerplate.

**Specification:**
- Define a declarative macro `impl_nesting_visitor!(visit_if_stmt, IfStmt, if_stmt; ...)` that expands each entry to the above pattern
- Replace the 8 hand-written methods with a single macro invocation

**Success Criteria:**
- [ ] ~80 lines of boilerplate replaced by a macro + ~10-line invocation
- [ ] Generated code is equivalent to the original
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-core/src/metrics.rs`

---

### 8.8 Reduce Policy Evaluation Boilerplate

**Priority:** 8 — Low impact, makes adding policies easier

**Problem:**
`hotspots-core/src/policy.rs` has six policy evaluators that all follow the same pattern:

```
for entry in deltas {
    if entry.suppression_reason.is_some() { continue; }
    if entry.status != <expected_status> { continue; }
    // ... check condition ...
    results.<failed|warnings>.push(PolicyResult { ... });
}
```

Additionally, `compare_policy_results` (lines 120–158) manually encodes enum ordering through 30+ match arms instead of using `#[derive(PartialOrd, Ord)]` or a simple integer mapping.

**Specification:**
- Consider a policy trait or table-driven approach where each policy declares its target statuses and condition predicate, and a single evaluator loop dispatches to them
- Replace the manual `compare_policy_results` match with `#[derive(PartialOrd, Ord)]` on the relevant enum (or an integer priority mapping)

**Success Criteria:**
- [ ] Loop/suppression/status-filter boilerplate defined once
- [ ] `compare_policy_results` simplified
- [ ] Adding a new policy requires only declaring its condition, not copying loop scaffolding
- [ ] `cargo test` passes with zero regressions

**Files to Modify:**
- `hotspots-core/src/policy.rs`

---

### 8.9 Template Engine for HTML Reports

**Priority:** 9 — Low impact now, high effort; defer until reports grow

**Problem:**
`hotspots-core/src/html.rs` (~1,030 lines) builds HTML via `format!()` with large inline string literals containing CSS and JavaScript. Drawbacks:
- No compile-time validation of HTML structure
- CSS and JS are plain string literals with no syntax highlighting or linting in editors
- XSS injection risk if user-controlled data is ever interpolated without escaping (currently mitigated since all data is internal, but fragile)

**Specification:**
- Option A (lower effort): Extract embedded CSS and JS to separate files loaded via `include_str!()` for better editor tooling
- Option B (higher effort): Adopt a compile-time template engine such as `askama` for full type-safe templating

**Recommendation:** Start with Option A (include_str!) as a low-effort improvement. Migrate to Option B only if HTML reports grow significantly in complexity.

**Success Criteria:**
- [ ] CSS extracted to `hotspots-core/src/assets/report.css` and loaded via `include_str!()`
- [ ] JS extracted to `hotspots-core/src/assets/report.js` and loaded via `include_str!()`
- [ ] Report output identical to current output
- [ ] `cargo test` passes

**Files to Modify:**
- `hotspots-core/src/html.rs`
- `hotspots-core/src/assets/report.css` (new)
- `hotspots-core/src/assets/report.js` (new)

---

### Priority Summary

| # | Task | Impact | Effort |
|---|---|---|---|
| 8.1 | Extract shared tree-sitter metric logic | High | Medium |
| 8.2 | Extract snapshot enrichment pipeline in CLI | Medium | Low |
| 8.3 | Reuse AST-based fan-out for call graph | Medium | Medium |
| 8.4 | Fix Go CC/NS metric edge cases | Low | Low |
| 8.5 | Add `From<GitContext>` for `CommitInfo` | Low | Low |
| 8.6 | Add Python/Go test file default excludes | Low | Low |
| 8.7 | Macro for nesting depth visitors | Low | Low |
| 8.8 | Reduce policy evaluation boilerplate | Low | Low |
| 8.9 | Template engine for HTML reports | Low | High |

---

**Last Updated:** 2026-02-14
**Author:** Stephen Collins
**Reviewers:** [Pending]
**Architecture:** Revised to include call graph analysis in CLI
