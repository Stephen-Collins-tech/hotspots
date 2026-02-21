# GitHub Action

Complete guide to using the Hotspots GitHub Action in your workflows.

## Overview

The Hotspots GitHub Action provides zero-config complexity analysis for pull requests and commits.

**Key Features:**
- 🚀 **Zero configuration** - Works out of the box
- 🎯 **PR-aware** - Automatically detects PRs and runs delta analysis
- 📊 **HTML Reports** - Interactive reports as workflow artifacts
- 💬 **PR Comments** - Posts results directly to pull requests
- ⚡ **Fast** - Cached binary downloads, incremental analysis
- 🔒 **Deterministic** - Byte-for-byte reproducible results

**Supported Languages:** TypeScript, JavaScript, Go, Java, Python, Rust

---

## Quick Start

### Basic Setup

Create `.github/workflows/hotspots.yml`:

```yaml
name: Hotspots

on:
  pull_request:
  push:
    branches: [main]

jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write  # For PR comments

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for delta analysis

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

**That's it!** The action will:
- Analyze your code on every PR
- Post results as PR comments
- Generate HTML reports
- Fail builds on policy violations

---

## Inputs

### Required Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `github-token` | GitHub token for posting PR comments | `github.token` (auto) |

### Optional Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `path` | Path to analyze | `.` (repo root) |
| `policy` | Policy to enforce | `critical-introduction` |
| `min-lrs` | Minimum LRS threshold (overrides policy) | - |
| `config` | Path to config file | Auto-discover |
| `fail-on` | When to fail: `error`, `warn`, `never` | `error` |
| `version` | Hotspots version to use | `latest` |
| `post-comment` | Post PR comment | `true` |

### Input Details

#### `path`

Path to analyze (file or directory).

```yaml
# Analyze specific directory
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    path: src/

# Analyze entire repository
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    path: .
```

#### `policy`

Policy mode for enforcement.

**Available policies:**
- `critical-introduction` (default) - Block new critical-risk functions
- `strict` - Block any complexity increase
- `moderate` - Allow moderate increases, block high/critical
- `custom` - Use config file thresholds

```yaml
# Strict policy (no regressions)
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    policy: strict

# Custom policy via config file
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    policy: custom
    config: .hotspots.ci.json
```

#### `min-lrs`

Override policy with minimum LRS threshold.

```yaml
# Only flag functions with LRS ≥ 8.0
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    min-lrs: 8.0
```

#### `fail-on`

Control when the action fails the build.

**Options:**
- `error` (default) - Fail on blocking policy violations
- `warn` - Fail on warnings too
- `never` - Never fail (reporting only)

```yaml
# Warning mode (don't fail builds)
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    fail-on: never
```

#### `version`

Specify Hotspots version.

```yaml
# Pin to specific version
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    version: 1.2.3

# Use latest (default)
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    version: latest
```

---

## Outputs

The action provides structured outputs for use in subsequent steps.

| Output | Type | Description |
|--------|------|-------------|
| `violations` | JSON array | Policy violations |
| `passed` | boolean | Whether analysis passed |
| `summary` | string | Markdown summary |
| `report-path` | string | Path to HTML report |
| `json-output` | string | Path to JSON output |

### Using Outputs

```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

- name: Check Results
  run: |
    echo "Passed: ${{ steps.hotspots.outputs.passed }}"
    echo "Summary: ${{ steps.hotspots.outputs.summary }}"

- name: Upload Report
  uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report
    path: ${{ steps.hotspots.outputs.report-path }}
```

---

## How It Works

### PR Context (Delta Mode)

When run on a pull request:

1. **Detects merge-base** - Automatically finds common ancestor
2. **Analyzes changes** - Only checks modified functions
3. **Compares complexity** - Before vs. after
4. **Evaluates policies** - Checks for violations
5. **Posts PR comment** - Results directly in PR
6. **Updates on push** - Edits existing comment

**Behavior:**
- First run: Creates new comment
- Subsequent runs: Updates existing comment
- Multiple commits: Shows latest analysis

### Push Context (Snapshot Mode)

When run on main/default branch:

1. **Analyzes entire codebase** - All functions
2. **Creates snapshot** - Stores baseline
3. **Reports violations** - All high-complexity functions
4. **Shows job summary** - In workflow run

**Behavior:**
- Snapshots stored in `.hotspots/snapshots/`
- Used as baseline for future PRs
- No PR comments (not applicable)

---

## PR Comments

### Example Comment

```markdown
# 🔍 Hotspots Analysis

**Status:** ❌ 2 blocking violation(s)

### Summary
- **Mode:** Delta (comparing vs. `main`)
- **Changed functions:** 5
- **New functions:** 2
- **Removed functions:** 1

### ❌ Blocking Violations

| Function | File | LRS | Change | Policy |
|----------|------|-----|--------|--------|
| `processPayment` | `src/payment.ts:120` | 9.2 | +1.5 | Critical introduction |
| `validateOrder` | `src/orders.ts:45` | 8.7 | +2.1 | Band transition (moderate → high) |

### ⚠️ Warnings

| Function | File | LRS | Reason |
|----------|------|-----|--------|
| `parseInput` | `src/parser.ts:78` | 5.8 | Approaching high threshold |

### 👀 Watch

3 function(s) approaching moderate threshold

---

[View full HTML report](https://github.com/yourorg/repo/actions/runs/123456789)
```

### Comment Behavior

- **Single comment per PR** - Updates existing comment
- **Collapsible details** - Large reports are collapsed
- **Direct links** - Click function names to jump to code
- **Color-coded** - Risk levels visually distinct
- **Dismissable** - Can be minimized if needed

---

## Workflow Examples

### Basic PR Check

```yaml
name: Complexity Check

on: [pull_request]

jobs:
  hotspots:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### With Custom Config

```yaml
name: Hotspots

on: [pull_request, push]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          config: .hotspots.ci.json
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

Create `.hotspots.ci.json`:
```json
{
  "exclude": ["**/*.test.ts", "**/__tests__/**"],
  "min_lrs": 6.0,
  "thresholds": {
    "moderate": 5.0,
    "high": 8.0,
    "critical": 10.0
  }
}
```

### Monorepo Setup

```yaml
name: Hotspots

on: [pull_request]

jobs:
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/frontend
          github-token: ${{ secrets.GITHUB_TOKEN }}

  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/backend
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Upload HTML Report as Artifact

```yaml
name: Hotspots

on: [pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        id: hotspots
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: hotspots-report-${{ github.sha }}
          path: ${{ steps.hotspots.outputs.report-path }}
          retention-days: 30
```

### Warning Mode (Don't Fail Builds)

```yaml
name: Hotspots

on: [pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          fail-on: never
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Custom Failure Handling

```yaml
name: Hotspots

on: [pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        id: hotspots
        with:
          fail-on: never
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: Custom Failure Logic
        if: steps.hotspots.outputs.passed == 'false'
        run: |
          echo "::warning::Hotspots found violations"
          echo "Summary: ${{ steps.hotspots.outputs.summary }}"
          # Custom notification, Slack message, etc.
```

### Multi-Language Project

```yaml
name: Hotspots

on: [pull_request]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: .  # Analyzes all supported files
          config: .hotspotsrc.json
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

`.hotspotsrc.json`:
```json
{
  "include": [
    "src/**/*.{ts,js}",
    "backend/**/*.go",
    "scripts/**/*.py"
  ],
  "exclude": [
    "**/*.test.*",
    "**/node_modules/**"
  ]
}
```

---

## Permissions

### Required Permissions

```yaml
permissions:
  contents: read        # Checkout code
  pull-requests: write  # Post PR comments
```

### Minimal Permissions

If you don't want PR comments:

```yaml
permissions:
  contents: read

jobs:
  analyze:
    steps:
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          post-comment: false
```

---

## Troubleshooting

### "failed to extract git context"

**Cause:** Shallow git clone.

**Fix:** Use `fetch-depth: 0`:
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
```

### "merge-base not found"

**Cause:** Base branch not available.

**Fix:** Fetch base branch:
```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
    ref: ${{ github.event.pull_request.head.ref }}

- name: Fetch base branch
  run: git fetch origin ${{ github.event.pull_request.base.ref }}
```

### PR Comments Not Posting

**Causes:**
1. Missing `pull-requests: write` permission
2. `github-token` not provided
3. `post-comment: false`

**Fix:**
```yaml
permissions:
  pull-requests: write

steps:
  - uses: Stephen-Collins-tech/hotspots-action@v1
    with:
      github-token: ${{ secrets.GITHUB_TOKEN }}
      post-comment: true
```

### Binary Download Fails

**Cause:** Network issues or unsupported platform.

**Fix:** Build from source:
```yaml
- uses: dtolnay/rust-toolchain@stable

- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    version: build-from-source
```

### "Path does not exist"

**Cause:** Invalid `path` input.

**Fix:** Verify path exists:
```yaml
- name: List directory
  run: ls -la src/

- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    path: src/
```

---

## Advanced Usage

### Conditional Execution

```yaml
# Only on specific branches
on:
  pull_request:
    branches: [main, develop]

# Only on specific paths
on:
  pull_request:
    paths:
      - 'src/**'
      - '**.ts'
      - '**.js'
```

### Matrix Strategy

```yaml
jobs:
  analyze:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        path: [frontend, backend, shared]
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/${{ matrix.path }}
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Scheduled Analysis

```yaml
name: Weekly Complexity Report

on:
  schedule:
    - cron: '0 0 * * 0'  # Every Sunday at midnight

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          fail-on: never  # Just report, don't fail

      - uses: actions/upload-artifact@v4
        with:
          name: weekly-complexity-report
          path: .hotspots/report.html
```

---

## Performance

### Caching

The action automatically caches:
- Hotspots binary (per version)
- Analysis snapshots (`.hotspots/`)

**Cache behavior:**
- Binary: Cached for 7 days
- Snapshots: Persisted in repository

### Execution Time

**Typical execution times:**
- Small project (<100 files): 10-30 seconds
- Medium project (100-500 files): 30-60 seconds
- Large project (500+ files): 1-3 minutes

**Optimization tips:**
- Use `path` to analyze specific directories
- Exclude test files with config
- Pin to specific `version` (avoids version checks)

---

## Related Documentation

- [CLI Reference](../reference/cli.md) - Command-line interface
- [CI/CD Integration](./ci-integration.md) - Other CI systems
- [Configuration](./configuration.md) - Config file options
- [Output Formats](./output-formats.md) - JSON, HTML, Text formats
- [Policy Engine](./usage.md#policy-engine) - Policy rules

---

## Getting Help

- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 [Open an Issue](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 📖 [Documentation](https://docs.hotspots.dev)

---

## Next Steps

After setting up the action:

1. **Tune thresholds** - Adjust for your codebase
2. **Add config file** - Customize behavior
3. **Review reports** - Understand complexity patterns
4. **Refactor hotspots** - Reduce high-complexity functions
5. **Monitor trends** - Track complexity over time

**Happy analyzing!** 🚀
