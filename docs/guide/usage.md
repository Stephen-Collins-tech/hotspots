# Usage & Workflows

## Basic Usage

### Point-in-Time Analysis

**Analyze a file or directory:**

```bash
# Text output (default)
hotspots analyze src/

# JSON output
hotspots analyze src/ --format json

# Analyze specific file
hotspots analyze src/api.ts
```

**Filter results:**

```bash
# Show only top 10 most complex functions
hotspots analyze src/ --top 10

# Show only functions with LRS >= 5.0
hotspots analyze src/ --min-lrs 5.0

# Combine filters
hotspots analyze src/ --top 10 --min-lrs 5.0 --format json
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

Hotspots must be run from within a git repository for snapshot/delta modes.

### Creating Snapshots

**Create a snapshot for the current commit:**

```bash
# In a git repository
cd my-repo
hotspots analyze . --mode snapshot --format json
```

This will:
- Analyze all TypeScript files in the repository
- Create a snapshot with commit metadata (SHA, parents, timestamp, branch)
- Persist to `.hotspots/snapshots/<commit_sha>.json`
- Update `.hotspots/index.json`

**What gets stored:**
- All functions with their metrics (CC, ND, FO, NS, LRS, band)
- Commit information (SHA, parents, timestamp, branch)
- Function IDs (`<file_path>::<symbol>`)

### Computing Deltas

**Compare current state vs parent commit:**

```bash
hotspots analyze . --mode delta --format json
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

### Comparing Any Two Refs (`hotspots diff`)

`hotspots diff` compares snapshots between any two git refs — not just a commit and its parent. Both refs must have existing snapshots.

```bash
# Compare current branch against main
hotspots diff main HEAD

# Compare two release tags
hotspots diff v1.0.0 v2.0.0 --format json

# Review top 10 riskiest changes with policy check
hotspots diff main HEAD --top 10 --policy
```

**Prerequisites:** snapshots must exist for both refs. Create them with:

```bash
# Create snapshot for each ref (use --force if one already exists)
git checkout main && hotspots analyze . --mode snapshot
git checkout my-branch && hotspots analyze . --mode snapshot
```

Policy is evaluated on the **full** changed set before any `--top` truncation, so violations in lower-ranked functions are never silently dropped.

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

## Higher-Level Analysis

In addition to per-function output, snapshot mode offers three higher-level views accessible
with `--level` or `--explain`.

### File-Level Risk View (`--level file`)

Aggregate per-function data up to the file level and see a ranked file table:

```bash
# Ranked file risk table (requires --mode snapshot --format text)
hotspots analyze . --mode snapshot --format text --level file

# Limit to top 20 files
hotspots analyze . --mode snapshot --format text --level file --top 20
```

Columns: `#`, `file`, `fns`, `loc`, `max_cc`, `avg_cc`, `critical`, `churn`, `file_risk`.

A file with 40 functions averaging cc=12 is a maintenance liability even if no single
function individually tops the per-function list. The composite `file_risk_score` captures this:

```
file_risk = max_cc × 0.4 + avg_cc × 0.3 + log2(fn_count + 1) × 0.2 + churn_factor × 0.1
```

### Module Instability View (`--level module`)

See Robert Martin's instability metric at the directory level:

```bash
hotspots analyze . --mode snapshot --format text --level module
```

Columns: `#`, `module`, `files`, `fns`, `avg_cc`, `afferent`, `efferent`, `instability`, `risk`.

- **Instability near 0.0** — everything depends on this module; risky to change
- **Instability near 1.0** — depends on others but nothing depends on it; safe to change
- **`module_risk = high`** — when `instability < 0.3` AND `avg_complexity > 10`

The interesting hotspots are high-complexity modules with low instability (hard to change
AND everything depends on them).

### Per-Function Explanations (`--explain`)

See a human-readable breakdown of each function's risk score including individual metric
contributions, activity signals, and a co-change coupling section:

```bash
hotspots analyze . --mode snapshot --format text --explain
hotspots analyze . --mode snapshot --format text --explain --top 10
```

The co-change section at the bottom shows pairs of files that frequently change together
in the same commit. High co-change with no static dependency = hidden implicit coupling.
This signal is mined from the last 90 days of git history.

### Snapshot Without Persisting (`--no-persist`)

Run analysis in snapshot mode without writing to disk — useful for one-off inspection:

```bash
hotspots analyze . --mode snapshot --no-persist --format json | jq .aggregates.file_risk
```

### Regenerating a Snapshot (`--force`)

Snapshots are immutable by default. Use `--force` if you need to regenerate one:

```bash
hotspots analyze . --mode snapshot --force
```

### Precise Per-Function Touch Metrics (`--per-function-touches`)

By default, touch metrics are file-level. For more accurate per-function activity signals:

```bash
hotspots analyze . --mode snapshot --per-function-touches
```

**Warning:** Approximately 50× slower. Only use when precise per-function touch counts are needed.

---

## Coordination Pre-flight (`hotspots coordinate`)

Before splitting a task across multiple agents or developers, run `hotspots coordinate` to discover hidden co-change dependencies and get a partition recommendation.

```bash
hotspots coordinate --files auth.rs,session.rs,middleware.rs
```

This reads co-change history and ownership signals already computed by Hotspots — no new data sources, no analysis pass required.

**What it tells you:**

- Which file pairs within your set co-change frequently (with raw `coupling_ratio`)
- Which files *outside* your set are strong co-change partners (hidden dependencies you didn't plan for)
- Which files are safe to modify in parallel vs which should be serialised

**JSON output for scripts and orchestrators:**

```bash
hotspots coordinate --files auth.rs,session.rs --json
```

The JSON schema is stable — `pairs`, `hidden_dependencies`, `ownership`, `parallel_safe`, and `serialize` fields are consistent across versions.

**No risk labels.** `coordinate` outputs raw signal values. The caller decides what a `coupling_ratio` of 0.7 means in their context — a human can read the table, a script can apply its own threshold.

See [CLI reference](../reference/cli.md#hotspots-coordinate) for the full field reference and threshold documentation.

---

## Common Workflows

### Daily Development

```bash
# 1. Check current complexity
hotspots analyze . --format text

# 2. Before making changes, create snapshot
hotspots analyze . --mode snapshot --format json

# 3. Make your changes...

# 4. See what changed
hotspots analyze . --mode delta --format json

# 5. Commit changes
git commit -m "Add feature"

# 6. Create snapshot for new commit
hotspots analyze . --mode snapshot --format json
```

### CI/CD Integration

**Mainline branch (persist snapshot for the merge commit):**

```yaml
- name: Create snapshot
  run: hotspots analyze . --mode snapshot --force
- name: Cache snapshot
  uses: actions/cache/save@v4
  with:
    path: .hotspots/snapshots
    key: hotspots-snapshot-${{ github.sha }}
```

**PR branch (diff against base snapshot):**

```yaml
- name: Restore base snapshot
  uses: actions/cache/restore@v4
  with:
    path: .hotspots/snapshots
    key: hotspots-snapshot-${{ github.event.pull_request.base.sha }}
- name: Create HEAD snapshot
  run: hotspots analyze . --mode snapshot --force
- name: Diff PR vs base
  run: |
    hotspots diff \
      ${{ github.event.pull_request.base.sha }} \
      ${{ github.sha }} \
      --format text --policy
```

The `--policy` flag evaluates policy rules on the full changed set and exits 1 on blocking failures. For a ready-made GitHub Action see [GitHub Action](github-action.md).

### Refactoring Validation

```bash
# Before refactoring
hotspots analyze . --mode snapshot --format json > before.json

# Make refactoring changes...

# After refactoring
hotspots analyze . --mode snapshot --format json > after.json

# See the improvement
hotspots analyze . --mode delta --format json
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
hotspots prune --unreachable --dry-run

# Prune unreachable snapshots older than 30 days
hotspots prune --unreachable --older-than 30

# Prune all unreachable snapshots
hotspots prune --unreachable
```

**Safety:** Only prunes snapshots unreachable from `refs/heads/*` (local branches). Never prunes reachable snapshots.

### Set Compaction Level

```bash
# Set compaction level (currently only Level 0 is implemented)
hotspots compact --level 0
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
hotspots analyze src/ --format json
```

Outputs structured JSON with all metrics, risk components, LRS, and band.

---

## Examples

### Find Most Complex Functions

```bash
hotspots analyze src/ --top 5 --format text
```

### Find Functions Needing Refactoring

```bash
hotspots analyze src/ --min-lrs 9.0 --format json
```

### Track Complexity Over Time

```bash
# On every commit (e.g., in pre-commit hook or CI)
hotspots analyze . --mode snapshot --format json
```

Then use deltas to see trends:
```bash
hotspots analyze . --mode delta --format json
```

### Compare Two Commits

```bash
# Checkout first commit
git checkout <sha1>
hotspots analyze . --mode snapshot --format json > commit1.json

# Checkout second commit
git checkout <sha2>
hotspots analyze . --mode snapshot --format json > commit2.json

# Compare manually or use delta mode
hotspots analyze . --mode delta --format json
```

---

## Troubleshooting

### "Path does not exist"

Make sure you're pointing to a valid file or directory:
```bash
hotspots analyze ./src  # Correct
hotspots analyze src    # Also correct (relative path)
```

### "failed to extract git context"

Snapshot/delta modes require a git repository:
```bash
# Make sure you're in a git repo
cd my-git-repo
hotspots analyze . --mode snapshot
```

### "snapshot already exists and differs"

Snapshots are immutable by default. This error means a snapshot already exists for the
current commit but its content differs from the freshly-computed result.

Repeated `analyze` runs on the same commit should be idempotent. If you see this error,
common causes are:
- A config change between runs that altered scores
- A tool version upgrade that changed metric computation

To regenerate the snapshot intentionally:
```bash
hotspots analyze . --mode snapshot --force
```

### No output in delta mode

If delta shows no changes:
- Check that parent snapshot exists (should be in `.hotspots/snapshots/`)
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

## Policy Engine

The policy engine evaluates complexity regressions and enforces quality gates in CI/CD.

### Running Policy Checks

```bash
# Analyze with policy evaluation
hotspots analyze . --mode delta --policy --format json
```

Output includes a `policy` section with failures and warnings.

### Built-in Policies

**Blocking Policies (cause non-zero exit code):**

1. **Critical Introduction** - Triggers when a function becomes Critical
   - New functions introduced as Critical
   - Existing functions crossing into Critical band

2. **Excessive Risk Regression** - Triggers when LRS increases by ≥1.0
   - Modified functions only
   - Threshold: +1.0 LRS (fixed)

**Warning Policies (informational only):**

3. **Watch Threshold** - Functions entering the "watch zone"
   - Range: `watch_min` to `watch_max` (default: 4.0-6.0)
   - Proactive alert before functions become high-risk

4. **Attention Threshold** - Functions entering the "attention zone"
   - Range: `attention_min` to `attention_max` (default: 6.0-9.0)
   - Alerts for functions approaching critical complexity

5. **Rapid Growth** - Functions with high percentage LRS increase
   - Threshold: `rapid_growth_percent` (default: 50%)
   - Detects sudden complexity spikes

6. **Suppression Missing Reason** - Suppressions without documentation
   - Warns when `// hotspots-ignore:` has no reason
   - Encourages documenting why functions are suppressed

7. **Net Repo Regression** - Overall repository complexity increase
   - Sum of all function LRS scores increased
   - Warning only (allows controlled growth)

**Example policy output:**

```json
{
  "policy": {
    "failed": [
      {
        "id": "critical-introduction",
        "severity": "blocking",
        "function_id": "src/api.ts::handleRequest",
        "message": "Function src/api.ts::handleRequest introduced as Critical"
      }
    ],
    "warnings": [
      {
        "id": "watch-threshold",
        "severity": "warning",
        "function_id": "src/db.ts::query",
        "message": "Function src/db.ts::query entered watch threshold range (LRS: 4.5)"
      },
      {
        "id": "net-repo-regression",
        "severity": "warning",
        "message": "Repository total LRS increased by 3.20",
        "metadata": {
          "total_delta": 3.20
        }
      }
    ]
  }
}
```

### Configuring Warning Thresholds

Customize warning ranges in your config file:

```json
{
  "warnings": {
    "watch": {
      "min": 5.0,
      "max": 7.0
    },
    "attention": {
      "min": 7.0,
      "max": 10.0
    },
    "rapid_growth_percent": 75.0
  }
}
```

To exclude specific functions from policy checks, use [Suppression Comments](/guide/suppression).

---

## HTML Reports

Generate interactive HTML reports for better visualization:

```bash
hotspots analyze . --mode snapshot --format html
```

**HTML report features:**
- Interactive sorting by any column
- Filter by risk band and driver label
- Search by function name
- Color-coded risk bands and driver badges
- **Action column** in triage table: per-function refactoring recommendation (driver × quadrant)
- **Trend charts** (snapshot mode, requires ≥2 prior snapshots):
  - Stacked bar chart: band-count distribution over time (up to 30 snapshots)
  - Line charts: activity risk and top-1% concentration over time
  - Hover tooltip on band chart with per-band counts
- Responsive design
- Self-contained (no external dependencies)

**Delta mode HTML:**

```bash
hotspots analyze . --mode delta --format html > delta-report.html
```

Shows function changes with:
- Status badges (new/modified/deleted/unchanged)
- Before/after metrics
- Band transitions
- Policy violations highlighted

**Open in browser:**

```bash
hotspots analyze . --mode snapshot --format html
open .hotspots/report.html  # macOS
xdg-open .hotspots/report.html  # Linux
start .hotspots/report.html  # Windows
```

---

## See Also

- [Metrics & LRS](../reference/metrics.md) - Local Risk Score details
