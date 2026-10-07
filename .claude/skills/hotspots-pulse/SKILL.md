---
name: hotspots-pulse
description: Summarize the latest state of the hotspots CLI repo from a cold start — STATUS.md, TASKS.md promotion handoffs, open PRs/issues, unreleased changelog delta, active worktrees. Read-only. Use when starting a session or when asked "where are we", "latest state", or "what's in flight" for this repo.
---

# hotspots-pulse

Read-only orientation. Do not edit files, commit, or push.

## Steps

Run these (independent, so in parallel where possible):

```bash
# 1. Session handoff doc (this repo's equivalent of HANDOFF.md).
sed -n '1,80p' STATUS.md 2>/dev/null || echo "no STATUS.md"

# 2. Promotion-brief task handoffs from hotspots-research.
sed -n '1,80p' TASKS.md 2>/dev/null || echo "no TASKS.md"

# 3. Open PRs and issues.
gh pr list --limit 10
gh issue list --limit 10

# 4. Recent commits and unreleased changelog delta.
git log --oneline -10
sed -n '/\[Unreleased\]/,/^## \[/p' CHANGELOG.md 2>/dev/null | head -30

# 5. Latest release tag, to check whether STATUS.md claims match reality.
gh release list --limit 3

# 6. Active worktrees (this repo runs many concurrently — both hotspots.worktrees/ and .claude/worktrees/).
git worktree list

# 7. Uncommitted changes in this checkout.
git status --short
```

If a step fails (no `gh` auth, missing file), say so and continue with the rest.

## Report

Under 200 words, in this order:

1. **In flight**: open PRs with any merge-readiness caveats STATUS.md notes (e.g. "DO NOT MERGE as-is" reasons), open issues grouped by theme if there's an obvious cluster (e.g. several "no research backing" issues).
2. **Since STATUS.md**: anything in `git log` or `gh pr list`/`gh issue list` newer than STATUS.md's dated header that it doesn't mention.
3. **Promotion handoffs**: any `TASKS.md` entry not marked `done`, with its brief path.
4. **Unreleased**: whether `CHANGELOG.md`'s `[Unreleased]` section has entries not yet in the latest release tag — flag if a schema/version-bump note applies.
5. **This worktree**: which worktree you're in, and whether it's behind `origin/main` or has uncommitted changes.

Flag anything stale: a STATUS.md whose dated header is more than ~1-2 weeks old, or a PR/issue STATUS.md describes as open that `gh` now shows closed/merged.
