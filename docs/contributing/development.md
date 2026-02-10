# Development Setup

Complete guide to setting up your development environment for Hotspots.

## Prerequisites

### Required

- **Rust 1.75+** - Core implementation language
- **Git** - Version control
- **Cargo** - Comes with Rust

### Optional (for full development)

- **Node.js 18+** - For MCP server and GitHub Action development
- **npm 8+** - For JavaScript packages
- **jq** - For JSON validation and testing

## Quick Start

### 1. Clone Repository

```bash
# Fork the repository first (on GitHub)
# Then clone your fork
git clone https://github.com/YOUR_USERNAME/hotspots.git
cd hotspots
```

### 2. Install Rust (if needed)

```bash
# Install rustup (Rust version manager)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version  # Should be 1.75 or higher
cargo --version
```

### 3. Build the Project

```bash
# Build in debug mode (faster compilation)
cargo build

# Build in release mode (optimized, slower compilation)
cargo build --release
```

**Output:**
- Debug binary: `./target/debug/hotspots`
- Release binary: `./target/release/hotspots`

### 4. Run Tests

```bash
# Run all tests
cargo test

# Run specific package tests
cargo test --package hotspots-core
cargo test --package hotspots-cli

# Run with output visible
cargo test -- --nocapture
```

### 5. Run the CLI

```bash
# Using debug build
./target/debug/hotspots --help
./target/debug/hotspots analyze src/

# Using release build (faster)
./target/release/hotspots --help
./target/release/hotspots analyze src/
```

---

## Project Structure

```
hotspots/
├── hotspots-core/          # Core analysis library (Rust)
│   ├── src/
│   │   ├── language/      # Language parsers & CFG builders
│   │   │   ├── typescript/
│   │   │   ├── javascript/
│   │   │   ├── go/
│   │   │   ├── java/
│   │   │   ├── python/
│   │   │   └── rust/
│   │   ├── metrics.rs     # Metrics calculation (CC, ND, FO, NS)
│   │   ├── delta.rs       # Delta mode logic
│   │   ├── snapshot.rs    # Snapshot persistence
│   │   ├── policy.rs      # Policy evaluation
│   │   ├── config.rs      # Configuration loading
│   │   └── ...
│   └── tests/             # Integration & golden tests
│       ├── fixtures/      # Test code files
│       └── golden/        # Expected output files
│
├── hotspots-cli/           # CLI binary (Rust)
│   └── src/
│       └── main.rs        # CLI entry point, argument parsing
│
├── packages/               # TypeScript packages
│   ├── mcp-server/        # Model Context Protocol server
│   │   ├── src/
│   │   └── package.json
│   └── types/             # TypeScript type definitions
│       └── package.json
│
├── action/                 # GitHub Action
│   ├── action.yml
│   └── dist/
│
├── docs/                   # Documentation (VitePress)
│   ├── .vitepress/
│   ├── index.md
│   └── ...
│
├── examples/               # Example code and integrations
│   └── ai-agents/
│
├── tests/                  # Repository-level tests
│   └── fixtures/
│
├── Cargo.toml             # Workspace configuration
├── CLAUDE.md              # Coding conventions
├── CONTRIBUTING.md        # Contribution guide
└── README.md              # Project overview
```

---

## Development Workflow

### Making Changes

1. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** in the appropriate directory
   - Core logic: `hotspots-core/src/`
   - CLI: `hotspots-cli/src/`
   - MCP server: `packages/mcp-server/`
   - Docs: `docs/`

3. **Run formatting and linting:**
   ```bash
   cargo fmt
   cargo clippy
   ```

4. **Build and test:**
   ```bash
   cargo build
   cargo test
   ```

5. **Commit your changes:**
   ```bash
   git add .
   git commit -m "feat: add your feature description"
   ```

See [CLAUDE.md](../../CLAUDE.md) for commit message conventions.

### Testing Your Changes

#### Unit Tests

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test metrics
cargo test snapshot

# Run with output visible
cargo test test_name -- --nocapture

# Run tests for a specific crate
cargo test --package hotspots-core
```

#### Integration Tests

```bash
# Run integration tests
cargo test --test integration_tests

# Run golden tests (deterministic output verification)
cargo test --test golden_tests
```

#### Manual Testing

```bash
# Test on a real file
./target/debug/hotspots analyze tests/fixtures/typescript/simple.ts

# Test JSON output
./target/debug/hotspots analyze src/ --format json | jq .

# Test snapshot mode
./target/debug/hotspots analyze src/ --mode snapshot --format json

# Test delta mode
./target/debug/hotspots analyze src/ --mode delta --policy --format text
```

### Adding Test Fixtures

When adding new functionality:

1. **Create test fixture** in `tests/fixtures/<language>/`
   ```bash
   echo 'function test() { if (x) { return 1; } return 0; }' > tests/fixtures/typescript/new_test.ts
   ```

2. **Generate golden output:**
   ```bash
   cargo build --release
   ./target/release/hotspots analyze tests/fixtures/typescript/new_test.ts --format json > tests/golden/new_test.json
   ```

3. **Verify output manually** before committing

4. **Add golden test case** to `tests/golden_tests.rs`

---

## IDE Setup

### VS Code

**Recommended Extensions:**
- `rust-lang.rust-analyzer` - Rust language server
- `tamasfe.even-better-toml` - TOML syntax highlighting
- `vadimcn.vscode-lldb` - Debugger

**Settings (`.vscode/settings.json`):**
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

**Launch Configuration (`.vscode/launch.json`):**
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug hotspots",
      "cargo": {
        "args": ["build", "--bin=hotspots", "--package=hotspots-cli"]
      },
      "args": ["analyze", "tests/fixtures/typescript/simple.ts"],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

### Cursor

Cursor inherits VS Code configuration. Same extensions and settings work.

**Additional for AI-assisted development:**
- Use Hotspots MCP server for complexity analysis during development
- Configure `.cursorrules` for project-specific AI guidelines

### IntelliJ IDEA / CLion

**Recommended Plugins:**
- Rust (official JetBrains plugin)
- TOML

**Run Configuration:**
- Name: `Hotspots CLI`
- Command: `run`
- Arguments: `--bin hotspots -- analyze tests/fixtures/typescript/simple.ts`

---

## Building Components

### Core Library

```bash
# Build core library only
cargo build --package hotspots-core

# Run core tests only
cargo test --package hotspots-core

# Build with specific features (if any)
cargo build --package hotspots-core --features "feature-name"
```

### CLI Binary

```bash
# Build CLI only
cargo build --package hotspots-cli

# Run CLI directly with cargo
cargo run --package hotspots-cli -- analyze src/

# Install locally for testing
cargo install --path hotspots-cli
hotspots --version
```

### MCP Server (TypeScript)

```bash
# Navigate to MCP server directory
cd packages/mcp-server

# Install dependencies
npm install

# Build
npm run build

# Test locally
npm start

# Link for local development
npm link
```

### GitHub Action

```bash
cd action

# Install dependencies
npm install

# Build
npm run build

# Test locally (requires act)
act pull_request
```

---

## Debugging

### Rust Debugging with LLDB

```bash
# Debug with lldb
rust-lldb ./target/debug/hotspots

# In lldb:
(lldb) run analyze tests/fixtures/typescript/simple.ts
(lldb) breakpoint set --name analyze_with_config
(lldb) continue
```

### Print Debugging

```rust
// Use dbg! macro for quick debugging
dbg!(&some_variable);

// Use eprintln! to print to stderr (doesn't pollute stdout)
eprintln!("Debug: value = {}", value);
```

### Environment Variables

```bash
# Enable Rust backtrace
RUST_BACKTRACE=1 cargo test

# Full backtrace
RUST_BACKTRACE=full cargo test

# Enable logging (if using env_logger)
RUST_LOG=debug cargo run -- analyze src/
```

---

## Performance Profiling

### Cargo Flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bin hotspots -- analyze large-project/

# Output: flamegraph.svg
```

### Cargo Bench

```bash
# Run benchmarks (if configured)
cargo bench

# Run specific benchmark
cargo bench benchmark_name
```

### Manual Timing

```bash
# Use time command
time ./target/release/hotspots analyze src/

# Or hyperfine for statistical analysis
brew install hyperfine  # macOS
hyperfine './target/release/hotspots analyze src/'
```

---

## Common Tasks

### Update Dependencies

```bash
# Check for outdated dependencies
cargo outdated

# Update dependencies
cargo update

# Update specific dependency
cargo update -p tree-sitter
```

### Clean Build

```bash
# Remove build artifacts
cargo clean

# Rebuild from scratch
cargo build --release
```

### Generate Documentation

```bash
# Generate Rust docs
cargo doc --open

# Build VitePress docs
cd docs
npm install
npm run docs:dev  # Development server
npm run docs:build  # Production build
```

### Run Linters

```bash
# Format code
cargo fmt

# Check formatting without changing files
cargo fmt -- --check

# Run clippy (linter)
cargo clippy

# Clippy with all warnings
cargo clippy -- -W clippy::all

# Fix clippy suggestions automatically
cargo clippy --fix
```

---

## Troubleshooting

### "cargo: command not found"

**Solution:** Install Rust via rustup:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### "linker `cc` not found"

**Solution (macOS):**
```bash
xcode-select --install
```

**Solution (Linux):**
```bash
# Debian/Ubuntu
sudo apt-get install build-essential

# Fedora
sudo dnf install gcc
```

### "could not compile `tree-sitter-xyz`"

**Solution:** Update tree-sitter parsers:
```bash
cargo update
cargo clean
cargo build
```

### Tests Failing

```bash
# Run specific test to see output
cargo test test_name -- --nocapture

# Check if golden files are outdated
# Regenerate golden files if needed:
./target/release/hotspots analyze tests/fixtures/typescript/simple.ts --format json > tests/golden/simple.json
```

### Slow Compilation

**Solutions:**
- Use debug builds during development: `cargo build` (not `--release`)
- Enable incremental compilation (already default)
- Use `cargo check` instead of `cargo build` for quick error checking
- Consider using `sccache` for distributed compilation caching

```bash
# Quick error checking (no binary output)
cargo check

# Install sccache
cargo install sccache
export RUSTC_WRAPPER=sccache
```

---

## Continuous Integration

Our CI runs:

1. **Format Check:** `cargo fmt -- --check`
2. **Linting:** `cargo clippy -- -D warnings`
3. **Tests:** `cargo test`
4. **Build:** `cargo build --release`

**Run locally before pushing:**
```bash
cargo fmt -- --check && cargo clippy -- -D warnings && cargo test && cargo build --release
```

---

## Release Builds

For release builds with maximum optimization:

```bash
# Build optimized binary
cargo build --release

# Strip debug symbols (smaller binary)
strip ./target/release/hotspots

# Check binary size
ls -lh ./target/release/hotspots
```

---

## Related Documentation

- [Adding Language Support](./adding-languages.md) - Implement a new language parser
- [Architecture Overview](../architecture/overview.md) - System design and components
- [Testing Strategy](../architecture/testing.md) - Testing approach and patterns
- [Release Process](./releases.md) - How to create releases
- [CLAUDE.md](../../CLAUDE.md) - Coding conventions and rules

---

## Getting Help

- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 [Open an Issue](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 📖 [Documentation](https://docs.hotspots.dev)

---

**Ready to contribute?** Check out [good first issues](https://github.com/Stephen-Collins-tech/hotspots/labels/good%20first%20issue) to get started!
