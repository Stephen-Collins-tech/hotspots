# Coordinate Subcommand — Status

_Last updated: 2026-06-14 (WAL design notes added)_

## What shipped (PR #97, branch `feat/coordinate-subcommand`)

- `hotspots coordinate --files a.rs,b.rs` — pre-flight coordination signal tool
- Outputs: within-set co-change pairs, hidden dependencies (files outside the set that co-change with input files), ownership signals (90-day author concentration), parallel/serialize partition recommendation
- JSON auto-selected when stdout is not a TTY; `--json` / `--text` force explicitly
- Hidden deps filtered to coupling_ratio ≥ 0.7 and capped at 10 in JSON (`hidden_dependencies_omitted` count for overflow) — high-signal only, keeps token footprint small
- 13 integration tests covering partition logic, hidden dep detection, real git history, edge cases
- `docs/guide/claude-code-integration.md` — CLAUDE.md snippet for drop-in Claude Code adoption

## Why it matters

Current agent orchestration frameworks decompose tasks by capability. Nobody uses **codebase topology as a parallelization constraint**. Hotspots already has the signals (co-change, ownership, defect history) — `coordinate` exposes them to an orchestration layer. See `resummarize_crdt.txt` for the full framing.

## Drop-in adoption path (current)

Users paste the snippet from `docs/guide/claude-code-integration.md` into their repo's `CLAUDE.md`. Claude Code reads it and calls `hotspots coordinate` before splitting work. No MCP server needed — hotspots is already a CLI tool people install.

## Next steps (not started, rough priority order)

### 1. Function-level coordinate
`hotspots coordinate --files auth.rs --level function` — partition at function granularity, not file. Two agents could safely work on the same file if their function scopes don't co-change. Requires per-function co-change mining (precedent: `--per-function-touches` already exists).

**Function identity**: use fully-qualified name (`module::function`) as the primary key — already what hotspots mines. Rename tracking is a future concern; FQN breaks on rename but renames are rare compared to edits.

### 2. Local WAL for in-flight claims
`.hotspots/coordinate.wal` — append-only log of agent claims/releases on function/file scopes. Orchestrator writes claims before spawning agents, reads WAL on mid-task discovery to re-partition. Entries are hotspots primitives only (function IDs, file paths, coupling ratios).

Format sketch:
```json
{"ts": 1718123456, "agent": "agent-1", "op": "claim", "scope": "auth.rs::validate_token"}
{"ts": 1718123489, "agent": "agent-1", "op": "release", "scope": "auth.rs::validate_token"}
```

`hotspots coordinate --check-wal` factors live claims into the partition recommendation.

**Expiry**: v1 uses explicit release only + `--reset-wal` escape hatch. Lease TTL (`expires_at`) can be added once we see what failure modes actually appear in practice.

**Read model**: the partition is derived state, computed at read time — never stored. On `--check-wal`, read all entries → filter released/expired → build `scope → agent` map → subtract claimed scopes from the hotspots partition recommendation. The store is dumb append + read; conflict resolution happens in the reader.

**`WalStore` abstraction** (the key design decision): the store exposes two operations only:
```rust
trait WalStore {
    fn append(&mut self, entry: WalEntry) -> Result<()>;
    fn entries(&self) -> Result<Vec<WalEntry>>;
}
```
`LocalWalStore` writes line-delimited JSON to `.hotspots/coordinate.wal` with `O_APPEND`. A remote backend is a drop-in swap — the partition logic doesn't change.

### 3. Remote WAL / multi-machine
Same protocol, pluggable backend via `WalStore`. WAL entries need to be mergeable without a central lock — grow-only set per function ID is the minimal CRDT that fits. Relevant when agents run on separate machines.

**Concurrent claim race**: two agents can both see a scope as unclaimed and both claim it. Options: compare-and-swap at the store layer (Redis SET NX), or optimistic retry in the coordinator (re-read WAL after claiming, back off on conflict). Decision deferred until we have a real workload to test against.

### 4. Partition explainability
A `reason` field (or `--explain` flag) on the partition output — which coupling signal drove two files into the same partition. Makes the output debuggable and builds trust. Low effort once the WAL read model is in place.

### 5. MCP server
Expose `coordinate` as an MCP tool so Claude Code picks it up from `.mcp.json` without any CLAUDE.md configuration. Better drop-in story for wider adoption. Only worth the effort when there are users asking for it.

## Deferred / out of scope for now

- Encryption on WAL entries (add after compression/transport layer is settled)
- General-purpose message bus (coordinate is scoped to hotspots primitives only)
- Live broadcast / push mode (re-check is pull-based; push requires agent-to-agent protocol hotspots doesn't own)
