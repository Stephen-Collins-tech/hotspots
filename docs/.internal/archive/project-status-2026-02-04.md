# Hotspots - Project Status Summary

**Last Updated:** 2026-02-06
**Branch:** `feature/ai-first-integration`
**Version:** 0.0.1 (pre-release)

## 🎯 Project Overview

**Hotspots** is a static analysis tool for TypeScript/JavaScript that measures code complexity and blocks regressions in CI/CD pipelines. Built with AI-assisted development in mind from day one.

- **Domain:** hotspots.dev
- **Binary:** `hotspots`
- **Packages:** `@hotspots/types`, `@hotspots/mcp-server`
- **GitHub:** https://github.com/Stephen-Collins-tech/hotspots

## 📊 Current Status

**Overall Progress:** 13/30 tasks completed (43%)

**Active Phase:** Phase 7 - AI-First Integration ✅ COMPLETED

**Recent Work (2026-02-06):**
- ✅ Renamed entire project from "faultline" to "hotspots"
- ✅ Fixed all clippy warnings (Task 7.1)
- ✅ Created JSON schemas and TypeScript types (Task 7.2)
- ✅ Built Claude MCP server (Task 7.3)
- ✅ Created AI integration documentation (Task 7.4)
- ✅ Built reference implementation examples (Task 7.5)

## 🏗️ Architecture

```
hotspots/
├── hotspots-core/          # Core analysis library (Rust)
├── hotspots-cli/           # CLI binary (Rust)
├── action/                 # GitHub Action (TypeScript)
├── packages/
│   ├── types/              # @hotspots/types (TypeScript definitions)
│   └── mcp-server/         # @hotspots/mcp-server (Claude integration)
├── schemas/                # JSON Schema definitions
├── docs/                   # Documentation
└── tests/                  # Test fixtures
```

### Core Components

**Rust Workspace:**
- `hotspots-core`: Analysis engine, CFG builder, metrics calculation
- `hotspots-cli`: Command-line interface with JSON/HTML/text output

**TypeScript Packages:**
- `@hotspots/types`: Type definitions, type guards, helper functions
- `@hotspots/mcp-server`: Model Context Protocol server for Claude Desktop/Code

**GitHub Action:**
- PR-aware delta analysis
- HTML report generation
- Automated PR comments
- Policy enforcement

## ✅ Completed Features

### Phase 1: Language Completeness
- JavaScript support (.js, .mjs, .cjs)
- JSX/TSX support (React)
- Break/continue CFG routing (correctness fix)

### Phase 2: CI/CD Integration
- GitHub Action with binary caching
- HTML report generation (interactive, responsive)
- Proactive warning system (Watch, Attention, Action Required)
- PR comment posting with delta analysis

### Phase 3: Configuration & Policies
- JSON configuration file support (.hotspotsrc.json)
- Include/exclude patterns (glob-based)
- Custom thresholds and weights
- Suppression comments (`// hotspots-ignore: reason`)

### Phase 7: AI-First Integration ✅ COMPLETE
- **Task 7.1 ✅** - All clippy warnings fixed
- **Task 7.2 ✅** - JSON schemas and TypeScript types
- **Task 7.3 ✅** - Claude MCP server with `hotspots_analyze` tool
- **Task 7.4 ✅** - AI integration documentation (docs/AI_INTEGRATION.md)
- **Task 7.5 ✅** - Reference implementation examples (4 working examples)

## 🔧 Key Features

### Complexity Metrics
- **CC** (Cyclomatic Complexity) - Decision points
- **ND** (Nesting Depth) - Maximum nesting level
- **FO** (Fan-Out) - Number of called functions
- **NS** (Non-Structured exits) - Early returns, throws, etc.
- **LRS** (Logarithmic Risk Score) - Composite metric: `ln(CC+1) + ln(ND+1) + ln(FO+1) + ln(NS+1)`

### Risk Bands
- **Low:** LRS < 3.0
- **Moderate:** LRS 3.0-6.0
- **High:** LRS 6.0-9.0
- **Critical:** LRS ≥ 9.0

### Output Formats
- **Text:** Human-readable tables
- **JSON:** Structured, machine-readable (with JSON Schema)
- **HTML:** Interactive reports with sorting/filtering

### Analysis Modes
- **Snapshot:** Full codebase analysis
- **Delta:** Changed functions only (git-based)

## 📦 Package Status

### @hotspots/types (v1.0.0)
**Status:** Built, not yet published to npm

**Contents:**
- Complete TypeScript type definitions
- Type guards: `isHotspotsOutput()`, `isFunctionReport()`, `isPolicyResult()`
- Helper functions: `filterByRiskBand()`, `getHighestRiskFunctions()`, `policyPassed()`
- Comprehensive JSDoc with examples

**Location:** `packages/types/`

### @hotspots/mcp-server (v1.0.0)
**Status:** Built and tested, not yet published to npm

**Contents:**
- MCP server for Claude Desktop/Code integration
- `hotspots_analyze` tool with full parameter support
- Human-readable summaries with risk breakdowns
- Environment variable support (HOTSPOTS_PATH)

**Location:** `packages/mcp-server/`

## 📚 Documentation

### Created
- ✅ `docs/json-schema.md` - Comprehensive JSON output format guide
- ✅ `schemas/*.schema.json` - 4 JSON Schema files (Draft 07)
- ✅ `packages/types/README.md` - TypeScript types usage guide
- ✅ `packages/mcp-server/README.md` - MCP server setup and usage
- ✅ `action/README.md` - GitHub Action documentation (updated with JSON output section)

### Existing
- `README.md` - Main project documentation
- `TASKS.md` - Detailed task breakdown and progress tracking
- `CLAUDE.md` - Development conventions and rules
- `docs/USAGE.md` - CLI usage guide
- `RELEASE_PROCESS.md` - Release and publishing workflow

## 🚧 In Progress / TODO

### Next Priority
- **Task 2.4** - GitHub PR Annotations (P1)
- **Task 3.3** - Enhanced Policy Engine (P2)
- **Phase 4** - Performance optimizations
- **Phase 5** - Developer experience improvements
- **Phase 6** - Polish and additional documentation

## 🔨 Build & Test Status

### Rust
```bash
cargo clippy --all-targets --all-features    # ✅ 0 warnings
cargo test --workspace                        # ✅ 145 tests passing
cargo build --release                         # ✅ Builds successfully
```

**Binary Location:** `target/release/hotspots` (4.8MB)

### TypeScript
```bash
# @hotspots/types
cd packages/types && npm run build           # ✅ Compiles successfully

# @hotspots/mcp-server
cd packages/mcp-server && npm run build      # ✅ Compiles successfully
cd packages/mcp-server && node test-analyze.js  # ✅ Tests pass
```

### GitHub Action
```bash
cd action && npm run build                    # ✅ Builds dist/index.js
```

## 📝 Recent Commits

```
d983890 docs: mark Tasks 7.2 and 7.3 as complete, update progress to 37%
08fe30d chore: add .faultline to gitignore, add MCP tests, update TASKS.md
865e802 feat: add MCP server for Claude integration (Task 7.3)
3ff8d4e refactor: rename faultline to hotspots across entire codebase
e656958 feat: add JSON schemas and TypeScript types for AI (Task 7.2)
7155093 fix: fix clippy errors, extend TASKS.md
```

## 🎯 Next Steps

### Immediate (Before v1.0.0)
1. **Publish packages** - Publish @hotspots/types and @hotspots/mcp-server to npm
2. **Release v1.0.0** - First stable release with AI-first features ✨

### Short Term
1. **GitHub PR Annotations (Task 2.4)** - Inline complexity annotations
2. **Performance optimization** - Large codebase support
3. **IDE integration** - VS Code extension

### Long Term
1. **Multi-language support** - Python, Java, Go
2. **Historical trending** - Track complexity over time
3. **Team dashboard** - Web UI for complexity tracking

## 🔐 Configuration

### Project Config (.hotspotsrc.json)
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
    "**/*.spec.ts",
    "**/node_modules/**"
  ]
}
```

### Claude Desktop (MCP Server)
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

## 🧪 Testing

**Total Tests:** 145 (all passing)

**Coverage:**
- Unit tests: 107 tests (core functionality)
- Integration tests: 38 tests (end-to-end workflows)
- Golden tests: Deterministic output validation

**Test Execution:**
```bash
cargo test --workspace              # All tests
cargo test --package hotspots-core  # Core library only
cargo test --lib                    # Unit tests only
```

## 📊 Metrics

**Project Stats:**
- **Rust Files:** ~30 files
- **Lines of Code (Rust):** ~8,000 LOC
- **TypeScript Files:** ~15 files
- **Lines of Code (TypeScript):** ~2,000 LOC
- **Documentation:** ~10 markdown files

**Build Times:**
- Clean build (debug): ~15s
- Clean build (release): ~25s
- Incremental build: ~2s

## 🤝 Contributing

Development follows conventions in `CLAUDE.md`:
- Single-line commits under 72 characters
- Minimal, focused changes
- Fix all errors before committing
- Run full test suite after changes

## 📄 License

MIT License - See LICENSE file

---

**For detailed task breakdown, see:** [TASKS.md](TASKS.md)
**For usage instructions, see:** [docs/USAGE.md](docs/USAGE.md)
**For development guidelines, see:** [CLAUDE.md](CLAUDE.md)
