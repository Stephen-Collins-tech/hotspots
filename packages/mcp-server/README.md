# @hotspots/mcp-server

An [MCP](https://modelcontextprotocol.io) server that gives Claude Code, Claude Desktop, Cursor
and other MCP clients a live tap on the `hotspots` risk signal. It shells out to the `hotspots`
binary (it does not reimplement any analysis) and keeps a local **sample log** of every call.

Read-only with respect to the analyzed codebase. No network calls.

## Install

Requires Node 18+ and a `hotspots` build that includes `hotspots coordinate`
(see the [install guide](../../README.md)).

```bash
cd packages/mcp-server
npm install
npm run build
```

The binary is resolved from `HOTSPOTS_PATH` if set, otherwise `hotspots` on `PATH`.

## Configure a client

`claude_desktop_config.json` (Claude Code: `claude mcp add hotspots -- node /abs/path/dist/index.js`):

```json
{
  "mcpServers": {
    "hotspots": {
      "command": "node",
      "args": ["/absolute/path/to/hotspots/packages/mcp-server/dist/index.js"],
      "env": { "HOTSPOTS_PATH": "/absolute/path/to/hotspots" }
    }
  }
}
```

## Tools

| Tool | What it does |
|---|---|
| `hotspots_coordinate` | Ownership/entanglement check before editing files. Give exactly one of `files`, `staged`, `diff`. `path` defaults to the server's working directory. |
| `hotspots_analyze` | Snapshot risk triage (fire / debt / watch / ok) for a repo or subtree. Options: `minLrs`, `allFunctions`, `touchMode`, `coldStart`. |
| `hotspots_policy_check` | `hotspots diff <base> <head> --policy`. Returns `{ exitCode, exitMeaning, diff }`; exit 1 means blocking violations. |
| `hotspots_sample_history` | Recent sample-log rows, newest first. Filters: `tool`, `repoPath`, `limit` (default 20, max 500), `since`. |
| `hotspots_sample_trends` | Rising/falling/flat trend over the latest samples (`window`, default 10): `riskTrend` from analyze samples, `ownershipTrend` from coordinate samples. Optional `file`. |

`hotspots_analyze` passes `--no-persist` internally: snapshots are immutable per commit, so
without it a second sample of the same commit would fail. The sample log is this tool's record.

## Sample log

Every successful `coordinate`, `analyze` and `policy_check` call appends one row to
`<repo>/.hotspots/mcp-samples.db` (SQLite; one log per repo, next to the other `.hotspots/`
state). The server writes `.hotspots/.gitignore` so the DB is never committed, even if the repo
does not ignore `.hotspots/`.

Rows hold the tool input, the full output, and a small `summary_json` used for trends.
Outputs are stored as uncompressed JSON: `summary_json` is the source for trend queries, and
SQLite handles the size fine for local use. To reset the log, delete `.hotspots/mcp-samples.db*`.

Analyze summaries map the CLI triage buckets to the column names `criticalCount` (fire),
`highCount` (debt), `watchCount` (watch), `okCount` (ok). `maxLrs` is the highest LRS among
functions listed in the output; default triage lists top functions for fire/debt/watch only.

## Development

```bash
npm test                       # unit tests; integration tests skip without a binary
HOTSPOTS_PATH=/path/to/hotspots npm test   # include integration tests
```
