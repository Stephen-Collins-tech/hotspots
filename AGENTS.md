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

## Understanding Quadrants and Activity Risk

Every function in a Hotspots snapshot has a `quadrant` field. Use it — not the raw risk score — to determine urgency:

| Quadrant | Complexity | Recent Activity | What to do |
|---|---|---|---|
| `fire` | High | High | Act now — live regression risk |
| `debt` | High | Low | Schedule proactively — structural debt |
| `simple-active` | Low | High | Monitor only |
| `simple-stable` | Low | Low | Ignore |

**Critical:** `activity_risk` (the composite score) is a decay function over git history. It **never reaches zero** even if a function hasn't been touched in months. A high score alone does NOT mean a function is actively changing.

To determine true activity, always check **both**:
- `quadrant` — the authoritative fire/debt classification
- `touches_30d` — commits touching this function in the last 30 days

A `debt`-quadrant function with `touches_30d == 0` is structural debt (stable but complex). Never describe it as "actively changing." A `fire`-quadrant function with `touches_30d > 0` is a live regression surface.
