# Hotspots GitHub Action

Analyze TypeScript/JavaScript complexity and block regressions in your CI pipeline.

## Features

- 🚀 **Zero configuration** - Works out of the box
- 🎯 **PR-aware** - Automatically detects PRs and runs delta analysis
- 📊 **HTML Reports** - Interactive reports as workflow artifacts
- 💬 **PR Comments** - Posts results directly to pull requests
- ⚡ **Fast** - Cached binary downloads, incremental analysis
- 🔒 **Deterministic** - Byte-for-byte reproducible results

## Quick Start

Add to your `.github/workflows/hotspots.yml`:

```yaml
name: Hotspots

on:
  pull_request:
  push:
    branches: [main]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for delta analysis

      - uses: ./action  # or yourorg/hotspots@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

## Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `path` | Path to analyze | `.` (repository root) |
| `policy` | Policy to enforce | `critical-introduction` |
| `min-lrs` | Minimum LRS threshold (overrides policy) | - |
| `config` | Path to hotspots config file | - |
| `fail-on` | When to fail (`error`, `warn`, `never`) | `error` |
| `version` | Hotspots version to use | `latest` |
| `github-token` | GitHub token for posting comments | `${{ github.token }}` |
| `post-comment` | Post results as PR comment | `true` |

## Outputs

| Output | Description |
|--------|-------------|
| `violations` | JSON array of policy violations |
| `passed` | Whether analysis passed (`true`/`false`) |
| `summary` | Markdown summary of results |
| `report-path` | Path to generated HTML report |
| `json-output` | Path to full JSON output file |

### JSON Output Format

Hotspots produces structured JSON output following a versioned schema. This enables:
- **AI Assistant Integration**: LLMs can parse and reason about complexity
- **Custom Tooling**: Build dashboards, reports, or analysis tools
- **CI/CD Integration**: Validate output and enforce custom policies

#### Schema & Types

JSON Schema definitions are available in `schemas/`:
- `hotspots-output.schema.json` - Complete output format (JSON Schema Draft 07)
- `function-report.schema.json` - Individual function analysis
- `metrics.schema.json` - Raw complexity metrics
- `policy-result.schema.json` - Policy violations/warnings

TypeScript types are available via npm:
```bash
npm install @hotspots/types
```

```typescript
import type { HotspotsOutput, FunctionReport } from '@hotspots/types';
import { filterByRiskBand, getHighestRiskFunctions } from '@hotspots/types';

const output: HotspotsOutput = JSON.parse(jsonOutput);
const highRisk = filterByRiskBand(output.functions, 'high');
```

#### Example Output Structure

```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123...",
    "parents": ["def456..."],
    "timestamp": 1234567890,
    "branch": "main"
  },
  "analysis": {
    "scope": "full",
    "tool_version": "1.0.0"
  },
  "functions": [
    {
      "function_id": "/path/to/file.ts::functionName",
      "file": "/path/to/file.ts",
      "line": 42,
      "metrics": {
        "cc": 8,
        "nd": 2,
        "fo": 4,
        "ns": 2
      },
      "lrs": 7.2,
      "band": "high"
    }
  ],
  "policy_results": {
    "failed": [],
    "warnings": []
  }
}
```

See [docs/json-schema.md](../docs/json-schema.md) for complete documentation and integration examples (TypeScript, Python, Go, Rust).

## Usage Examples

### Basic Usage

```yaml
- uses: ./action
```

### Custom Policy

```yaml
- uses: ./action
  with:
    policy: strict
    fail-on: warn
```

### With Config File

```yaml
- uses: ./action
  with:
    config: .hotspotsrc.json
```

### Monorepo Setup

```yaml
- uses: ./action
  with:
    path: packages/frontend
```

### Upload HTML Report

```yaml
- uses: ./action
  id: hotspots

- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report
    path: ${{ steps.hotspots.outputs.report-path }}
```

### Don't Fail Build (Warning Only)

```yaml
- uses: ./action
  with:
    fail-on: never
```

## How It Works

### PR Context (Delta Mode)

When run on a pull request:
1. Detects merge-base automatically
2. Analyzes only changed functions
3. Reports new violations or regressions
4. Posts results as PR comment
5. Updates existing comment on subsequent runs

### Push Context (Snapshot Mode)

When run on main branch:
1. Analyzes entire codebase
2. Reports all violations
3. Creates baseline snapshot
4. Shows job summary

## Output Examples

### PR Comment

```markdown
# Hotspots Analysis Results

**Mode:** Delta (PR analysis)

**Summary:** 2 error(s), 1 warning(s), 3 info

## ❌ Errors

| Function | File | LRS | Policy |
|----------|------|-----|--------|
| handleRequest | src/api.ts:45 | 9.2 | critical-introduction |
| processPayment | src/payment.ts:120 | 8.7 | critical-introduction |

## ⚠️ Warnings

| Function | File | LRS | Policy |
|----------|------|-----|--------|
| parseInput | src/parser.ts:78 | 5.8 | attention-threshold |

## 👀 Watch

3 function(s) approaching thresholds
```

### Job Summary

The action automatically posts results to the GitHub Actions job summary, visible in the workflow run details.

## Configuration

You can customize behavior with a `.hotspotsrc.json` file:

```json
{
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 2.0,
    "nd": 3.0,
    "fo": 2.5,
    "ns": 1.5
  },
  "exclude": [
    "**/*.test.ts",
    "**/*.spec.ts"
  ]
}
```

## Permissions

The action requires the following permissions:

```yaml
permissions:
  contents: read       # Required to checkout code
  pull-requests: write # Required to post PR comments
```

## Troubleshooting

### Binary Download Fails

If the action can't download the prebuilt binary, it will attempt to build from source. Ensure you have Rust/Cargo available:

```yaml
- uses: actions-rs/toolchain@v1
  with:
    toolchain: stable

- uses: ./action
```

### PR Comments Not Posting

Ensure `github-token` is provided and has `pull-requests: write` permission:

```yaml
- uses: ./action
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

### Delta Analysis Not Working

Ensure you're fetching full git history:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
```

## Development

### Building the Action

```bash
cd action
npm install
npm run build
```

### Testing Locally

```bash
npm run all  # Format, lint, and package
```

### Releasing

The action is bundled with `@vercel/ncc` to create a single `dist/index.js` file. After making changes:

```bash
npm run package
git add dist/
git commit -m "chore: rebuild action"
```

## Related

- [Hotspots CLI Documentation](../README.md)
- [Configuration Guide](../docs/configuration.md)
- [Metrics Rationale](../docs/metrics-rationale.md)

## License

MIT
