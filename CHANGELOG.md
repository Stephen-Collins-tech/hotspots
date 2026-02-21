# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-02-21

### Added

#### Multi-Language Support
- **Go** (`.go`) — full CFG with defer, goroutines, select, type switches
- **Python** (`.py`) — full CFG with comprehensions, context managers, match statements, exception handlers
- **Rust** (`.rs`) — full CFG with match, if-let, `?` operator, loop labels
- **Java** (`.java`) — full CFG with lambdas, try-with-resources, switch expressions, synchronized blocks

#### Call Graph Engine
- Import-based call graph resolution (TypeScript/JavaScript/Go/Python/Rust/Java)
- PageRank, betweenness centrality, and strongly-connected-component (SCC) detection
- Dependency depth computation per function
- Call graph signals fed into activity risk scoring

#### Activity Risk Scoring
- Per-function **Activity Risk** score combining:
  - `complexity` — LRS (base)
  - `churn` — lines changed in the last 30 days
  - `activity` — commit count in the last 30 days
  - `recency` — days since last change (branch-aware)
  - `fan_in` — number of callers detected by call graph
  - `cyclic_dependency` — SCC membership penalty
  - `depth` — dependency depth penalty
  - `neighbor_churn` — lines changed in direct dependencies
- Branch-aware recency: compares against branch divergence point, not just HEAD

#### Driver Labels & Explain Mode
- **`--explain`** flag: human-readable per-function risk breakdown with individual metric
  contributions, activity signals, and a co-change coupling section
- **Driver labels** classify the primary reason a function is flagged: `high_complexity`,
  `deep_nesting`, `high_churn_low_cc`, `high_fanout_churning`, `high_fanin_complex`,
  `cyclic_dep`, or `composite`
- **`driver_detail`** field: near-miss dimension detail for composite drivers (e.g., `cc (P72), nd (P68)`)
- Percentile-relative thresholds for each driver dimension (default P=75)
- **Quadrant classification**: each function tagged `fire`, `debt`, `watch`, or `ok` based on
  band × activity (high/critical + active = `fire`, high/critical + quiet = `debt`, etc.)
- **Action text**: per-function refactoring recommendation derived from driver × quadrant

#### Higher-Level Views
- **`--level file`** — ranked file risk table (max CC, avg CC, function count, LOC, churn)
- **`--level module`** — module instability table (afferent/efferent coupling, instability score)

#### HTML Report
- **Trend charts** in snapshot HTML report: stacked band-count chart, activity-risk line, and
  top-1% share line — all drawn with Canvas 2D from embedded history (up to 30 snapshots)
- **Action column** in triage table: per-function refactoring recommendation (driver × quadrant)
- Hover tooltip on band chart showing date + per-band counts

#### Agent-Optimized JSON (Schema v3)
- `--all-functions` flag: emit all functions (overrides `--min-lrs` / `--top` filters) for
  agent consumption
- Schema v3 (`AgentSnapshotOutput`): triage-first structure with `fire`/`debt`/`watch`/`ok`
  quadrant buckets, each entry carrying `action`, `driver`, `quadrant`, and key metrics

#### Output & CLI
- **JSONL output format** (`--format jsonl`) — one JSON object per line for streaming pipelines
- **`--no-persist`** flag — run snapshot analysis without writing to `.hotspots/`
- **`--per-function-touches`** flag — precise per-function touch counts via `git log -L`;
  results cached on disk (cold run ~50× slower; subsequent warm runs significantly faster)
- **`hotspots config show`** — display resolved configuration (weights, thresholds, filters)
- **`hotspots config validate`** — validate config file without running analysis

#### Performance
- Per-function touch cache: warm runs reuse on-disk cache instead of re-running `git log -L`

## [0.0.1] - 2025-01-25

### Added

#### Core Analysis Features
- **Local Risk Score (LRS)** computation for TypeScript functions
- Four complexity metrics: Cyclomatic Complexity (CC), Nesting Depth (ND), Fan-Out (FO), Non-Structured Exits (NS)
- Risk band classification: Low, Moderate, High, Critical
- Deterministic, byte-for-byte identical output for identical input

#### Analysis Modes
- **Snapshot Mode**: Capture codebase state at a point in time
- **Delta Mode**: Compare two snapshots to detect risk changes
- Git-native integration with automatic snapshot management

#### Policy Engine
- **Critical Introduction**: Block when functions become Critical risk
- **Excessive Risk Regression**: Block when LRS increases by ≥1.0
- **Net Repo Regression**: Warn when total repository risk increases
- CI/CD integration with exit code enforcement

#### Trend Analysis
- **Risk Velocity**: Track rate of LRS change over commit history
- **Hotspot Stability**: Identify stable vs. volatile high-risk functions
- **Refactor Effectiveness**: Detect and classify refactoring outcomes
- Configurable history window (default: 10 snapshots)

#### Aggregation Views
- File-level aggregates: sum LRS, max LRS, High+ count
- Directory-level aggregates with recursive rollup
- Delta aggregates: net LRS delta, regression count per file

#### Visualizations
- Interactive Vega-Lite charts showing risk evolution
- Risk concentration over time (stacked area chart)
- LRS timeline, band timeline, delta breakdown
- Repository distribution histogram
- Example code for generating visualization data

#### CLI Features
- Multiple output formats: text (default) and JSON
- Filtering options: `--top N`, `--min-lrs <threshold>`
- Git-based dynamic versioning
- Comprehensive help and usage documentation

### Technical
- TypeScript parsing via SWC
- Rust 1.75+ required
- MIT License
- Comprehensive test suite (46 unit tests + integration tests)
- Full documentation in `docs/`

### Repository Structure
- `hotspots-core`: Core library for LRS computation
- `hotspots-cli`: Command-line interface
- `examples/`: Example code (visualization export)
- `visualizations/`: Interactive charts and specs
- `docs/`: Comprehensive documentation

[1.0.0]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v1.0.0
[0.0.1]: https://github.com/Stephen-Collins-tech/hotspots/releases/tag/v0.0.1
