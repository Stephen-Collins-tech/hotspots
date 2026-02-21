# Claude Code Rules

This file contains conventions and rules for Claude Code when working on this project.

## General Principles

- **Keep changes minimal and focused.** Do not refactor, rename, or restructure code beyond what is required for the task at hand.
- If you see something that could be improved but isn't part of the current task, mention it but don't change it.
- Only add comments, docstrings, or type annotations to code you actually changed.
- Avoid over-engineering. Don't add features, configurability, or abstractions that weren't requested.

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
