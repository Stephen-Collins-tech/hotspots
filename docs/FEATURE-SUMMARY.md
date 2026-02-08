# Hotspots - Complete Feature Summary

**Last Updated:** 2026-01-18  
**Status:** Production-ready with full git-native history tracking

---

## Executive Summary

Hotspots is a **git-native structural analysis tool** for TypeScript that:

1. **Analyzes code complexity** using four metrics (CC, ND, FO, NS) combined into a Local Risk Score (LRS)
2. **Tracks complexity over time** via immutable commit-scoped snapshots
3. **Computes deltas** showing how complexity changes between commits
4. **Manages history** with pruning and compaction capabilities
5. **Generates visualizations** from snapshot data

Hotspots is deterministic, git-native, and designed for CI/CD integration.

---

## Core Functionality

### 1. Point-in-Time Analysis

**Command:**
```bash
hotspots analyze <path> [options]
```

**Features:**
- Analyzes TypeScript files (`.ts`, excludes `.d.ts`)
- Computes four metrics per function:
  - **CC** (Cyclomatic Complexity) - Branching complexity
  - **ND** (Nesting Depth) - Maximum nesting level
  - **FO** (Fan-Out) - Distinct function calls
  - **NS** (Non-Structured Exits) - Early returns, breaks, continues, throws
- Calculates **LRS** (Local Risk Score) - Weighted composite score
- Assigns **Risk Bands** (Low, Moderate, High, Critical)

**Output Formats:**
- **Text** (default): Tabular format sorted by LRS
- **JSON**: Structured data with all metrics and risk components

**Filtering Options:**
- `--top <N>`: Show only top N most complex functions
- `--min-lrs <float>`: Filter by minimum LRS threshold
- `--format text|json`: Output format

**Example:**
```bash
hotspots analyze src/ --top 10 --min-lrs 5.0 --format json
```

---

### 2. Git History Tracking

#### Snapshot Mode

**Command:**
```bash
hotspots analyze . --mode snapshot --format json
```

**What It Does:**
- Extracts git context (SHA, parents, timestamp, branch)
- Analyzes all TypeScript files in the repository
- Creates immutable snapshot with all function metrics
- Persists to `.hotspots/snapshots/<commit_sha>.json`
- Updates `.hotspots/index.json` atomically

**Snapshot Contents:**
- Commit metadata (SHA, parents, timestamp, branch)
- All functions with metrics (CC, ND, FO, NS, LRS, band)
- Function IDs (`<relative_path>::<symbol>`)
- Analysis metadata (tool version, scope)

**Key Properties:**
- **Immutable**: Snapshots never change (filename = commit SHA)
- **Idempotent**: Safe to re-run on same commit
- **Deterministic**: Byte-for-byte identical for identical code

#### Delta Mode

**Command:**
```bash
hotspots analyze . --mode delta --format json
```

**What It Does:**
- Loads current snapshot and parent snapshot (from `parents[0]`)
- Compares functions by `function_id`
- Computes numeric deltas (cc, nd, fo, ns, lrs)
- Identifies band transitions
- Shows function status (new/deleted/modified/unchanged)

**Delta Output:**
- Function status and before/after states
- Numeric deltas for all metrics
- Band transitions (e.g., "moderate" → "high")
- Baseline flag (true if no parent snapshot)

**Use Cases:**
- See what changed in a commit
- Track complexity trends
- Identify functions crossing risk thresholds
- Validate refactoring (negative deltas = improvement)

---

### 3. History Management

#### Prune Unreachable Snapshots

**Command:**
```bash
hotspots prune --unreachable [options]
```

**Options:**
- `--unreachable`: Required flag (explicit confirmation)
- `--older-than <days>`: Only prune commits older than N days
- `--dry-run`: Preview what would be pruned without deleting

**What It Does:**
- Computes reachable commits from `refs/heads/*` (local branches)
- Identifies unreachable snapshots
- Optionally filters by age
- Deletes snapshot files and updates index atomically

**Safety:**
- Never prunes reachable snapshots
- Dry-run mode for preview
- Atomic operations (no partial state)

**Example:**
```bash
# Preview what would be pruned
hotspots prune --unreachable --dry-run

# Prune unreachable snapshots older than 30 days
hotspots prune --unreachable --older-than 30
```

#### Compaction

**Command:**
```bash
hotspots compact --level <0|1|2>
```

**Levels:**
- **Level 0** (default): Full snapshots (fully implemented)
- **Level 1**: Deltas only (metadata placeholder)
- **Level 2**: Band transitions only (metadata placeholder)

**Current Status:**
- Level 0 is fully implemented and production-ready
- Levels 1-2 are metadata placeholders for future implementation

**What It Does:**
- Updates compaction level in `.hotspots/index.json`
- Future: Will rewrite snapshots to use deltas/transitions only

---

### 4. PR vs Mainline Workflows

#### Automatic PR Detection

Hotspots automatically detects PR context via CI environment variables:
- `GITHUB_EVENT_NAME=pull_request`
- `GITHUB_REF=refs/pull/123/head`

**PR Mode Behavior:**
- Compares vs merge-base (common ancestor), not direct parent
- Does **not** persist snapshots (avoids polluting history)
- Computes delta showing PR changes relative to base branch
- Never hard-fails on ambiguous context

**Mainline Mode Behavior:**
- Compares vs direct parent (`parents[0]`)
- Persists snapshots to `.hotspots/snapshots/`
- Updates index atomically
- Builds complete history over time

---

## Data Structures

### Snapshot Format

```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123...",
    "parents": ["def456..."],
    "timestamp": 1705600000,
    "branch": "main"
  },
  "analysis": {
    "scope": "full",
    "tool_version": "0.1.0"
  },
  "functions": [
    {
      "function_id": "src/api.ts::handler",
      "file": "src/api.ts",
      "line": 42,
      "metrics": {
        "cc": 5,
        "nd": 2,
        "fo": 3,
        "ns": 1
      },
      "lrs": 4.8,
      "band": "moderate"
    }
  ]
}
```

### Delta Format

```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123...",
    "parent": "def456..."
  },
  "baseline": false,
  "deltas": [
    {
      "function_id": "src/api.ts::handler",
      "status": "modified",
      "before": {
        "metrics": {"cc": 5, "nd": 2, "fo": 3, "ns": 1},
        "lrs": 4.8,
        "band": "moderate"
      },
      "after": {
        "metrics": {"cc": 7, "nd": 3, "fo": 3, "ns": 1},
        "lrs": 6.2,
        "band": "high"
      },
      "delta": {
        "cc": 2,
        "nd": 1,
        "fo": 0,
        "ns": 0,
        "lrs": 1.4
      },
      "band_transition": {
        "from": "moderate",
        "to": "high"
      }
    }
  ]
}
```

### On-Disk Layout

```
.hotspots/
  snapshots/
    <sha1>.json
    <sha2>.json
    ...
  index.json
```

**Index Format:**
```json
{
  "schema_version": 1,
  "compaction_level": 0,
  "commits": [
    {
      "sha": "abc123...",
      "parents": ["def456..."],
      "timestamp": 1705600000
    }
  ]
}
```

---

## Analysis and Visualization Tools

### Report Generator (`analysis/update-report`)

**Purpose:** Extract data from snapshots for visualization

**Command:**
```bash
cd analysis
cargo run --bin update-report [options]
```

**Options:**
- `--repo <path>`: Repository path (default: `../hotspots-synthetic-hotspot-linear-repo`)
- `--target-function <id>`: Target function ID (auto-detected if not specified)
- `--output-dir <path>`: Output directory (default: `./analysis`)

**What It Does:**
- Reads all snapshots from `.hotspots/snapshots/`
- Extracts timeline data for target function (LRS series)
- Computes deltas between commits
- Extracts repo distribution from latest snapshot
- Updates `analysis/data.json` for Vega visualizations

**Auto-Detection:**
- If `data.json` doesn't exist, auto-detects:
  - Repo name from directory name
  - Target function (highest LRS in latest snapshot)

**Output:**
- `data.json` with `lrs_series`, `deltas_long`, `repo_distribution`
- Used by Vega-Lite charts in `index.html`

---

## Key Features and Invariants

### Determinism

- **Byte-for-byte identical output** for identical input
- Function order: sorted by `function_id` (ASCII lexical)
- File order: sorted by path
- JSON key order: deterministic
- Formatting, comments, whitespace don't affect results

### Git-Native Semantics

- **Correct under all git operations:**
  - Rebase (creates new snapshots)
  - Merge (uses `parents[0]` for delta)
  - Cherry-pick (creates new snapshot)
  - Revert (produces negative deltas)
  - Force-push (snapshots remain immutable)

- **Handles edge cases:**
  - Detached HEAD (`branch = None`)
  - Shallow clones (warns and continues)
  - Multiple parents (uses first parent for delta)
  - Initial commits (baseline = true)

### Immutability

- Snapshots are **never overwritten**
- Filename = commit SHA (authoritative identity)
- Idempotent operations (safe to re-run)
- Atomic writes (temp file + rename pattern)

### Function Identity

- Format: `<relative_file_path>::<symbol>`
- Paths normalized to `/` (forward slashes)
- Stable across commits (file moves = delete + add)
- Not tied to line numbers or physical location

---

## Metrics and Risk Scoring

### Metrics

1. **Cyclomatic Complexity (CC)**
   - Formula: `CC = E - N + 2` (from CFG)
   - Additional increments: short-circuit operators, switch cases, catch clauses
   - Minimum: 1 (empty function)

2. **Nesting Depth (ND)**
   - Maximum depth of nested control structures
   - Counts: if, loops, switch, try
   - Excludes: lexical scopes, object literals

3. **Fan-Out (FO)**
   - Number of distinct functions called
   - Counts chained calls separately (`foo().bar().baz()` = 3)
   - Deduplicates by string representation

4. **Non-Structured Exits (NS)**
   - Early returns, breaks, continues, throws
   - Excludes final return statement
   - Measures control flow disruption

### Risk Transforms

Each metric is transformed to a risk component:

- **R_cc** = `min(log2(CC + 1), 6)` - Logarithmic, capped at 6
- **R_nd** = `min(ND, 8)` - Linear, capped at 8
- **R_fo** = `min(log2(FO + 1), 6)` - Logarithmic, capped at 6
- **R_ns** = `min(NS, 6)` - Linear, capped at 6

### Local Risk Score (LRS)

```
LRS = 1.0 * R_cc + 0.8 * R_nd + 0.6 * R_fo + 0.7 * R_ns
```

**Weights:**
- R_cc: 1.0 (highest - control flow complexity)
- R_nd: 0.8 (high - nesting depth)
- R_ns: 0.7 (medium-high - non-structured exits)
- R_fo: 0.6 (medium - coupling)

**Risk Bands:**
- **Low**: LRS < 3
- **Moderate**: 3 ≤ LRS < 6
- **High**: 6 ≤ LRS < 9
- **Critical**: LRS ≥ 9

**Theoretical Maximum:** LRS = 22.0 (when all components are maxed)

---

## Supported TypeScript Features

### Fully Supported

- Function declarations (`function name() {}`)
- Function expressions (`const f = function() {}`)
- Arrow functions (`const f = () => {}`)
- Class methods (`class C { method() {} }`)
- Object literal methods (`{ method() {} }`)
- All control flow: if/else, loops (for/while/do-while/for-in/for-of), switch, try/catch/finally
- Type annotations (parsed but not analyzed)
- Generics (parsed but not analyzed)

### Explicitly Unsupported

- JSX/TSX syntax (parse error)
- Generator functions (`function*`) (parse error)
- Experimental decorators (not parsed)
- Async/await (parsed but not modeled in CFG)

See [docs/ts-support.md](ts-support.md) for complete details.

---

## CLI Commands Reference

### `hotspots analyze`

**Basic analysis:**
```bash
hotspots analyze <path> [--format text|json] [--top N] [--min-lrs <float>]
```

**Snapshot mode:**
```bash
hotspots analyze . --mode snapshot --format json
```

**Delta mode:**
```bash
hotspots analyze . --mode delta --format json
```

### `hotspots prune`

```bash
hotspots prune --unreachable [--older-than <days>] [--dry-run]
```

### `hotspots compact`

```bash
hotspots compact --level <0|1|2>
```

---

## Testing and Validation

### Test Coverage

- **Unit Tests**: Parser, CFG, metrics, risk calculation
- **Integration Tests**: End-to-end analysis workflows
- **Golden Tests**: Deterministic output verification
- **CI Invariant Tests**: Snapshot immutability, determinism, delta correctness
- **Git History Tests**: Rebase, merge, cherry-pick, revert, force-push

### Test Suites

- `hotspots-core/tests/integration_tests.rs` - Basic analysis
- `hotspots-core/tests/golden_tests.rs` - Output determinism
- `hotspots-core/tests/ci_invariant_tests.rs` - Core invariants
- `hotspots-core/tests/git_history_tests.rs` - Git operations

**Run tests:**
```bash
cargo test --workspace
```

---

## Development Tools

### Dev Script

```bash
./dev analyze src/  # Equivalent to: cargo run -- analyze src/
```

### Install Script

```bash
./install-dev.sh [install-dir]  # Builds and installs to system PATH
```

### Report Generator

```bash
cd analysis
cargo run --bin update-report  # Generates data.json from snapshots
```

---

## Project Structure

```
hotspots/
  hotspots-core/          # Core library
    src/
      analysis.rs         # Main analysis entry point
      ast.rs              # AST utilities
      cfg/                # Control Flow Graph
      delta.rs            # Delta computation
      discover.rs         # Function discovery
      git.rs              # Git context extraction
      metrics.rs          # Metric calculation
      parser.rs           # TypeScript parsing
      prune.rs            # History pruning
      report.rs           # Report generation
      risk.rs             # LRS calculation
      snapshot.rs         # Snapshot persistence
    tests/                # Test suites
  
  hotspots-cli/          # CLI application
    src/main.rs           # Command-line interface
  
  analysis/               # Visualization tools
    update-report.rs      # Report generator
    *.vl.json             # Vega-Lite chart specs
    index.html            # Visualization viewer
    data.json             # Generated data (gitignored)
  
  docs/                   # Documentation
    FEATURE-SUMMARY.md    # This document
    USAGE.md              # Usage guide
    metrics-calculation-and-rationale.md  # Metrics specification
    git-history-integration-summary.md    # History system details
    ...
  
  tests/                  # Test fixtures
    fixtures/             # TypeScript test files
    golden/               # Expected JSON outputs
```

---

## Key Design Decisions

### Why Git-Native?

- Snapshots are tied to commits (immutable, verifiable)
- Deltas use git parent relationships (semantically correct)
- Works correctly under all git operations
- No external database or state management needed

### Why Function-Level Granularity?

- Functions are natural units of complexity
- Enables precise tracking of individual function evolution
- Matches developer mental model
- Sufficient for identifying hotspots

### Why Deterministic?

- Enables regression testing
- CI/CD integration (reproducible results)
- Reliable trend analysis
- No surprises from non-deterministic behavior

### Why Four Metrics?

- CC: Control flow complexity (primary risk)
- ND: Readability (cognitive load)
- FO: Coupling (change risk)
- NS: Structure (control flow disruption)

Each measures a distinct dimension without overlap.

---

## Limitations

### Current Limitations

1. **TypeScript Only**: No support for JavaScript, Python, etc.
2. **Function-Level Only**: No statement-level or module-level metrics
3. **No Semantic Analysis**: Type information not used for analysis
4. **No Size Metric**: LOC not included in LRS
5. **No Context Awareness**: Doesn't account for test coverage, criticality, etc.

### Known Edge Cases

- Break/continue statements route to exit (loop context tracking needed)
- Generator functions cause parse errors
- JSX/TSX syntax rejected

See [docs/limitations.md](limitations.md) for complete details.

---

## Future Enhancements (Not Yet Implemented)

1. **Batch History Processing**: `hotspots history --all` command (see [docs/future-history-command.md](future-history-command.md))
2. **Compaction Levels 1-2**: Delta-only and band-transition-only storage
3. **Multi-Language Support**: Extend to JavaScript, Python, etc.
4. **Module-Level Metrics**: Aggregate LRS at file/module level
5. **Fan-In Metrics**: Track how many functions call a given function

---

## Documentation Index

- **[USAGE.md](USAGE.md)** - How to use Hotspots
- **[metrics-calculation-and-rationale.md](metrics-calculation-and-rationale.md)** - Complete metrics specification
- **[git-history-integration-summary.md](git-history-integration-summary.md)** - History system technical details
- **[capabilities-and-use-cases.md](capabilities-and-use-cases.md)** - Use cases and workflows
- **[architecture.md](architecture.md)** - System architecture
- **[ts-support.md](ts-support.md)** - TypeScript feature support
- **[limitations.md](limitations.md)** - Known limitations
- **[synthetic-harness-research.md](synthetic-harness-research.md)** - Synthetic test harness design

---

## Quick Reference

### Common Workflows

**Daily development:**
```bash
hotspots analyze . --format text                    # Check complexity
hotspots analyze . --mode snapshot --format json    # Create snapshot
# Make changes...
hotspots analyze . --mode delta --format json       # See what changed
```

**CI/CD integration:**
```bash
# Mainline: persist snapshots
hotspots analyze . --mode snapshot --format json

# PR: compare vs merge-base (auto-detected)
hotspots analyze . --mode delta --format json
```

**History management:**
```bash
hotspots prune --unreachable --dry-run              # Preview
hotspots prune --unreachable --older-than 30        # Clean up
```

**Visualization:**
```bash
cd analysis
cargo run --bin update-report                       # Generate data.json
python3 -m http.server 8000                         # View charts
```

---

## Status Summary

✅ **Production-Ready Features:**
- Point-in-time analysis
- Snapshot creation and persistence
- Delta computation
- History pruning
- Compaction level 0 (full snapshots)
- PR/mainline workflow detection
- Deterministic output
- CI/CD integration

🚧 **Future Enhancements:**
- Batch history processing (`history` command)
- Compaction levels 1-2 (delta/transition storage)
- Multi-language support
- Module-level metrics

---

**Hotspots is a complete, production-ready tool for tracking code complexity over time with git-native semantics.**
