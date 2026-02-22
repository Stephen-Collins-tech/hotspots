# Contributing to Hotspots

Thank you for your interest in contributing to Hotspots! 🎉

## Quick Start

1. **Fork** the repository
2. **Clone** your fork: `git clone https://github.com/YOUR_USERNAME/hotspots.git`
3. **Set up** your development environment (see below)
4. **Create** a feature branch: `git checkout -b feature/your-feature-name`
5. **Make** your changes
6. **Test**: `cargo test`
7. **Commit**: Follow our [commit conventions](./CLAUDE.md)
8. **Push** and create a pull request

## Development Setup

### Prerequisites

- Rust 1.70+ (`rustup install stable`)
- Git
- Node.js 18+ (for MCP server and action)

### Build

```bash
# Clone the repository
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots

# Build the project
cargo build --release

# Install git hooks (runs fmt + clippy + tests before each commit)
make install-hooks

# Run tests
cargo test

# Run the CLI
./target/release/hotspots --help
```

See [Development Setup Guide](./docs/contributing/development.md) for detailed instructions.

## How to Contribute

### Reporting Bugs

- Use the [GitHub issue tracker](https://github.com/Stephen-Collins-tech/hotspots/issues)
- Search existing issues first
- Include:
  - Hotspots version (`hotspots --version`)
  - Operating system
  - Steps to reproduce
  - Expected vs. actual behavior

### Suggesting Features

- Open a [GitHub discussion](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- Describe the use case
- Explain why it would be useful

### Submitting Code

#### Code Quality

- Follow Rust conventions (run `cargo fmt` and `cargo clippy`)
- Write tests for new functionality
- Update documentation as needed
- Follow our [coding conventions](./CLAUDE.md)

#### Commit Messages

Use conventional commits format:

```
<type>: <description>

[optional body]

Co-Authored-By: Your Name <you@example.com>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

**Examples:**
```
feat: add Python language support

fix: correct cyclomatic complexity calculation for switch statements

docs: update installation instructions

refactor: extract metrics calculation into separate module
```

See our [CLAUDE.md](./CLAUDE.md) file for detailed commit guidelines.

#### Pull Request Process

1. Ensure all tests pass: `cargo test`
2. Update documentation if needed
3. Add yourself to Co-Authors in the commit
4. Request review from maintainers
5. Address review feedback
6. Once approved, maintainers will merge

### Adding Language Support

Want to add support for a new language? See our comprehensive guide:

📖 **[Adding Language Support Guide](./docs/contributing/adding-languages.md)**

This includes:
- Parser integration
- CFG (Control Flow Graph) builder
- Metrics extraction
- Test fixtures and golden files
- Documentation updates

## Project Structure

```
hotspots/
├── hotspots-core/      # Core analysis library
│   ├── src/
│   │   ├── language/  # Language parsers & CFG builders
│   │   ├── metrics.rs # Metrics calculation
│   │   ├── delta.rs   # Delta mode logic
│   │   └── ...
│   └── tests/         # Integration & unit tests
├── hotspots-cli/       # CLI binary
├── packages/           # TypeScript packages
│   ├── mcp-server/    # Model Context Protocol server
│   └── types/         # TypeScript type definitions
├── action/             # GitHub Action
├── docs/               # Documentation
└── examples/           # Example code
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run golden tests (deterministic output verification)
cargo test --test golden_tests

# Run integration tests (pytest suite)
make test-integration

# Run comprehensive tests (auto-detect pytest; falls back to legacy script)
make test-comprehensive
```

### Integration Tests

- Location: `integration/` (pytest-based E2E tests) and `integration/legacy/` (fallback script).
- Entry points:
  - `make test-integration` — runs `pytest -q integration`.
  - `make test-comprehensive` — runs pytest if available, else `python3 integration/legacy/test_comprehensive.py`.
- CI runs `make test-comprehensive` and uploads artifacts from `test-repo-comprehensive/`.

### Golden Files

- No manual path fixing needed. Golden tests normalize file paths at assertion time for cross-platform consistency.

## Documentation

- All docs are in `docs/` directory
- Documentation powers [docs.hotspots.dev](https://docs.hotspots.dev)
- Use markdown with frontmatter for metadata
- Test docs locally before submitting

## Code of Conduct

- Be respectful and inclusive
- Provide constructive feedback
- Help others learn and grow
- Follow GitHub's [Community Guidelines](https://docs.github.com/en/site-policy/github-terms/github-community-guidelines)

## Recognition

Contributors are recognized in:
- Git commit history (Co-Authored-By)
- Release notes (CHANGELOG.md)
- GitHub contributors page

## Questions?

- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 [Open an Issue](https://github.com/Stephen-Collins-tech/hotspots/issues)

## Detailed Guides

For more detailed information, see:

- [Development Setup](./docs/contributing/development.md) - Detailed dev environment setup
- [Adding Languages](./docs/contributing/adding-languages.md) - Language implementation guide
- [Release Process](./docs/contributing/releases.md) - How releases are created
- [Architecture](./docs/architecture/overview.md) - System architecture overview

---

**Thank you for contributing!** 🚀
