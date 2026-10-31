# Claude Code Rules

## General Principles

- Keep changes minimal and focused. Do not refactor, rename, or restructure beyond the task.
- If you see something worth improving that is not part of the task, mention it; don't change it.
- Only add comments, docstrings, or type annotations to code you changed.
- Avoid over-engineering: no features, configurability, or abstractions that weren't requested.
- Follow existing code style and patterns.

## Git

- **Always create a feature branch before starting work.** Never commit directly to `main`; open a PR.
- Branch name: `<type>/<short-description>`, same type as the commit (`feat/sarif-output`,
  `fix/cfg-panic-on-dead-code`, `chore/update-deps`, `refactor/...`, `test/...`, `docs/...`).
- Enable hooks on a fresh clone: `git config core.hooksPath .githooks`.
- One logical unit of work per branch; split unrelated changes.
- **Commit messages are a single line under 72 characters**, `<type>: <description>`
  (e.g. `feat: add suppression comments support`). No body unless explicitly requested.
- Before committing, run these and fix any issues (pre-commit hooks enforce them too):
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`

## Code Changes

- **When adding a field to a struct or enum**, first grep the whole codebase for every
  constructor, pattern match, destructuring, and test that uses the type, then update all of
  them before compiling.
- Before changing a type, field, or function, grep for all usages and plan all edits up front.
- If the change will touch more than 5 files, list them and get confirmation first. If scope
  turns out larger than expected, say so before proceeding.
- Batch related edits across files, then compile once with `cargo check`; fix all errors
  until the project compiles with zero errors. Do not report code you haven't compiled.
- Run the full test suite, not just new tests, and fix failures before calling the task done.
- **CFG builders must be iterative.** Never recurse over child nodes in a language's `cfg_builder.rs`; use an explicit heap work stack (see the Rust and Go builders). Applies to any new language.
- Include test updates and documentation updates (concise, user-facing) in the same change as
  the feature.

## Public repo: no private references

This repo is public and must stand alone. Nothing in it — code, comments, docs, help or runtime
text, tests, commit messages, PR text, issues, changelog — may reference private material:

- No private sibling repos by name or path (any `../hotspots-*` path other than public ones) and
  no absolute local paths. If unsure whether a repo is private, treat it as private.
- No research finding or experiment IDs (`F123`, `META-NN`), brief or handoff links, or phrases like
  "validated by our research repo". State the behavior and any caveat instead ("in our evaluation",
  "unvalidated heuristic"). Do not strengthen a claim to fill the gap.
- No working notes, handoff files, task briefs, or status trackers (`TASKS.md`, `STATUS.md`,
  `docs/.internal/`, `.claude/skills/*-pulse`). Those live in the maintainers' private workspace; do
  not create them here.
- No analytics numbers, pipeline details, roadmap, or strategy.
- Commit subjects feed the generated changelog: never put finding IDs or private repo names in a
  commit message or PR title.
- Before opening a PR, run: `git grep -n -E "\bF[0-9]{2,3}\b|META-[0-9]+|\.\./hotspots-|/Users/"` and clear every hit you added (workspace crate paths like `../hotspots-core` are fine).
