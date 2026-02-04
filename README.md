# Faultline

Static analysis tool for TypeScript, JavaScript, and React that computes a Local Risk Score (LRS) based on control flow complexity metrics.

## Quickstart

### GitHub Action (Recommended for CI/CD)

Add to `.github/workflows/faultline.yml`:

```yaml
name: Faultline

on: [pull_request, push]

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for delta analysis

      - uses: yourorg/faultline@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

See [action/README.md](action/README.md) for full GitHub Action documentation.

### CLI Usage

```bash
# Build the project
cargo build --release

# Analyze TypeScript, JavaScript, or React files
./target/release/faultline analyze src/main.ts
./target/release/faultline analyze src/app.js
./target/release/faultline analyze src/Component.tsx
./target/release/faultline analyze src/Button.jsx

# Analyze a directory (all .ts, .js, .tsx, .jsx files)
./target/release/faultline analyze src/

# Output as JSON
./target/release/faultline analyze src/main.ts --format json

# Show only top 10 results
./target/release/faultline analyze src/ --top 10

# Filter by minimum LRS
./target/release/faultline analyze src/ --min-lrs 5.0
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

Faultline includes 7 built-in policies to enforce code quality:

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
// faultline-ignore: legacy code, refactor planned for Q2 2026
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

Customize behavior with `.faultlinerc.json`:

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

Add Faultline to your GitHub Actions workflow:

```yaml
- uses: yourorg/faultline@v1
```

See [action/README.md](action/README.md) for configuration options.

### Binary Releases

Download prebuilt binaries from [GitHub Releases](https://github.com/yourorg/faultline/releases):

```bash
# Linux
wget https://github.com/yourorg/faultline/releases/latest/download/faultline-linux-x64.tar.gz
tar -xzf faultline-linux-x64.tar.gz
sudo mv faultline /usr/local/bin/

# macOS (Intel)
wget https://github.com/yourorg/faultline/releases/latest/download/faultline-darwin-x64.tar.gz
tar -xzf faultline-darwin-x64.tar.gz
sudo mv faultline /usr/local/bin/

# macOS (Apple Silicon)
wget https://github.com/yourorg/faultline/releases/latest/download/faultline-darwin-arm64.tar.gz
tar -xzf faultline-darwin-arm64.tar.gz
sudo mv faultline /usr/local/bin/
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
git clone https://github.com/Stephen-Collins-tech/faultline
cd faultline
./install-dev.sh
# or
make install
```

This will:
- Build the release binary
- Install it to `~/.local/bin/faultline`
- Make it available globally (if `~/.local/bin` is in your PATH)

**Note:** If `~/.local/bin` is not in your PATH, add this to your shell config:
```bash
export PATH="${HOME}/.local/bin:${PATH}"
```

### Manual Build

```bash
git clone https://github.com/Stephen-Collins-tech/faultline
cd faultline
cargo build --release
# Binary will be at ./target/release/faultline
```

## Usage

### Basic Analysis

```bash
./target/release/faultline analyze path/to/file.ts
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
./target/release/faultline analyze path/to/file.ts --format json
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

**Supported languages:** TypeScript, JavaScript, JSX, TSX

**Supported file extensions:** `.ts`, `.tsx`, `.js`, `.jsx`, `.mts`, `.cts`, `.mjs`, `.cjs`

**Supported constructs:**
- Function declarations, expressions, arrow functions
- Class methods, object literal methods
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

Faultline produces **byte-for-byte identical output** for identical input:

- Function order is deterministic (sorted by span start)
- File order is deterministic (sorted by path)
- Output format is stable (JSON key order, float precision)
- Whitespace and comments do not affect results

This makes faultline suitable for CI/CD integration and regression testing.

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

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.
