# Commands

Hotspots has three commands. This page says why each one exists, when to reach for it, how
to run it, how to read what it prints, and how it relates to the others. The full flag
tables are in the [CLI & Config Reference](REFERENCE.md); workflows are in
[Usage & Workflows](USAGE.md).

| Command | One-line purpose |
|---|---|
| [`analyze`](#analyze) | Score every function and show where to look first (plus ownership, pruning, config and agent-contract flags) |
| [`diff`](#diff) | Show what a change did to risk between two git refs |
| [`train`](#train) | Fit a repo-specific ranker from your own bug-fix history |

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
and tells you which ones deserve attention. Everything else builds on its output. A handful
of smaller, related jobs — ownership risk for a file set, snapshot-storage maintenance,
config inspection, and printing the agent contract — used to be separate subcommands; they
are now flags on `analyze` instead (see below), so the CLI has one entry point for anything
that isn't `diff` or `train`.

**When to use it.** Any time you want to know where the risky code is: locally while
reviewing a repo, in CI on every pull request, or to produce a report to share.

**How.**

```bash
hotspots analyze                                   # analyze the current directory
hotspots analyze src/ --format json                # a specific path, machine-readable
hotspots analyze . --format html                   # interactive report
hotspots diff --auto-analyze --policy              # gate a PR
```

`PATH` defaults to `.`. There is no `--mode` flag — every run does the same full analysis
(churn, touch counts, recency, activity score and tier), writes the result to `.hotspots/`
by default, and gates on PR checks the same way. Pull-request checks (comparing the current
code to a prior commit, optionally gating with `--policy`) are `diff`'s job, not a flag of
`analyze` — see below.

**Options worth knowing.**

- `--explain` adds a short reason next to each Critical or High function.
- `--level file` shows a ranked table of files instead of functions.
- `--axes` prints three separate rankings, Risk, Coupling and Ownership, instead of one
  merged list, because a file can be a problem for different reasons.
- `--cold-start` ranks a repo that has too little bug-fix history to train on. It picks a
  strategy per repo and prints which one it used.
- `--html-output PATH` (with a non-HTML `--format`) also writes the HTML report from the
  same analysis, so you get JSON and the report for one commit without analysing twice.
- `--top`, `--min-lrs`, `--config` limit and tune the output; `--no-persist` (persisted by
  default now) and `--force` control writing the snapshot.
- `HOTSPOTS_JOBS`, `HOTSPOTS_SKIP_TOUCH_METRICS`, and `HOTSPOTS_SKIP_GATE` replace the old
  `--jobs`/`--skip-touch-metrics`/`--skip-gate` flags — set them as environment variables
  instead (the latter two are on if set to any value, same as `NO_COLOR`).

**Reading the output.** Each function has an LRS and a **band** (Low, Moderate, High,
Critical; cutoffs 3, 6 and 9), plus an **activity risk** and a **tier** (`active` or
`quiet`). Read the band to judge structure and the activity risk to decide order. Activity
risk cannot be lowered by a fix; it decays as the file goes quiet. Formats are `text`,
`json`, `html` and `sarif`. The evidence behind the score, and its limits, is written up in
[the activity risk formula](https://github.com/Stephen-Collins-tech/hotspots-public-research/blob/main/docs/findings/activity-risk-formula.md) and
[why the quadrant became an activity tier](https://github.com/Stephen-Collins-tech/hotspots-public-research/blob/main/docs/findings/quadrant-replaced-by-activity-tier.md).

**How it relates to the others.** `diff` compares the snapshots `analyze` writes; `train`
makes `analyze` rank with your repo's own history.

### Ownership risk (`--files`/`--diff`/`--staged`/`--ownership`)

Before you or an AI coding agent start editing a set of files, it helps to know whether
ownership of those files is concentrated in one person. These flags report ownership facts
from git history alone, with no code analysis — this used to be the standalone `coordinate`
command.

```bash
hotspots analyze . --files src/api.ts,src/db.ts
hotspots analyze . --staged
hotspots analyze . --ownership            # defaults to --staged when no file selector is given
git diff main | hotspots analyze . --diff
```

`--files`, `--diff`, and `--staged` are mutually exclusive ways to pick the file set; they
(and bare `--ownership`) are mutually exclusive with a plain analysis, `--prune`,
`--show-config`/`--validate-config`, and `--help-agent`.

Output is JSON with, for each file, `author_count`, `author_entropy` (0 means one owner,
higher means more diffuse), `newcomer_rate`, and `knowledge_mode`, which is `"concentrated"`
when ownership is narrow enough that a handoff is more likely than a real conflict.
`ownership_risk` (0 to 1) is the same entropy normalized for the number of authors, so files
with different author counts compare fairly, and `ownership_risk_band` (`low`, `medium` or
`high`) turns it into a tier. Higher means more diffuse ownership and more cross-author
collisions; use it to rank a set of files by coordination risk. There is deliberately no
recommendation field and no coupling facts, because the coupling signals did not hold up in
our evaluation. `analyze --axes` shows the same ownership signal across the whole repo;
these flags answer it for the files you are about to touch.

### Snapshot maintenance (`--prune`)

Every run writes to `.hotspots/`, so history grows with each commit analyzed. This used to
be the standalone `prune` command.

```bash
hotspots analyze --prune --unreachable --dry-run       # see what would go
hotspots analyze --prune --unreachable --older-than 30
hotspots analyze --prune --compact 1 --dry-run         # store snapshots as deltas
```

Use it after force-pushes or branch deletions, and on long-lived repos whose `.hotspots/`
directory has grown large. `--unreachable`, `--older-than`, `--compact`, and `--dry-run` all
require `--prune`; `--older-than` additionally requires `--unreachable`. Output is the number
of snapshots pruned or converted and the storage saved; `--dry-run` prints the same numbers
without changing anything. Compaction level 2 keeps only band changes, so it loses per-commit
detail. It cleans up what a plain `analyze` writes and `diff` reads; pruning a ref's snapshot
just means it is re-analyzed next time.

### Config inspection (`--show-config`/`--validate-config`)

Hotspots reads a `.hotspotsrc.json` for includes, excludes, output limits and policy
severity. A typo should fail loudly before CI relies on it. This used to be the standalone
`config` command.

```bash
hotspots analyze --validate-config          # exit 1 if the file is invalid
hotspots analyze --show-config              # the resolved settings, plus telemetry state
```

`--validate-config` prints the file it checked or the error. `--show-config` prints the
effective filters and policy severities after merging defaults, plus a line showing whether
telemetry is currently enabled (see [Telemetry](TELEMETRY.md)). Unknown keys are rejected,
which includes keys from earlier versions (weights, thresholds, warning thresholds, scoring
and pattern settings); those values are now built in. Every other command reads this file.

### Agent contract (`--help-agent`)

A coding agent driving this CLI non-interactively needs the execution rules, flag names,
JSON shape, and exit codes in one dense pass, not scattered across prose written for a human
reader skimming a page. This used to be the standalone `agent` command.

```bash
hotspots analyze --help-agent             # full contract (analyze + diff)
hotspots analyze --help-agent=analyze     # scoped to analyze
hotspots analyze --help-agent=diff        # scoped to diff
```

Output is plain text to stdout — the output itself *is* the documentation, not data to
parse. Four sections: Execution Contract (how to invoke this CLI safely), Command & Schema
Map (flags and JSON shape), Exit Code Semantics, and Guardrails & Anti-Patterns (common
mistakes). It describes `analyze` and `diff`; it doesn't run them or change their behavior.

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

**How it relates to the others.** `diff --auto-analyze` with no explicit base is the same
comparison against the parent commit, used by the GitHub Action; passing explicit refs lets
you pick any two.

---

## train

**Why it exists.** Generic scoring treats every repo the same. `train` learns from a repo's
own history of bug-fix commits which structural features actually predict fixes there, and
saves a ranker that `analyze` then uses.

**When to use it.** On a repo with a real fix history (enough confirmed fixes to learn from).
For a repo without that, use `analyze --cold-start` instead.

**How.**

```bash
hotspots train .          # train; the pre-flight screen and P@K eval always run
hotspots train . --blame  # function-level labels (slower, more precise)
```

The model is written to `.hotspots/ranker.json`. Once it exists, every `analyze` run loads it
and says so on stderr. Training always starts with a pre-flight data-quality screen (aborts
if the repo's signal is too flat to be worth training on) and always ends with a Precision@K
report — there is no `--screen`/`--eval` flag to gate either one, and no
`--n-estimators`/`--max-depth`/`--quiet` either (tree count and depth use fixed defaults;
progress verbosity follows whether stderr is a terminal).

**Reading the output.** The P@K table reports Precision@K against the plain LRS ranking. If
P@K is well above the base rate, keep the model; if it is about the base rate, skip it,
because the default ranking is just as good. See
[Training a Repo-Specific Ranker](USAGE.md#training-a-repo-specific-ranker).

**How it relates to the others.** It only changes how `analyze` ranks. It does not change LRS,
bands or policy results.
