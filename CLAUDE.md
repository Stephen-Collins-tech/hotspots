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
- Include test updates and documentation updates (concise, user-facing) in the same change as
  the feature.
