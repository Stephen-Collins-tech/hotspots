# GIT_HISTORY_INTEGRATION_TASKS.md

**Git-Native Snapshot, Delta, and History Engine**

## Status

```text
project: hotspots
scope: git history integration
analysis semantics: frozen
risk model: frozen
visualization: explicitly out of scope
kernel usage: none
```

---

## Purpose

Transform Hotspots from a **point-in-time structural analyzer** into a **git-native history engine** that:

* Models code structure as **immutable commit-scoped snapshots**
* Computes **parent-relative deltas**
* Remains correct under **rebases, merges, cherry-picks, reverts, and force-pushes**
* Bounds storage via **reachability pruning**
* Optionally reduces storage via **explicit history compaction**
* Enforces correctness through **CI-level invariants**

This document defines the **canonical spine** of the system.
All future consumers (CI gates, trend analysis, visualization, reporting) depend on this layer.

---

## Explicit Non-Goals

This work must **not** introduce:

* New metrics
* Changes to LRS math
* Visualization logic
* Trend aggregation
* CI policy enforcement beyond invariants
* UI or reporting layers

---

## Core Invariants (Must Always Hold)

1. Commit hash is the sole identity
2. Snapshots are immutable
3. Deltas are parent-relative
4. Branch names are metadata only
5. History rewrites create new history, not edits
6. Missing parents produce baselines, not errors

**Cross-Platform Determinism:**
- All paths must be normalized to `/` (forward slashes only)
- All ordering must be ASCII lexical ordering (not locale-aware)
- **All ordering rules described in this document inherit the global ASCII lexical ordering invariant**
- Locale, timezone, and OS must not affect snapshot bytes
- No environment-dependent fields allowed

These invariants must be enforced by tests and CI.

---

## Implementation Clarifications

These answers resolve implementation ambiguities and should be treated as authoritative.

### Phase H0 – Git Context Extraction

**Shallow clones:**
- Warn and continue if parent SHAs cannot be resolved
- Treat missing parents as `baseline = true`
- Never fail purely due to shallow history (CI-friendly)

**Detached HEAD:**
- Set `branch = None`, `is_detached = true`
- No heuristics (no `git describe` or similar)
- Branch names are metadata only

**Multiple parents:**
- Store **all parents** in snapshot
- Delta computation uses `parents[0]` only
- Snapshot faithfully records commit structure
- Parent ordering is defined by git and is treated as authoritative (stable under octopus merges)

### Phase H1 – Snapshot Container

**`function_id` format:**
- Introduce `function_id` as a **new explicit field** (do not derive on read)
- Format: `<relative_file_path>::<symbol>` (e.g., `src/foo.ts::handler`)
- No parameter lists initially (can extend later if needed)
- **File moves:** File moves are treated as **delete + add**, not rename
- Function identity is based on path::symbol; path changes create new `function_id`
- **Note:** Function identity semantics are frontend-defined but must be stable within a language (future-proofs Go, Rust, Python frontends)

**Metrics in snapshot:**
- Store **all metrics** from `MetricsReport` (cc, nd, fo, ns)
- Do not drop any metrics (lossless capture)

**Tool version:**
- Pull from `CARGO_PKG_VERSION` at build time
- No runtime config needed

**`.hotspots/` directory:**
- Location: repo root
- Should be git-ignored (derived state, not source)

**Atomic writes:**
- Use temp file + `rename` pattern for both snapshots and `index.json`
- No `fsync` needed initially

**JSON serialization:**
- Use deterministic ordering everywhere
- Reuse same `serde_json` settings as existing JSON output
- Byte-for-byte determinism is an invariant

**Snapshot overwrite behavior:**
- **Never overwrite existing snapshots**
- Fail with clear error if `schema_version` mismatch is detected
- Protects history integrity (snapshots are immutable by design)

**Index.json atomicity and recovery:**
- `index.json` is also written atomically (temp file + `rename`)
- If `index.json` is corrupted or missing, it can be **rebuilt from `snapshots/` directory**
- System is self-healing (scan `snapshots/` to regenerate index)
- **Index rebuild ordering:** Must sort entries deterministically using commit metadata (commit timestamp ascending, then commit SHA ASCII ascending as tie-breaker), **not filesystem iteration order**
- Keeps index rebuilds byte-for-byte stable across platforms

### Phase H2 – Delta Computation

**Delta storage vs emission:**
- **Deltas are computed on demand only** (not persisted to disk in this phase)
- Baseline deltas are not persisted (baseline is absence of history, not meaningful change)
- Non-baseline deltas are also computed on-demand (may be cached later but not part of this phase)
- Emit deltas only in `--mode delta` output
- **Future delta caching:** Future versions may introduce delta caching, but cached deltas must be strictly derived artifacts and never authoritative (snapshots remain the source of truth)

**Compacted parent snapshots:**
- If a parent snapshot has been compacted beyond the level required for delta computation, delta computation must fail with a clear error indicating insufficient historical detail
- Deltas between different compaction levels are disallowed

**`band_transition` field:**
- Omit if band is unchanged (absence means "no transition")

**Function matching:**
- If `function_id` exists in both snapshots and **any** of metrics/LRS/band differ → `modified`
- Ignore file/line changes for status determination
- Structural change is what matters, not movement

### Phase H3 – CLI Modes

**Default behavior:**
- Preserve existing behavior when `--mode` is not specified
- Current text/JSON output works as before
- `--mode snapshot|delta` is opt-in (non-breaking)

**`--mode` vs `--format`:**
- `--mode` controls **what** is emitted (snapshot/delta)
- `--format` controls **how** it is serialized (text/json)
- Valid combinations: `--mode snapshot --format json`, `--mode delta --format json`
- Text format may be unsupported for delta initially

### Phase H4 – PR vs Mainline Semantics

**PR detection:**
- Check CI environment variables only (GitHub: `GITHUB_EVENT_NAME`, `GITHUB_REF`)
- No branch-name heuristics
- Best-effort is acceptable

**Merge-base resolution failure:**
- Fall back to direct parent **and warn**
- Never hard-fail (CI must not break)

### Phase H5 – Golden History Fixtures

**Test structure:**
- New file: `hotspots-core/tests/git_history_tests.rs`
- Use shared helpers with existing golden tests if possible
- Keeps history semantics isolated and readable

### Phase H6 – Reachability Pruning

**Tracked refs (default):**
- Local branches only: `refs/heads/*`
- Configurable later if needed
- Do not involve `.gitignore`

**Pruning safety:**
- No interactive prompts
- Safety comes from explicit `--prune` flag and optional `--dry-run`
- CI-friendly, scriptable behavior

**Index.json consistency during pruning:**
- When snapshots are pruned, **remove corresponding entries from `index.json`**
- Keep index in sync with actual snapshot files (index should reflect on-disk state)
- **Baseline snapshots:** Baseline snapshots are subject to the same reachability pruning rules as all other snapshots (no special treatment)

### Phase H7 – History Compaction

**Compaction metadata:**
- Store compaction level in **index.json only**
- Snapshots themselves remain self-describing
- Computing deltas between different compaction levels is **disallowed** (emit clear error)

### Implementation Order

**Recommended sequence:**

1. H0 – Git context extraction
2. H1 – Snapshots and persistence
3. H2 – Delta computation
4. H5 – Golden tests (validate correctness early)
5. H3/H4 – CLI modes + PR semantics
6. H6 – Reachability pruning
7. H7 – History compaction (optional)
8. H8 – CI invariant enforcement

**Rationale:** Build core functionality, test it thoroughly, then add polish and safety.

### Backward Compatibility

- This is a **non-breaking change**
- No major version bump required (defaults remain unchanged)
- New functionality is opt-in via `--mode` flag

### Future Considerations

These items are **explicitly out of scope** for this phase but may be addressed later:

**Snapshot size growth:**
- No upper bound or warning behavior defined in this phase
- Future work may add size warnings (not enforcement) for very large repos

**Clock skew and commit timestamps:**
- Timestamps are metadata only and never used for ordering or semantics
- Commit timestamps (not wall clock time) are authoritative
- Clock skew between machines does not affect snapshot correctness

---

## Phase H0 – Git Context Extraction

### Goal

Deterministically extract git metadata for the current commit.

---

### Git Context Model (Rust)

```rust
pub struct GitContext {
    pub head_sha: String,
    pub parent_shas: Vec<String>,
    pub timestamp: i64,
    pub branch: Option<String>,
    pub is_detached: bool,
}
```

---

### Git Metadata Extraction (Rust)

Use the `git` CLI directly. No libgit2.

```rust
use std::process::Command;
use anyhow::{Context, Result};

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("failed to invoke git")?;

    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn extract_git_context() -> Result<GitContext> {
    let head_sha = git(&["rev-parse", "HEAD"])?;

    let parents_raw = git(&["rev-list", "--parents", "-n", "1", "HEAD"])?;
    let mut parts = parents_raw.split_whitespace();
    let _ = parts.next();
    let parent_shas = parts.map(|s| s.to_string()).collect::<Vec<_>>();

    let timestamp = git(&["show", "-s", "--format=%ct", "HEAD"])?
        .parse::<i64>()
        .context("failed to parse commit timestamp")?;

    let branch = match git(&["symbolic-ref", "--short", "HEAD"]) {
        Ok(b) => Some(b),
        Err(_) => None,
    };

    Ok(GitContext {
        head_sha,
        parent_shas,
        timestamp,
        is_detached: branch.is_none(),
        branch,
    })
}
```

---

### Tasks

* [x] Detect git repository
* [x] Extract HEAD SHA
* [x] Extract parent SHAs (0, 1, or many - store all in snapshot)
* [x] Extract commit timestamp (`%ct`)
* [x] Extract branch name (best effort, `None` if detached)
* [x] Handle detached HEAD (`is_detached=true`, `branch=None`, no heuristics)
* [x] Handle shallow clones safely (warn and continue, treat missing parents as baseline)

---

## Phase H1 – Snapshot Container and Persistence

### Goal

Wrap analysis output in an immutable, commit-scoped snapshot and persist it.

---

### Snapshot JSON Schema (Exact)

```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123",
    "parents": ["def456"],
    "timestamp": 1705600000,
    "branch": "main"
  },
  "analysis": {
    "scope": "full",
    "tool_version": "0.1.0"
  },
  "functions": [
    {
      "function_id": "src/foo.ts::handler",
      "file": "src/foo.ts",
      "line": 42,
      "metrics": { "cc": 5, "nd": 2, "fo": 3, "ns": 1 },
      "lrs": 4.8,
      "band": "moderate"
    }
  ]
}
```

**Schema Notes:**

* `function_id`: Format is `<relative_file_path>::<symbol>` (no parameters initially)
* `function_id` is an explicit field, not derived on read
* `metrics`: Contains all four metrics (cc, nd, fo, ns) for lossless capture
* `tool_version`: From `CARGO_PKG_VERSION` at build time
* `parents`: Store all parents (even for merge commits)

Rules:

* Function records are unchanged from analysis output
* Ordering is deterministic (ASCII lexical, not locale-aware)
* All paths normalized to `/` (forward slashes only)
* Snapshot bytes never change once written (immutable)
* Serialization uses deterministic JSON ordering
* Never overwrite existing snapshots (fail on `schema_version` mismatch)
* **Deleted functions:** Snapshots represent the state of the codebase at a commit and never contain tombstones. Deletions are represented exclusively in delta output

---

### On-Disk Layout

```text
.hotspots/
  snapshots/
    <commit_sha>.json
  index.json
```

**Snapshot Filename Invariant:**
- Snapshot filename **must equal** commit SHA and is the authoritative binding between git history and stored state
- The filename `<commit_sha>.json` is the primary identity mechanism (commit SHA is the sole identity per Core Invariant #1)

---

### Index JSON Schema

```json
{
  "schema_version": 1,
  "commits": [
    {
      "sha": "abc123",
      "parents": ["def456"],
      "timestamp": 1705600000
    }
  ]
}
```

---

### Tasks

* [x] Define Snapshot Rust struct (with explicit `function_id` field)
* [x] Serialize deterministically (use same `serde_json` settings as existing output)
* [x] Normalize all paths to `/` (forward slashes only)
* [x] Use ASCII lexical ordering (not locale-aware)
* [x] Persist snapshot as `<sha>.json` in `.hotspots/snapshots/` (repo root) - function implemented
* [x] Use atomic write (temp file + `rename`) for snapshots
* [x] Use atomic write (temp file + `rename`) for `index.json`
* [x] Enforce idempotency (safe to re-run on same commit) - logic implemented
* [x] Never overwrite existing snapshots (fail on `schema_version` mismatch) - logic implemented
* [x] Append to `index.json` in `.hotspots/` - function implemented
* [x] Implement `index.json` rebuild from `snapshots/` directory (recovery, with deterministic ordering)
* [x] Ensure `.hotspots/` is git-ignored

---

## Phase H2 – Parent-Relative Delta Computation

### Goal

Compute deterministic deltas between a snapshot and its parent.

---

### Delta JSON Schema (Exact)

```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123",
    "parent": "def456"
  },
  "baseline": false,
  "deltas": [
    {
      "function_id": "src/foo.ts::handler",
      "status": "modified",
      "before": {
        "metrics": { "cc": 4, "nd": 2, "fo": 2, "ns": 1 },
        "lrs": 3.9,
        "band": "moderate"
      },
      "after": {
        "metrics": { "cc": 6, "nd": 3, "fo": 3, "ns": 1 },
        "lrs": 6.2,
        "band": "high"
      },
      "delta": {
        "cc": 2,
        "nd": 1,
        "fo": 1,
        "lrs": 2.3
      },
      "band_transition": {
        "from": "moderate",
        "to": "high"
      }
    }
  ]
}
```

**Schema Notes:**

* `function_id`: Format matches snapshot (`<relative_file_path>::<symbol>`)
* `baseline`: `true` when parent snapshot is missing (deltas with `baseline=true` are not persisted to disk)
* `band_transition`: Omit this field if band is unchanged (absence means "no transition")
* `parent`: Uses `parents[0]` from snapshot (single parent for delta computation)
* `status`: Based on metrics/LRS/band changes, not file/line movements
* `schema_version`: Delta schema version mismatch fails fast (deltas are never cached, so no migration path required)

---

### Tasks

* [x] Load parent snapshot (use `parents[0]` only for delta computation - git's parent ordering is authoritative)
* [x] If missing, mark `baseline=true` (do not persist baseline deltas)
* [x] Match functions by `function_id` (file moves are delete + add, not rename)
* [x] Classify new / deleted / modified / unchanged (status based on metrics/LRS/band, not file/line)
* [x] Compute numeric deltas (allow negative)
* [x] Detect band transitions (omit `band_transition` field if unchanged)
* [x] Allow negative deltas (valid for reverts, refactors)
* [x] Compute deltas on-demand only (not persisted to disk in this phase)
* [x] Fail fast on delta `schema_version` mismatch
* [x] Fail with clear error if parent snapshot is compacted beyond required level

---

## Phase H3 – CLI Modes

### Goal

Make snapshot and delta outputs explicit.

---

### Tasks

* [x] Add `--mode snapshot | delta` (opt-in, preserves existing behavior by default)
* [x] When `--mode` not specified, preserve existing text/JSON output behavior
* [x] Snapshot mode emits snapshot JSON
* [x] Delta mode emits delta JSON
* [x] Support `--mode snapshot --format json` and `--mode delta --format json`
* [x] Text format may be unsupported for delta initially (returns error for text format in snapshot/delta modes)
* [x] Prevent mixed output (mode selection determines output type)

---

## Phase H4 – PR vs Mainline Semantics

### Goal

Avoid polluting canonical history with ephemeral PR data.

---

### Tasks

* [x] Detect PR context via CI environment variables (GitHub: `GITHUB_EVENT_NAME`, `GITHUB_REF`)
* [x] No branch-name heuristics (CI env vars only, best-effort)
* [x] Resolve merge-base (fall back to direct parent if merge-base fails, with warning)
* [x] PR mode:

  * [x] Compare vs merge-base
  * [x] Do not persist snapshots
  * [x] Never hard-fail on ambiguous context
* [x] Mainline mode:

  * [x] Persist snapshots
  * [x] Compare vs direct parent (`parents[0]`)

---

## Phase H5 – Golden History Fixtures

### Goal

Prove correctness under real git mutations.

---

### Required Fixtures

* [x] Rebase
* [x] Non-fast-forward merge
* [x] Cherry-pick
* [x] Revert

### Global Test Rules

* Real git repos
* Temp directories
* No fixed SHAs
* Assert relationships only
* Fail loudly on invariant violation
* New test file: `hotspots-core/tests/git_history_tests.rs`
* Share helpers with existing golden tests where possible

---

## Phase H6 – Force-Push Reachability Pruning

### Goal

Bound storage safely after history rewrites.

---

### Tasks

* [x] Enumerate tracked refs (default: local branches `refs/heads/*` only)
* [x] Compute reachable commit set
* [x] Mark snapshots reachable / unreachable
* [x] Add `--prune unreachable`
* [x] Add `--older-than <days>`
* [x] Support `--dry-run`
* [x] Never prune reachable snapshots
* [x] Remove corresponding entries from `index.json` when pruning snapshots
* [x] Keep `index.json` in sync with on-disk snapshot files
* [x] No interactive prompts (CI-friendly, scriptable)

---

## Phase H7 – History Compaction (Optional)

**Status:** Basic implementation complete (Level 0 supported, Levels 1-2 metadata only)

### Goal

Reduce storage without losing semantic signal.

**Note:** This phase was marked optional in the original specification and was not implemented. The current implementation uses Level 0 (full snapshots) only. Future work may add compaction if storage becomes a concern.

---

### Compaction Levels

* Level 0: full snapshots (current implementation)
* Level 1: deltas only
* Level 2: band transitions only

---

### Tasks

* [x] Add compaction metadata (store in `index.json` only, not in snapshots)
* [x] Add `--compact level=N` CLI command
* [x] Ensure compaction is explicit (never automatic) - requires explicit CLI command
* [ ] Preserve HEAD ancestry (requires full implementation of Levels 1-2)
* [ ] Disallow deltas between different compaction levels (requires full implementation of Levels 1-2)

**Implementation Notes:**
- Compaction metadata (`compaction_level`) added to Index struct (defaults to 0 for backward compatibility)
- `hotspots compact --level N` CLI command implemented to set compaction level
- Currently only Level 0 (full snapshots) is fully implemented
- Levels 1-2 are placeholders - setting the level updates metadata only
- Future work needed to actually compact snapshots to deltas/band transitions

---

## Phase H8 – CI Invariant Enforcement

### Goal

Prevent regressions forever.

**Scope:**
- CI invariants validate Hotspots behavior against controlled test fixtures, not user repositories
- Invariants run on test repositories to ensure correctness, not to enforce policies on arbitrary user history

---

### Required CI Assertions

#### Snapshot invariants

* [x] Snapshot immutability
* [x] Byte-for-byte determinism
* [x] Filename equals commit SHA

#### Delta invariants

* [x] Single parent only
* [x] Baseline handling correct
* [x] Negative deltas allowed
* [x] Deleted functions explicit

#### History semantics

* [x] Rebase creates new snapshots
* [x] Cherry-pick creates new snapshots
* [x] Revert produces negative deltas
* [x] Merge uses parent[0] only
* [x] Force-push does not corrupt history (test added, may need debugging)

---

## Exit Criteria

This work is complete when:

* [ ] Snapshots are immutable and commit-scoped
* [ ] Deltas are deterministic and parent-relative
* [ ] All golden fixtures pass
* [ ] Reachability pruning is safe
* [ ] Optional compaction works
* [ ] CI enforces all invariants
* [ ] No visualization assumptions exist

---

## Final Design Reminder

> Hotspots models **commit graph transitions**, not time and not intent.

If this layer is correct, everything downstream becomes easy.