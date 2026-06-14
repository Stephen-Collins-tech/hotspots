# Scope: Function-level coordinate (`--level function`)

_Created: 2026-06-14_

## What it does

`hotspots coordinate --files auth.rs,user.rs --level function` partitions at function granularity instead of file granularity. Two agents can safely work on the same file if their function scopes don't co-change. The output shape is identical to `--level file` except `parallel_safe` and `serialize` contain FQNs (`auth.rs::validate_token`) instead of file paths.

## Approach

### Function identity

Fully-qualified name: `file::function` or `file::Type::method`. This is already what the existing parsers produce (`FunctionNode.name`). No new identity scheme needed.

Rename tracking is out of scope for v1 — FQN breaks on rename, which is an accepted limitation shared by the rest of hotspots' function-level analysis.

### Parsing — reuse existing infrastructure

`hotspots-core` already has:
- `LanguageParser` / `ParsedModule` traits with implementations for Rust (syn), Go, Python, Java, C#, C, JS/TS (SWC/tree-sitter)
- `discover_functions()` returns `Vec<FunctionNode>`, each with a name and `SourceSpan { start_line, end_line }`
- `Language::from_path()` for language detection
- `create_parser()` in `analysis.rs` dispatches to the right parser

The parsers operate on the *current* file state. This is a known limitation: historical commits are attributed to whatever function occupies those lines today. Acceptable for v1.

ECMAScript parsers require an SWC `SourceMap`. A new `create_parser_simple(language)` helper that constructs one internally keeps the coordinate path clean without leaking SWC types into `coordinate.rs`.

### Git mining — new function in `git.rs`

New: `extract_function_co_change_pairs(repo_root, files, window_days, min_count)`

Steps:
1. Run `git log --name-only --format=COMMIT:%H --diff-filter=AM --since=<window>` — same as `extract_co_change_pairs`, filtered to the input files only
2. For each commit, run `git diff-tree --no-commit-id -p --unified=0 <sha> -- <files>` to get changed line ranges (hunk headers: `@@ -old +new,count @@`)
3. Parse each input file with the existing parser to get `FunctionNode` list (done once per file, cached across commits)
4. For each changed hunk, find which functions overlap the changed line range → collect FQNs touched in this commit
5. Build commit → `[fqn, fqn, ...]` sets; mine pairs using the same algorithm as `extract_co_change_pairs`

Output type: `CoChangePair` but with `file_a` / `file_b` replaced by FQNs. Either reuse `CoChangePair` with a note that fields contain FQNs, or introduce `FunctionCoChangePair` — TBD at implementation time based on how much the types diverge.

### Coordinate layer — new function in `coordinate.rs`

New: `coordinate_functions(repo_root, files)` — same shape as `coordinate()`:
- Calls `extract_function_co_change_pairs`
- Runs `partition_pairs` (reused as-is — it operates on string identifiers, doesn't care if they're file paths or FQNs)
- `parallel_safe` / `serialize` contain FQNs
- Ownership signals remain at file level (no change)
- Hidden deps remain at file level (cross-file coupling is still file-granularity)

### CLI — `--level` flag

In `hotspots-cli/src/cmd/coordinate.rs` and `main.rs`:
- Add `--level <file|function>` flag, default `file`
- `handle_coordinate` dispatches to `coordinate()` or `coordinate_functions()` based on flag
- JSON and text output shapes are the same; FQNs appear where file paths did

## Files to change

| File | Change |
|---|---|
| `hotspots-core/src/git.rs` | Add `extract_function_co_change_pairs` |
| `hotspots-core/src/coordinate.rs` | Add `coordinate_functions`; extract `create_parser_simple` or import from analysis |
| `hotspots-core/src/analysis.rs` | Possibly expose `create_parser` or extract a shared helper |
| `hotspots-cli/src/cmd/coordinate.rs` | Add `--level` flag, dispatch |
| `hotspots-cli/src/main.rs` | Wire `--level` into `CoordinateArgs` |
| `hotspots-core/tests/coordinate_tests.rs` | Add function-level integration tests |

## Known limitations / decisions deferred

- Parsers use current file state, not historical — renames cause misattribution
- `create_parser` exposes SWC `SourceMap`; coordinate needs a clean internal wrapper
- `CoChangePair` reuse vs. new `FunctionCoChangePair` type — decide at implementation
- Hidden deps and ownership stay at file level for now
- Languages with no parser (unsupported extensions) are silently skipped at function level, same as existing behavior for file-level analysis

## Acceptance criteria

1. `hotspots coordinate --files auth.rs,user.rs --level function` produces output with FQNs in `parallel_safe` / `serialize`
2. Two functions in the same file that don't co-change appear in different partitions
3. `--level file` (default) behavior is unchanged
4. JSON and text output both work
5. Integration tests cover: same-file split, cross-file coupling, unsupported language graceful skip
