# Hotspots

Static analysis tool for TypeScript, JavaScript, React, and Go that computes a Local Risk Score (LRS) based on control flow complexity metrics.

## Quickstart

### GitHub Action (Recommended for CI/CD)

Add to `.github/workflows/hotspots.yml`:

```yaml
name: Hotspots

on: [pull_request, push]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for delta analysis

      - uses: yourorg/hotspots@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [action/README.md](action/README.md) for full GitHub Action documentation.

### CLI Usage

```bash
# Build the project
cargo build --release

# Analyze TypeScript, JavaScript, React, or Go files
./target/release/hotspots analyze src/main.ts
./target/release/hotspots analyze src/app.js
./target/release/hotspots analyze main.go
./target/release/hotspots analyze src/Component.tsx
./target/release/hotspots analyze src/Button.jsx

# Analyze a directory (all .ts, .js, .tsx, .jsx files)
./target/release/hotspots analyze src/

# Output as JSON
./target/release/hotspots analyze src/main.ts --format json

# Show only top 10 results
./target/release/hotspots analyze src/ --top 10

# Filter by minimum LRS
./target/release/hotspots analyze src/ --min-lrs 5.0
```

## What is LRS?

Local Risk Score (LRS) is a composite metric that measures the complexity and risk of individual functions. It combines four metrics:

- **Cyclomatic Complexity (CC)**: Measures the number of linearly independent paths through a function
- **Nesting Depth (ND)**: Measures the maximum depth of nested control structures
- **Fan-Out (FO)**: Measures the number of distinct functions called
- **Non-Structured Exits (NS)**: Measures early returns, breaks, continues, and throws

Each metric is transformed to a risk component (R_cc, R_nd, R_fo, R_ns) and weighted to produce the final LRS.

See [docs/lrs-spec.md](docs/lrs-spec.md) for full details.

## Features

### 🤖 Built for AI Coding Assistants

Hotspots is designed from day one for AI-assisted development. Deterministic, machine-readable complexity analysis that AI agents can use to review code, guide refactoring, and generate better code.

- ✅ **Claude MCP Server** - Direct tool access in Claude Desktop/Code
- ✅ **Structured JSON** - Machine-readable output with TypeScript types
- ✅ **Deterministic** - Same code always produces identical results
- ✅ **Fast Execution** - Suitable for iterative AI workflows

**Quick Start with Claude:**

```bash
npm install -g @hotspots/mcp-server
```

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "hotspots": {
      "command": "npx",
      "args": ["@hotspots/mcp-server"]
    }
  }
}
```

Then ask Claude: *"Analyze my codebase for complexity"*

See [docs/AI_INTEGRATION.md](docs/AI_INTEGRATION.md) for complete AI integration guide with workflows for Claude, GPT-4, Cursor, and Copilot.

### CI/CD Integration

- **GitHub Action**: Zero-config integration for pull requests and CI pipelines ([docs](action/README.md))
  - Automatic PR/push detection and delta analysis
  - PR comments with results and violations
  - HTML report artifacts
  - Job summaries in workflow runs
- **Policy Engine**: Automated quality gates for complexity regressions
  - Block critical function introductions
  - Detect excessive risk regressions (+1.0 LRS threshold)
  - Proactive warnings for functions entering high-risk zones
- **Git History Tracking**: Snapshot and delta modes for commit-to-commit comparison
- **HTML Reports**: Interactive, sortable, filterable reports
- **Configuration Files**: Project-specific thresholds, weights, and file patterns

### Policy Enforcement

Hotspots includes 7 built-in policies to enforce code quality:

**Blocking (fail CI):**
- Critical Introduction - Functions entering critical risk band
- Excessive Risk Regression - LRS increases ≥1.0

**Warnings:**
- Watch Threshold - Functions approaching moderate complexity
- Attention Threshold - Functions approaching critical complexity
- Rapid Growth - Functions with >50% LRS increase
- Suppression Missing Reason - Undocumented suppressions
- Net Repo Regression - Overall repository complexity increase

See [docs/USAGE.md#policy-engine](docs/USAGE.md#policy-engine) for details.

### Suppression Comments

Suppress policy violations for specific functions while keeping them in reports:

```typescript
// hotspots-ignore: legacy code, refactor planned for Q2 2026
function complexLegacyParser(input: string) {
  // High complexity code...
}
```

Functions with suppression comments:
- Excluded from policy failures (Critical Introduction, Excessive Risk Regression, warnings)
- Included in all reports with `suppression_reason` field
- Included in repository-level metrics
- Validated for missing reasons

See [docs/USAGE.md#suppressing-policy-violations](docs/USAGE.md#suppressing-policy-violations) for details.

### Configuration

Customize behavior with `.hotspotsrc.json`:

```json
{
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "warnings": {
    "watch": { "min": 4.0, "max": 6.0 },
    "attention": { "min": 6.0, "max": 9.0 },
    "rapid_growth_percent": 50.0
  },
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts"]
}
```

See [docs/USAGE.md#configuration](docs/USAGE.md#configuration) for details.

## Requirements

- Rust 1.75 or later
- Cargo

## Installation

### GitHub Action (Recommended)

Add Hotspots to your GitHub Actions workflow:

```yaml
- uses: yourorg/hotspots@v1
```

See [action/README.md](action/README.md) for configuration options.

### Binary Releases

Download prebuilt binaries from [GitHub Releases](https://github.com/yourorg/hotspots/releases):

```bash
# Linux
wget https://github.com/yourorg/hotspots/releases/latest/download/hotspots-linux-x64.tar.gz
tar -xzf hotspots-linux-x64.tar.gz
sudo mv hotspots /usr/local/bin/

# macOS (Intel)
wget https://github.com/yourorg/hotspots/releases/latest/download/hotspots-darwin-x64.tar.gz
tar -xzf hotspots-darwin-x64.tar.gz
sudo mv hotspots /usr/local/bin/

# macOS (Apple Silicon)
wget https://github.com/yourorg/hotspots/releases/latest/download/hotspots-darwin-arm64.tar.gz
tar -xzf hotspots-darwin-arm64.tar.gz
sudo mv hotspots /usr/local/bin/
```

### Local Development (No Installation)

For development, use the `dev` script which runs `cargo run`:

```bash
./dev --version
./dev analyze src/main.ts
./dev analyze src/ --format json
```

This runs the tool directly without installing it globally. Useful for:
- Quick testing during development
- Avoiding PATH configuration
- Always using the latest code

### Development Installation (Global)

Install to your local bin directory (`~/.local/bin`):

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots
cd hotspots
./install-dev.sh
# or
make install
```

This will:
- Build the release binary
- Install it to `~/.local/bin/hotspots`
- Make it available globally (if `~/.local/bin` is in your PATH)

**Note:** If `~/.local/bin` is not in your PATH, add this to your shell config:
```bash
export PATH="${HOME}/.local/bin:${PATH}"
```

### Manual Build

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots
cd hotspots
cargo build --release
# Binary will be at ./target/release/hotspots
```

## Usage

### Basic Analysis

```bash
./target/release/hotspots analyze path/to/file.ts
```

### Output Formats

**Text format (default):**
```
LRS     File              Line  Function
11.2    src/api.ts        88    handleRequest
9.8     src/db/migrate.ts 41    runMigration
```

**JSON format:**
```bash
./target/release/hotspots analyze path/to/file.ts --format json
```

```json
[
  {
    "file": "src/api.ts",
    "function": "handleRequest",
    "line": 88,
    "metrics": {
      "cc": 15,
      "nd": 4,
      "fo": 8,
      "ns": 3
    },
    "risk": {
      "r_cc": 4.0,
      "r_nd": 4.0,
      "r_fo": 3.0,
      "r_ns": 3.0
    },
    "lrs": 11.2,
    "band": "high"
  }
]
```

### Options

**Output:**
- `--format text|json|html`: Output format (default: text)
- `--top <N>`: Show only top N results by LRS
- `--min-lrs <float>`: Filter results by minimum LRS threshold

**Modes:**
- `--mode snapshot|delta`: Create snapshot or compute delta vs parent commit
- `--policies`: Enable policy evaluation (delta mode only)

**Configuration:**
- `--config <path>`: Path to configuration file

See [docs/USAGE.md](docs/USAGE.md) for complete documentation.

## Risk Bands

- **Low**: LRS < 3
- **Moderate**: 3 ≤ LRS < 6
- **High**: 6 ≤ LRS < 9
- **Critical**: LRS ≥ 9

## Language Support

See [docs/language-support.md](docs/language-support.md) for full details.

**Supported languages:**
- **ECMAScript:** TypeScript, JavaScript, JSX, TSX
- **Go:** Full Go language support

**Supported file extensions:**
- **ECMAScript:** `.ts`, `.tsx`, `.js`, `.jsx`, `.mts`, `.cts`, `.mjs`, `.cjs`
- **Go:** `.go`

**Supported constructs:**
- **ECMAScript:** Function declarations, expressions, arrow functions, class methods, object literal methods
- **Go:** Functions, methods, control flow, defer, goroutines, select statements
- All control flow constructs (if, loops, switch, try/catch/finally)
- JSX/TSX elements (elements don't inflate complexity; embedded control flow is counted)
- Labeled break/continue with correct loop targeting

**Not supported:**
- Generator functions (`function*`)
- Async/await CFG modeling
- Experimental decorators

## Known Limitations

- Generator functions cause analysis errors
- Async/await not modeled in control flow (treated as regular statements)

See [docs/limitations.md](docs/limitations.md) for full details.

## Determinism

Hotspots produces **byte-for-byte identical output** for identical input:

- Function order is deterministic (sorted by span start)
- File order is deterministic (sorted by path)
- Output format is stable (JSON key order, float precision)
- Whitespace and comments do not affect results

This makes hotspots suitable for CI/CD integration and regression testing.

## Development

### Testing

**Unit Tests:**
```bash
cargo test
```

**Comprehensive Integration Tests:**
```bash
# Using Python script directly
python3 test_comprehensive.py

# Or using Make
make test-comprehensive

# Or using shell script
./scripts/run-tests.sh comprehensive

# Run all tests (unit + comprehensive)
make test-all
# or
./scripts/run-tests.sh all
```

The comprehensive test suite validates all phases:
- **Policy Engine**: 7 built-in policies (blocking + warnings)
- **Suppression Comments**: Comment extraction and policy filtering
- **Trend Semantics**: Risk Velocity, Hotspot Stability, Refactor Effectiveness
- **Aggregation Views**: File and Directory aggregates
- **Output Formats**: JSON, Text, and HTML formats

### Code Quality

```bash
# Format code
cargo fmt

# Lint code
cargo clippy
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for future plans, including:
- Multi-language support (Python, Rust, Go)
- Enterprise features
- IDE integrations
- Ecosystem growth

Quick reference: [ROADMAP_SUMMARY.md](ROADMAP_SUMMARY.md)

## Contributing

Contributions welcome! See open issues or propose new features.

For major changes, please review the [ROADMAP.md](ROADMAP.md) first to ensure alignment with project direction.

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.
