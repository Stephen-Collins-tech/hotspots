# `hotspots diff` — Feature Requirements

## Overview

`hotspots diff <base> <head>` shows only the functions whose LRS or CC changed between two git refs. It is the PR-focused complement to `hotspots analyze --mode delta`, which compares HEAD against its immediate parent.

Intended workflows:
- **Local review:** `hotspots diff main HEAD` before opening a PR
- **CI/PR check:** `hotspots diff $BASE_SHA $HEAD_SHA` in GitHub Actions
- **Historical comparison:** `hotspots diff v1.0.0 v2.0.0`

---

## CLI Interface

```
hotspots diff <base> <head> [OPTIONS]

Arguments:
  <base>   Git ref for the baseline (branch name, tag, or SHA)
  <head>   Git ref for the comparison point (branch name, tag, or SHA)

Options:
  --format <FORMAT>       Output format: text (default), json, html [default: text]
  --output <FILE>         Write output to file instead of stdout
  --policy <FILE>         Evaluate policy rules; exit 1 on blocking failures
  --auto-analyze          Analyze missing refs automatically using git worktrees
  --top <N>               Limit output to top N changed functions (by |ΔLRS|)
  --config <FILE>         Path to hotspots config file
```

### Argument forms accepted for `<base>` and `<head>`

- Branch names: `main`, `origin/main`
- Tags: `v1.2.0`
- Commit SHAs (full or abbreviated): `abc1234`, `abc1234def5678`
- Relative refs: `HEAD`, `HEAD~1`, `HEAD~3`

Resolution: all refs are passed through `git rev-parse <ref>` to obtain the canonical full SHA before snapshot lookup.

---

## Behavior

### Snapshot lookup

Both refs are resolved to full SHAs. Snapshots are loaded from `.hotspots/snapshots/<sha>.json.zst`.

**If a snapshot is missing (default — no `--auto-analyze`):**

Print a clear error for each missing ref and exit non-zero:

```
error: no snapshot found for base ref 'main' (abc1234ef...)
  → run: git checkout main && hotspots analyze

error: no snapshot found for head ref 'HEAD' (def5678ab...)
  → run: git checkout def5678ab && hotspots analyze

Once both snapshots exist, re-run: hotspots diff main HEAD
```

**If `--auto-analyze` is set:**

For each missing snapshot, analyze that ref in an isolated git worktree:

1. `git worktree add <tmpdir> <sha>`
2. Run the same analysis that `hotspots analyze` would run, using the main repo's config
3. Persist the resulting snapshot to the main repo's `.hotspots/snapshots/<sha>.json.zst`
4. `git worktree remove --force <tmpdir>`

Progress is printed to stderr:
```
[hotspots] no snapshot for 'main' (abc1234) — analyzing in temp worktree...
[hotspots] no snapshot for 'HEAD' (def5678) — analyzing in temp worktree...
[hotspots] computing diff...
```

The current working tree is never modified. If worktree creation or analysis fails, any created worktrees are cleaned up before exiting.

### Delta computation

Once both snapshots are loaded, call `Delta::new(head_snapshot, Some(&base_snapshot))`. This is the same engine used by `hotspots analyze --mode delta`.

The delta contains:
- `New` — functions present in head but not base
- `Deleted` — functions present in base but not head
- `Modified` — functions present in both with changed metrics
- `Unchanged` — functions present in both with identical metrics

### Filtering

By default, `Unchanged` functions are omitted from output. All other statuses are shown.

`--top N` retains only the N functions with the largest `|ΔLRS|` (absolute value), after the `Unchanged` filter.

### Output formats

| Format | Content |
|--------|---------|
| `text` (default) | Human-readable table of changed functions, printed to stdout |
| `json` | Full `Delta` struct as pretty-printed JSON |
| `jsonl` | One JSON object per changed function, newline-delimited |
| `html` | Interactive HTML report via `render_html_delta()` — same as delta mode |
| `sarif` | Not yet implemented for diff — returns an error (deferred to a future release) |

Text format columns: `STATUS`, `FUNCTION`, `FILE`, `LRS (before→after)`, `CC (before→after)`, `BAND`. Note: LINE is not available in text output because `FunctionState` does not carry a line number; use `--format json` to get line numbers.

`--top N` limits the number of functions shown across all formats. Selection uses a risk-aware sort key: New functions rank by `after.lrs`, Deleted by `before.lrs`, Modified by `|Δlrs|`. This ensures a newly-introduced critical function is never buried below a trivial modification.

### Policy evaluation

When `--policy <FILE>` is provided, policy rules are evaluated against the delta (same as `hotspots analyze --mode delta --policy`). Exit code 1 if any blocking failures.

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success; no policy failures |
| 1 | Policy failure (blocking rule violated) |
| 2 | Usage error (bad args, ref doesn't resolve, worktree failure) |
| 3 | Snapshot not found — re-run with `--auto-analyze` to generate missing snapshots |

Exit code 3 is intentionally distinct from 2 so CI scripts can detect "snapshots not yet generated" and either fail with a clear message or retry with `--auto-analyze`.

---

## `--auto-analyze` Details

### Worktree lifecycle

```
tmp_dir = tempdir() in system temp (e.g. /tmp/hotspots-diff-abc1234)
git worktree add <tmp_dir> <sha>
run analysis with repo_root = tmp_dir, config from main repo
write snapshot to <main_repo>/.hotspots/snapshots/<sha>.json.zst
git worktree remove --force <tmp_dir>
```

### Config resolution

The config file used for auto-analysis is resolved from the **main repo root**, not the worktree. This ensures consistent thresholds and weights across both refs.

### Touch/churn metrics

Churn and per-function touch metrics require git history, which is fully available in a normal worktree. However, **shallow clones** (e.g. CI pipelines using `fetch-depth: 1`) will have truncated history, causing touch/churn metrics to silently degrade or return zero. When a shallow clone is detected (via `git rev-parse --is-shallow-repository`), print a warning to stderr:

```
warning: shallow clone detected — touch/churn metrics may be incomplete
  → consider: fetch-depth: 0 in your GitHub Actions checkout step
```

### Failure handling

If worktree creation fails (e.g. detached HEAD, ref doesn't exist), print a clear error and exit code 2. If analysis within the worktree fails, clean up the worktree before exiting.

### Dirty working tree

`git worktree` does not touch the current checkout. The user's working tree, index, and stash are unaffected regardless of their state.

---

## Implementation Plan

### Files to create
- `hotspots-cli/src/cmd/diff.rs` — new subcommand handler

### Files to modify
- `hotspots-cli/src/main.rs` — add `Diff` variant to `Commands` enum
- `hotspots-cli/src/cmd/mod.rs` — `pub mod diff`
- `hotspots-core/src/git.rs` — add `resolve_ref_to_sha(repo_root, ref) -> Result<String>`
- `hotspots-core/src/lib.rs` — export new public API if needed

### Reused without modification
- `hotspots-core/src/delta.rs` — `Delta::new()`, `DeltaAggregates`
- `hotspots-core/src/snapshot.rs` — `load_snapshot()`, `persist_snapshot()`
- `hotspots-core/src/html.rs` — `render_html_delta()`
- `hotspots-core/src/report.rs` — `render_json()`
- `hotspots-core/src/policy.rs` — policy evaluation
- `hotspots-cli/src/cmd/analyze.rs` — reference for delta output + policy wiring

### Phase 1 — Core diff (no `--auto-analyze`)
1. Add `resolve_ref_to_sha()` to `git.rs`
2. Add `Diff` subcommand to CLI with `base`, `head`, `--format`, `--output`, `--policy`, `--top`
3. Implement `cmd/diff.rs`: resolve refs → load snapshots → error if missing → compute delta → render
4. Wire exit codes
5. Tests: unit tests for ref resolution; integration test with two pre-built snapshots

### Phase 2 — `--auto-analyze`
1. Implement `analyze_ref_in_worktree(repo_root, sha, config) -> Result<Snapshot>`
2. Integrate into diff flow: check for missing snapshots before erroring, run worktree analysis
3. Cleanup guard (ensure worktree removed even on panic/early return)
4. Tests: integration test that triggers auto-analysis for a missing ref

---

## Resolved Design Decisions

- **Summary line in text output:** Yes — implemented as `N modified, N new, N deleted` before the table.
- **`--mode` flag:** Not added — diff is always a delta by definition.
- **`--auto-analyze` head ref persistence:** Snapshots are persisted under the resolved SHA, which is correct regardless of whether head is a branch or detached.
- **`--output` scope:** Works for all formats (text, json, jsonl, html), not just HTML.
- **SARIF for diff:** Deferred. Requires a new `render_sarif_delta()` function. Currently returns exit code 2 with a clear message.
- **LINE column in text output:** Omitted — `FunctionState` does not carry line numbers. Use `--format json` to get line numbers.
