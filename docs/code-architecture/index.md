# Codebase Guide

This section documents how the Hotspots codebase is organized for contributors and maintainers. It is intentionally separate from user-facing CLI docs: it explains crates, modules, data flow, invariants, and where to make changes.

For deeper background, design records, and historical reviews, see [Architecture Notes](/architecture/).

## Repository map

| Path | Purpose |
|---|---|
| `hotspots-core/` | Rust library containing analysis, scoring, git enrichment, snapshot/delta logic, policies, and renderers. |
| `hotspots-cli/` | Thin CLI layer: argument parsing, command dispatch, terminal output, and command-specific orchestration. |
| `action/` | JavaScript GitHub Action wrapper around the CLI. The committed runtime is `action/dist/index.js`. |
| `docs/` | VitePress documentation site. |
| `tests/`, `hotspots-core/tests/` | Integration, invariant, golden, and language parity tests. |
| `packages/`, `action/` | TypeScript packages and action packaging surface. |

## Main design rules

Hotspots relies on a few architectural invariants:

- **Deterministic output:** identical source and git inputs should produce byte-for-byte stable output.
- **Per-function static analysis:** CFG and raw complexity metrics are computed per function.
- **Explicit ordering after parallel work:** file analysis may run in parallel, but results are sorted before output.
- **Snapshot immutability:** snapshots are keyed by commit SHA and treated as immutable historical records.
- **Quadrant-aware activity:** `quadrant`, not raw `activity_risk` alone, is the primary fire/debt classification.

## Read next

- [Analysis Pipeline](./pipeline.md)
- [Core Crate Modules](./core-modules.md)
- [CLI and GitHub Action](./cli-and-action.md)
- [Data Model and Persistence](./data-model.md)
- [Contributor Change Guide](./change-guide.md)
