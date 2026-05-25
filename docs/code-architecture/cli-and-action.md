# CLI and GitHub Action

This page explains the runtime wrappers around `hotspots-core`: the Rust CLI and the JavaScript GitHub Action.

## CLI crate

`hotspots-cli` is the user-facing binary crate. Its job is orchestration, not analysis.

Important paths:

| Path | Purpose |
|---|---|
| `hotspots-cli/src/main.rs` | Top-level CLI parser and command dispatch. |
| `hotspots-cli/src/cmd/analyze.rs` | `hotspots analyze`: snapshot and delta analysis modes. |
| `hotspots-cli/src/cmd/diff.rs` | `hotspots diff`: compare snapshots/commits. |
| `hotspots-cli/src/cmd/trends.rs` | Historical trend commands. |
| `hotspots-cli/src/cmd/config.rs` | Config inspection/validation commands. |
| `hotspots-cli/src/cmd/init.rs` | Config initialization. |
| `hotspots-cli/src/output/` | CLI-specific output helpers. |

The CLI should generally:

1. Parse arguments with `clap`.
2. Resolve config.
3. Determine git context when needed.
4. Call `hotspots-core` APIs.
5. Render output and set exit status.

## Command architecture

The key command families are:

- `analyze`: produce current reports, snapshots, or delta-mode analysis.
- `diff`: compare base/head snapshots and evaluate policies.
- `trends`: inspect historical risk movement.
- `config`: inspect resolved configuration.
- `prune`: remove old local artifacts.

When adding a command, keep reusable business logic in `hotspots-core` and make the CLI command a thin adapter.

## GitHub Action

The GitHub Action lives in `action/`.

| Path | Purpose |
|---|---|
| `action/action.yml` | Public action inputs/outputs and runtime entry point. |
| `action/src/main.ts` | TypeScript source for the action. |
| `action/dist/index.js` | Committed bundled JavaScript runtime used by GitHub Actions. |
| `action/__tests__/` | Jest tests for action helper behavior. |

The action does not contain static analysis logic. It resolves a `hotspots` binary and invokes the CLI.

Binary resolution order:

1. Use `binary-path` input when provided.
2. Resolve/download a release binary for the configured version.
3. Fall back to building from source when running inside the repository and Rust is available.

## Why `dist/` is committed but CLI binaries are not

JavaScript GitHub Actions execute checked-in JavaScript. For this reason, `action/dist/index.js` is committed after running:

```bash
npm -C action run package
```

The Rust CLI binary is not committed. CI can build it on GitHub-hosted runners and pass it to the action through `binary-path` or workflow artifacts. Release assets provide prebuilt binaries for normal external users.

## Action test workflow

The action validation workflow builds or restores the CLI binary once, uploads it as a short-lived artifact, and then runs `uses: ./action` with:

```yaml
with:
  binary-path: .hotspots-bin/hotspots
```

This tests the local action code against a trusted binary built by GitHub Actions without adding binary files to git.
