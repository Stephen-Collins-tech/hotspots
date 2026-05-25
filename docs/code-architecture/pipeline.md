# Analysis Pipeline

Hotspots is a pipeline-oriented analyzer. Most commands eventually turn source files into function reports, optionally enrich those reports with git and graph data, then render or persist results.

## High-level flow

```text
CLI command
  ↓
configuration discovery and resolution
  ↓
source file discovery and filtering
  ↓
per-file static analysis
  ↓
function reports
  ↓
snapshot/delta/policy/report command behavior
```

## Static analysis flow

The central library entry points are in `hotspots-core/src/lib.rs`:

- `analyze`
- `analyze_with_config`
- `analyze_with_progress`

The core per-file flow is:

```text
source file
  ↓
language detection
  ↓
parse source into language-specific AST
  ↓
discover functions
  ↓
build per-function CFG
  ↓
extract raw metrics
  ↓
score LRS and patterns
  ↓
FunctionRiskReport[]
```

Important implementation locations:

| Stage | Main files |
|---|---|
| Config resolution | `hotspots-core/src/config.rs` |
| Source collection | `hotspots-core/src/lib.rs` |
| File analysis | `hotspots-core/src/analysis.rs` |
| Language detection/parsing | `hotspots-core/src/language/`, `hotspots-core/src/parser.rs` |
| Function discovery | `hotspots-core/src/discover.rs`, language parser modules |
| CFG construction | `hotspots-core/src/cfg.rs`, `hotspots-core/src/cfg/builder.rs`, `hotspots-core/src/language/*/cfg_builder.rs` |
| Metrics | `hotspots-core/src/metrics.rs` |
| LRS/risk bands | `hotspots-core/src/risk.rs`, `hotspots-core/src/scoring.rs` |
| Reports | `hotspots-core/src/report.rs` |

## Snapshot and delta flow

Snapshot mode enriches static function reports with commit-aware data:

```text
FunctionRiskReport[]
  ↓
git context and commit metadata
  ↓
touch/churn metrics
  ↓
call graph metrics
  ↓
quadrant classification
  ↓
snapshot persisted by commit SHA
```

Delta mode compares snapshots or analyzes against a base commit:

```text
base snapshot + head snapshot
  ↓
function matching by stable identifiers
  ↓
new / modified / deleted / unchanged classification
  ↓
policy evaluation
  ↓
text/json/html/sarif output
```

Relevant files:

- `hotspots-core/src/snapshot.rs`
- `hotspots-core/src/delta.rs`
- `hotspots-core/src/policy.rs`
- `hotspots-core/src/git.rs`
- `hotspots-core/src/touch_cache.rs`

## Determinism boundaries

File analysis uses Rayon in `analyze_with_progress`, so worker completion order is nondeterministic. The implementation restores determinism by sorting intermediate results by file index before producing final reports.

When adding parallelism, always add an explicit deterministic sort before exposing results, persisting snapshots, or comparing deltas.
