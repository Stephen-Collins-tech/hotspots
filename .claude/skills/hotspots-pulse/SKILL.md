---
name: hotspots-pulse
description: Summarize the current state of the hotspots CLI repo from a cold start: open PRs and issues, recent commits, unreleased changelog entries, CI, worktrees. Derived entirely from git and gh, so it works on any clone. Read-only. Use when starting a session or when asked "where are we" or "what's in flight" for this repo.
---

# hotspots-pulse

Read-only orientation. Do not edit files, commit, or push. Needs only `git` and (optionally)
an authenticated `gh`; no other tooling or sibling repos.

## Steps

Run these (independent, so in parallel where possible):

```bash
git fetch origin --quiet

# 1. Open PRs and issues.
gh pr list --limit 10
gh issue list --limit 10

# 2. Recent commits, unreleased changelog entries, latest release.
git log --oneline -10 origin/main
git show origin/main:CHANGELOG.md | sed -n '/\[Unreleased\]/,/^## \[/p' | head -30
gh release list --limit 3

# 3. CI on main.
gh run list --branch main --limit 5

# 4. Worktrees and local state.
git worktree list
git branch --show-current
git status -sb | head -5
```

If a step fails (no `gh` auth, no network), say so and continue with the rest.

## Report

Under 200 words, in this order:

1. **In flight**: open PRs (title, branch, age), grouped issues when there is an obvious cluster.
2. **Recent**: the last few commits on `origin/main`, ignoring automated `chore:` changelog commits.
3. **Unreleased**: entries under `[Unreleased]` not in the latest release; note schema or version-bump implications.
4. **CI**: whether the latest runs on `main` are green.
5. **This worktree**: branch, behind `origin/main`?, uncommitted changes.

## Maintainer layer (optional)

Internal CLI status (release decisions, merge caveats, promotion handoffs) is tracked privately
in `../hotspots-research`'s `PULSE.md` under the `hotspots-cli` thread, not in this repo. If
`../hotspots-research` and `status` exist locally, also run
`status -C ../hotspots-research show` and
`git -C ../hotspots-research show origin/main:PULSE.md | grep hotspots-cli`, and fold any caveats
(for example "do not merge") into the In-flight section. Contributors should ignore this section.
