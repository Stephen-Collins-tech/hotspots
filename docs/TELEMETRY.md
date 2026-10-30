# Telemetry

Hotspots has **opt-in, anonymous** usage telemetry. It is off unless you turn it on.

## What is sent

At most one request per 24 hours, only while enabled:

```json
{
  "name": "cli_ping",
  "meta": {
    "v": "2.5.0",
    "os": "linux",
    "arch": "x86_64",
    "channel": "npm",
    "cmd": "analyze",
    "iid": "9f2c4e1a7b3d58f0a1c6e2d4b7f90312"
  }
}
```

| Field | Meaning |
|:--|:--|
| `v` | CLI version |
| `os`, `arch` | Operating system and CPU architecture |
| `channel` | How it was installed: `npm`, `pypi`, `cargo`, `brew` or `binary` (guessed from the executable path) |
| `cmd` | Which subcommand ran (name only, never its arguments) |
| `iid` | Random install id, generated when you enable telemetry. Not derived from your machine, user or paths |

Never sent: source code, file or repo names, paths, languages, scores, results, arguments, hostnames or usernames. The server records the request's country (from the CDN) and does not store your IP address.

## Controls

```bash
hotspots telemetry status    # enabled or disabled
hotspots telemetry show      # print the exact payload, send nothing
hotspots telemetry enable
hotspots telemetry disable
hotspots telemetry reset     # new random install id
```

On the first interactive run (stdin and stderr are terminals) hotspots asks once whether to enable it; the default is no. It never asks, and never sends, in CI or other non-interactive runs.

Environment overrides that always win over a saved "enabled":

- `DO_NOT_TRACK=1`
- `HOTSPOTS_TELEMETRY=0`
- any of `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `BUILDKITE`, `JENKINS_URL`

State lives in `~/.hotspots/telemetry.json`; deleting it resets everything. The request has a 1 second timeout, failures are silent, and telemetry never changes output or exit codes.

## Why

To learn how many installs are actually in use and which versions and install channels matter, since download counts include CI and mirrors.
