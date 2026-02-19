# CLI Reference

Complete command-line reference for Hotspots.

## Installation

```bash
# Install from source
cargo install --path hotspots-cli

# Or use pre-built binary
# Download from GitHub releases
```

## Global Options

All commands support:

```bash
hotspots --help     # Show help
hotspots --version  # Show version
```

---

## Commands

### `hotspots analyze`

Analyze source files for complexity metrics.

**Supported Languages:** TypeScript, JavaScript, Go, Java, Python, Rust

#### Basic Usage

```bash
# Analyze a single file
hotspots analyze src/app.ts

# Analyze a directory (recursive)
hotspots analyze src/

# Analyze with JSON output
hotspots analyze src/ --format json
```

#### Options

##### `<path>`
**Required.** Path to source file or directory to analyze.

```bash
hotspots analyze src/
hotspots analyze lib/utils.ts
```

##### `--format <format>`
**Optional.** Output format: `text`, `json`, or `html`.
**Default:** `text`

```bash
# Human-readable text output (default)
hotspots analyze src/ --format text

# Machine-readable JSON output
hotspots analyze src/ --format json

# Interactive HTML report (requires --mode)
hotspots analyze src/ --format html --mode snapshot
```

**Note:** HTML format requires `--mode snapshot` or `--mode delta`.

##### `--mode <mode>`
**Optional.** Output mode: `snapshot` or `delta`.

```bash
# Snapshot mode: capture current state
hotspots analyze src/ --mode snapshot --format json

# Delta mode: compare against parent commit
hotspots analyze src/ --mode delta --format json
```

**Snapshot mode:**
- Captures current complexity state with git metadata
- Persists to `.hotspots/snapshots/` (mainline only)
- Updates `.hotspots/index.json`
- Computes aggregates for output

**Delta mode:**
- Compares current state vs parent commit
- Supports policy evaluation with `--policy`
- Shows complexity changes (ΔLRS)
- PR mode: compares vs merge-base
- Mainline mode: compares vs direct parent

##### `--policy`
**Optional.** Evaluate policies (only valid with `--mode delta`).

```bash
# Run policy checks
hotspots analyze src/ --mode delta --policy --format text

# Fail CI build on policy violations
hotspots analyze src/ --mode delta --policy --format json || exit 1
```

**Policy Types:**
- **Blocking failures:** Exit code 1 on violations
  - No regressions allowed (LRS must not increase)
  - No band transitions to higher risk
- **Warnings:** Exit code 0, informational
  - Watch threshold (approaching moderate)
  - Attention threshold (approaching high)
  - Rapid growth (>50% LRS increase)
  - Net repository regression

**Requires:** `--mode delta`

##### `--top <N>`
**Optional.** Show only top N functions by LRS.
**Overrides:** Config file `top` value.

```bash
# Show top 20 highest-complexity functions
hotspots analyze src/ --top 20

# No limit (show all)
hotspots analyze src/
```

##### `--min-lrs <threshold>`
**Optional.** Filter functions below minimum LRS threshold.
**Overrides:** Config file `min_lrs` value.

```bash
# Only show functions with LRS ≥ 5.0
hotspots analyze src/ --min-lrs 5.0

# Show everything (no filter)
hotspots analyze src/ --min-lrs 0.0
```

##### `--config <path>`
**Optional.** Path to configuration file.
**Default:** Auto-discover from project root.

```bash
# Use specific config file
hotspots analyze src/ --config custom-config.json

# Use CI-specific config
hotspots analyze src/ --config .hotspots.ci.json
```

See [Configuration](../guide/configuration.md) for config file format.

##### `--output <path>`
**Optional.** Output file path (for HTML format).
**Default:** `.hotspots/report.html`

```bash
# Write HTML report to custom location
hotspots analyze src/ --mode snapshot --format html --output reports/complexity.html
```

**Only applicable to HTML format.**

##### `--explain`
**Optional.** Show human-readable per-function risk breakdown.
**Only valid with:** `--mode snapshot --format text`
**Mutually exclusive with:** `--level`

```bash
# Show ranked functions with full risk factor explanations
hotspots analyze . --mode snapshot --format text --explain

# Limit to top 10 functions
hotspots analyze . --mode snapshot --format text --explain --top 10
```

Displays per-function metric contributions (CC, ND, FO, NS), activity signals (churn,
touch count, fan-in, SCC, depth), and a co-change coupling section at the end showing
the top 10 high/moderate source-file pairs.

**Note:** In snapshot mode with `--format text`, you must specify either `--explain`
or `--level <LEVEL>`.

##### `--level <LEVEL>`
**Optional.** Switch to a higher-level ranked view instead of per-function output.
**Only valid with:** `--mode snapshot --format text`
**Mutually exclusive with:** `--explain`

| Value    | Output                                                               |
|----------|----------------------------------------------------------------------|
| `file`   | Ranked file risk table (max CC, avg CC, function count, LOC, churn) |
| `module` | Ranked module instability table (afferent, efferent, instability)    |

```bash
# File-level risk view (ranked by composite file_risk_score)
hotspots analyze . --mode snapshot --format text --level file

# Module (directory) instability view
hotspots analyze . --mode snapshot --format text --level module

# Limit to top 20 entries
hotspots analyze . --mode snapshot --format text --level file --top 20
```

**Note:** In snapshot mode with `--format text`, you must specify either `--level`
or `--explain`.

##### `--force` / `-f`
**Optional.** Overwrite an existing snapshot if one already exists for this commit.

```bash
hotspots analyze . --mode snapshot --force
```

Snapshots are normally immutable (identified by commit SHA). Use `--force` to
regenerate a snapshot after a config change or to correct a prior run.

**Mutually exclusive with:** `--no-persist`

##### `--no-persist`
**Optional.** Analyze without writing the snapshot to disk.
**Only valid with:** `--mode snapshot` or `--mode delta`
**Mutually exclusive with:** `--force`

```bash
# Run snapshot analysis without saving to .hotspots/
hotspots analyze . --mode snapshot --no-persist --format json
```

Useful for one-off inspection or CI pipelines where snapshot history is not needed.

##### `--per-function-touches`
**Optional.** Use `git log -L` to compute per-function touch counts instead of
file-level counts.
**Only valid with:** `--mode snapshot` or `--mode delta`

```bash
hotspots analyze . --mode snapshot --per-function-touches
```

**Warning:** Approximately 50× slower than the default. Default touch metrics are
file-level (all functions in a file share the same `touch_count_30d`). Use this flag
when precise per-function activity signals are required.

#### Examples

**Basic analysis (text output):**
```bash
hotspots analyze src/
```

**JSON output for CI:**
```bash
hotspots analyze src/ --format json --min-lrs 5.0 > analysis.json
```

**Snapshot with HTML report:**
```bash
hotspots analyze src/ --mode snapshot --format html
# Opens .hotspots/report.html
```

**Delta with policy enforcement:**
```bash
hotspots analyze src/ --mode delta --policy --format text
# Exit code 1 if blocking failures detected
```

**PR mode (automatic in CI):**
```bash
# In GitHub Actions with PR context
hotspots analyze src/ --mode delta --policy --format json
# Compares vs merge-base automatically
```

**Override config settings:**
```bash
hotspots analyze src/ --config .hotspots.ci.json --min-lrs 6.0 --top 50
```

**File-level risk view:**
```bash
hotspots analyze . --mode snapshot --format text --level file
hotspots analyze . --mode snapshot --format text --level file --top 20
```

**Module instability view:**
```bash
hotspots analyze . --mode snapshot --format text --level module
```

**Human-readable per-function explanations with co-change section:**
```bash
hotspots analyze . --mode snapshot --format text --explain
```

**Snapshot without persisting (read-only inspection):**
```bash
hotspots analyze . --mode snapshot --no-persist --format json
```

---

### `hotspots prune`

Prune unreachable snapshots to reduce storage.

#### Usage

```bash
# Prune unreachable snapshots
hotspots prune --unreachable

# Dry-run (preview what would be deleted)
hotspots prune --unreachable --dry-run

# Prune only snapshots older than 30 days
hotspots prune --unreachable --older-than 30
```

#### Options

##### `--unreachable`
**Required.** Must be explicitly specified to confirm pruning.

**Safety:** Prevents accidental data loss.

##### `--older-than <days>`
**Optional.** Only prune snapshots older than N days.

```bash
# Keep recent history, prune old unreachable snapshots
hotspots prune --unreachable --older-than 90
```

##### `--dry-run`
**Optional.** Preview pruning without deleting.

```bash
# See what would be pruned
hotspots prune --unreachable --dry-run
```

#### Output

```
Pruned 15 snapshots

Pruned commit SHAs:
  abc123...
  def456...
  ...

Reachable snapshots: 42
Unreachable snapshots kept (due to age filter): 8
```

#### How Pruning Works

1. **Find repository root** (searches up for `.git`)
2. **Identify reachable commits** via `refs/heads/*` (local branches)
3. **Mark unreachable snapshots** for deletion
4. **Apply age filter** (if `--older-than` specified)
5. **Delete snapshot files** from `.hotspots/snapshots/`
6. **Update index** (`.hotspots/index.json`)

**Note:** Only prunes unreachable snapshots. Reachable history is preserved.

---

### `hotspots compact`

Compact snapshot history to reduce storage.

#### Usage

```bash
# Set compaction level
hotspots compact --level 0
```

#### Options

##### `--level <N>`
**Required.** Compaction level: `0`, `1`, or `2`.

**Levels:**
- **Level 0:** Full snapshots (current implementation)
- **Level 1:** Deltas only (planned)
- **Level 2:** Band transitions only (planned)

**Note:** Levels 1 and 2 are not yet implemented. Command only sets metadata.

#### Output

```
Compaction level set to 0 (was 0)
```

---

### `hotspots trends`

Analyze complexity trends from snapshot history.

#### Usage

```bash
# Analyze trends (last 10 snapshots, top 5 functions)
hotspots trends .

# Custom window and top-K
hotspots trends . --window 20 --top 10

# JSON output
hotspots trends . --format json > trends.json
```

#### Options

##### `<path>`
**Required.** Path to repository root.

##### `--format <format>`
**Optional.** Output format: `json` or `text`.
**Default:** `json`

```bash
hotspots trends . --format text
```

##### `--window <N>`
**Optional.** Number of snapshots to analyze.
**Default:** `10`

```bash
# Analyze last 20 snapshots
hotspots trends . --window 20
```

##### `--top <K>`
**Optional.** Top K functions for hotspot analysis.
**Default:** `5`

```bash
# Track top 10 hotspots
hotspots trends . --top 10
```

#### Output (Text Format)

```
Trends Analysis
================================================================================

Risk Velocities:
Function                                 Velocity     Direction    First LRS    Last LRS
----------------------------------------------------------------------------------------------------
processOrder                             0.50         positive     3.50         5.50
validateInput                            -0.20        negative     4.20         3.00

Hotspot Stability:
Function                                 Stability    Overlap      Appearances
----------------------------------------------------------------------------------------
processPayment                           stable       0.90         9/10
handleError                              emerging     0.60         6/10

Refactor Effectiveness:
Function                                 Outcome      Improvement  Sustained
----------------------------------------------------------------------------------------
refactoredFunction                       successful   -2.50        5

Summary:
  Risk velocities: 15
  Hotspots analyzed: 8
  Refactors detected: 3
```

#### Metrics Explained

**Risk Velocity:**
- LRS change per snapshot
- Positive: increasing complexity
- Negative: decreasing complexity
- Flat: stable

**Hotspot Stability:**
- Stable: consistently in top-K (>80% overlap)
- Emerging: recently entered top-K (60-80% overlap)
- Volatile: intermittently in top-K (<60% overlap)

**Refactor Effectiveness:**
- Successful: sustained LRS reduction (>3 commits)
- Partial: temporary improvement (1-2 commits)
- Cosmetic: no sustained improvement

---

### `hotspots config validate`

Validate configuration file without running analysis.

#### Usage

```bash
# Validate auto-discovered config
hotspots config validate

# Validate specific config file
hotspots config validate --path custom-config.json
```

#### Options

##### `--path <path>`
**Optional.** Path to config file.
**Default:** Auto-discover from current directory.

#### Output

**Valid config:**
```
Config valid: .hotspotsrc.json
```

**No config found:**
```
No config file found. Using defaults.
```

**Invalid config:**
```
Config validation failed: thresholds.moderate (6.0) must be less than thresholds.high (5.0)
```

Exit code 1 on validation failure.

---

### `hotspots config show`

Show resolved configuration (merged defaults + config file).

#### Usage

```bash
# Show resolved config
hotspots config show

# Show specific config file
hotspots config show --path .hotspots.ci.json
```

#### Options

##### `--path <path>`
**Optional.** Path to config file.
**Default:** Auto-discover from current directory.

#### Output

```
Configuration:
  Source: .hotspotsrc.json

Weights:
  cc: 1.0
  nd: 0.8
  fo: 0.6
  ns: 0.7

Thresholds:
  moderate: 3.0
  high: 6.0
  critical: 9.0

Filters:
  min_lrs: 3.0
  top: 50
  include: all files
  exclude: active (custom patterns)
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (or warnings only) |
| `1` | Error or blocking policy failure |

**Policy Evaluation:**
- Exit code `0` if only warnings (watch, attention, rapid growth)
- Exit code `1` if blocking failures (regressions, band transitions)

---

## Configuration Priority

CLI flags override config file values:

1. **CLI flags** (highest priority)
2. **Config file** (`.hotspotsrc.json`, etc.)
3. **Defaults** (lowest priority)

```bash
# Config file says min_lrs: 3.0, CLI overrides to 5.0
hotspots analyze src/ --min-lrs 5.0
```

See [Configuration](../guide/configuration.md) for details.

---

## Environment Variables

Hotspots respects git environment variables for repository operations:

- `GIT_DIR` - Override `.git` directory location
- `GIT_WORK_TREE` - Override working directory

**CI/CD Detection (PR mode):**
- `GITHUB_EVENT_NAME=pull_request` - GitHub Actions PR
- `CI_MERGE_REQUEST_IID` - GitLab MR
- `CIRCLE_PULL_REQUEST` - CircleCI PR
- `TRAVIS_PULL_REQUEST` - Travis CI PR

When PR context is detected, delta mode compares vs merge-base instead of direct parent.

---

## Common Workflows

### Local Development

```bash
# Quick complexity check
hotspots analyze src/ --top 20

# Detailed analysis with filtering
hotspots analyze src/ --min-lrs 5.0 --format json | jq .
```

### CI/CD Integration

**GitHub Actions:**
```yaml
- name: Complexity Analysis
  run: hotspots analyze src/ --mode delta --policy --format json
```

**With config file:**
```yaml
- name: Complexity Analysis
  run: hotspots analyze src/ --config .hotspots.ci.json --mode delta --policy
```

See [CI Integration](../guide/ci-integration.md) for complete examples.

### Snapshot Management

```bash
# Capture baseline snapshot
hotspots analyze src/ --mode snapshot --format json > baseline.json

# Prune old snapshots (keep 30 days)
hotspots prune --unreachable --older-than 30 --dry-run
hotspots prune --unreachable --older-than 30

# Analyze trends
hotspots trends . --window 20 --format text
```

### Debugging

```bash
# Validate configuration
hotspots config validate

# Show resolved config (check what's active)
hotspots config show

# Test with dry-run
hotspots analyze src/ --format json --dry-run  # (if supported)
```

---

## Troubleshooting

### "Path does not exist"

**Cause:** Invalid path argument.

**Fix:** Verify path exists:
```bash
ls -la src/
hotspots analyze src/
```

### "not in a git repository"

**Cause:** Snapshot/delta mode requires git repository.

**Fix:** Initialize git or use basic analysis:
```bash
git init
# OR
hotspots analyze src/ --format json  # (no --mode flag)
```

### "--policy flag is only valid with --mode delta"

**Cause:** Using `--policy` without `--mode delta`.

**Fix:**
```bash
hotspots analyze src/ --mode delta --policy
```

### "HTML format requires --mode snapshot or --mode delta"

**Cause:** Using `--format html` without `--mode`.

**Fix:**
```bash
hotspots analyze src/ --format html --mode snapshot
```

### "Config validation failed"

**Cause:** Invalid configuration file.

**Fix:** Validate and fix config:
```bash
hotspots config validate
# Read error message, fix config file
cat .hotspotsrc.json | jq .  # Check JSON syntax
```

### "unreachable flag must be specified"

**Cause:** Safety check for prune command.

**Fix:**
```bash
hotspots prune --unreachable
```

### "text format without --explain is not supported for snapshot mode"

**Cause:** Using `--mode snapshot --format text` without `--explain` or `--level`.

**Fix:** Add `--explain`, `--level`, or use JSON format:
```bash
hotspots analyze . --mode snapshot --format text --explain
hotspots analyze . --mode snapshot --format text --level file
hotspots analyze . --mode snapshot --format json
```

### "--level is only valid with --mode snapshot"

**Cause:** Using `--level` without `--mode snapshot --format text`.

**Fix:**
```bash
hotspots analyze . --mode snapshot --format text --level file
```

### "--level and --explain are mutually exclusive"

**Cause:** Both `--level` and `--explain` flags specified together.

**Fix:** Use one or the other:
```bash
hotspots analyze . --mode snapshot --format text --level file
# OR
hotspots analyze . --mode snapshot --format text --explain
```

### "--no-persist and --force are mutually exclusive"

**Cause:** Both `--no-persist` and `--force` flags specified together.

**Fix:** Use one or the other:
```bash
hotspots analyze . --mode snapshot --no-persist   # analyze without saving
hotspots analyze . --mode snapshot --force         # overwrite existing snapshot
```

---

## Related Documentation

- [Configuration Guide](../guide/configuration.md) - Config file format
- [CI Integration](../guide/ci-integration.md) - GitHub Actions, GitLab CI
- [Output Formats](../guide/output-formats.md) - JSON schema, HTML reports
- [LRS Specification](./lrs-spec.md) - How LRS is calculated
- [Policy Engine](../guide/usage.md#policy-engine) - Policy rules and enforcement
