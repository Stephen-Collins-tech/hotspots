---
name: hotspots-pulse
description: Summarize the latest state of the hotspots CLI repo from a cold start: PULSE.md threads, unsynced pushes, staleness, TASKS.md promotion handoffs, open PRs/issues, unreleased changelog delta, worktrees. Read-only. Use when starting a session or when asked "where are we", "latest state", or "what's in flight" for this repo.
---

# hotspots-pulse

Read-only orientation. Do not edit files, commit, or push. When run via the `/pulse`
dispatcher, use `git -C <repo>` / `status -C <repo>` and absolute paths.

## Steps

Run these (independent, so in parallel where possible):

```bash
git fetch origin --quiet

# 1. Staleness and unsynced pushes (status = ~/projects/.../dev-tools/status).
status check
status show

# 2. Committed threads, from main (not this worktree's possibly-stale checkout).
git show origin/main:PULSE.md | sed -n '1,40p'

# 3. Promotion-brief task handoffs from hotspots-research.
git show origin/main:TASKS.md | grep -n "^## Task\|Status:" | head -30

# 4. Open PRs and issues.
gh pr list --limit 10
gh issue list --limit 10

# 5. Recent commits, unreleased changelog delta, latest release.
git log --oneline -10 origin/main
git show origin/main:CHANGELOG.md | sed -n '/\[Unreleased\]/,/^## \[/p' | head -30
gh release list --limit 3

# 6. Active worktrees (this repo runs many: hotspots.worktrees/ and .claude/worktrees/) and local state.
git worktree list
git status --short
```

If a step fails (no `gh` auth, no `status`), say so and continue with the rest.

## Report

Under 200 words, in this order:

1. **In flight**: open PULSE.md threads, open PRs with any caveats the threads or Notes record
   (e.g. "do not merge" reasons), open issues grouped by theme when there is an obvious cluster.
2. **Since PULSE.md**: unsynced pushes and commits on origin/main newer than `updated_at`.
3. **Promotion handoffs**: any `TASKS.md` task not marked `done`, with its brief path.
4. **Unreleased**: entries under `[Unreleased]` not in the latest release; flag schema/version-bump notes.
5. **This worktree**: which worktree, behind `origin/main`?, uncommitted changes.

Flag anything `status check` marks STALE, and any thread claim `gh` contradicts (a PR a thread
calls open that `gh` shows merged). Hand-written history under PULSE.md's `## Notes` is
consulted only for the thread being resumed.
