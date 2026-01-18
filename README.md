# Faultline

Static analysis tool for TypeScript functions that computes a Local Risk Score (LRS) based on control flow complexity metrics.

## Quickstart

```bash
# Build the project
cargo build --release

# Analyze a TypeScript file
./target/release/faultline analyze src/main.ts

# Analyze a directory
./target/release/faultline analyze src/

# Output as JSON
./target/release/faultline analyze src/main.ts --format json

# Show only top 10 results
./target/release/faultline analyze src/ --top 10

# Filter by minimum LRS
./target/release/faultline analyze src/ --min-lrs 5.0
```

## What is LRS?

Local Risk Score (LRS) is a composite metric that measures the complexity and risk of individual TypeScript functions. It combines four metrics:

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

```bash
git clone <repo-url>
cd faultline
cargo build --release
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

```bash
# Run tests
cargo test

# Run integration tests
cargo test --test integration_tests

# Format code
cargo fmt

# Lint code
cargo clippy
```

## License

[Add license information]
