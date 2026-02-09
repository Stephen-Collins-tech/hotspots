# Hotspots Documentation

**Multi-language complexity analysis for high-leverage refactoring**

Hotspots identifies high-risk, high-complexity code that changes frequently - your true refactoring priorities.

## Quick Links

- 🚀 [Get Started](./getting-started/quick-start.md) - 5-minute introduction
- 📖 [User Guide](./guide/usage.md) - Complete CLI reference
- 📚 [API Reference](./reference/metrics.md) - Technical specifications
- 🏗️ [Architecture](./architecture/overview.md) - System design
- 🤝 [Contributing](./contributing/index.md) - How to contribute

## What is Hotspots?

Hotspots analyzes your codebase to find functions that are:
- **High complexity** - Hard to understand and maintain
- **Frequently changed** - Modified often in git history
- **High risk** - Combination of complexity and change frequency

This helps you prioritize refactoring efforts on code that actually matters.

## Supported Languages

- TypeScript (.ts)
- JavaScript (.js, .mjs, .cjs)
- JSX (.jsx)
- TSX (.tsx)
- Go (.go)
- Python (.py)
- Rust (.rs)
- Java (.java)

## Key Features

- **📊 Multiple Metrics** - Cyclomatic Complexity, Nesting Depth, Fan-Out, Non-Structured Exits
- **📈 Git History Analysis** - Track complexity changes over time
- **🎯 Risk Scoring** - Leverage Risk Score (LRS) combines complexity + change frequency
- **🚦 Policy Engine** - Block risky changes in CI/CD
- **🔇 Suppression Comments** - Handle false positives gracefully
- **📝 Multiple Outputs** - JSON, HTML, and text formats
- **🤖 AI Integration** - MCP server for AI tools
- **⚡ Fast** - Rust-based analysis engine

## Getting Started

### Installation

```bash
# Install from GitHub releases
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-$(uname -s)-$(uname -m) -o hotspots
chmod +x hotspots
mv hotspots /usr/local/bin/
```

### Quick Start

```bash
# Analyze a directory
hotspots analyze src/

# Track changes over time (snapshot mode)
hotspots analyze src/ --mode snapshot

# Compare with baseline (delta mode)
hotspots analyze src/ --mode delta --policy

# View trends
hotspots trends src/
```

See the [Quick Start Guide](./getting-started/quick-start.md) for more.

## Documentation Sections

### 🚀 Getting Started

Perfect for new users:

- [Installation](./getting-started/installation.md) - Install Hotspots
- [Quick Start](./getting-started/quick-start.md) - 5-minute tutorial
- [React Projects](./getting-started/quick-start-react.md) - React-specific guide

### 📖 User Guide

Learn how to use Hotspots:

- [CLI Usage](./guide/usage.md) - Commands and options
- [Configuration](./guide/configuration.md) - Config files
- [CI Integration](./guide/ci-integration.md) - Use in CI/CD
- [GitHub Action](./guide/github-action.md) - GitHub Actions
- [Suppression](./guide/suppression.md) - Suppress warnings
- [Output Formats](./guide/output-formats.md) - JSON, HTML, text

### 📚 Reference

Technical documentation:

- [Metrics](./reference/metrics.md) - How metrics are calculated
- [LRS Specification](./reference/lrs-spec.md) - Risk score formula
- [CLI Reference](./reference/cli.md) - Complete CLI reference
- [JSON Schema](./reference/json-schema.md) - Output schemas
- [Language Support](./reference/language-support.md) - Language features
- [Limitations](./reference/limitations.md) - Known limitations

### 🏗️ Architecture

For contributors:

- [Overview](./architecture/overview.md) - System design
- [Design Decisions](./architecture/design-decisions.md) - Key choices
- [Invariants](./architecture/invariants.md) - System guarantees
- [Multi-Language](./architecture/multi-language.md) - Language support architecture
- [Testing](./architecture/testing.md) - Testing strategy

### 🤝 Contributing

Want to help?

- [Contributing Guide](./contributing/index.md) - How to contribute
- [Development Setup](./contributing/development.md) - Dev environment
- [Adding Languages](./contributing/adding-languages.md) - Add language support
- [Release Process](./contributing/releases.md) - How releases work

### 🔌 Integrations

Use with other tools:

- [MCP Server](./integrations/mcp-server.md) - Model Context Protocol
- [AI Agents](./integrations/ai-agents.md) - AI agent examples
- [API](./integrations/api.md) - Programmatic API

## Use Cases

**Find Technical Debt Hotspots**
```bash
hotspots analyze src/ --mode snapshot
```

**Block Risky Changes in CI**
```bash
hotspots analyze src/ --mode delta --policy --fail-on blocking
```

**Track Refactoring Progress**
```bash
hotspots trends src/ --window 10
```

**Generate HTML Reports**
```bash
hotspots analyze src/ --format html --output report.html
```

## Community

- 🐛 [Report Bugs](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 💬 [Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 Support: [GitHub Issues](https://github.com/Stephen-Collins-tech/hotspots/issues)

## License

MIT License - See [LICENSE](../LICENSE) for details.

---

**Ready to get started?** → [Installation Guide](./getting-started/installation.md)
