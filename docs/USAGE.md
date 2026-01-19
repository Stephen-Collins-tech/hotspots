# How to Use Faultline

## Installation

### Build from Source

```bash
git clone <repo-url>
cd faultline
cargo build --release
```

The binary will be at `target/release/faultline`.

### Install to System Path (Dev Version)

```bash
./install-dev.sh
```

This builds and installs `faultline` to `~/.local/bin` (or a custom directory).

---

## Basic Usage

### Point-in-Time Analysis

**Analyze a file or directory:**

```bash
# Text output (default)
faultline analyze src/

# JSON output
faultline analyze src/ --format json

# Analyze specific file
faultline analyze src/api.ts
```

**Filter results:**

```bash
# Show only top 10 most complex functions
faultline analyze src/ --top 10

# Show only functions with LRS >= 5.0
faultline analyze src/ --min-lrs 5.0

# Combine filters
faultline analyze src/ --top 10 --min-lrs 5.0 --format json
```

**Example output (text):**
```
LRS     File              Line  Function
11.2    src/api.ts        88    handleRequest
9.8     src/db/migrate.ts 41    runMigration
7.5     src/utils.ts      15    processData
```

---

## Git History Tracking

### Prerequisites

Faultline must be run from within a git repository for snapshot/delta modes.

### Creating Snapshots

**Create a snapshot for the current commit:**

```bash
# In a git repository
cd my-repo
faultline analyze . --mode snapshot --format json
```

This will:
- Analyze all TypeScript files in the repository
- Create a snapshot with commit metadata (SHA, parents, timestamp, branch)
- Persist to `.faultline/snapshots/<commit_sha>.json`
- Update `.faultline/index.json`

**What gets stored:**
- All functions with their metrics (CC, ND, FO, NS, LRS, band)
- Commit information (SHA, parents, timestamp, branch)
- Function IDs (`<file_path>::<symbol>`)

### Computing Deltas

**Compare current state vs parent commit:**

```bash
faultline analyze . --mode delta --format json
```

This will:
- Load the parent snapshot (from `parents[0]`)
- Compare functions by `function_id`
- Show what changed: new, deleted, modified, unchanged
- Display metric deltas and band transitions

**Delta output shows:**
- Function status (new/deleted/modified/unchanged)
- Before/after metrics and LRS
- Numeric deltas (cc, nd, fo, ns, lrs)
- Band transitions (e.g., "moderate" → "high")

**Example delta output:**
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
      "function_id": "src/api.ts::handleRequest",
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

---

## Common Workflows

### Daily Development

```bash
# 1. Check current complexity
faultline analyze . --format text

# 2. Before making changes, create snapshot
faultline analyze . --mode snapshot --format json

# 3. Make your changes...

# 4. See what changed
faultline analyze . --mode delta --format json

# 5. Commit changes
git commit -m "Add feature"

# 6. Create snapshot for new commit
faultline analyze . --mode snapshot --format json
```

### CI/CD Integration

**Mainline branch (persist snapshots):**

```yaml
# .github/workflows/complexity.yml
- name: Track complexity
  run: |
    faultline analyze . --mode snapshot --format json
```

**PR branch (compare vs merge-base, don't persist):**

```yaml
# Automatically detected in PR context
- name: Check complexity changes
  run: |
    faultline analyze . --mode delta --format json > delta.json
    # Parse delta.json and fail if critical functions degraded
```

Faultline automatically detects PR context via `GITHUB_EVENT_NAME` and `GITHUB_REF` environment variables.

### Refactoring Validation

```bash
# Before refactoring
faultline analyze . --mode snapshot --format json > before.json

# Make refactoring changes...

# After refactoring
faultline analyze . --mode snapshot --format json > after.json

# See the improvement
faultline analyze . --mode delta --format json
```

Look for:
- Negative deltas (CC, ND, LRS decreased)
- Band transitions to lower risk (e.g., "high" → "moderate")
- Overall LRS reduction

---

## History Management

### Prune Unreachable Snapshots

After force-pushes or branch deletions, clean up orphaned snapshots:

```bash
# Dry-run: see what would be pruned
faultline prune --unreachable --dry-run

# Prune unreachable snapshots older than 30 days
faultline prune --unreachable --older-than 30

# Prune all unreachable snapshots
faultline prune --unreachable
```

**Safety:** Only prunes snapshots unreachable from `refs/heads/*` (local branches). Never prunes reachable snapshots.

### Set Compaction Level

```bash
# Set compaction level (currently only Level 0 is implemented)
faultline compact --level 0
```

**Note:** Levels 1-2 are metadata placeholders for future implementation.

---

## Output Formats

### Text Format (Default)

```
LRS     File              Line  Function
11.2    src/api.ts        88    handleRequest
9.8     src/db/migrate.ts 41    runMigration
```

### JSON Format

```bash
faultline analyze src/ --format json
```

Outputs structured JSON with all metrics, risk components, LRS, and band.

---

## Risk Bands

Functions are classified into risk bands based on LRS:

| Band      | Range      | Interpretation                          |
|-----------|------------|-----------------------------------------|
| Low       | LRS < 3    | Simple, maintainable functions          |
| Moderate  | 3 ≤ LRS < 6| Moderate complexity, review recommended |
| High      | 6 ≤ LRS < 9| High complexity, refactor recommended   |
| Critical  | LRS ≥ 9    | Very high complexity, urgent refactor   |

---

## Examples

### Find Most Complex Functions

```bash
faultline analyze src/ --top 5 --format text
```

### Find Functions Needing Refactoring

```bash
faultline analyze src/ --min-lrs 9.0 --format json
```

### Track Complexity Over Time

```bash
# On every commit (e.g., in pre-commit hook or CI)
faultline analyze . --mode snapshot --format json
```

Then use deltas to see trends:
```bash
faultline analyze . --mode delta --format json
```

### Compare Two Commits

```bash
# Checkout first commit
git checkout <sha1>
faultline analyze . --mode snapshot --format json > commit1.json

# Checkout second commit
git checkout <sha2>
faultline analyze . --mode snapshot --format json > commit2.json

# Compare manually or use delta mode
faultline analyze . --mode delta --format json
```

---

## Troubleshooting

### "Path does not exist"

Make sure you're pointing to a valid file or directory:
```bash
faultline analyze ./src  # Correct
faultline analyze src    # Also correct (relative path)
```

### "failed to extract git context"

Snapshot/delta modes require a git repository:
```bash
# Make sure you're in a git repo
cd my-git-repo
faultline analyze . --mode snapshot
```

### "snapshot already exists"

Snapshots are immutable. If you get this error, the snapshot already exists for this commit. This is safe to ignore if you're re-running the same command.

### No output in delta mode

If delta shows no changes:
- Check that parent snapshot exists (should be in `.faultline/snapshots/`)
- Verify you're comparing against the correct parent (uses `parents[0]`)
- First commit will show `baseline: true` with all functions marked `new`

---

## Development Mode

For development, use the `dev` script:

```bash
# Run without building
./dev analyze src/

# Equivalent to: cargo run -- analyze src/
```

---

## See Also

- [Capabilities and Use Cases](capabilities-and-use-cases.md) - Detailed feature overview
- [Metrics Calculation and Rationale](metrics-calculation-and-rationale.md) - How metrics are computed
- [Git History Integration Summary](git-history-integration-summary.md) - Technical details
- [LRS Specification](lrs-spec.md) - Local Risk Score details
