# @hotspots/mcp-server

An [MCP](https://modelcontextprotocol.io) server that gives Claude Code, Claude Desktop, Cursor
and other MCP clients a live tap on the `hotspots` risk signal, led by a short ranked **review brief** for a human reviewer. It shells out to the `hotspots`
binary (it does not reimplement any analysis) and keeps a local **sample log** of every call.

Read-only with respect to the analyzed codebase. No network calls.

## Install

Requires Node 18+ and **hotspots CLI 2.4.0 or newer** (`--blend-churn` shipped in 2.4.0; see the
[install guide](../../README.md)). The server checks `hotspots --version` on first use and returns
an error for an older binary.

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

| `hotspots_review_brief` | **Lead tool.** A short ranked shortlist (default 5, max 10) of files, with the heaviest function, a human should look at first. One plain reason each (complexity, recent churn, ownership). Blends churn by default (`blendChurn: false` to turn off). The agent is told to present it to you, not act on it. |
| `hotspots_coordinate` | Ownership companion: author count, entropy, newcomer rate, concentrated knowledge. Give exactly one of `files`, `staged`, `diff`. |
| `hotspots_analyze` | Snapshot risk triage (fire / debt / watch / ok). Options: `minLrs`, `allFunctions`, `touchMode`, `coldStart`, `blendChurn` (adds `churn_percentile` and `file_risk_score_blended` to `architecture.file_risk`, plus a top-level `blended_file_ranking`). |
| `hotspots_sample_history` | Recent sample-log rows, newest first. Filters: `tool`, `repoPath`, `limit` (default 20, max 500), `since`. |
| `hotspots_sample_trends` | Rising/falling/flat trend over the latest samples (`window`, default 10). Optional `file`. |
| `hotspots_policy_check` | Not registered by default. Set `HOTSPOTS_MCP_POLICY_CHECK=1` to enable `hotspots diff <base> <head> --policy`; exit 1 means blocking violations. |

The brief describes where to look, never what to change. `--blend-churn` is opt-in in the CLI and is
not predicted to help every repo; use `blendChurn: false` if the plain ranking reads better for yours.

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
HOTSPOTS_PATH=/path/to/hotspots npm test   # needs hotspots >= 2.4.0
```

Integration tests skip locally when no binary is found (`HOTSPOTS_PATH`, else `hotspots` on `PATH`).
In CI (`CI` set) a missing binary fails the run, and the `MCP Server Tests` job in
`.github/workflows/ci.yml` builds the binary and sets `HOTSPOTS_PATH`, so they always run there.

```bash
npm test
```
