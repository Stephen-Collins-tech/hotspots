# Hotspots Capabilities and Use Cases

## Overview

Hotspots is a **git-native structural analysis tool** that tracks code complexity over time as immutable commit-scoped snapshots and computes parent-relative deltas. It provides both point-in-time analysis and historical tracking of code structure.

---

## Core Capabilities

### 1. Point-in-Time Analysis (Original Functionality)

Analyze TypeScript code structure and compute complexity metrics:

```bash
# Analyze a file or directory
hotspots analyze src/

# Output as JSON
hotspots analyze src/ --format json

# Filter by LRS threshold
hotspots analyze src/ --min-lrs 5.0

# Show top N most complex functions
hotspots analyze src/ --top 10
```

**Metrics Computed:**
- **CC** (Cyclomatic Complexity) - Branching complexity
- **ND** (Nesting Depth) - Maximum nesting level
- **FO** (Fan-Out) - Function calls made
- **NS** (Nested Scopes) - Number of nested scopes
- **LRS** (Logarithmic Risk Score) - Combined risk score
- **Risk Band** - Classification (low, moderate, high, critical)

---

### 2. Git History Tracking (New)

Track code structure changes across git commits:

#### Create Snapshots

```bash
# Analyze and create snapshot for current commit
hotspots analyze . --mode snapshot --format json

# Snapshot is automatically persisted to .hotspots/snapshots/<commit_sha>.json
# Index is updated in .hotspots/index.json
```

**What Snapshots Capture:**
- Full snapshot of all functions at a commit
- Commit metadata (SHA, parents, timestamp, branch)
- All metrics (cc, nd, fo, ns) for every function
- Function IDs (`<file_path>::<symbol>`) for stable tracking
- Analysis metadata (tool version, scope)

#### Compute Deltas

```bash
# Compare current state vs parent commit
hotspots analyze . --mode delta --format json

# Shows what changed since last commit:
# - New functions
# - Deleted functions
# - Modified functions (with metric deltas)
# - Band transitions (e.g., moderate → high)
```

**Delta Output Includes:**
- Status for each function (new/deleted/modified/unchanged)
- Before/after states (metrics, LRS, band)
- Numeric deltas (e.g., CC: +2, LRS: +1.5)
- Band transitions (e.g., "moderate" → "high")

---

### 3. PR vs Mainline Workflows

#### PR Mode (Automatic)

In CI environments with PR context (GitHub Actions), Hotspots automatically:

```bash
# In PR CI - automatically detected
export GITHUB_EVENT_NAME=pull_request
export GITHUB_REF=refs/pull/123/head

# Compares vs merge-base (common ancestor), doesn't persist
hotspots analyze . --mode delta --format json
```

**PR Behavior:**
- ✅ Computes delta vs merge-base (not direct parent)
- ✅ Does **not** persist snapshots (avoids polluting history)
- ✅ Shows changes in the PR relative to base branch
- ✅ CI-friendly (never hard-fails on ambiguous context)

#### Mainline Mode (Default)

In regular git repositories:

```bash
# Creates snapshot and compares vs direct parent
hotspots analyze . --mode snapshot --format json
hotspots analyze . --mode delta --format json
```

**Mainline Behavior:**
- ✅ Persists snapshots to `.hotspots/snapshots/`
- ✅ Compares vs direct parent (`parents[0]`)
- ✅ Updates index atomically
- ✅ Builds complete history over time

---

### 4. History Management

#### Prune Unreachable Snapshots

After force-pushes or branch deletions, clean up orphaned snapshots:

```bash
# Dry-run: see what would be pruned
hotspots prune --unreachable --dry-run

# Prune unreachable snapshots older than 30 days
hotspots prune --unreachable --older-than 30

# Prune all unreachable snapshots
hotspots prune --unreachable
```

**Pruning Behavior:**
- Only prunes snapshots unreachable from `refs/heads/*` (local branches)
- Never prunes reachable snapshots (safety guarantee)
- Updates `index.json` to stay in sync
- Optional age filter (only prune old unreachable snapshots)

#### Set Compaction Level

Configure storage strategy (currently Level 0 - full snapshots):

```bash
# Set compaction level (0 = full snapshots, 1 = deltas only, 2 = band transitions only)
hotspots compact --level 0
```

**Note:** Currently only Level 0 is fully implemented. Levels 1-2 are placeholders for future work.

---

## Use Cases

### 1. CI/CD Integration

**Monitor Complexity Changes in PRs:**

```yaml
# .github/workflows/complexity-check.yml
name: Complexity Check
on: [pull_request]
jobs:
  check-complexity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Check complexity changes
        run: |
          hotspots analyze . --mode delta --format json > delta.json
          # Parse delta.json and fail if critical functions degraded
```

**Gate Merges Based on Complexity:**

- Block PRs that increase complexity beyond thresholds
- Flag functions crossing risk bands (e.g., moderate → high)
- Track complexity trends over time

---

### 2. Historical Analysis

**Track Complexity Trends:**

```bash
# Run on every commit (e.g., in pre-commit hook or CI)
hotspots analyze . --mode snapshot --format json

# Analyze historical patterns:
# - When did complexity spike?
# - Which commits introduced high-risk functions?
# - How does complexity evolve over time?
```

**Answer Questions Like:**
- "Did this refactor actually reduce complexity?"
- "When did this function become high-risk?"
- "What's the complexity trend for this module?"
- "Which commits introduced the most complexity?"

---

### 3. Code Review Assistance

**Understand Impact of Changes:**

```bash
# In PR review - see exactly what changed
hotspots analyze . --mode delta --format json
```

**Delta Output Shows:**
- Functions with increased complexity (`delta.cc > 0`)
- Functions with decreased complexity (`delta.cc < 0`)
- New high-risk functions
- Functions that crossed risk bands

**Reviewers Can:**
- Focus on functions with complexity increases
- Verify refactoring claims (check for negative deltas)
- Identify functions entering high-risk territory

---

### 4. Refactoring Validation

**Verify Refactoring Success:**

```bash
# Before refactoring
hotspots analyze . --mode snapshot --format json > before.json

# After refactoring
hotspots analyze . --mode snapshot --format json > after.json

# Compare deltas - successful refactoring should show negative deltas
hotspots analyze . --mode delta --format json
```

**Success Indicators:**
- Negative CC deltas (reduced branching)
- Negative LRS deltas (overall complexity reduced)
- Functions moving to lower risk bands

---

### 5. Regression Detection

**Detect Complexity Regressions:**

```bash
# Track complexity over time
# Every commit creates a snapshot
# Deltas show exactly what changed

# Alert on:
# - Sudden complexity spikes
# - Functions crossing critical thresholds
# - Multiple functions degrading simultaneously
```

---

## Example Workflows

### Daily Development Workflow

```bash
# 1. Analyze current state
hotspots analyze . --format text

# 2. Create snapshot before major changes
hotspots analyze . --mode snapshot --format json

# 3. Make changes, then see delta
hotspots analyze . --mode delta --format json

# 4. Commit changes
git commit -m "Refactor complexity"
hotspots analyze . --mode snapshot --format json  # Capture new state
```

### CI Pipeline Integration

```yaml
# .github/workflows/main.yml
on:
  push:
    branches: [main]
  pull_request:

jobs:
  complexity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      # PR mode: compare vs merge-base
      - name: Analyze complexity delta
        run: hotspots analyze . --mode delta --format json > complexity-delta.json
      
      # Mainline mode: persist snapshot (on main branch only)
      - name: Persist snapshot
        if: github.ref == 'refs/heads/main'
        run: hotspots analyze . --mode snapshot --format json
      
      # Optional: Upload delta for review
      - name: Upload complexity report
        uses: actions/upload-artifact@v2
        with:
          name: complexity-delta
          path: complexity-delta.json
```

### Maintenance Workflow

```bash
# Weekly/monthly: Clean up unreachable snapshots
hotspots prune --unreachable --older-than 90 --dry-run  # Preview
hotspots prune --unreachable --older-than 90            # Execute

# Review complexity trends
# Use index.json and snapshots to build historical reports
```

---

## Data Formats

### Snapshot JSON

Complete state of codebase at a commit:

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

### Delta JSON

Changes from parent commit:

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
      "before": { "metrics": {...}, "lrs": 3.9, "band": "moderate" },
      "after": { "metrics": {...}, "lrs": 6.2, "band": "high" },
      "delta": { "cc": 2, "nd": 1, "fo": 1, "lrs": 2.3 },
      "band_transition": { "from": "moderate", "to": "high" }
    }
  ]
}
```

---

## Key Features

### ✅ Immutability

- Snapshots are never overwritten (immutable)
- Filename = commit SHA (authoritative identity)
- Safe to run multiple times (idempotent)

### ✅ Git-Native

- Correct under all git operations (rebase, merge, cherry-pick, revert, force-push)
- Respects git history structure
- Works with shallow clones

### ✅ Deterministic

- Byte-for-byte identical output for identical input
- Cross-platform compatible (path normalization, ASCII ordering)
- No timestamps or randomness in snapshots

### ✅ Storage Efficient

- Pruning removes unreachable snapshots
- Optional compaction (future Levels 1-2)
- Self-healing index (can rebuild from snapshots)

### ✅ CI-Friendly

- No interactive prompts
- Graceful error handling
- PR mode doesn't pollute history
- Never hard-fails on ambiguous context

---

## Integration Examples

### With Issue Tracking

```bash
# Create snapshot for commit
hotspots analyze . --mode snapshot --format json

# Parse snapshot, identify high-risk functions
# Open issues for functions in "critical" band
# Link issues to commit SHAs
```

### With Code Review Tools

```bash
# Generate delta in PR
hotspots analyze . --mode delta --format json > review-data.json

# Post as PR comment showing:
# - Complexity changes
# - Functions that crossed risk bands
# - Functions with largest deltas
```

### With Monitoring/Alerting

```bash
# Run in CI, parse delta.json
# Alert on:
# - Functions entering "critical" band
# - Large complexity increases (>50% LRS increase)
# - Multiple functions degrading in same commit
```

---

## What You Can Build On Top

The snapshot/delta format provides a foundation for:

1. **Visualization Tools** - Build dashboards showing complexity trends
2. **Trend Analysis** - Identify patterns, regressions, improvements
3. **Policy Enforcement** - Gate merges based on complexity thresholds
4. **Historical Reports** - Generate reports showing evolution over time
5. **Automated Refactoring Suggestions** - Identify high-risk functions for refactoring
6. **Team Metrics** - Track complexity contributions per team/developer

---

## Limitations

- **TypeScript only** - Currently supports TypeScript (`.ts` files)
- **Function-level granularity** - Tracks functions, not individual statements
- **No visualization** - Provides data only, no built-in charts/graphs
- **No trend aggregation** - Snapshots/deltas are raw data (aggregation is external)
- **No policy enforcement** - Provides data, doesn't block commits (CI integration needed)

---

## Getting Started

1. **Install Hotspots** (if not already installed)
2. **Run analysis** on your repository:
   ```bash
   hotspots analyze . --mode snapshot --format json
   ```
3. **Integrate into CI** for continuous tracking
4. **Review deltas** in PRs to understand complexity changes
5. **Build reports** using snapshot/delta JSON data

---

**The system is production-ready and provides a solid foundation for tracking code complexity over time with git-native semantics.**
