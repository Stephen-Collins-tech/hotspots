# Contributing to Hotspots

Thank you for your interest in contributing! This guide covers everything you need to get started.

## Prerequisites

- **Rust 1.75+** — Core implementation language (`rustc --version`)
- **Git**
- **Cargo** — Comes with Rust

Optional for full development:
- **Node.js 18+** — For MCP server and GitHub Action development
- **jq** — For JSON validation and testing

## Quick Setup

```bash
# 1. Fork the repository on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/hotspots.git
cd hotspots

# 2. Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3. Build
cargo build

# 4. Run tests
cargo test

# 5. Verify the CLI works
./target/debug/hotspots analyze tests/fixtures/typescript/simple.ts
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
│   │   ├── metrics.rs     # CC, ND, FO, NS calculation
│   │   ├── delta.rs       # Delta mode logic
│   │   ├── snapshot.rs    # Snapshot persistence
│   │   ├── policy.rs      # Policy evaluation
│   │   └── config.rs      # Configuration loading
│   └── tests/
│       ├── fixtures/      # Test code files
│       └── golden/        # Expected output files
│
├── hotspots-cli/           # CLI binary (Rust)
│   └── src/main.rs        # CLI entry point
│
├── packages/               # TypeScript packages
│   ├── mcp-server/        # Model Context Protocol server
│   └── types/             # TypeScript type definitions
│
├── action/                 # GitHub Action
├── docs/                   # VitePress documentation
├── Cargo.toml              # Workspace configuration
├── CLAUDE.md               # Coding conventions
└── CONTRIBUTING.md         # Contribution guide
```

---

## Development Workflow

### Making Changes

```bash
# 1. Create a feature branch
git checkout -b feature/your-feature-name

# 2. Make changes in the appropriate directory

# 3. Check formatting and linting
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# 4. Build and test
cargo build
cargo test

# 5. Commit (see commit conventions below)
git add .
git commit -m "feat: your feature description"
```

### Commit Conventions

All commit messages must be a single line, under 72 characters:

```
<type>: <concise description>
```

**Types:** `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

**Examples:**
```
feat: add suppression comments support
fix: correct ND calculation for try blocks
docs: update quick start for React projects
```

Never include a multi-paragraph body unless explicitly requested.

### Before Committing

Run these checks — they are enforced by pre-commit hooks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

---

## Running Tests

```bash
# All tests
cargo test

# Specific package
cargo test --package hotspots-core

# With visible output
cargo test -- --nocapture

# Golden tests (deterministic output verification)
cargo test --test golden_tests

# Integration tests
cargo test --test integration_tests
```

### Adding Test Fixtures

When adding new functionality:

```bash
# 1. Create a test fixture
echo 'function test() { if (x) { return 1; } return 0; }' \
  > tests/fixtures/typescript/new_test.ts

# 2. Generate golden output
cargo build --release
./target/release/hotspots analyze tests/fixtures/typescript/new_test.ts \
  --format json > tests/golden/new_test.json

# 3. Verify output manually, then add a golden test case in tests/golden_tests.rs
```

---

## Adding a Language

Adding a new language parser involves:

1. **Create `hotspots-core/src/language/<lang>/`** with:
   - `parser.rs` — Tree-sitter or syn-based parser
   - `cfg_builder.rs` — Control Flow Graph builder

2. **Register in `hotspots-core/src/metrics.rs`** — add an `extract_<lang>_metrics()` function

3. **Add file extensions** to the language detection logic

4. **Add test fixtures** in `tests/fixtures/<lang>/` with golden files

5. **Implement metric semantics** consistent with existing languages:
   - CC via CFG formula, with language-specific increments (switch cases, catch clauses, boolean ops)
   - ND as maximum depth of control structures
   - FO as distinct function calls (deduplicated)
   - NS as early exits excluding final tail return

See `hotspots-core/src/language/go/` for a well-documented example. Full guide: [adding-languages.md](./adding-languages.md).

---

## CI

Our CI runs on every PR:

1. `cargo fmt --all -- --check` — Format check
2. `cargo clippy --all-targets --all-features -- -D warnings` — Linting
3. `cargo test` — Full test suite
4. `cargo build --release` — Release build

Run locally before pushing:
```bash
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test && \
cargo build --release
```

---

## Release Checklist

When cutting a release:

- [ ] Update version in `Cargo.toml`
- [ ] Update version in `action/package.json`
- [ ] Update `CHANGELOG.md` with release notes
- [ ] Run full CI checks locally
- [ ] Tag the release: `git tag v1.x.x`
- [ ] Build binaries for all platforms (Linux x86_64, macOS x86_64, macOS ARM64, Windows x86_64)
- [ ] Create GitHub release with binaries attached
- [ ] Test the GitHub Action with the new release tag
- [ ] Update documentation if needed

**Building release binaries:**

```bash
# macOS ARM64 (Apple Silicon)
cargo build --release --target aarch64-apple-darwin
tar -czf hotspots-darwin-aarch64.tar.gz -C target/aarch64-apple-darwin/release hotspots

# Linux x86_64
cargo build --release --target x86_64-unknown-linux-gnu
tar -czf hotspots-linux-x86_64.tar.gz -C target/x86_64-unknown-linux-gnu/release hotspots
```

Full release process: [releases.md](./releases.md).

---

## Getting Help

- [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions) — Questions and ideas
- [Open an Issue](https://github.com/Stephen-Collins-tech/hotspots/issues) — Bug reports and feature requests
- [Good First Issues](https://github.com/Stephen-Collins-tech/hotspots/labels/good%20first%20issue) — Start here

See also: [CLAUDE.md](../../CLAUDE.md) for detailed coding conventions, [CONTRIBUTING.md](../../CONTRIBUTING.md) in the repository root.
