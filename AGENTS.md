# Repository Guidelines

## Project Structure & Modules
- `hotspots-core/`: Core Rust library (analysis, metrics, policies, git, CFG).
- `hotspots-cli/`: CLI binary (`hotspots`).
- `docs/`: VitePress docs (`npm -C docs run docs:dev|docs:build`).
- `action/`, `packages/`: GitHub Action and TS packages.
- `tests/`, `hotspots-core/tests/`: Rust integration and golden tests.
- `assets/`, `examples/`, `scripts/`: Logos, fixtures, helpers.

## Build, Test, and Dev Commands
- Build: `cargo build --release` or `make build`.
- Run CLI locally: `./dev.sh analyze src/` or `cargo run --package hotspots-cli -- analyze src/`.
- Unit tests: `cargo test` or `make test` (library tests only).
- Comprehensive tests: `make test-comprehensive` (pytest or `integration/legacy/test_comprehensive.py`).
- All tests: `make test-all`.
- Install hooks: `make install-hooks` (fmt + clippy + tests on commit).

## Coding Style & Naming
- Language: Rust 2021; format with `cargo fmt`, lint with `cargo clippy -D warnings`.
- Naming: crates/modules `snake_case`, types/traits `PascalCase`, consts `SCREAMING_SNAKE_CASE`.
- Indentation: Rust defaults (4 spaces); avoid unnecessary refactors; keep changes focused.
- Workspace lints are enforced; PRs must be warning-free.

## Testing Guidelines
- Framework: Rust test harness; use `#[test]` functions and `_tests.rs` files.
- Locations: unit tests near sources; integration/golden tests in `hotspots-core/tests/`.
- Useful flags: `cargo test -- --nocapture`, `cargo test -p hotspots-core`.
- Optional coverage: `cargo tarpaulin` (see docs) if installed.

## Commit & Pull Request Guidelines
- Conventional commits: `<type>: <description>` (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`).
- Commit messages: single line, ≤72 chars (see `CLAUDE.md`).
- Before pushing: `cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`.
- PRs: small scope, clear description, link issues; include CLI output or screenshots where relevant; update docs when flags/behavior change.

## Security & Configuration Tips
- Config: `.hotspotsrc.json` controls include/exclude patterns (tests, builds, fixtures are excluded by default).
- Secrets: do not commit tokens or private data; binaries install to `~/.local/bin`.
- Docs deploy: `wrangler.toml` targets Pages; build with `npm -C docs run docs:build`.

## Agent-Specific Notes
- Follow `CLAUDE.md`: keep diffs minimal, batch edits, and always run fmt + clippy + tests before proposing changes.
