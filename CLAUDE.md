# Claude Code Rules

This file contains conventions and rules for Claude Code when working on this project.

## Research sync

This CLI is the promotion target for `../hotspots-research`. Before implementing any ranker change,
new snapshot field, or formula modification, check
[`../hotspots-research/docs/promotion-tracker.md`](../hotspots-research/docs/promotion-tracker.md)
to confirm the finding is validated and see whether a cross-repo task is already tracked.

When a CLI change completes a promotion, update the tracker row to `promoted` and link the commit or PR.

If the promotion changes any formula, weight, threshold, or ranking rule, **add an entry to
[`docs/reference/scoring-changelog.md`](docs/reference/scoring-changelog.md)** in the same PR.
The entry must include the version, the before/after values, and a back-link to the finding.
This is how `hotspots-research` knows what formula version the CLI is running.

### Reading a promotion brief

Each finding that is ready to implement has a brief in
[`../hotspots-research/docs/promotion-briefs/`](../hotspots-research/docs/promotion-briefs/).
**Before writing any code for a promoted finding, read its brief.** The brief is the
authoritative spec — it overrides any general intuition about what "seems right."

Rules for consuming a brief:

- **Files to change** is exhaustive. Do not modify files not listed there.
- **Exact names** are mandatory. Use the struct fields, flag names, enum variants, and constants exactly as spelled in the brief table.
- **Do not** is a hard constraint. If the brief says "do not add a flag", do not add it even if it seems useful.
- **Acceptance criteria** is the definition of done. Mark the brief `done` and update the tracker only when every numbered item passes.
- If the brief is ambiguous or a criterion cannot be met, note the blocker in the tracker row and ask — do not silently work around it.

## General Principles

- **Keep changes minimal and focused.** Do not refactor, rename, or restructure code beyond what is required for the task at hand.
- If you see something that could be improved but isn't part of the current task, mention it but don't change it.
- Only add comments, docstrings, or type annotations to code you actually changed.
- Avoid over-engineering. Don't add features, configurability, or abstractions that weren't requested.

## Git Branching

**ALWAYS create a feature branch before starting any new work.** Never commit directly to `main`.

- Branch naming: `<type>/<short-description>` — use the same type prefix as the commit
  - `feat/sarif-output`, `fix/cfg-panic-on-dead-code`, `chore/update-deps`, `refactor/simplify-risk-scoring`
- Create the branch before making any file changes: `git checkout -b <branch-name>`
- One logical unit of work per branch. If a task spawns unrelated changes, split them into separate branches.
- After the branch is ready, open a PR rather than merging directly to `main`.

This applies to **all** work categories:
- **feat** — new features or capabilities
- **fix** — bug fixes
- **chore** — dependency updates, release prep, CI/tooling changes
- **refactor** — code restructuring with no behavior change
- **test** — adding or fixing tests
- **docs** — documentation-only changes

## Git Commits

- **All commit messages MUST be a single line, under 72 characters.**
- Never use multi-paragraph commit messages unless explicitly asked.
- Format: `<type>: <concise description>` (e.g., `feat: add suppression comments support`)
- Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`
- Never include a commit body unless explicitly requested.
- **Before committing**, always run the following and fix any issues. Pre-commit hooks enforce this, but run manually first to avoid hook failures:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`

## Code Changes

### When Modifying Structs/Enums

**CRITICAL:** When adding a new field to a struct or enum, you MUST:

1. **First**, grep the entire codebase for all usages of that type
2. **Then**, identify every constructor, pattern match, destructuring, and test that references it
3. **Finally**, update ALL occurrences before attempting to compile

This prevents cascading compilation errors across multiple files.

### Before Making Changes

- When modifying a type definition, grep for all usage sites first and plan all changes before editing
- List all files that will be modified if more than 5 files are affected
- Always grep for all usages of modified types/fields/functions before starting implementation

### Compilation Strategy

- **Batch all related edits across files FIRST, then compile ONCE.**
- Do not compile after each individual file edit — make all changes together.
- After any multi-file change, run the build command and fix ALL errors before reporting completion.
- Never present code that hasn't been verified to compile.
- Run the full test suite after changes, not just new tests.

### Autonomous Error Fixing

When implementing features that touch multiple files:

1. Make all necessary changes
2. Run `cargo check` (or appropriate build command)
3. If there are compilation errors, fix them ALL iteratively
4. Do not stop until the entire project compiles with zero errors
5. Run `cargo test` to verify existing tests still pass
6. Only then present the results

## Project-Specific Conventions

### Rust

- Use `cargo check` for fast compilation verification
- Use `cargo test` to run the test suite
- Use `cargo fmt --all -- --check` to verify formatting before committing
- Use `cargo clippy --all-targets --all-features -- -D warnings` to lint before committing
- Follow existing code style and patterns in the codebase

## Change Scope

- Before implementing, list all files that will be modified
- Get confirmation if the change will affect more than 5 files
- If you discover the scope is larger than initially expected, communicate this before proceeding

## Testing

- Run all relevant tests after implementation
- Fix any test failures autonomously before considering the task complete
- Include test updates in the same change as the feature implementation

## Documentation

- Update documentation when adding or modifying features
- Keep documentation concise and focused on what users need to know
- Documentation updates should be part of the same commit as the feature
