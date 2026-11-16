# Commands

Hotspots has seven commands. This page says why each one exists, when to reach for it, how
to run it, how to read what it prints, and how it relates to the others. The full flag
tables are in the [CLI & Config Reference](REFERENCE.md); workflows are in
[Usage & Workflows](USAGE.md).

| Command | One-line purpose |
|---|---|
| [`analyze`](#analyze) | Score every function and show where to look first |
| [`diff`](#diff) | Show what a change did to risk between two git refs |
| [`coordinate`](#coordinate) | Check ownership risk for files before you start work |
| [`train`](#train) | Fit a repo-specific ranker from your own bug-fix history |
| [`prune`](#prune) | Reclaim disk used by the `.hotspots/` snapshot history |
| [`config`](#config) | Check and inspect the configuration hotspots will use |
| [`telemetry`](#telemetry) | Opt in or out of anonymous usage pings |

Two ideas run through all of them:

- **LRS and the band** describe the structure of a function (branching, nesting, fan-out,
  early exits). They change only when the function's own code changes. This is what you
  improve by refactoring, and what CI gates key on.
- **Activity risk and the tier** describe how much a function's file has been worked on
  recently. They rise when you edit a file and fade as it goes quiet, so they answer
  "where should I look first", not "is this code good". See
  [Activity Risk](REFERENCE.md#activity-risk-score-snapshot-mode) and
  [Activity tier](REFERENCE.md#activity-tier).

---

## analyze

**Why it exists.** It is the core of the tool: it parses your source, scores each function,
and tells you which ones deserve attention. Everything else builds on its output.

**When to use it.** Any time you want to know where the risky code is: locally while
reviewing a repo, in CI on every pull request, or to produce a report to share.

**How.** Three modes, chosen with `--mode`:

| Mode | What it does | Use it for |
|---|---|---|
| *(none)* | Scores the code and prints a ranked list. No git history involved | A quick look at a folder or a single file |
| `snapshot` | Full analysis tied to the current commit. Adds churn, touch counts and recency, writes the result to `.hotspots/`, and reports an activity score and tier per function | Building the baseline, HTML reports, and the triage view |
| `delta` | Compares the current code to the parent commit's snapshot and lists what changed. Add `--policy` to fail (exit 1) on blocking violations | Pull-request checks |

```bash
hotspots analyze src/                              # quick ranked list
hotspots analyze . --mode snapshot --format html   # interactive report
hotspots analyze . --mode delta --policy           # gate a PR
```

**Options worth knowing.**

- `--explain` adds a short reason next to each Critical or High function.
- `--level file` shows a ranked table of files instead of functions.
- `--axes` prints three separate rankings, Risk, Coupling and Ownership, instead of one
  merged list, because a file can be a problem for different reasons.
- `--cold-start` ranks a repo that has too little bug-fix history to train on. It picks a
  strategy per repo and prints which one it used.
- `--skip-touch-metrics` skips all git history work (useful for benchmarking or very large
  repos); activity terms then contribute nothing.
- `--html-output PATH` (snapshot mode, with a non-HTML `--format`) also writes the HTML report
  from the same analysis, so you get JSON and the report for one commit without analysing twice.
- `--top`, `--min-lrs`, `--config`, `--jobs` limit and tune the output; `--no-persist` and
  `--force` control writing the snapshot.

**Reading the output.** Each function has an LRS and a **band** (Low, Moderate, High,
Critical; cutoffs 3, 6 and 9). In snapshot mode it also has an **activity risk** and a
**tier** (`active` or `quiet`). Read the band to judge structure and the activity risk to
decide order. Activity risk cannot be lowered by a fix; it decays as the file goes quiet.
Formats are `text`, `json`, `html` and `sarif`. The evidence behind the score, and its
limits, is written up in [the activity risk formula](https://github.com/Stephen-Collins-tech/hotspots-public-research/blob/main/docs/findings/activity-risk-formula.md) and
[why the quadrant became an activity tier](https://github.com/Stephen-Collins-tech/hotspots-public-research/blob/main/docs/findings/quadrant-replaced-by-activity-tier.md).

**How it relates to the others.** `diff` and `analyze --mode delta` both compare snapshots;
`train` makes `analyze` rank with your repo's own history; `prune` cleans up the snapshots
`analyze` writes; `config` checks the file `analyze` reads.

---

## diff

**Why it exists.** A pull request is a change, and the useful question is what it did to
risk, not what the whole repo looks like. `diff` compares two git refs.

**When to use it.** Reviewing a branch, checking a release range, or gating CI on a base
and head you choose explicitly (rather than "the parent commit").

**How.**

```bash
hotspots diff main HEAD
hotspots diff main HEAD --auto-analyze --policy   # analyze missing refs, then gate
hotspots diff v2.5.0 v2.6.0 --format html --output release-risk.html
```

Without `--auto-analyze`, both refs must already have snapshots; with it, hotspots analyzes
missing refs in temporary git worktrees.

**Reading the output.** Each changed function is listed as new, modified or deleted, with its
LRS before and after and any band change. The header gives the count of changes, the highest
band reached, and the number of changed lines inside touched functions. That line count is
reported rather than a combined risk score because, in our evaluation, it predicted which
PRs later needed a fix better than any LRS-based PR score. Policy failures, if requested,
follow the table.

Each changed file also carries a `file_risk_top_decile` flag (`[top 10%]` in `--format text`;
`file_risk_score`/`file_risk_diff_percentile`/`file_risk_top_decile` in `--format json`). This
ranks a file's `file_risk_score` against only the *other files changed in this same diff* —
not the whole repo — and flags the top 10% of that diff-local ranking. It's a relative
signal: the same file can be flagged in one diff and not in another, depending on what else
changed alongside it. A file `hotspots` can't score (deleted, or an unscoreable type) is
excluded from the ranking rather than shown with a misleading low score, and a diff with only
one scoreable file gets no flag — there's no relative ranking in a population of one. In our
evaluation this diff-local top decile was a real, cutoff-robust risk signal, even though the
raw `file_risk_score` value does not calibrate as an absolute probability.

**How it relates to the others.** `analyze --mode delta` is the same comparison against the
parent commit, used by the GitHub Action; `diff` lets you pick the refs.

---

## coordinate

**Why it exists.** Before you or an AI coding agent start editing a set of files, it helps to
know whether ownership of those files is concentrated in one person. `coordinate` reports
ownership facts from git history alone, with no code analysis.

**When to use it.** At the start of a task, and before handing a file set to an agent.

**How.**

```bash
hotspots coordinate . --files src/api.ts,src/db.ts
hotspots coordinate . --staged
git diff main | hotspots coordinate . --diff
```

**Reading the output.** JSON with, for each file, `author_count`, `author_entropy` (0 means
one owner, higher means more diffuse), `newcomer_rate`, and `knowledge_mode`, which is
`"concentrated"` when ownership is narrow enough that a handoff is more likely than a real
conflict. `ownership_risk` (0 to 1) is the same entropy normalized for the number of authors,
so files with different author counts compare fairly, and `ownership_risk_band` (`low`,
`medium` or `high`) turns it into a tier. Higher means more diffuse ownership and more
cross-author collisions; use it to rank a set of files by coordination risk. There is deliberately no recommendation field and no coupling facts, because the
coupling signals did not hold up in our evaluation.

**How it relates to the others.** `analyze --axes` shows the same ownership signal across the
whole repo; `coordinate` answers it for the files you are about to touch.

---

## train

**Why it exists.** Generic scoring treats every repo the same. `train` learns from a repo's
own history of bug-fix commits which structural features actually predict fixes there, and
saves a ranker that `analyze` then uses.

**When to use it.** On a repo with a real fix history (enough confirmed fixes to learn from).
For a repo without that, use `analyze --cold-start` instead.

**How.**

```bash
hotspots train . --eval          # train, then report Precision@K
hotspots train . --blame --eval  # function-level labels (slower, more precise)
```

The model is written to `.hotspots/ranker.json`. Once it exists, every `analyze` run loads it
and says so on stderr.

**Reading the output.** `--eval` prints Precision@K against the plain LRS ranking. If P@K is
well above the base rate, keep the model; if it is about the base rate, skip it, because the
default ranking is just as good. See [Training a Repo-Specific Ranker](USAGE.md#training-a-repo-specific-ranker).

**How it relates to the others.** It only changes how `analyze` ranks. It does not change LRS,
bands or policy results.

---

## prune

**Why it exists.** Every snapshot run writes to `.hotspots/`, so history grows with each
commit analyzed.

**When to use it.** After force-pushes or branch deletions, and on long-lived repos whose
`.hotspots/` directory has grown large.

**How.**

```bash
hotspots prune --unreachable --dry-run       # see what would go
hotspots prune --unreachable --older-than 30
hotspots prune --compact 1 --dry-run         # store snapshots as deltas
```

**Reading the output.** The number of snapshots pruned or converted and the storage saved.
`--dry-run` prints the same numbers without changing anything. Compaction level 2 keeps only
band changes, so it loses per-commit detail.

**How it relates to the others.** It cleans up what `analyze --mode snapshot` writes, and
`diff` and `analyze --mode delta` read; pruning a ref's snapshot just means it is re-analyzed
next time.

---

## config

**Why it exists.** Hotspots reads a `.hotspotsrc.json` for includes, excludes, output limits
and policy severity. A typo should fail loudly before CI relies on it.

**When to use it.** After editing the file, and in CI to validate it.

**How.**

```bash
hotspots config validate          # exit 1 if the file is invalid
hotspots config show              # the resolved settings
```

**Reading the output.** `validate` prints the file it checked or the error. `show` prints the
effective filters and policy severities after merging defaults. Unknown keys are rejected,
which includes keys from earlier versions (weights, thresholds, warning thresholds,
scoring and pattern settings); those values are now built in.

**How it relates to the others.** Every other command reads this file.

---

## telemetry

**Why it exists.** The maintainer cannot see how hotspots is used. This is an opt-in,
anonymous daily ping to answer a few questions about versions, platforms and which commands
get used. It is off unless you turn it on, and the tool behaves the same either way.

**When to use it.** If you are willing to share anonymous usage counts. See
[Telemetry](TELEMETRY.md) for exactly what is and is not sent.

**How.**

```bash
hotspots telemetry status
hotspots telemetry show      # print the exact payload without sending it
hotspots telemetry enable
hotspots telemetry disable
```

**Reading the output.** `status` says whether pings are on; `show` prints the payload.
