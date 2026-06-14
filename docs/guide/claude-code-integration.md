# Coding Agent Integration

Give any AI coding agent automatic access to codebase coordination signals before it splits work across files or sub-agents.

## Prerequisites

`hotspots` must be installed and on `PATH`:

```bash
hotspots --version
```

If not installed, see [installation](https://github.com/Stephen-Collins-tech/hotspots).

## The snippet

The content is the same for every agent. Add it to whichever file your agent reads for repo-level instructions:

| Agent | File |
|---|---|
| Claude Code | `CLAUDE.md` |
| GitHub Copilot | `.github/copilot-instructions.md` |
| Cursor | `.cursorrules` |
| Windsurf | `.windsurfrules` |
| Aider | pass via `--read CONVENTIONS.md` (or any file you `--read`) |

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

## What happens

The snippet gives agents two things, at two levels:

### Context awareness (all agents)

Before touching any files, the agent runs `hotspots coordinate` and learns:
- **hidden_dependencies** — files outside the task set that historically move with the input files. The agent flags these to the developer before starting: *"session.rs is frequently modified alongside auth.rs — should I include it?"*

This is useful for any agent, even one working sequentially. Surfacing a hidden dependency before work starts is better than discovering it mid-task or in a broken build.

### Parallelism (agents that support sub-agent spawning)

For agents that can split work across concurrent sub-agents (Claude Code today; others as they evolve), the partition recommendation drives how work is divided:
- **parallel_safe** — files that rarely co-change; safe to assign to concurrent agents
- **serialize** — files that co-change frequently; must be worked sequentially

If the agent discovers additional files mid-task, it re-runs `hotspots coordinate` with the expanded file list before continuing.
