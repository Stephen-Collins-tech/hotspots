# Data Model and Persistence

Hotspots has three important layers of data: per-function static reports, enriched snapshots, and deltas/policy results.

## Function reports

`FunctionRiskReport` is the primary static analysis output. It includes source location, raw metrics, risk components, LRS, risk band, patterns, and related metadata.

Produced by:

- `hotspots-core/src/analysis.rs`
- `hotspots-core/src/report.rs`

Used by:

- CLI rendering
- snapshot construction
- HTML/JSON/SARIF output
- tests and golden fixtures

## Snapshots

Snapshots represent a commit-level view of function risk. They are built from current function reports and enriched with git, activity, and graph context.

Key concepts:

- A snapshot belongs to a commit SHA.
- Snapshots are immutable once written.
- Snapshot output should be deterministic.
- Activity fields must be interpreted with quadrant context.

Important fields include:

| Field | Meaning |
|---|---|
| `lrs` | Static local risk score. |
| `activity_risk` | Composite activity-weighted score; decays but does not become zero. |
| `quadrant` | Authoritative classification: `fire`, `debt`, `simple-active`, or `simple-stable`. |
| `touches_30d` | Recent commit touches for true recent activity. |
| `churn` fields | Change volume over git history/windows. |
| Graph metrics | Fan-in, PageRank, SCC/cycle information, dependency signals. |

Use `quadrant` and `touches_30d` together when describing urgency. A `debt` function with `touches_30d == 0` is stable structural debt, not actively changing code.

## Deltas

Deltas compare a base snapshot to a head snapshot.

Function states include:

- new
- modified
- deleted
- unchanged

Policies consume deltas to decide whether a PR should fail, warn, or pass.

Relevant files:

- `hotspots-core/src/delta.rs`
- `hotspots-core/src/policy.rs`
- `hotspots-cli/src/cmd/diff.rs`

## Persistence

Persistence is split between snapshot files and SQLite-backed helpers.

| Component | Files |
|---|---|
| Snapshot construction/enrichment | `hotspots-core/src/snapshot.rs` |
| SQLite helpers | `hotspots-core/src/db/` |
| Git context/churn | `hotspots-core/src/git.rs` |
| Touch cache | `hotspots-core/src/touch_cache.rs` |
| Pruning | `hotspots-core/src/prune.rs` |

## Deterministic persistence rules

When changing persistence code:

- Sort collections before serialization.
- Avoid hash-map iteration order in output.
- Write snapshots atomically where possible.
- Preserve backwards compatibility or add explicit schema/version handling.
- Do not mutate existing commit snapshots in place.

## Output formats

The same data can be rendered as:

- human text
- compact text
- JSON
- JSONL
- HTML
- SARIF

Renderers should not recompute analysis. They should format already-computed reports, snapshots, deltas, or policy results.
