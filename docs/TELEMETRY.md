# Telemetry

Hotspots has **opt-in, anonymous** usage telemetry. It is off unless you turn it on.

## What I'm asking for

I'm a solo maintainer and I can't see how hotspots is used. Download counts include CI runs and mirrors, so they don't tell me how many people actually run it. If you're willing, enabling telemetry sends one small anonymous ping a day (the fields below, nothing else). It helps me answer a few questions:

- How many real installs are active, and does anyone keep using it after the first run?
- Which versions are still in use, so I know when it's safe to drop old behavior?
- Which install channels (`npm`, `pypi`, `cargo`, `brew`, binary) and platforms to test and support first?
- Which subcommands get used, so I improve those rather than guess?

That's all I use it for. I don't sell it, share it, or join it to anything else. You can say no, and the tool works exactly the same.

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
| `v` | hotspots version |
| `os` | Operating system (e.g. `linux`, `macos`, `windows`) |
| `arch` | CPU architecture (e.g. `x86_64`, `aarch64`) |
| `channel` | Install channel guessed from the executable path: `npm`, `pypi`, `cargo`, `brew` or `binary` |
| `cmd` | Name of the subcommand that ran, never its arguments |
| `iid` | Random install id, generated when you enable telemetry. Not derived from your machine, user or paths; `hotspots telemetry reset` replaces it |

That table is the complete list. It mirrors `COLLECTED` in `hotspots-cli/src/cmd/telemetry.rs`, which `hotspots telemetry show` and the opt-in prompt print, and a test fails if the code and this table disagree.

**Added by the server on receipt:** the time of the request, the request's country (from the CDN; your IP address is not stored) and the fixed User-Agent `hotspots-cli`. Nothing else is stored.

**Never collected:** source code, file or repo names, paths, languages, scores, analysis results, command arguments, environment variables, hostnames, usernames, IP addresses.

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
