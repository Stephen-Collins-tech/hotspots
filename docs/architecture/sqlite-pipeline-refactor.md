# SQLite Pipeline Refactor Plan

## Problem: Peak Memory in Snapshot Mode

Running `hotspots analyze . --mode snapshot` on a large monorepo (e.g. expo/expo, ~51k
functions) currently holds multiple large structures in memory simultaneously:

| Phase | Structure | Approx size |
|-------|-----------|-------------|
| After analysis | `Vec<FunctionRiskReport>` | ~23 MB |
| While building call graph | `Vec<FunctionRiskReport>` + `CallGraph` | ~48 MB |
| After `Snapshot::new` | `CallGraph` + `Vec<FunctionSnapshot>` | ~75 MB |
| During enrichment (churn, touch, risk…) | `Vec<FunctionSnapshot>` | ~50 MB |
| JSON output (pre-T4.7) | `Vec<FunctionSnapshot>` + JSON `String` | ~200 MB |

Peak was ~250 MB before T4.7 (streaming JSON output). With T4.7 in place, peak is ~75 MB
(CallGraph + FunctionSnapshot Vec overlap). The goal of the full pipeline refactor is to
reduce this to ~25 MB by never having more than one large structure alive at a time.

---

## How the Current Code Flows

**Entry point**: `hotspots-cli/src/cmd/analyze.rs → handle_mode_output → Snapshot branch`

```
analyze_with_progress(path, ...)
  → Vec<FunctionRiskReport>           ← parallel rayon, all collected before returning

build_call_graph(&reports, repo_root)
  → CallGraph { nodes, edges, ... }   ← built while Vec<FunctionRiskReport> still alive

Snapshot::new(git_context, reports)
  → Vec<FunctionSnapshot>             ← consumes + drops Vec<FunctionRiskReport>
                                        but CallGraph is still alive here

SnapshotEnricher
  .with_churn(...)                    ← mutates Vec in place
  .with_touch_metrics(...)            ← mutates Vec in place
  .with_callgraph(&graph, ...)        ← reads CallGraph, mutates Vec; then graph can drop
  .enrich(...)                        ← activity_risk, percentiles, driver labels, quadrants
  .build()
  → Snapshot { functions: Vec<FunctionSnapshot>, summary, ... }

snapshot.populate_patterns(...)       ← mutates Vec in place

emit_snapshot_output(...)
  → snapshot.write_json_to(stdout)    ← streams from Vec (T4.7 already done)
```

---

## Refactored Pipeline: SQLite as the In-Process Buffer

The refactored pipeline replaces the Vec-based pipeline with a TempDb (in-memory SQLite)
that acts as the single store. Each phase writes results and then the in-memory structure
is dropped. Only one large thing lives in RAM at a time.

```
analyze_with_progress(path, ...)
  → Vec<FunctionRiskReport>

db.insert_reports(sha, &reports)      ← write all rows to SQLite (includes raw callee lists)
drop(reports)                         ← Vec<FunctionRiskReport> freed (~23 MB recovered)

db.build_call_graph(sha, repo_root)
  → CallGraph                         ← loaded from SQLite: only (function_id, callee_names)
                                        much leaner than loading full FunctionRiskReport

  [graph algorithm phase]
  pagerank = graph.pagerank(...)
  betweenness = graph.betweenness_centrality_approx(k)
  scc_info = graph.find_strongly_connected_components()
  depths = graph.compute_dependency_depth()
  fan_in_map = graph.build_fan_in_map()

db.update_callgraph_metrics(sha, &graph, &fan_in_map, &pagerank, ...)
  ← SQL UPDATE: write fan_in, fan_out, pagerank, betweenness, scc, depths back to rows

drop(graph)                           ← CallGraph freed (~25 MB recovered)
drop(pagerank, betweenness, ...)

db.update_churn(sha, &churn_by_file)  ← SQL UPDATE per file path
db.update_touch(sha, &touch_data)     ← SQL UPDATE per file path

db.update_activity_risk(sha, &weights)
  ← streams 1k rows at a time: load (lrs, churn, touch, callgraph cols)
  → call scoring::compute_activity_risk per row
  → batch UPDATE activity_risk, risk_factors

db.update_percentile_flags(sha)
  ← pure SQL: NTILE(100) OVER (ORDER BY activity_risk) in a CTE → UPDATE

db.update_driver_and_quadrant(sha, percentile)
  ← streams rows to compute distribution thresholds (percentile_idx)
  → classify each row via existing Rust logic
  → batch UPDATE driver, driver_detail, quadrant

db.update_patterns(sha, &thresholds)
  ← streams rows: load (cc, nd, fo, ns, loc, fan_in, scc_size, touch, days_since)
  → call patterns::classify per row
  → batch UPDATE patterns

summary = db.compute_summary(sha, betweenness_approximate)
  ← SQL GROUP BY band + window functions for top-k shares

db.write_snapshot_json_to(sha, &commit, &analysis, &summary, &mut stdout)
  ← cursor streams rows, serializes each function as JSON, writes directly
```

**Peak memory with refactored pipeline**: max(Vec\<FunctionRiskReport\>, CallGraph, 1k-row
batch) ≈ **25 MB** — a 10× reduction from the current ~250 MB peak.

---

## "Raw Partial" Graph in SQLite

The raw callee lists (what you called the "raw partial graph") are stored as a `callees TEXT`
column (JSON array of callee name strings, straight from the AST) in the `functions` table:

```sql
-- functions table (abbreviated)
function_id  TEXT  -- e.g. "src/auth/login.ts::validateToken"
file         TEXT  -- absolute path
callees      TEXT  -- JSON: ["checkPermission", "hashPassword", ...]
fan_in       INTEGER  -- written back after graph computation
fan_out      INTEGER
pagerank     REAL
betweenness  REAL
scc_id       INTEGER
...
```

`build_call_graph` reads just `(function_id, file, callees)` from SQLite — no metrics, no
enrichment data. This is the lean graph load. The CallGraph struct holds only:
- `nodes: HashSet<String>` — function IDs (~4 MB for 51k functions)
- `edges: HashMap<String, Vec<String>>` — callee ID lists (~20 MB)

After `update_callgraph_metrics` writes computed values back, the CallGraph is dropped.
The "raw partial" (callee lists in the `callees` column) stays on disk as audit data
but is no longer needed in RAM.

For degree-only metrics (fan_in, fan_out), these could in principle be computed with SQL
`COUNT` queries without loading the graph into memory at all. The iterative algorithms
(PageRank, betweenness, SCC) require the full edge structure in RAM for performance —
doing 30 PageRank iterations via SQL would be ~100× slower than in-memory traversal.

---

## Files Changed

| File | Change |
|------|--------|
| `hotspots-core/src/db/mod.rs` | Add `callees TEXT` to schema; add `insert_reports`, `build_call_graph`, `update_callgraph_metrics`, `update_churn`, `update_touch`, `update_activity_risk`, `update_percentile_flags`, `update_driver_and_quadrant`, `update_patterns`, `compute_summary`, `write_snapshot_json_to` to `TempDb` |
| `hotspots-cli/src/cmd/analyze.rs` | New `build_and_stream_snapshot_via_db` function; wire into snapshot mode |

The delta mode pipeline (`build_enriched_snapshot`) is **unchanged** — it still returns a
`Snapshot` struct because delta computation needs to load two snapshots and diff them.

---

## Enrichment Phases That Use SQL vs Rust Streaming

| Phase | Approach | Why |
|-------|----------|-----|
| Fan-in/fan-out | In-memory from CallGraph (also writable as SQL COUNT) | Graph already loaded |
| PageRank | In-memory (iterative, 30 passes over all edges) | SQL would be 100× slower |
| Betweenness | In-memory (BFS per source node) | Same reason |
| SCC | In-memory (Tarjan DFS) | Requires global stack/visited state |
| Dependency depth | In-memory (BFS level-by-level) | Graph already loaded |
| Churn | SQL UPDATE per file | Single-pass, file → rows mapping |
| Touch metrics | SQL UPDATE per file | Same |
| Activity risk | Rust streaming: 1k rows at a time → batch UPDATE | Pure function per row |
| Percentile flags | SQL NTILE window function | Pure aggregation |
| Driver labels | Rust streaming: load distribution, then label per row | Needs percentile thresholds first |
| Quadrant | Rust streaming: same pass as driver labels | Depends on driver + touch data |
| Patterns | Rust streaming: 1k rows at a time → batch UPDATE | Pure function per row |
| Summary stats | SQL: GROUP BY band, SUM(activity_risk), etc. | Efficient aggregation |
| JSON output | SQL cursor → serde_json per row | Already done (T4.7) |

---

## CPU Utilization Impact

### Where CPU comes from today

| Phase | CPU character | Bounded? |
|-------|--------------|----------|
| Analysis (rayon workers) | Parallel, all cores, dominant consumer | Yes — `--jobs N` flag |
| Touch cache cold start | Sequential `git log -1` per stale file → rapid subprocess fan-out | Yes — batch calls now |
| `build_call_graph` | Single-threaded, O(F) construction | No (fast though) |
| PageRank | Single-threaded, 30 × O(E) iterations | No |
| Betweenness approx | Single-threaded, k × O(V+E) BFS | No |
| SCC / dependency depth | Single-threaded, O(V+E) | No |
| Enrichment (churn/touch/risk/patterns) | Single-threaded, O(F) passes | No (fast) |
| JSON serialization (old path) | Single-threaded, O(F), large allocator pressure | Eliminated by T4.7 |

### What the SQLite pipeline changes for CPU

**Adds overhead:**

- **INSERT on write**: Each `FunctionRiskReport` row requires serializing the `callees`
  field to a JSON string (Vec\<String\> → text). At 51k functions with an average of ~5
  callees each, this is ~255k string serializations — cheap but not free.
- **SQL UPDATE passes**: Each enrichment phase issues a batch of UPDATE statements rather
  than mutating a Vec index. A Vec write is a pointer store; a SQLite UPDATE is a B-tree
  key lookup + page dirtying. Rough overhead: 2–5× per-row cost vs direct mutation.
- **Cursor deserialization**: Streaming reads deserialize each row back from SQLite types
  into Rust values. Similar overhead to the INSERT path.
- **Multiple passes**: The pipeline makes ~8 passes over the data (graph write, churn,
  touch, activity\_risk, percentiles, driver labels, patterns, output) rather than the
  current ~6 in-memory passes. Two extra passes for the graph write/read cycle.

**Removes or reduces overhead:**

- **Allocator pressure**: The current pipeline allocates a 50 MB `Vec<FunctionSnapshot>`
  and then a 150 MB JSON `String` in quick succession, forcing the allocator to find and
  manage large contiguous regions. SQLite's page cache avoids both of these large Rust heap
  allocations. Fewer large allocations = less allocator CPU and fewer OS page faults.
- **Cache locality per batch**: Processing 1 000 rows at a time from a cursor fits in L3
  cache. Processing 51k functions at once does not. Each enrichment pass has better
  spatial locality in the batched model.
- **Eliminated JSON string allocation (T4.7)**: The `serde_json::to_string_pretty` call
  that built a 150–200 MB string for the entire snapshot is already gone. That was the
  single largest CPU + allocator event in the output phase.
- **No swap / OOM-killer overhead**: On the Docker benchmark (512 MB limit), the old
  pipeline was running close to the limit. When the OS is under memory pressure it
  spends CPU on page reclaim. Reducing peak memory from ~250 MB to ~25 MB eliminates
  that hidden CPU tax.

### Net CPU effect

For the graph algorithms (PageRank, betweenness, SCC) there is **no change** — same code,
same complexity. These are the dominant single-threaded CPU consumers after the analysis
phase and the SQLite refactor does not touch them.

For everything else the overhead of SQLite operations (B-tree, serialization) is offset by
the reduction in allocator pressure and improved cache behavior. On a warm L3 cache with
25 MB working set vs a cold 250 MB working set, the later enrichment phases run faster.

Observed benchmark behaviour is expected to show:
- Analysis phase CPU profile: unchanged (still rayon workers up to `--jobs` limit)
- Post-analysis CPU: flatter, shorter spikes (no large-allocation events)
- Total wall-clock time: roughly neutral to 10–20% slower on small repos (SQLite overhead
  dominates), roughly neutral to faster on large repos (cache + allocator pressure win)

The SQLite refactor is primarily a **memory** optimization. CPU is a secondary benefit
for very large repos where memory pressure was causing OS-level overhead.

---

## What Is NOT Changing

- The `CallGraph` struct and all graph algorithm implementations — no changes needed
- The `scoring::compute_activity_risk` function — called per-row from the streaming loop
- The `patterns::classify` function — called per-row from the streaming loop
- The `Snapshot` struct and all its serialization — still used for delta mode and persistence
- The `.json.zst` snapshot file format — backward-compatible persistence unchanged
- The `SnapshotEnricher` — still used for delta mode

---

## Persistence (non-benchmark case)

When `--no-persist` is NOT passed, the snapshot must also be written to
`.hotspots/snapshots/<sha>.json.zst`. With the DB pipeline, this means loading a full
`Snapshot` from TempDb after enrichment — a one-time cost only paid when persisting.
Alternatively, the `SnapshotDb` (`.hotspots/snapshots.db`) can be used for persistence,
avoiding the round-trip through the `Snapshot` struct entirely.

For now: the DB pipeline handles `--no-persist` (the benchmark case). Persistence can
be converted to use `SnapshotDb` in a follow-on.
