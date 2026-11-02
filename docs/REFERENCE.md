# Reference

## CLI Commands

### `hotspots analyze <path>`

Core analysis command. Scans source files, computes metrics, scores functions.

```
hotspots analyze <PATH> [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--format` | `text` | `text`, `json`, `html`, `sarif` |
| `--mode` | — | `snapshot`, `delta` |
| `--top N` | none | Show top N functions by LRS |
| `--min-lrs F` | `0.0` | Filter functions below this LRS |
| `--config PATH` | auto | Path to config file |
| `--output PATH` | `.hotspots/report.html` | Output file (HTML/SARIF) |
| `--explain` | off | Per-function risk breakdown + phrase-table explanations for CRITICAL/HIGH when a trained ranker is active (snapshot+text only) |
| `--level` | — | `file` aggregate view (ranked file risk table) (snapshot+text only) |
| `--policy` | off | Evaluate policies; exit 1 on blocking violations (delta only) |
| `--force` | off | Overwrite existing snapshot |
| `--no-persist` | off | Skip writing snapshot to disk |
| `--skip-touch-metrics` | off | Skip all git log I/O: touch counts, churn, recency, directed coupling and burst score are omitted. For benchmarking or very large repos |
| `--all-functions` | off | Output flat array instead of triage buckets (snapshot JSON only) |
| `--skip-gate` | off | Disable suppression gate P@10 check |
| — | — | Suppression gate verdicts are smoothed: `Suppressed` is only reported after 3 consecutive raw `Suppressed` readings across runs, tracked in `.hotspots/gate_history.json` |
| `--cold-start` | off | Gini/label-density-gated ranking with no trained ranker required — see [Cold-Start Ranking](#cold-start-ranking) |
| `-j N` / `--jobs N` | CPU count | Parallel worker threads |

**Notes:**
- `--explain` and `--level` are mutually exclusive
- `--force` and `--no-persist` are mutually exclusive
- Snapshot mode text output requires `--explain` or `--level`
- SARIF requires `--mode snapshot`; HTML requires `--mode snapshot` or `--mode delta`
- `--policy` requires `--mode delta`
- `--cold-start` is not compatible with `--mode` — it bypasses the trained-ranker/snapshot pipeline entirely
### `hotspots diff <base> <head>`

Compare snapshots between any two git refs. Both must have existing snapshots.

```
hotspots diff <BASE> <HEAD> [OPTIONS]
```

Accepts: branch names, tags, full/short SHAs, `HEAD~N` relative refs.

| Flag | Description |
|---|---|
| `--format` | `text` (default), `json`, `html` |
| `--output PATH` | Write output to file |
| `--policy` | Evaluate policies; exit 1 on blocking violations |
| `--top N` | Limit to N changed functions by \|ΔLRS\| |
| `--config PATH` | Config file |
| `--auto-analyze` | Generate missing snapshots via git worktrees |

Exit codes: 0 = success, 1 = policy failure, 2 = auto-analysis failed, 3 = snapshot missing.

`--top` applies after policy evaluation — violations outside the top N are still detected.

### `hotspots train [PATH]`

Fit a ranker from fix-commit history. Model saved to `.hotspots/ranker.json` and auto-loaded by `hotspots analyze`.

Before fitting a RandomForest, `hotspots train` runs a pre-flight comparison (the **regime
screener**) between Ridge regression and a depth-2 RandomForest. If Ridge already
matches the forest's ranking quality (`Δρ < 0.03`), it fits Ridge instead and skips
RandomForest training — faster, and no less accurate on repos where the signal is
linear. See [`docs/USAGE.md`](USAGE.md#ridge-vs-randomforest-automatic-model-class-selection)
for the full verdict table. This selection is automatic; there is no flag to force one
model class over the other.

Before training, the command prints an estimate and (for repos with > 1,000 functions) prompts for confirmation:

```
hotspots train: 12914 functions · 200 trees · 365 days of git history (file-level labels) · estimated ~4m 30s
Proceed? [y/N]
  [10/200]  ~4m 5s remaining
  [100/200]  ~2m 1s remaining
  [200/200]
Model class: RandomForest (regime=STRONG, Δρ=+0.14)
Trained: 200 trees × depth 6 | 12914 samples | elapsed 4m 25s
```

When the screener selects Ridge, RandomForest training is skipped:

```
Model class: Ridge (regime=LINEAR, Δρ=+0.03) — RandomForest training skipped
Trained: 0 trees × depth 0 | 874 samples | elapsed 2s
```

| Flag | Default | Description |
|---|---|---|
| `--blame` | off | Blame-based function-level labels (slower, more precise) |
| `--label-window DAYS` | `365` | Days of history to scan |
| `--n-estimators N` | `200` | Trees in RandomForest (ignored if the screener selects Ridge) |
| `--max-depth N` | `6` | Maximum tree depth (ignored if the screener selects Ridge) |
| `--output PATH` | `.hotspots/ranker.json` | Model output path |
| `--eval` | off | Print Precision@K table after training |
| `--screen` | off | Pre-flight check; aborts when mean hotspots score is too flat |
| `--yes` / `-y` | off | Skip confirmation prompt (CI / non-interactive) |
| `--quiet` / `-q` | off | Suppress per-tree progress lines; estimate and completion still shown |

Requires: ≥ 50 functions in snapshot, ≥ 5 positive and ≥ 10 negative labels. Fix keywords: `fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`.

The trained model (`model_version 6`) uses 9 features: `lrs`, `cc`, `nd`, `loc`, `fo`, `fan_in`, `total_churn`, `authors_90d`, `directed_coupling`. (`convention_bug_fix_count` was removed in `model_version 6` — in our testing it ratchets the trained ranker monotonically, never improving once a function is flagged.) Models trained with an older version are rejected on load with a retrain message.

`hotspots analyze` prints which model class the loaded ranker uses:

```
hotspots: using trained ranker (model class: Ridge)
```

Once a model is trained, `hotspots analyze . --mode snapshot --explain` adds a `✦` phrase line below each CRITICAL/HIGH function — e.g. `✦ Churns heavily and is load-bearing.` — derived from which features rank in the top 20th percentile for that repo. No `✦` lines appear without a trained ranker.

### Cold-Start Ranking

`hotspots analyze <PATH> --cold-start` produces a ranking without a trained ranker and
without the label thresholds `hotspots train` requires (≥ 50 functions, ≥ 5 positive / ≥
10 negative labels). It skips the trained-ranker lookup and snapshot pipeline entirely —
`--mode` cannot be combined with it.

```
hotspots analyze . --cold-start
hotspots analyze . --cold-start --top 20
```

Routing is decided per-repo by two independent gates, checked in this order, and printed
before the ranked list (`cold-start route: formula|anomaly|uniform-prior`):

1. **Uniform-prior guard.** If no file stands out even by raw commit count (top decile of
   files by `commit_count` accounts for less than 20% of total commits), there's no basis
   for a ranking — prints `(uniform prior — no ranking to show; all files equally likely)`
   and returns an empty list rather than a manufactured one.
2. **Low-label-density gate.** If the repo has *some* fix-commit history but the function-level
   positive rate is low (5–30% of functions, with a Shannon-entropy floor to filter out
   near-degenerate splits) — plenty of history, not enough confirmed fixes to trust a
   supervised model — routes to the same label-free anomaly score as gate 3 below.
3. **Gini-coefficient gate** (fallback). Computes the Gini coefficient of `commit_count`
   across all functions:
   - **High concentration** (a few files dominate commit activity, Gini ≥ 0.55) → the
     existing formula score (`activity_risk`/`lrs`) is already a sufficient day-one
     ranking; no model needed.
   - **Low concentration** (Gini < 0.55) → fits a label-free `IsolationForest` on an
     8-feature history vector (`commit_count`, `author_count`, `author_entropy`,
     `burst_score`, `isolation_rate`, `age_days`, `last_touch_days`, `authors_90d`) and
     ranks by anomaly score.

Both anomaly routes (gate 2 and the low-Gini branch of gate 3) use the same
`IsolationForest` implementation — memory-bounded, streaming, no full-matrix
intermediate structure. Neither requires a `.hotspots/ranker.json` model file.

**Gini dead zone.** The Gini-coefficient gate is stateless and per-run — a repo whose
Gini sits just under the 0.55 low threshold could flip from `formula` to `anomaly` on
a marginal commit. Set `cold_start_gini_dead_zone` in `.hotspotsrc.json` (default `0.0`)
to widen the ambiguous middle zone that already defaults to `formula`: the effective
low threshold becomes `0.55 - cold_start_gini_dead_zone`, so Gini values in
`[0.55 - cold_start_gini_dead_zone, 0.55)` route to `formula` instead of `anomaly`.
The 0.55 and 0.60 constants themselves are unchanged; the dead zone only shrinks the
`anomaly` region. Must be non-negative and less than 0.55.

```json
{
  "cold_start_gini_dead_zone": 0.03
}
```

### `hotspots coordinate [PATH]`

Reports ownership risk for a set of files before work starts. Needs only git history (no snapshot or AST analysis) and prints JSON. See
[USAGE.md](USAGE.md#hotspots-coordinate) for a longer walkthrough.

```
hotspots coordinate [PATH] (--files a.rs,b.rs | --diff | --staged)
```

| Flag | Description |
|---|---|
| `--files <FILES>` | Comma-separated file paths to analyze |
| `--diff` | Derive the file set from a unified diff read on stdin |
| `--staged` | Derive the file set from `git diff --cached --name-only` |

JSON output only today. `PATH` defaults to `.`.

Output is the master-schema envelope's `coordinate` section (`schema_version: 6`):
- `ownership`: per-file `author_count`, `author_entropy`, `newcomer_rate`, and `knowledge_mode` (`"concentrated"` for narrow ownership: at most 3 authors and normalized entropy below 0.5; otherwise `null`).

There is no `recommendation` field, and no `within_set`/`hidden_dependencies` coupling
facts either — both removed in hotspots 2.0 for lack of validated evidence. See
[USAGE.md](USAGE.md#hotspots-coordinate) for the full rationale and an example walkthrough.

### `hotspots prune`

Reduce the disk space used by the `.hotspots/` snapshot history. Two independent actions,
at least one of which must be given:

```
hotspots prune --unreachable [--older-than DAYS] [--dry-run]
hotspots prune --compact LEVEL [--dry-run]
```

| Flag | Description |
|---|---|
| `--unreachable` | Remove snapshots whose commit is no longer reachable from `refs/heads/*`, `refs/tags/*` or `refs/remotes/*` (after a force-push or branch deletion), and snapshots that reference a commit no longer in git history |
| `--older-than DAYS` | With `--unreachable`: only remove unreachable snapshots older than this many days |
| `--compact LEVEL` | Compact the retained history: `0` keeps full snapshots (the default, and the way to undo a compaction setting), `1` stores intermediate snapshots as deltas against their parent, `2` drops snapshots in which no function changed band |
| `--dry-run` | Report what would change without modifying any files |

**Why it exists.** Every `analyze --mode snapshot` run writes a snapshot, so history grows
with every commit analyzed. `prune` is the cleanup for that growth.

**When to use it.** Run `--unreachable --dry-run` after force-pushes or deleting branches;
run `--compact 1` on a long-lived repo whose `.hotspots/` directory has grown large. Level 2
loses per-commit detail, so use it only when you need band-change history rather than every
score.

**Reading the output.** It prints the number of snapshots pruned or converted and, for
compaction, the storage saved. `--dry-run` prints the same numbers without changing anything.

### `hotspots config`

```bash
hotspots config show              # show resolved config (merged defaults + file)
hotspots config show --path FILE  # show specific file
hotspots config validate          # validate auto-discovered config (exit 1 on failure)
hotspots config validate --path FILE
```

### Global flags

```bash
hotspots --help
hotspots --version
```

### Environment variables

- `NO_COLOR` — disable ANSI colors in text output
- `GIT_DIR`, `GIT_WORK_TREE` — override git repository location
- `GITHUB_EVENT_NAME=pull_request` — triggers merge-base comparison in delta mode
- `CI_MERGE_REQUEST_IID` (GitLab), `CIRCLE_PULL_REQUEST` (CircleCI), `TRAVIS_PULL_REQUEST` (Travis) — same effect

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (or warnings only) |
| 1 | Error or blocking policy failure |
| 2 | Auto-analysis failed (`hotspots diff --auto-analyze` only) |
| 3 | Snapshot missing (`hotspots diff` only) |

---

## Metrics

### The four structural metrics

**CC — Cyclomatic Complexity**
Number of independent decision paths. Counts: `if`, `else if`, `for`, `while`, `do/while`, `case`, `catch`, `&&`, `||`, ternary. A function with no branches has CC 1.

**ND — Nesting Depth**
Maximum depth of nested control structures (`if`, loops, `try`/`catch`, `switch`). Each additional level degrades readability non-linearly. ND ≥ 5 almost always warrants refactoring.

**FO — Fan-Out**
Distinct functions called from within this function. Each call segment in a chained expression counts independently (`foo().bar().baz()` = 3). High FO = high external coupling.

**NS — Non-Structured Exits**
Count of early returns, throws, breaks, and continues (excluding the final tail return). Scattered exits make control flow hard to trace and postconditions hard to reason about.

**LOC — Lines of Code**
Physical line count. Reported for context only; it is not part of the LRS score.

### LRS formula

```
R_cc = min(log2(CC + 1), 6.0)     # logarithmic, capped at 6
R_nd = min(ND, 8.0)               # linear, capped at 8
R_fo = min(log2(FO + 1), 6.0)     # logarithmic, capped at 6
R_ns = min(NS, 6.0)               # linear, capped at 6

LRS = 1.0×R_cc + 0.8×R_nd + 0.6×R_fo + 0.7×R_ns
```

Logarithmic scaling for CC and FO: going from CC 1→4 matters more than CC 40→44. Linear for ND and NS: each additional level contributes uniformly. Caps prevent a single extreme value from dominating.

**Theoretical range:** 1.0 (trivial) to 20.2 (all four at cap).

**Weight rationale:** CC (1.0) = primary defect correlate; ND (0.8) = captures complexity CC can miss; NS (0.7) = implicit exit conditions; FO (0.6) = external coupling weighted lower.

### Risk bands

| Band | LRS range | Typical action |
|---|---|---|
| Critical | ≥ 9.0 | Refactor now |
| High | 6.0–8.9 | Refactor next time you touch it |
| Moderate | 3.0–5.9 | Monitor; block increases in CI |
| Low | < 3.0 | Leave alone |

Thresholds are configurable.

### Field stability: absolute vs. population-relative

Output fields fall into two categories with different stability guarantees. This
matters when diffing output commit-to-commit or gating CI on it:

| Field | Category | Behavior |
|---|---|---|
| `lrs` | Absolute | Depends only on that function's own metrics (`cc`, `nd`, `fo`, `ns`). Unchanged unless the function itself changes. |
| `activity_risk` | Absolute | `lrs` plus its file's git signals on a fixed scale (see Activity Risk below). Does not depend on any other function. Changes when the function, or any commit touching its file, changes. |
| `band` (risk band) | Absolute | A fixed threshold on `lrs` (see Risk bands above). Moves only when the function's own score crosses a threshold. |
| `tier` | Population-relative | `active` when the file's touch count is above the repo median or it changed in the last 30 days. The median moves with the rest of the repo, so a function can flip tier even if its own file did not change. |

**Implication for CI gates:** if you want a gate that only fires when a function's
*own* risk changed — not when the rest of the repo's distribution shifted — key it on
`lrs`, `activity_risk`, and `band`. Gates keyed on
`tier` can trip on a commit that didn't touch the flagged function at all, because
those labels are recomputed relative to the whole population on every run.

No output field is renamed or restructured by this distinction — it is purely
documentation of behavior that already exists in `compute_percentiles`
and `compute_tiers` (`hotspots-core/src/snapshot.rs`).

### Activity Risk Score (snapshot mode)

Combines LRS with the file's git history into one absolute score. Each term is scaled
to [0, 1] on a fixed scale, so no term swamps the others and the score never depends on
the rest of the repo:
```
Activity Risk = LRS / 20.2
  + min(log2(1 + lines_added_and_deleted) / 14, 1)   # churn (180-day window)
  + min(touch_count / 50, 1)                          # touch frequency (365-day window)
  + max(0, 1 − days_since_last_change / 35)           # recency
```
The scale constants are fixed in the code (`hotspots-core/src/scoring.rs`) and are not
configurable. The range is roughly 0 to 4; a missing signal contributes 0, so with no
git data Activity Risk is just `LRS / 20.2`. `risk_factors` in the JSON output breaks the
score into `{complexity, churn, touch, recency}`.

**How to read it, and why you can't "fix" it.** Activity Risk estimates where attention is
likely to be needed next, not how good the code is. It is built from how recently and how
often a file has changed, so working on a file raises it for every function in that file
(churn, touch and recency are file-level), including functions you did not edit. A fix
therefore cannot lower it directly; it decays on its own as the file goes quiet (recency
over about 35 days, churn over 180 days, touch over 365 days). On a feature branch,
recency is measured from before the branch diverged, so your own edits do not reset it.
The part a developer can change and see improve is LRS and its band, so use LRS for "did
this refactor help" and Activity Risk for "where should I look first". Within each triage
bucket, functions are sorted by Activity Risk, so editing one function can reorder its
file's other functions in that list.

Only the four terms above feed the score. Fan-in, cyclic dependency, dependency depth,
burst and neighbor churn do not: in our evaluation fan-in and cyclic dependency added
nothing measurable, dependency depth was sign-inconsistent across repos, and the burst
score is a full-history, effectively monotonic value that would make Activity Risk a
one-way ratchet. Cyclic dependency, dependency depth, neighbor churn, PageRank and
betweenness are no longer computed. `fan_in` is still reported per function, and
`burst_score` is still computed as a cold-start ranking feature (`trainer`).

`burst_score` is a sliding 30-day-window max/mean commit ratio per file (always ≥ 1.0;
higher means commits cluster into frantic bursts rather than steady, spread-out
changes). For each commit touching a file, it counts how many of that file's commits
fall within the following 30 days, then divides the largest such count by the mean —
a file with 5 commits crammed into one week scores higher than one with 5 commits
spread evenly across a year, even though both have `commit_count = 5`.

Computed as part of the same single full-history `git log --name-only` pass used for
the other cold-start signals (`commit_count`, `author_count`, `author_entropy`,
`isolation_rate`, `age_days`, `last_touch_days`) — one subprocess per `hotspots
analyze` run, not one per file and not a separate walk from those other six signals.
Files with fewer than 2 commits get the baseline `1.0` (no burst signal); files with
no matching commit history keep `burst_score = None`.

Validated against real-world defect data: it's one of the strongest signals in a
CVE/OSV-linked-file logistic regression (largest standardized coefficient of 5
candidate signals, positive across all leave-one-repo-out folds tested) in our cross-repo evaluation.

**Touch window:** the git-log window behind `touch_count` (renamed from `touch_count_30d`
in hotspots 2.0 — the name was always wrong, see the field's rename note) and
`days_since_last_change` is 365 days by default (`git::TOUCH_WINDOW_DAYS`), based
on our pre-registered 13-repo cutoff-safe evaluation — 365 days
gave the strongest, monotonic gain of every window tested, with a size-confound check
ruling out "it's just counting more commits." On a high-commit-velocity repo this is a
real, measured wall-clock cost (a single git subprocess call, but scanning more history:
+16ms on a moderate-velocity repo, up to +943ms on a very active one in testing) —
use `--skip-touch-metrics` to skip git history entirely if that cost matters more than
the ranking-quality gain. The window is fixed and not configurable.

### Call graph metrics (snapshot mode)

- **Fan-in** — functions that call this function (blast radius)

Fan-in is the only call graph metric still computed. PageRank, betweenness, strongly
connected components, dependency depth and neighbor churn were removed (no validated
consumer).

### Activity tier

Each function gets a `tier` of `active` or `quiet`, based on activity alone:

- `active`: its file has more touches (in the touch window) than the repo median, or it
  changed in the last 30 days.
- `quiet`: everything else.

**Why it exists.** The tier answers "is this code being worked on right now?". It makes no
claim about how complex the code is; `band` carries that separately, so you can still
select "complex and active" by combining the two. In our evaluation, adding a complexity
gate to the activity set did not improve how well it predicted where fixes land, which is
why the tier is activity-only.

**How to use it.** The triage view groups functions into `active` and `quiet`, each sorted
by `activity_risk`. Start with the top of `active`. A function in `quiet` with a high
`band` is dormant structural risk: not urgent, but worth knowing about before the file
becomes active.

`tier` is population-relative (the median is taken from the current run) — see "Field
stability" above before keying a CI gate on it.

---

## Configuration

Config file is auto-discovered from project root in this order:
1. `--config <path>` CLI flag (explicit override)
2. `.hotspotsrc.json`
3. `hotspots.config.json`
4. `"hotspots"` key in `package.json`

The project root is determined by walking up from the analyzed path to find `.git`. CLI flags take precedence over config file values.

Validate: `hotspots config validate` / Inspect resolved: `hotspots config show`

### Full schema

```json
{
  "schema_version": 1,
  "include": ["src/**/*.ts"],
  "exclude": [
    "**/*.test.ts", "**/*.spec.ts",
    "**/node_modules/**", "**/__tests__/**", "**/__mocks__/**",
    "**/dist/**", "**/build/**", "**/vendor/**",
    "**/*.pb.go", "**/zz_generated*.go"
  ],
  "min_lrs": 0.0,
  "top": null,
  "policy": {
    "critical_introduction": "warn",
    "critical_introduction_reason": "eval/ scripts are one-shot research code reviewed case-by-case, not shipped services — approved by @stephenc222 2026-07-06",
    "excessive_risk_regression": "block"
  }
}
```

**Fixed, not configurable.** The LRS component weights, the risk band cutoffs (3 / 6 / 9), the activity-risk scale constants, the touch window (365 days), the churn window (180 days), the co-change window (90 days, minimum 3 co-changes) and the rapid-growth cutoff (50%) are built in. They were previously configurable but nothing in our evaluation supported tuning them per repo, and one shared definition keeps scores comparable across repos and over time. A config file that still sets one of these keys is rejected as an unknown field.

**Validation rules:**
- `policy.*` values must be one of `"block"`, `"warn"`, `"off"`
- `policy.<name>_reason` is **required** (non-empty) whenever `policy.<name>` is not `"block"`
- Unknown fields are rejected (to catch typos)
- `schema_version` must not exceed the version this build of hotspots supports

**`schema_version`:** defaults to the current config schema version (currently `1`) when
omitted, so existing `.hotspotsrc.json` files without it keep working unchanged. Bumped
only when a breaking change to config semantics needs migration or explicit
detection, mirroring `schema_version` on snapshot and delta files.

**`policy`:** severity overrides for the two blocking CI policies. Both default to
`"block"`. `critical-introduction` fires identically whether a function is brand-new or
an existing function that regressed to Critical — a Critical function needs review
either way, so there is no separate new-vs-regressed knob, only overall severity.
Set to `"warn"` to report without failing CI (e.g. a research repo where new
one-shot scripts are expected to score high on introduction), or `"off"` to disable
the policy entirely. `policy` only changes what happens once something *is* Critical; the band cutoffs
themselves are fixed.

Downgrading a policy below `"block"` requires a `<name>_reason` string — mirroring the
`// hotspots-ignore: <reason>` convention for per-function suppression — so that anyone
reviewing a `.hotspotsrc.json` diff sees *why* a blocking gate was weakened, not just
that it was. `hotspots config validate` rejects a downgrade with a missing or
whitespace-only reason. This does not make the override unbypassable — anyone with
commit access to the config can still weaken it, the same as anyone with access to a CI
workflow file can remove a required check — but it does mean the change can't be silent.

---

## JSON Schema

### Schema versions

As of hotspots 2.0, every JSON-emitting command shares **one envelope and one
`schema_version` (currently `6`)** — the pre-2.0 model of separate version
constants per command (`AGENT_SNAPSHOT_SCHEMA_VERSION`, `DELTA_SCHEMA_VERSION`,
a bare unversioned `--axes` map, etc.) was unified in the 2.0 master-schema cut.
The envelope always carries `schema_version`, `commit`, and `analysis`; everything
else is an **optional section**, present only when that command/flag computed it:

| Section | Present when | Contents |
|---|---|---|
| `functions` | always | Full per-function array (`FunctionSnapshot`) |
| `summary` | always | Population-level aggregate stats |
| `triage` | `--mode snapshot`, not `--all-functions` | `active`/`quiet` groups, each with a count and the top functions by `activity_risk` |
| `architecture` | `--all-functions` | `file_risk`/`modules` aggregates |
| `co_change` | `--all-functions` | File co-change pairs |
| `coordinate` | `hotspots coordinate` | Ownership/knowledge-mode facts for a file set |
| `delta` | `hotspots diff` | `deltas` array + PR-wide aggregates |
| `axes` | `--axes` | Risk/Coupling/Ownership per-axis rankings |

`--all-functions` toggles which sections a snapshot-mode run populates (full
`architecture`/`co_change` instead of the triage buckets) — it is not a
different schema version, just a different set of sections inside the same
envelope. Always check `schema_version` before consuming output in tooling, and
check for a section's presence (`Option`/possibly-absent key) rather than
assuming it's always there.

`analysis.formula_version` is a separate integer that tracks the scoring
formula (the `activity_risk` constants, `LrsWeights`, `RiskThresholds`), bumped
only when a weight, threshold or scale constant changes. `analysis.tool_version`
changes on every release; `formula_version` does not — use it in `hotspots
diff` tooling to tell a score delta caused by a tool upgrade apart
from one caused by a real code change.

**`formula_version` vs. config `schema_version` — not the same thing.**
`formula_version` tracks changes to hotspots' *built-in* weights/thresholds/scales
(the `scoring.rs` constants, `LrsWeights::default()` etc.) and is unaffected by a user's own
`.hotspotsrc.json` — a repo with fully custom weights sees the same
`formula_version` as one with no config at all, because *their* scoring didn't
change even when the shipped defaults did. Config `schema_version` (above) tracks
the *shape/meaning* of the config file format itself — e.g. a field being renamed
or changing units — independent of what any particular default value is. A release
that changes a default weight's value bumps `formula_version` only; a release that
changes what a config field *means* bumps `schema_version` (and `formula_version`
too, if that also changes computed scores). Note `formula_version` today only
covers default-value changes, not changes to the scoring formula's structure
(e.g. adding/removing a term from `compute_activity_risk`) — a structural change
is not guaranteed to bump it.

### Function fields (`functions[]`, `FunctionSnapshot`)

```json
{
  "function_id": "src/api/billing.ts::processPlanUpgrade",
  "file": "src/api/billing.ts",
  "line": 142,
  "language": "TypeScript",
  "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3, "loc": 48 },
  "lrs": 12.4,
  "band": "critical",
  "suppression_reason": null,
  "churn": { "lines_added": 156, "lines_deleted": 89 },
  "touch_count": 12,
  "days_since_last_change": 3,
  "callgraph": { "fan_in": 8 },
  "activity_risk": 2.18,
  "risk_factors": { "complexity": 0.59, "churn": 0.60, "touch": 0.24, "recency": 0.74 },
  "tier": "active",
  "subsystem": "",
  "authors_90d": 2,
  "directed_coupling": 0.31
}
```

This is the full per-function shape in `functions[]` — present on every `--mode
snapshot` run, with or without `--all-functions`. `suppression_reason` is omitted (not null)
when no suppression is present. `risk_factors` is the `{complexity, churn,
touch, recency}` breakdown that sums into `activity_risk` (see Activity Risk
above) — `fan_in`/`cyclic_dependency`/`depth`/`burst` have no field here; they were
removed from `RiskFactors` in hotspots 2.0.

**This is distinct from the triage view's per-function entries** (`triage.{active,quiet}.top[]`,
present only on `--mode snapshot` *without* `--all-functions`): those use a
slimmer shape (`function`, `file`, `line`, `band`, `tier`,
`lrs`, `activity_risk`, `metrics`, `touch_count`, `days_since_last_change`,
`fan_in`) that omits
`risk_factors`/`callgraph`'s other fields/`suppression_reason`/`subsystem`/
`authors_90d`/`directed_coupling`. Don't assume the two shapes are interchangeable
when writing a consumer against this schema.

`lrs`, `activity_risk`, and `band` are absolute — stable across runs unless the
function's own metrics/history change. `tier` is population-relative — it can change between
runs purely because other functions in the repo changed, even when this function did
not. See "Field stability" under Metrics above.

### Aggregates (`--all-functions`)

**`aggregates.file_risk`** — per-file ranked by `file_risk_score`:
```
file_risk_score = max_cc×0.4 + avg_cc×0.3 + log2(fn_count+1)×0.2
```
**Unvalidated heuristic** — these component weights have no validation
behind them, unlike `lrs`/`activity_risk`. Treat `file_risk_score` as a convenience
ranking, not a calibrated signal. (The formula previously included a `churn_factor×0.1`
term and a `file_churn` field; both were removed — `file_churn` was always 0 in
practice due to a file-path-matching bug in `populate_churn`'s lookup, confirmed via
our testing, including a direct smoke test against this repo.)

**Validated for within-diff review triage, not for finding a bug repo-wide.**
In our evaluation, `file_risk_score` was tested two different ways. Given a bounded
diff a reviewer is already looking at (e.g. a PR's changed files), ranking those files
by `file_risk_score` put the file that later needed a fix in the #1 slot 57.3% of the
time (134/234 real fix-commit instances, 25 repos/8 languages), vs. 19.7% for a raw
lines-changed baseline — a real win on 23 of 25 repos, tied on the other 2, losing on
none (McNemar exact test, p=2.65e-19). A sample-size convergence check confirms the
result is stable, not an artifact of how much data was collected. Given the whole repo
with no bounded diff, three separate attempts (an LLM agent with `hotspots` as an
optional tool, the same with an instance-aware prompt, and the triage view handed over
as static context) all converged on the same low hit rate as plain investigation with
no tool at all. Read `file_risk_score` as *"of the files already in front of you, which
deserves the closest look,"* not as a tool for finding which file in an entire
repository has a bug. (n=234 across 25 repos; still not formally pre-registered. A follow-up
found `max_cc` alone — the
single most complex function in the file — carries nearly all of this signal; the other
two terms add little at the margin, an explainability note, not a formula change.)

No "refactor soon" cutoff is officially recommended for `file_risk_score` in this doc.
If you want a rough high-risk marker, our 16-repo/82,574-file empirical
distribution puts the 90th percentile at approximately 7.0 (p50 ≈ 2.9, p95 ≈ 9.6) —
this is a percentile convenience marker from the real score distribution, not a
defect-correlation-validated threshold.

**`aggregates.co_change`** — file pairs that change together in the same commit:
```json
{
  "file_a": "hotspots-cli/src/main.rs",
  "file_b": "hotspots-core/src/aggregates.rs",
  "co_change_count": 14,
  "coupling_ratio": 0.78,
  "has_static_dep": false,
  "risk": "high"
}
```
`risk: "expected"` = a static import exists; co-change is explained.

### Delta output

`hotspots diff`'s JSON output is the 2.0 master-schema envelope (`schema_version: 6`,
unified with the snapshot schema version): the diff itself nested under `delta`. There
is no separate top-level `commit` — `delta.commit.sha` already carries the head commit,
and the master schema has zero field duplication (no value appears twice under
different paths), so it isn't repeated at the envelope's top level too.

```json
{
  "schema_version": 6,
  "delta": {
    "commit": { "sha": "abc123", "parent": "def456" },
    "baseline": false,
    "deltas": [{
      "function_id": "src/api/billing.ts::processPlanUpgrade",
      "status": "modified",
      "before": { "lrs": 11.0, "band": "high", "metrics": { "cc": 13, "nd": 3, "fo": 7, "ns": 2 } },
      "after":  { "lrs": 12.4, "band": "critical", "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 } },
      "delta": { "cc": 2, "nd": 1, "fo": 1, "ns": 1, "lrs": 1.4 },
      "band_transition": { "from": "high", "to": "critical" }
    }],
    "policy": {
      "failed": [{ "id": "critical-introduction", "severity": "blocking", "message": "..." }],
      "warnings": []
    },
    "aggregates": {
      "pr_summary": {
        "fn_changed_lines": 23,
        "band": "critical",
        "new_count": 0,
        "modified_count": 1,
        "deleted_count": 0,
        "regression_count": 1,
        "improvement_count": 0,
        "band_upgrades": 1,
        "policy_blocking": true
      }
    }
  }
}
```

Delta statuses: `new`, `deleted`, `modified`, `unchanged` (unchanged omitted by default).

**`delta.aggregates.pr_summary`** — collapses every changed function into a single PR-wide
view. `band` is the highest risk band reached by any new/modified function's `after`
state; the counts and `band_upgrades` summarize what changed.

**`fn_changed_lines`** is the count of diff-changed lines (`git diff -U0`) that fall inside a
touched function's line span, summed across the whole diff. In our evaluation, tested directly
against the real compiled binary, it was the strongest predictor of which PRs later need a
defect fix, beating every LRS-weighted score tried on every repo tested. That is why no
composite "PR risk score" is reported: read `fn_changed_lines` for "is this PR risky" and
`band` for "what is the worst code in this diff". Present whenever `aggregates` is attached
(always true for `hotspots diff` output).

---

## Supported Languages

| Language | Extensions |
|---|---|
| TypeScript | `.ts`, `.tsx`, `.mts`, `.cts`, `.mtsx`, `.ctsx` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs`, `.mjsx`, `.cjsx` |
| Go | `.go` |
| Python | `.py`, `.pyw` |
| Rust | `.rs` |
| Java | `.java` |
| C / C headers | `.c`, `.h` |
| C# | `.cs` |
| Vue | `.vue` |
| Astro | `.astro` |

All languages have full parity across all metrics and features. Vue and Astro score only the
`<script>` block / `---` frontmatter fence respectively — template/markup sections are not
analyzed for complexity.

**JSX note:** `.jsx` and `.tsx` files support JSX syntax. Plain `.js` files also enable JSX parsing (React webpack convention). JSX elements do not add CC; control flow in JSX (`&&`, ternary) does.

---

## Scoring Changelog

All changes to formulas, weights, thresholds, or ranking rules are tracked in git commit history. The LRS formula and default weights have been stable since v1.0. The trained ranker feature was introduced in a later release; the current model is `model_version 6` (9 features). Check `CHANGELOG.md` for version-specific details.

- The `authors_90d` and `directed_coupling` `--explain` phrases were reworded from directive to descriptive language (e.g. "no clear owner" → "no single frequent owner in recent history") after four independent tests found no outcome benefit from acting on the ownership/coupling diagnosis.
