# Contributing

## Setup

**Prerequisites:** Rust 1.75+ (`rustup install stable`), Git. Node.js 18+ only needed for GitHub Action development.

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots.git
cd hotspots
cargo build --release
cargo test
make install-hooks   # install pre-commit git hooks (fmt + clippy + tests)
```

Binaries: `target/debug/hotspots` (fast compile) and `target/release/hotspots` (optimized).

## Development workflow

```bash
cargo check                               # fast error checking
cargo build                              # debug build
cargo test                               # run all tests
cargo test --package hotspots-core       # core only
cargo test test_name -- --nocapture      # specific test with output
cargo test --test golden_tests           # golden file tests
make test-integration                    # pytest E2E tests
make test-comprehensive                  # pytest or legacy fallback
```

**Before every commit** (pre-commit hooks enforce this, but run manually first):
```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Contributing code

1. **Always branch before making changes.** Never commit directly to `main`.
   ```bash
   git checkout -b feat/your-feature-name
   ```
   Branch naming: `feat/`, `fix/`, `refactor/`, `test/`, `docs/`, `chore/`

2. Make changes. When modifying a struct or enum, grep all usages first to avoid cascading compile errors:
   ```bash
   grep -rn "TypeName" hotspots-core/src hotspots-cli/src
   ```
   Batch all related edits across files, then compile once.

3. Run checks (see above), fix all errors.

4. Commit with a single-line message under 72 characters:
   ```
   feat: add Python language support
   fix: correct CC calculation for switch statements
   docs: compress docs to five pages
   ```
   Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

5. Open a PR. One logical unit of work per branch.

**Code conventions (from CLAUDE.md):**
- Minimal and focused changes — do not refactor or rename beyond what the task requires
- No comments unless the WHY is non-obvious
- No error handling for scenarios that can't happen
- Before implementing, list all files to be modified; get confirmation if > 5 files

## Project structure

```
hotspots/
├── hotspots-core/src/
│   ├── language/          # per-language parsers and CFG builders
│   │   ├── typescript/
│   │   ├── javascript/
│   │   ├── go/
│   │   ├── java/
│   │   ├── python/
│   │   ├── rust/
│   │   └── ...
│   ├── cfg/               # CFG types and validation
│   ├── metrics.rs         # raw metric extraction
│   ├── risk.rs            # LRS formula
│   ├── patterns.rs        # pattern detection
│   ├── snapshot.rs        # snapshot persistence
│   ├── delta.rs           # delta computation
│   ├── policy.rs          # policy engine
│   ├── callgraph.rs       # fan-in/out, PageRank, betweenness, SCC
│   ├── git.rs             # git log, touch cache, ref resolution
│   └── config.rs          # config loading
├── hotspots-cli/src/
│   ├── main.rs
│   └── cmd/               # one file per subcommand
├── tests/
│   ├── fixtures/          # language-specific test code files
│   └── golden/            # expected JSON outputs (golden tests)
├── integration/           # pytest-based E2E tests
├── action/                # GitHub Action (Node.js)
└── docs/                  # documentation (this directory)
```

## Adding a language

Estimated effort: 7–14 days depending on language complexity.

**Prerequisites:** A tree-sitter parser exists for the target language (check [crates.io](https://crates.io) or [github.com/tree-sitter](https://github.com/tree-sitter)). Read [docs/ARCHITECTURE.md](ARCHITECTURE.md) first. Study an existing implementation — Go (`language/go/`) for a clean simple case, Python (`language/python/`) for complex language features.

### Step 1 — Dependency and module

Add to `hotspots-core/Cargo.toml`:
```toml
tree-sitter-<language> = "0.x.y"
```

Create `hotspots-core/src/language/<language>/mod.rs`, `parser.rs`, `cfg_builder.rs`.

### Step 2 — Parser

Implement `LanguageParser` trait in `parser.rs`:
- Parse source with `tree_sitter::Parser`
- Walk the AST to discover all function types (declarations, methods, closures, lambdas)
- Sort by source position (`start_byte`) for determinism
- Handle all function node kinds in the tree-sitter grammar (`tree-sitter parse file.ext --debug` shows node kinds)

### Step 3 — CFG builder

Implement `CfgBuilder` trait in `cfg_builder.rs`:
- Build one CFG per function
- Model all control structures: `if`/`else`, all loop types, `switch`, `try`/`catch`/`finally`
- Route early exits (`return`, `throw`, `break`, `continue`) to correct targets
- Create join nodes **lazily** — only when at least one live path reaches them (see Architecture)
- Track loop context stack for `break`/`continue` targets
- Track nesting depth during traversal

**Critical:** After a terminating statement sets `current_node = None`, subsequent statements must return early. Eager join node creation before confirming live paths causes CFG validation failures.

### Step 4 — Register

1. Add language variant to `Language` enum in `language/mod.rs`
2. Map file extensions in `Language::from_extension()`
3. Register parser in `analysis.rs`'s `create_parser()` dispatch
4. Register CFG builder in `cfg_builder.rs`'s `create_cfg_builder()` dispatch
5. Add `FunctionBody` variant if the language uses a different body representation

### Step 5 — Tests

Create `tests/fixtures/<language>/` with 5–7 test files covering: simple functions, loops, conditionals, early exits, nested control flow, language-specific constructs.

Generate golden files:
```bash
cargo build --release
./target/release/hotspots analyze tests/fixtures/<language>/simple.<ext> --format json > tests/golden/<language>-simple.json
```

Add unit tests in `hotspots-core/tests/<language>_tests.rs`. Verify:
- All function types discovered
- CC/ND/FO/NS match manual calculation
- Golden tests pass (deterministic output)
- No clippy warnings

**Common pitfalls:**
- Non-deterministic output — always sort by `start_byte`
- Missing function types — check all node kinds in the grammar
- Incorrect CC — debug by printing edge/node counts before `E − N + 2`
- Break/continue routing — maintain a `loop_stack: Vec<LoopContext>` with `header` and `exit` nodes

### Step 6 — Docs and PR

Update `docs/REFERENCE.md` language support table. Open a PR with all changes including golden files. Update `CHANGELOG.md`.

## Releases

Releases are fully automated via CI. **Never manually bump `Cargo.toml` versions** — the release workflow owns version bumps.

To create a release:
1. Ensure all changes are merged to `main` and CI is green
2. The release workflow triggers on version tags (`v*`)
3. It builds binaries for Linux x86_64, macOS x86_64, macOS ARM64, Windows x86_64
4. Creates a GitHub release with binaries and generated release notes
5. Updates the `v1` floating tag pointer

**Rolling back:** delete the release and tag with `gh release delete vX.Y.Z --yes` and `git push origin :refs/tags/vX.Y.Z`. Create a new patch release with the fix.

We follow [Semantic Versioning](https://semver.org/): MAJOR for breaking API changes, MINOR for new features, PATCH for bug fixes.

## Reporting bugs and requesting features

- Bugs: [GitHub Issues](https://github.com/Stephen-Collins-tech/hotspots/issues) — include version (`hotspots --version`), OS, steps to reproduce, expected vs actual
- Features: [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions) — describe the use case, not just the solution

## Code of conduct

Be respectful and constructive. Follow GitHub's [Community Guidelines](https://docs.github.com/en/site-policy/github-terms/github-community-guidelines).
