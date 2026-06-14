# Claude Code Integration

Add the following to your repo's `CLAUDE.md` to give Claude Code automatic access to coordination signals before splitting work across agents.

## CLAUDE.md snippet

```markdown
## Multi-agent coordination

Before splitting work across sub-agents or parallel tasks, run:

```bash
hotspots coordinate --files <comma-separated list of files to be modified>
```

Rules:
- Files listed under `serialize` must be worked sequentially — do not assign them to concurrent agents
- Files listed under `parallel_safe` can be worked concurrently
- Files listed under `hidden_dependencies` are outside your task set but historically change alongside your files — flag them to the user before proceeding
- If you discover additional files mid-task, re-run `hotspots coordinate` with the expanded file list before continuing
```

## Prerequisites

`hotspots` must be installed and on `PATH`. Verify with:

```bash
hotspots --version
```

See [installation](https://github.com/Stephen-Collins-tech/hotspots) for setup instructions.
