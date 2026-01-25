# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - 2025-01-25

### Added

#### Core Analysis Features
- **Local Risk Score (LRS)** computation for TypeScript functions
- Four complexity metrics: Cyclomatic Complexity (CC), Nesting Depth (ND), Fan-Out (FO), Non-Structured Exits (NS)
- Risk band classification: Low, Moderate, High, Critical
- Deterministic, byte-for-byte identical output for identical input

#### Analysis Modes
- **Snapshot Mode**: Capture codebase state at a point in time
- **Delta Mode**: Compare two snapshots to detect risk changes
- Git-native integration with automatic snapshot management

#### Policy Engine
- **Critical Introduction**: Block when functions become Critical risk
- **Excessive Risk Regression**: Block when LRS increases by ≥1.0
- **Net Repo Regression**: Warn when total repository risk increases
- CI/CD integration with exit code enforcement

#### Trend Analysis
- **Risk Velocity**: Track rate of LRS change over commit history
- **Hotspot Stability**: Identify stable vs. volatile high-risk functions
- **Refactor Effectiveness**: Detect and classify refactoring outcomes
- Configurable history window (default: 10 snapshots)

#### Aggregation Views
- File-level aggregates: sum LRS, max LRS, High+ count
- Directory-level aggregates with recursive rollup
- Delta aggregates: net LRS delta, regression count per file

#### Visualizations
- Interactive Vega-Lite charts showing risk evolution
- Risk concentration over time (stacked area chart)
- LRS timeline, band timeline, delta breakdown
- Repository distribution histogram
- Example code for generating visualization data

#### CLI Features
- Multiple output formats: text (default) and JSON
- Filtering options: `--top N`, `--min-lrs <threshold>`
- Git-based dynamic versioning
- Comprehensive help and usage documentation

### Technical
- TypeScript parsing via SWC
- Rust 1.75+ required
- MIT License
- Comprehensive test suite (46 unit tests + integration tests)
- Full documentation in `docs/`

### Repository Structure
- `faultline-core`: Core library for LRS computation
- `faultline-cli`: Command-line interface
- `examples/`: Example code (visualization export)
- `visualizations/`: Interactive charts and specs
- `docs/`: Comprehensive documentation

[0.0.1]: https://github.com/Stephen-Collins-tech/faultline/releases/tag/v0.0.1
