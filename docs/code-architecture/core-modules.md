# Core Crate Modules

`hotspots-core` is the main library crate. The CLI and GitHub Action exist primarily to drive this crate and present its output.

## Public entry points

`hotspots-core/src/lib.rs` exports the public analysis entry points:

- `analyze(path, options)`
- `analyze_with_config(path, options, resolved_config)`
- `analyze_with_progress(path, options, resolved_config, progress)`

It also re-exports commonly used types such as `FunctionRiskReport`, `ResolvedConfig`, `GitContext`, `CallGraph`, and `TouchMode`.

## Module groups

### Source analysis

| Module | Responsibility |
|---|---|
| `analysis.rs` | Per-file orchestration: parse, discover functions, compute reports. |
| `language/` | Language abstraction layer and parser/CFG implementations. |
| `parser.rs` | ECMAScript parser compatibility layer and parser tests. |
| `discover.rs` | Function discovery helpers and legacy ECMAScript discovery. |
| `cfg.rs`, `cfg/builder.rs` | Control-flow graph representation and ECMAScript CFG construction. |
| `metrics.rs` | Raw metric extraction: cyclomatic complexity, nesting, fan-out, exits. |
| `suppression.rs` | `hotspots-ignore` suppression comment handling. |

### Risk and prioritization

| Module | Responsibility |
|---|---|
| `risk.rs` | LRS weights, thresholds, bands, and base risk calculations. |
| `scoring.rs` | Activity-weighted scoring and composite risk logic. |
| `patterns.rs` | Pattern classification such as god function, churn magnet, hub function. |
| `aggregates.rs` | File and directory aggregates. |
| `compact.rs` | Compact/default output data shaping. |

### Git, snapshots, and deltas

| Module | Responsibility |
|---|---|
| `git.rs` | Git context, refs, churn extraction, commit metadata. |
| `snapshot.rs` | Snapshot construction, enrichment, persistence model. |
| `delta.rs` | Snapshot/function comparison. |
| `policy.rs` | Policy evaluation over deltas. |
| `touch_cache.rs` | Cache for expensive function touch calculations. |
| `db/` | SQLite-backed storage helpers. |
| `trends.rs` | Historical trend analysis. |
| `prune.rs` | Snapshot/cache pruning. |

### Graph and model enrichment

| Module | Responsibility |
|---|---|
| `callgraph.rs` | Call graph, fan-in/fan-out, PageRank, betweenness approximations. |
| `imports.rs` | Import extraction and module relationship helpers. |
| `models.rs` | Model/entity declaration extraction and association. |

### Rendering and output

| Module | Responsibility |
|---|---|
| `report.rs` | Text/JSON rendering for function reports. |
| `html.rs` | HTML report generation. |
| `sarif.rs` | SARIF output for code scanning integrations. |

## Language abstraction

The language layer normalizes TypeScript, JavaScript, Go, Java, Python, Rust, C#, and Vue-flavored inputs into common concepts:

- `Language`
- `LanguageParser`
- `ParsedModule`
- `FunctionNode`
- `FunctionBody`
- `CfgBuilder`
- `SourceSpan`

Most new language work should begin in `hotspots-core/src/language/` and then add golden tests under `hotspots-core/tests/fixtures/` and `hotspots-core/tests/golden_tests.rs`.

## Where not to put logic

Avoid putting core analysis behavior in `hotspots-cli`. CLI code should parse arguments, load config, call `hotspots-core`, and render command-specific output. If behavior needs tests independent of terminal invocation, it probably belongs in `hotspots-core`.
