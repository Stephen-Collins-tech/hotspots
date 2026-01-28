# Faultline

Static analysis tool for TypeScript, JavaScript, and React that computes a Local Risk Score (LRS) based on control flow complexity metrics.

## Quickstart

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

## Requirements

- Rust 1.75 or later
- Cargo

## Installation

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

- `--format text|json`: Output format (default: text)
- `--top <N>`: Show only top N results by LRS
- `--min-lrs <float>`: Filter results by minimum LRS threshold

## Risk Bands

- **Low**: LRS < 3
- **Moderate**: 3 ≤ LRS < 6
- **High**: 6 ≤ LRS < 9
- **Critical**: LRS ≥ 9

## Supported TypeScript Features

See [docs/ts-support.md](docs/ts-support.md) for full details.

**Supported:**
- Function declarations, expressions, arrow functions
- Class methods, object literal methods
- All control flow constructs (if, loops, switch, try/catch/finally)

**Not Supported (MVP):**
- JSX/TSX syntax
- Generator functions (`function*`)
- Experimental decorators

## Known Limitations

- Break/continue statements route to exit (loop context tracking needed)
- No support for JSX/TSX
- Generator functions cause analysis errors
- Labeled break/continue partially supported

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

The comprehensive test suite validates all four phases:
- **Policy Engine**: Critical Introduction, Excessive Risk Regression, Net Repo Regression
- **Trend Semantics**: Risk Velocity, Hotspot Stability, Refactor Effectiveness  
- **Aggregation Views**: File and Directory aggregates
- **Output Formats**: JSON and Text formats

### Code Quality

```bash
# Format code
cargo fmt

# Lint code
cargo clippy
```

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.
