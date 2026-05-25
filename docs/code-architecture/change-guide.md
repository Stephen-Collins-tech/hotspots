# Contributor Change Guide

Use this guide to choose where a code change belongs.

## Add or change a metric

Start in:

- `hotspots-core/src/metrics.rs`
- `hotspots-core/src/risk.rs`
- `hotspots-core/src/report.rs`

Then update:

- golden tests under `hotspots-core/tests/`
- JSON/schema docs if output changes
- user docs if the metric is public

Be careful: metric changes usually alter golden output and may affect policy behavior.

## Add a language feature

Start in the relevant language module under `hotspots-core/src/language/`.

Common steps:

1. Parse/discover the construct.
2. Ensure function spans and names are deterministic.
3. Model control flow in the CFG builder.
4. Add metric/golden fixtures.
5. Add parity tests when behavior should match another language.

## Add a new language

Typical work areas:

- `hotspots-core/src/language/mod.rs`
- `hotspots-core/src/language/parser.rs`
- new parser module under `hotspots-core/src/language/`
- CFG builder implementation
- metric extraction support
- fixtures and golden tests
- docs/reference language support updates

Keep the language layer behind the existing traits so the rest of the pipeline remains language-agnostic.

## Change scoring or quadrants

Start in:

- `hotspots-core/src/scoring.rs`
- `hotspots-core/src/risk.rs`
- `hotspots-core/src/snapshot.rs`

Also review:

- policy thresholds
- docs explaining quadrants
- tests involving activity risk, touch counts, and fire/debt classification

Do not describe `activity_risk` alone as active churn. Use `quadrant` and `touches_30d`.

## Change GitHub Action behavior

Start in:

- `action/src/main.ts`
- `action/action.yml`
- `action/__tests__/`
- `.github/workflows/test-action.yml`

After TypeScript changes, run:

```bash
npm -C action test
npm -C action run package
```

Commit the regenerated `action/dist/` files. Do not commit CLI binaries.

## Change CI workflow binary reuse

The intended model is:

- cache key includes runner OS and commit SHA
- restore exact key only for commit-scoped binaries
- build on GitHub-hosted runners on cache miss
- cache save is best-effort because parallel workflows may race
- pass binaries between jobs in the same workflow via artifacts when a strict dependency is needed

## Add a CLI command

Put reusable logic in `hotspots-core`; keep `hotspots-cli` as the command adapter.

Add command wiring in:

- `hotspots-cli/src/main.rs`
- `hotspots-cli/src/cmd/mod.rs`
- a new file under `hotspots-cli/src/cmd/`

Add integration tests for externally visible behavior.

## Documentation update checklist

When a change affects public behavior, update docs with the code change.

| Code change | Docs to review |
|---|---|
| CLI flags or command behavior | `docs/reference/cli.md`, `docs/guide/usage.md` |
| JSON, JSONL, HTML, or SARIF output | `docs/reference/json-schema.md`, `docs/guide/output-formats.md` |
| language support or parser behavior | `docs/reference/language-support.md`, `docs/contributing/adding-languages.md` |
| scoring, LRS, quadrants, or risk bands | `docs/reference/metrics.md`, `docs/reference/lrs-spec.md`, `docs/code-architecture/data-model.md` |
| GitHub Action inputs/outputs | `docs/guide/ci-cd.md`, `docs/guide/github-action.md`, `action/action.yml` |
| config keys or defaults | `docs/guide/configuration.md`, `.hotspotsrc.json` examples |
| suppression comments | `docs/guide/suppression.md` |

## Required validation

Before proposing changes, run the relevant subset. For broad changes, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

For action changes, also run:

```bash
npm -C action test
npm -C action run package
```
