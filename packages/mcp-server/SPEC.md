# `@hotspots/mcp-server` — requirements

**Status:** implemented. See "Open questions" for how each was resolved.
**Start here if you're a new session picking this up.** This file is the exhaustive spec;
implement against it directly. It is a tooling/packaging feature, not a research
promotion — there is no promotion brief for it, and this spec plays that role instead. The
same rules from the parent `CLAUDE.md` still apply (branch already created as
`feat/mcp-server`, single-line commits, `cargo fmt`/`clippy`/`test` are not relevant here
since this package is TypeScript, but any CLI/core changes this work requires still need
those checks run from the Rust workspace root).

## Why this exists

`hotspots` already computes and exposes an agent-facing risk signal — `hotspots coordinate`
(ownership/entanglement before touching a file set), `hotspots analyze --mode snapshot
--format json` (agent-optimized triage JSON), and `hotspots diff --policy` (blocking exit
codes for a gate). All of it today requires an agent to shell out to the CLI binary and parse
stdout. `examples/ai-agents/README.md` and `docs/USAGE.md`'s "See Also" section both link to
`packages/mcp-server/README.md` as the integration path for Claude Desktop / Claude Code —
but that package does not exist anywhere in the repo, in `TASKS.md`, or in `CHANGELOG.md`. It
is a dangling reference to something that was planned and never built. This spec is that
package.

**The product framing** (for context, not something to re-derive): `hotspots`' risk signal
accumulates continuously from git history — commits, bugfixes, ownership churn — the way sap
slowly builds pressure in a tree. A CLI invocation is a single sample pulled off that ongoing
stream, not a batch report that goes stale. The MCP server is the live tap: it lets an agent
pull a fresh sample mid-session instead of working from a cached report, and it keeps a local
log of every sample pulled so the *flow*, not just the latest drop, is inspectable later. See
"Local sample log" below — that's the mechanism, not a metaphor to over-engineer around.

## Goals

1. Expose `hotspots coordinate`, `hotspots analyze --mode snapshot`, and `hotspots diff
   --policy` as MCP tools callable by any MCP client (Claude Code, Claude Desktop, Cursor,
   etc.) without the client shelling out to the binary itself.
2. Every tool call is logged as one row in a local SQLite database (the "sample log") —
   durable, queryable, offline, zero network calls by default.
3. Expose a small set of read-only MCP tools over that log so an agent (or a human via the
   MCP client) can ask aggregate/trend questions — "has this file's risk score been rising
   across the last N samples," "how many coordinate checks this week flagged concentrated
   ownership" — without re-running `hotspots analyze` from scratch each time.
4. Ship as a real, installable npm package (`@hotspots/mcp-server`), consistent with the
   existing `@hotspots/types` package.

## Non-goals (do not build these)

- **Not a replacement for the CLI.** The MCP server shells out to the same `hotspots` binary
  examples/ai-agents already uses (via `execa`, same pattern as `HotspotsClient` in
  `examples/ai-agents/shared/hotspots-client.ts`) — it does not reimplement analysis logic in
  TypeScript.
- **Not cloud sync.** The sample log is local SQLite only. A cloud-sync target is explicitly
  future work (see "Explicitly deferred" below) — do not build any network call, API client,
  or remote schema in this pass.
- **Not `hotspots-analytics`.** `hotspots-analytics` (the private sibling repo under
  `hotspots-repos`) is a separate product with a separate, unrelated meaning — do not name
  anything in this package `analytics` in a way that could be confused with it in docs, and
  do not import from or write to that repo. This package's local SQLite log is scoped
  entirely to one machine's MCP server usage history; call it the **sample log** in all
  naming (table names, tool names, doc headers), never "analytics," to keep the two
  unambiguous.
- **Not write access.** No MCP tool in this package writes to the target repo, calls `git
  commit`, or mutates anything outside `.hotspots/` in the analyzed repo and the sample-log
  database file. Every exposed tool is read-only with respect to the codebase being analyzed.
- **Not a new risk-scoring feature.** If something here surfaces a gap in what `hotspots`
  itself computes (e.g., a missing field), note it in this spec's "Open questions" and stop —
  do not add new fields to `FunctionSnapshot`/`risk.rs`/etc. as part of this work. That
  requires a research finding and promotion brief, per the parent `CLAUDE.md`.

## Package layout

```
packages/mcp-server/
├── package.json
├── tsconfig.json
├── README.md                  # user-facing: install, configure, tool list (write last)
├── SPEC.md                    # this file — keep it, do not delete once implemented
├── src/
│   ├── index.ts                # MCP server entry point, stdio transport
│   ├── hotspots-client.ts       # thin wrapper around the CLI binary (see below)
│   ├── tools/
│   │   ├── coordinate.ts        # hotspots_coordinate tool
│   │   ├── analyze.ts           # hotspots_analyze tool
│   │   ├── policy-check.ts      # hotspots_policy_check tool (wraps `diff --policy`)
│   │   └── samples.ts           # hotspots_sample_history / hotspots_sample_trends tools
│   ├── sample-log/
│   │   ├── db.ts                 # SQLite connection, schema migration
│   │   ├── schema.sql            # the schema itself (see "Local sample log" below)
│   │   └── record.ts             # insert-a-sample helper, called by every tool in tools/
│   └── types.ts                 # tool input/output TS types (reuse @hotspots/types where the shape matches CLI JSON output 1:1)
└── test/
    ├── hotspots-client.test.ts
    ├── sample-log.test.ts
    └── tools/
        ├── coordinate.test.ts
        ├── analyze.test.ts
        ├── policy-check.test.ts
        └── samples.test.ts
```

**Dependencies** (mirror `examples/ai-agents/package.json` conventions where they overlap):
- `@modelcontextprotocol/sdk` — MCP server/stdio transport
- `execa` — subprocess execution (already the established pattern for shelling out to the
  `hotspots` binary; see `examples/ai-agents/shared/hotspots-client.ts`)
- `better-sqlite3` — synchronous SQLite, no async driver complexity needed for a local
  single-writer log
- `@hotspots/types` (workspace dependency) — reuse `HotspotsOutput`, `PolicyResult`, etc.
  where the CLI's JSON output is being passed through as-is
- `zod` — MCP tool input schema validation (standard MCP SDK pattern)
- Dev: `vitest` or the same test runner `examples/ai-agents` uses — check its `package.json`
  first and match it rather than introducing a second test runner into the workspace

## Binary discovery

Reuse the exact convention from `examples/ai-agents/shared/hotspots-client.ts` and its
README: `HOTSPOTS_PATH` environment variable overrides; default is `hotspots` resolved from
`PATH`. Throw the same `HotspotsNotFoundError` shape (or import/reuse it if
`examples/ai-agents`'s client is extracted to a shared location — check whether that's
feasible before duplicating the class; if duplicating, keep the error class names identical
for consistency).

## MCP tools to expose

Each tool's description string (shown to the calling agent/model) must explain *when* to use
it, not just what it does — MCP clients select tools by reading these, so vague descriptions
cause an agent to skip a tool it needed or call the wrong one.

### 1. `hotspots_coordinate`

Wraps `hotspots coordinate <path> [--files|--staged|--diff]`. Use this *before* an agent
starts editing a set of files, to check ownership/entanglement risk.

**Input schema:**
```typescript
{
  path: string;              // repo root, defaults to cwd if omitted — confirm CLI default behavior before assuming
  files?: string[];          // maps to --files (comma-joined for the CLI call)
  staged?: boolean;          // maps to --staged; mutually exclusive with files/diff
  diff?: string;             // unified diff text, piped to stdin, maps to --diff; mutually exclusive with files/staged
}
```
Exactly one of `files`, `staged`, `diff` must be provided — validate with zod `.refine()`
and return a clear MCP tool-error (not a thrown exception) if zero or more than one is set.

**Output:** the `coordinate` JSON envelope exactly as `hotspots coordinate` emits it today
(`schema_version`, `coordinate.input_files`, `coordinate.ownership[]` with `file`,
`author_count`, `author_entropy`, `newcomer_rate`, `knowledge_mode`) — pass through
unmodified, do not reshape. Confirm current `schema_version` value against a live
`hotspots coordinate` run before hardcoding anything that assumes a version number anywhere
in this package.

**Every call is recorded in the sample log** (see below) with `tool = 'coordinate'`.

### 2. `hotspots_analyze`

Wraps `hotspots analyze <path> --mode snapshot --format json`. Use this to get the current
risk triage (CRITICAL/HIGH/WATCH/OK buckets) for a repo or subtree before or during agent
work on it.

**Input schema:**
```typescript
{
  path: string;
  minLrs?: number;            // --min-lrs
  allFunctions?: boolean;     // --all-functions (flat array instead of triage buckets)
  touchMode?: 'auto' | 'per-function' | 'file' | 'hybrid' | 'none'; // --touch-mode (hybrid:N not exposed in v1 — see Open questions)
  coldStart?: boolean;        // --cold-start
}
```
Do not expose `--mode delta`, `--explain`, `--level`, `--format` other than `json`, `--force`,
`--no-persist`, `--output`, or any of the deprecated touch-mode flags through this tool —
v1 scope is snapshot-mode JSON only, matching the "agent-optimized" framing already in
`docs/USAGE.md`. If `--mode delta` support turns out to be wanted, that's a v2 tool
(`hotspots_diff`, distinct from `hotspots_policy_check` below), not an option bag on this
one.

**Output:** the snapshot JSON envelope as-is (triage buckets by default, flat array if
`allFunctions`). Reuse `HotspotsOutput` from `@hotspots/types` as the TS return type if its
shape still matches — verify against a live run first, the type package may be stale.

**Every call is recorded in the sample log** with `tool = 'analyze'`.

### 3. `hotspots_policy_check`

Wraps `hotspots diff <base> <head> --policy --format json`. Use this to evaluate whether a
set of changes (already committed or committable to two refs) would fail the repo's policy
gate — blocking violations (`critical-introduction`, `excessive-risk-regression`) vs.
warnings (`watch-threshold`, `attention-threshold`, `rapid-growth`,
`suppression-missing-reason`, `net-repo-regression`).

**Input schema:**
```typescript
{
  path: string;
  base: string;     // git ref
  head: string;      // git ref
  top?: number;       // --top N
  autoAnalyze?: boolean; // --auto-analyze (generate missing snapshots via git worktrees)
}
```

**Output:** the diff JSON, plus the tool must surface the CLI's exit code distinctly from a
parse/execution failure — `hotspots diff` uses exit codes 0/1/2/3 meaningfully (success /
policy failure / auto-analysis failed / snapshot missing), not just 0-vs-nonzero. The MCP
tool result should include a structured `exitCode` field and a human-readable `exitMeaning`
string derived from the table in `docs/REFERENCE.md`'s "Exit codes" section, so the calling
agent doesn't have to special-case raw exit codes itself.

**Every call is recorded in the sample log** with `tool = 'policy_check'`.

### 4. `hotspots_sample_history`

Read-only query over the local sample log (not the CLI — this tool never shells out).
Returns recent samples, optionally filtered.

**Input schema:**
```typescript
{
  tool?: 'coordinate' | 'analyze' | 'policy_check';  // filter by which tool produced the sample
  repoPath?: string;        // filter by repo
  limit?: number;            // default 20, max 500
  since?: string;             // ISO 8601 timestamp, filter to samples at/after this time
}
```

**Output:** array of sample-log rows (see schema below), newest first.

### 5. `hotspots_sample_trends`

Read-only aggregate query over the sample log. This is the "is the sap rising" tool — use it
to answer whether a file's or repo's risk signal is trending up or down across the samples
already collected, without re-running analysis.

**Input schema:**
```typescript
{
  repoPath: string;
  file?: string;             // narrow to one file's samples, where the sample recorded file-level data
  window?: number;            // number of most-recent samples to consider, default 10
}
```

**Output:**
```typescript
{
  sampleCount: number;        // how many samples matched (may be less than `window`)
  firstSampleAt: string;       // ISO timestamp
  lastSampleAt: string;
  // One of these two depending on what the matched samples contain — do not fabricate
  // a trend from mismatched sample types:
  riskTrend?: { direction: 'rising' | 'falling' | 'flat' | 'insufficient-data'; values: number[] };
  ownershipTrend?: { direction: 'concentrating' | 'diffusing' | 'flat' | 'insufficient-data'; values: (number | null)[] };
}
```
`insufficient-data` when `sampleCount < 2`. Trend direction is a simple sign-of-slope
comparison (e.g. linear regression slope, or even a first-vs-last comparison) — keep the
method simple and documented inline in the implementation; this is not a forecasting feature,
just "is it going up or down."

## Local sample log

**Location:** `.hotspots/mcp-samples.db` inside the analyzed repo (same directory family as
the existing `.hotspots/snapshots/`, `.hotspots/touch-cache.json.zst`, `.hotspots/index.json`
— consistent with where hotspots already keeps repo-local state). Not the MCP server
package's own directory — the log lives with the repo it's about, so it travels with that
repo's `.hotspots/` if someone inspects it, and a multi-repo setup naturally gets one log per
repo rather than one giant mixed log.

**Must be gitignored.** Confirm `.hotspots/` is already covered by a `.gitignore` entry in
target repos generally (it should be, since snapshots already live there) — if the MCP server
is used against a repo where `.hotspots/` isn't ignored, do not silently let the sample-log DB
file get committed; this is a correctness requirement, not a nice-to-have, since the DB will
contain file paths and risk data from whatever the agent was working on.

**Schema (`schema.sql`):**
```sql
CREATE TABLE IF NOT EXISTS samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tool TEXT NOT NULL CHECK (tool IN ('coordinate', 'analyze', 'policy_check')),
  taken_at TEXT NOT NULL,              -- ISO 8601, UTC
  repo_path TEXT NOT NULL,
  commit_sha TEXT,                      -- HEAD sha at sample time, if resolvable
  input_json TEXT NOT NULL,             -- the tool's input args, as JSON
  output_json TEXT NOT NULL,             -- the tool's full output, as JSON (yes, this can be large — see Open questions)
  exit_code INTEGER,                     -- for policy_check; NULL for the other two
  summary_json TEXT NOT NULL             -- small denormalized fields for fast trend queries, see below
);

CREATE INDEX IF NOT EXISTS idx_samples_repo_tool_time
  ON samples (repo_path, tool, taken_at);
```

`summary_json` exists so `hotspots_sample_trends` doesn't have to parse the full
`output_json` blob on every query. Shape, by tool:
- `coordinate`: `{ fileCount: number, concentratedCount: number }` (count of
  `knowledge_mode === "concentrated"` entries)
- `analyze`: `{ criticalCount: number, highCount: number, watchCount: number, okCount: number, maxLrs: number }`
- `policy_check`: `{ blockingViolations: number, warnings: number }`

**Migrations:** use a simple `user_version` PRAGMA check in `db.ts` — on connect, if
`user_version` is behind the package's current schema version, run the migration(s) in order.
v1 ships with exactly one schema version; design the migration runner now so v2 doesn't
require a rewrite, but do not build speculative migration scaffolding beyond "run an ordered
list of SQL files if `user_version` is behind."

## Explicitly deferred (do not build now, but don't design against it either)

- **Cloud sync of the sample log.** Mentioned as a real "maybe later" by the person who
  scoped this, not a hard commitment. If it happens, it's an opt-in push of `summary_json`
  rows (never raw `output_json`, which may contain file paths/code context) to some
  `hotspots-cloud` endpoint — but that's a separate spec, a separate repo
  (`hotspots-cloud`, private), and explicitly out of scope here. Do not add a `--sync` flag,
  a config field for a remote URL, or any networking dependency to this package.
- **Cross-repo sample log aggregation.** Each repo gets its own `.hotspots/mcp-samples.db`;
  nothing in this spec merges logs across repos or machines.
- **`hotspots_diff` (full delta, non-policy) as a separate tool.** Only the `--policy` path is
  exposed in v1 (`hotspots_policy_check`). A general-purpose delta-viewing tool is future
  scope if requested.
- **Write/mutation tools** (applying a suppression comment, bumping a policy threshold, etc.)
  — this package is read/observe only.

## Testing requirements

- Unit tests for the sample-log schema/migration logic against an in-memory or temp-file
  SQLite DB (no fixture repo needed).
- Unit tests for each tool's input validation (zod schema) — valid/invalid cases, especially
  the `hotspots_coordinate` "exactly one of files/staged/diff" constraint.
- Integration tests that actually shell out to a built `hotspots` binary against a small
  fixture repo (check whether `examples/ai-agents` or the main Rust test suite already has a
  fixture-repo pattern to reuse — do not build a new one if one exists) and assert the tool
  output matches the CLI's own JSON output for the same invocation, and that a sample-log row
  was written with the correct `summary_json`.
- A test for `hotspots_sample_trends` with `insufficient-data` (0 or 1 matching samples) and
  with a synthetic multi-sample sequence asserting the trend direction is computed correctly
  in both the rising and falling case.

## Acceptance criteria

1. `npm install && npm run build` succeeds in `packages/mcp-server` from a clean checkout.
2. All five tools (`hotspots_coordinate`, `hotspots_analyze`, `hotspots_policy_check`,
   `hotspots_sample_history`, `hotspots_sample_trends`) are registered and discoverable by an
   MCP client connecting over stdio.
3. Calling `hotspots_coordinate`, `hotspots_analyze`, or `hotspots_policy_check` against a
   real fixture repo produces output identical (modulo whitespace) to running the
   corresponding `hotspots` CLI command directly, and writes exactly one row to
   `.hotspots/mcp-samples.db`.
4. `hotspots_sample_history` and `hotspots_sample_trends` return correct results against a
   seeded sample log, including the `insufficient-data` case.
5. No network call is made anywhere in the package (verify by running the test suite with
   network access disabled, or by grep — no `fetch`/`http`/`https` imports outside of
   whatever the MCP SDK itself requires for its own transport, which is stdio here, not
   network).
6. `README.md` documents: install, how to point an MCP client's config at this server
   (concrete `claude_desktop_config.json`-style JSON snippet), the tool list with one-line
   descriptions, and where the sample log lives and how to delete/reset it.
7. `examples/ai-agents/README.md`'s existing link to `packages/mcp-server/README.md` now
   resolves to a real, accurate document — update that link's surrounding text if anything
   there has drifted from what actually got built.
8. `docs/USAGE.md`'s "See Also"-style reference to the MCP server (if one exists there once
   you check) is likewise accurate, not aspirational.

## Do not

- Do not reimplement any `hotspots-core` analysis logic in TypeScript — always shell out to
  the compiled binary.
- Do not add networking of any kind.
- Do not name anything `analytics` — use "sample log" / "samples" throughout, to stay
  unambiguous against the private `hotspots-analytics` repo.
- Do not expose `hotspots train`, `hotspots prune`, `hotspots compact`, `hotspots init`,
  `hotspots config`, or `hotspots upgrade` as MCP tools — none of them fit the "agent consults
  a live risk signal" use case this package exists for; adding them is scope creep.
- Do not modify `hotspots-core` or `hotspots-cli` Rust source as part of this work unless you
  discover the CLI's JSON output genuinely cannot support a tool above as specified — if that
  happens, stop and note it in "Open questions" rather than changing CLI behavior unreviewed.
- Do not touch `packages/types` beyond possibly importing from it — if its types are stale
  against current CLI JSON output, note that in "Open questions" rather than silently
  reshaping types out from under other consumers of that package.

## Open questions (resolve during implementation, or flag back if blocking)

1. **Exact current `schema_version` values** for `coordinate` and the snapshot-JSON envelope
   — confirmed as `5` for `coordinate` per `docs/USAGE.md` at spec-writing time, but verify
   against a live build before hardcoding anywhere, including in tests.
2. **`output_json` size in the sample log** — a full snapshot JSON for a large repo could be
   substantial. Decide during implementation whether to store it uncompressed, compress it
   (the existing `.hotspots/snapshots/*.json.zst` pattern uses zstd), or cap/truncate it with
   `summary_json` as the real source of truth for trend queries. Lean toward compressing with
   zstd for consistency with existing `.hotspots/` conventions, but this isn't locked — decide
   and document the choice in `README.md`.
3. **`--touch-mode hybrid:N`** — the input schema above only exposes the bare `hybrid` variant
   of `--touch-mode`, not `hybrid:N` with a custom N. Decide whether to add an optional
   `hybridThreshold?: number` field or leave N at its CLI default (5) for v1.
4. **Whether `examples/ai-agents/shared/hotspots-client.ts` should be extracted into a shared
   location** both it and this package import from, vs. this package having its own small
   client. Check for duplication risk before deciding; either is acceptable, but don't leave
   two subtly-different copies of the same binary-invocation logic without a documented reason.
5. **Default `path` resolution** — confirm what `hotspots coordinate`/`analyze` actually do
   today when no path is given from the CLI (cwd? error?) and match that behavior exactly
   rather than assuming.

### Resolutions (from the implementation)

1. `schema_version` is `5` for `coordinate`, snapshot and diff JSON (hotspots 2.2.0). Nothing in
   the package hardcodes it; only a test asserts it for `coordinate`.
2. `output_json` is stored uncompressed; `summary_json` drives trends. Documented in the README.
3. `hybrid:N` is not exposed; N stays at the CLI default.
4. Own small client (`src/hotspots-client.ts`): the ai-agents client is analyze-only and not in a
   published package. Error class names are identical.
5. `coordinate` defaults to `.`; `analyze` has no default (path required), so the tool requires it.
6. Snapshot triage buckets are `fire`/`debt`/`watch`/`ok`, not CRITICAL/HIGH/WATCH/OK; they are
   stored as `criticalCount`/`highCount`/`watchCount`/`okCount`. Default triage lists top
   functions only for non-ok buckets, so `maxLrs` covers listed functions. `hotspots_analyze`
   passes `--no-persist` internally (snapshots are immutable per commit).
7. `packages/types` was not touched and not used for the return types: the tools pass CLI JSON
   through untyped, and `HotspotsOutput` was not checked for staleness.
