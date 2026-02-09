# Quick Start

Get started with Hotspots in 5 minutes.

## Prerequisites

- Hotspots installed (see [Installation](./installation.md))
- A git repository with TypeScript, JavaScript, Go, Python, Rust, or Java code

## Basic Usage

### 1. Analyze a Directory

```bash
hotspots analyze src/
```

This will analyze all supported files in `src/` and show:
- Functions with highest complexity
- Risk scores (LRS)
- Metrics breakdown (CC, ND, FO, NS)

**Example output:**
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
HIGH (Leverage Risk Score 8.0+)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
./src/auth/validateUser.ts::validateUser
  LRS: 12.5  CC: 15  ND: 4  FO: 8  NS: 2
  Changes: 23  Risk: CRITICAL
```

### 2. Snapshot Mode (Track Over Time)

Create a baseline snapshot:

```bash
hotspots analyze src/ --mode snapshot --format json
```

This creates `.hotspots/snapshots/<commit-sha>.json` with current state.

Make changes, commit, and run again to track complexity evolution.

### 3. Delta Mode (Compare with Baseline)

Compare current state with the last snapshot:

```bash
hotspots analyze src/ --mode delta
```

Shows:
- **New functions** - Functions added since last snapshot
- **Modified functions** - Functions with changed complexity
- **Deleted functions** - Functions removed

**Example output:**
```
MODIFIED FUNCTIONS
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
./src/auth/validateUser.ts::validateUser
  Complexity: 12 → 15 (+3)
  Status: REGRESSION
  Changes: 23
```

### 4. Policy Enforcement (CI/CD)

Block risky changes in CI:

```bash
hotspots analyze src/ --mode delta --policy --fail-on blocking
```

This will:
- ✅ Pass if no policy violations
- ❌ Fail (exit code 1) if blocking policies fail

**Built-in policies:**
- **Critical Introduction** - New function with LRS > 10
- **Excessive Risk** - Modified function exceeds risk threshold
- **Attention Needed** - Function enters "watch" range
- **Rapid Growth** - Complexity increases too quickly
- **Suppression Hygiene** - Suppression comments without reason

### 5. Generate HTML Report

```bash
hotspots analyze src/ --format html --output report.html
```

Open `report.html` in your browser for interactive visualization.

### 6. View Trends

See complexity trends over time:

```bash
hotspots trends src/
```

Shows:
- **Risk velocity** - How quickly complexity is increasing
- **Hotspots** - Functions that are both complex and frequently changed
- **Refactor candidates** - High-leverage refactoring targets

## Common Workflows

### Workflow 1: Find Technical Debt

```bash
# Create baseline
hotspots analyze src/ --mode snapshot

# Generate report
hotspots analyze src/ --format html --output debt-report.html
```

Review the HTML report to prioritize refactoring.

### Workflow 2: Block Risky PRs

Add to `.github/workflows/ci.yml`:

```yaml
- name: Hotspots Analysis
  uses: Stephen-Collins-tech/hotspots@v1
  with:
    path: src/
    mode: delta
    policy: true
    fail-on: blocking
```

### Workflow 3: Track Refactoring Progress

```bash
# Before refactoring
hotspots analyze src/ --mode snapshot

# After refactoring
hotspots analyze src/ --mode delta

# View trends
hotspots trends src/ --window 10
```

## Configuration

Create `.hotspotsrc.json` in your project root:

```json
{
  "exclude": [
    "**/*.test.ts",
    "**/__tests__/**"
  ],
  "thresholds": {
    "high": 8.0,
    "moderate": 5.0,
    "low": 3.0
  },
  "min_lrs": 3.0
}
```

See [Configuration Guide](../guide/configuration.md) for all options.

## Suppression Comments

Suppress warnings for specific functions:

```typescript
// hotspots-ignore: legacy code, will refactor in Q2
function complexLegacyFunction() {
  // ...
}
```

See [Suppression Guide](../guide/suppression.md) for more.

## Next Steps

- [Usage Guide](../guide/usage.md) - Complete CLI reference
- [Configuration](../guide/configuration.md) - Config file setup
- [CI Integration](../guide/ci-integration.md) - Use in CI/CD pipelines
- [Metrics Reference](../reference/metrics.md) - How metrics are calculated

## Quick Reference

```bash
# Analyze directory
hotspots analyze <path>

# Snapshot mode
hotspots analyze <path> --mode snapshot

# Delta mode with policies
hotspots analyze <path> --mode delta --policy

# HTML output
hotspots analyze <path> --format html --output report.html

# JSON output
hotspots analyze <path> --format json

# Trends
hotspots trends <path>

# Help
hotspots --help
hotspots analyze --help
```

## Troubleshooting

**"No functions found"**
- Ensure you're analyzing supported file types
- Check that files aren't excluded by `.gitignore` or config

**"No baseline snapshot found"**
- Run `hotspots analyze <path> --mode snapshot` first
- Ensure `.hotspots/` directory exists

**"Policy violations but no error"**
- Add `--fail-on blocking` to exit with error code 1

## Getting Help

- 📖 [Documentation](../index.md)
- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 🐛 [Report Issues](https://github.com/Stephen-Collins-tech/hotspots/issues)
