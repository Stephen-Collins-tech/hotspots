# Usage Guide

## Basic Analysis

```bash
hotspots analyze src/              # text output, all functions
hotspots analyze src/ --top 20     # top 20 by LRS
hotspots analyze src/ --min-lrs 5  # only LRS ≥ 5.0
hotspots analyze src/ --format json
hotspots analyze src/ --format jsonl | grep '"band":"critical"'
```

## Snapshot Mode

Snapshot mode captures a full analysis tied to the current git commit. It enables:
- Delta comparisons (`--mode delta`)
- Trend tracking (`hotspots trends`)
- HTML trend charts
- Trained ranker scoring

```bash
# Capture current state
hotspots analyze . --mode snapshot

# Snapshot with detailed per-function explanation
hotspots analyze . --mode snapshot --format text --explain --top 10

# Snapshot without saving to disk
hotspots analyze . --mode snapshot --no-persist --format json

# Regenerate an existing snapshot (e.g. after config change)
hotspots analyze . --mode snapshot --force
```

Snapshots are stored as `.hotspots/snapshots/<commit-sha>.json.zst` and are immutable by default.

### Higher-level views (snapshot mode only)

```bash
# File-level risk table (ranked by composite file_risk_score)
hotspots analyze . --mode snapshot --format text --level file

# Module instability (Robert Martin's metric at directory level)
hotspots analyze . --mode snapshot --format text --level module
```

File risk score = `max_cc×0.4 + avg_cc×0.3 + log2(fn_count+1)×0.2 + churn_factor×0.1`. Module instability near 0 = everything depends on it (risky to change); near 1 = safe to change. High-complexity + low-instability modules are the priority targets.

## Delta Mode

Delta mode compares the current state against the parent commit snapshot.

```bash
hotspots analyze . --mode delta --format text
hotspots analyze . --mode delta --policy        # exit 1 on blocking violations
hotspots analyze . --mode delta --format json   # machine-readable output
```

In PR context (GitHub Actions, GitLab CI, CircleCI, Travis), delta mode automatically compares against the merge-base rather than the direct parent. Detection is via environment variables (`GITHUB_EVENT_NAME=pull_request`, `CI_MERGE_REQUEST_IID`, etc.).

## `hotspots diff`

Compare snapshots between any two git refs (not just parent → HEAD):

```bash
hotspots diff main HEAD               # compare branch vs main
hotspots diff v1.0.0 v2.0.0          # compare releases
hotspots diff main HEAD --top 10 --policy
hotspots diff main HEAD --format json
```

Both refs must have existing snapshots. If one is missing:
```bash
git checkout main && hotspots analyze . --mode snapshot
git checkout my-branch && hotspots analyze . --mode snapshot
hotspots diff main HEAD
```

`--top N` applies *after* policy evaluation, so violations outside the top N are still detected.

**Exit codes:** 0 = success, 1 = policy failure, 2 = auto-analysis failed, 3 = snapshot missing.

## Policy Engine

The policy engine runs in delta mode (`--mode delta --policy` or `hotspots diff ... --policy`).

**Blocking by default (exit code 1) — severity configurable, see below:**
- `critical-introduction` — new or existing function crosses LRS ≥ 9.0
- `excessive-risk-regression` — LRS increases by ≥ 1.0 on a modified function

**Warnings (exit code 0, informational):**
- `watch-threshold` — function entering watch range (default LRS 2.5–3.0)
- `attention-threshold` — function entering attention range (default LRS 5.5–6.0)
- `rapid-growth` — LRS increase > 50% on any function
- `suppression-missing-reason` — `// hotspots-ignore:` with no reason text
- `net-repo-regression` — total LRS increased across all changes (any positive delta)

Configure thresholds in `.hotspotsrc.json`:
```json
{
  "warning_thresholds": {
    "watch_min": 2.5,
    "watch_max": 3.0,
    "attention_min": 5.5,
    "attention_max": 6.0,
    "rapid_growth_percent": 50.0
  }
}
```

### Downgrading a blocking policy for your repo

`critical-introduction` fires the same way whether a function is brand-new or an
existing function that regressed — a Critical function needs review either way. But
some repos have a legitimately different baseline: a research repo shipping dense,
one-shot experiment scripts will routinely introduce Critical-scoring functions on
day one, and blocking CI on every one of them is noise, not signal.

If that's your repo, downgrade the policy in `.hotspotsrc.json` instead of annotating
every function with `// hotspots-ignore` or excluding files forever (which also loses
future regression detection on them):

```json
{
  "policy": {
    "critical_introduction": "warn",
    "critical_introduction_reason": "eval/ scripts are one-shot research code reviewed case-by-case, not shipped services — approved by @yourhandle 2026-07-06"
  }
}
```

Each policy accepts `"block"` (default), `"warn"` (report, exit 0), or `"off"`
(skip entirely). Downgrading below `"block"` **requires** a `<name>_reason` string —
`hotspots config validate` rejects a missing or blank one. This mirrors the
`// hotspots-ignore: <reason>` convention: the point isn't to make the gate
unbypassable (anyone with commit access to `.hotspotsrc.json` can weaken it, same as
anyone with access to a CI workflow file can remove a required check) — it's to make
sure the change can't be silent. A reviewer of the config diff sees *why* the gate was
weakened, in the same place they see *that* it was.

The same `"block"`/`"warn"`/`"off"` + `_reason` pattern applies to
`excessive_risk_regression` via `policy.excessive_risk_regression` /
`policy.excessive_risk_regression_reason`. Full field reference:
[Configuration → `policy`](/REFERENCE#configuration).

## CI/CD Setup

### GitHub Action (recommended)

```yaml
name: Hotspots
on:
  pull_request:
  push:
    branches: [main]

jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0        # required for git history
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

The action posts PR comments, generates HTML reports, and fails on blocking policy violations.

**Action inputs:**

| Input | Default | Description |
|---|---|---|
| `github-token` | — | Required for PR comments |
| `path` | `.` | Path to analyze |
| `policy` | `critical-introduction` | `critical-introduction`, `strict`, `moderate`, `custom` |
| `fail-on` | `error` | `error`, `warn`, `never` |
| `config` | auto-discover | Path to config file |
| `version` | `latest` | Pin a specific version |
| `post-comment` | `true` | Post PR comment |

**Action outputs:** `violations` (JSON), `passed` (bool), `summary` (markdown), `report-path`, `json-output`.

**Monorepo:**
```yaml
jobs:
  frontend:
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/frontend
          github-token: ${{ secrets.GITHUB_TOKEN }}
  backend:
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          path: packages/backend
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

**Upload HTML report as artifact:**
```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report-${{ github.sha }}
    path: ${{ steps.hotspots.outputs.report-path }}
    retention-days: 30
```

### Manual CI (GitLab, CircleCI, Jenkins, etc.)

```bash
# Fail CI on blocking violations
hotspots analyze src/ --mode delta --policy
```

For GitLab CI:
```yaml
hotspots:
  stage: analyze
  image: rust:latest
  before_script:
    - cargo install hotspots-cli
  script:
    - hotspots analyze src/ --mode delta --policy
  artifacts:
    paths: [.hotspots/report.html]
    expire_in: 1 week
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

**Troubleshooting:**
- `"failed to extract git context"` — use `fetch-depth: 0` in checkout
- `"merge-base not found"` — fetch the base branch explicitly: `git fetch origin $BASE_BRANCH`
- PR comments not posting — ensure `pull-requests: write` permission and `github-token` is set

## Output Formats

### Text

```bash
hotspots analyze src/ --format text             # basic table
hotspots analyze . --mode snapshot --format text --explain  # with per-function detail
hotspots analyze . --mode snapshot --format text --level file
```

Color-coded by risk band (critical=red, high=yellow, moderate=blue, low=green). Disable: `NO_COLOR=1 hotspots analyze ...`.

The `--explain` view adds per-function risk breakdown. When a trained ranker is active (run `hotspots train` first), it also emits a `✦` phrase line for each CRITICAL/HIGH function derived from which signals are in the top 20th percentile for the repo — e.g.:

```
  0.56  hotspots-core/src/aggregates.rs:694  compute_module_instability_from_edges
         ✦ Tightly coupled to other hotspots, depended on by many callers, and high cyclomatic complexity.
           Worth prioritising before next release.
```

No `✦` lines appear without a trained ranker.

### JSON

```bash
hotspots analyze src/ --format json
hotspots analyze . --mode snapshot --format json --all-functions  # full flat array (schema v2)
hotspots analyze . --mode snapshot --format json --include-models  # add model risk map
```

Default snapshot JSON uses schema v4 (triage-first structure: `fire`/`debt`/`watch`/`ok` buckets). Use `--all-functions` for the flat `functions` array (schema v2). Always check `schema_version` in tooling.

Useful `jq` patterns:
```bash
# Critical functions only
jq '.functions[] | select(.band == "critical")' output.json

# Count by band
jq '.aggregates.by_band' output.json

# Top 10 by LRS
jq '.functions | sort_by(.lrs) | reverse | .[0:10]' output.json

# Functions with a specific pattern
jq '.functions[] | select(.patterns[]? == "god_function") | .function_id' output.json
```

### JSONL (streaming)

One JSON object per line — ideal for pipelines and large repos:

```bash
hotspots analyze src/ --format jsonl | grep '"band":"critical"'
hotspots analyze src/ --format jsonl | jq -c 'select(.lrs > 9)'
```

### HTML

Interactive self-contained report with sortable table, risk landscape scatter plot, pattern breakdown panel, and trend charts (requires ≥ 2 snapshots):

```bash
hotspots analyze . --mode snapshot --format html
open .hotspots/report.html   # macOS
```

### SARIF (GitHub Code Scanning)

```bash
hotspots analyze . --mode snapshot --format sarif --output .hotspots/results.sarif
```

Requires `--mode snapshot`. Maps bands to SARIF levels: critical→error, high→warning, moderate→note. Integrate with GitHub code scanning:

```yaml
- name: Run Hotspots
  run: hotspots analyze . --mode snapshot --format sarif --output .hotspots/results.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: .hotspots/results.sarif
```

## Suppression Comments

Suppress CI policy failures while keeping the function visible in reports:

```typescript
// hotspots-ignore: legacy payment processor, rewrite scheduled Q2 2026
function legacyBillingLogic() { ... }
```

Rules:
- Comment must be on the line **immediately before** the function (no blank line between)
- Format: `// hotspots-ignore: <reason>`
- Reason is required (missing reason triggers a warning, not a hard failure)
- Suppressed functions still appear in all reports with a `suppression_reason` field
- Suppressed functions still count toward net repo regression

Good reasons: complex algorithm with test coverage, generated code, migration pending with date. Bad reasons: "TODO fix this later", no reason at all.

## Touch Metrics

Touch metrics measure how often functions change in git history.

```bash
# Default: hybrid mode (file-level for most, per-function for active files)
hotspots analyze . --mode snapshot

# Full per-function precision (slower cold start, cached after first run)
hotspots analyze . --mode snapshot --per-function-touches

# File-level only (fastest, disables per-function cache)
hotspots analyze . --mode snapshot --no-per-function-touches

# Skip all git I/O (for very large repos, 50k+ functions)
hotspots analyze . --mode snapshot --skip-touch-metrics
```

Per-function touch results are cached in `.hotspots/touch-cache.json.zst`. First run on a new commit is slow; subsequent runs are fast.

Configure in `.hotspotsrc.json`:
```json
{
  "per_function_touches": false,
  "hybrid_touch_threshold": 5
}
```

## Snapshot Management

```bash
# Prune snapshots for deleted/force-pushed branches
hotspots prune --unreachable --dry-run
hotspots prune --unreachable --older-than 30

# Compact snapshot storage
hotspots compact --level 0

# Analyze trends across snapshot history
hotspots trends .
hotspots trends . --window 20 --top 10 --format text
```

`hotspots trends` reports risk velocities (LRS change per snapshot), hotspot stability (consistent top-K presence), and refactor effectiveness (sustained LRS reduction).

## Training a Repo-Specific Ranker

By default, hotspots ranks by LRS. Training fits a model from your repo's bug-fix history to re-rank based on which structural features actually predict bugs in *your* codebase.

```bash
# File-level labels (fast)
hotspots train .

# Blame-based function-level labels (more precise, slower)
hotspots train . --blame

# Train and immediately check whether the model is better than base LRS
hotspots train . --blame --eval
```

Example `--eval` output:
```
P@K evaluation (365-day fix-label window):
  K      P@K      base_rate
  10     0.400    0.084
  20     0.300    0.084
```

If `P@K` is well above `base_rate`, apply the model. If `P@K ≈ base_rate`, skip it — the default LRS ranking is just as good.

Once `.hotspots/ranker.json` exists, every `hotspots analyze` loads it automatically, re-scores triage quadrants, and prints which model class is in effect:

```
hotspots: using trained ranker (model class: Ridge)
```

Training requires: ≥ 50 functions in snapshot, ≥ 5 positive and ≥ 10 negative labels from fix commits. Fix keywords: `fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`.

Training is most valuable on repos with 1+ year of history, recognizable fix-commit conventions, and bug clusters in specific files. Use `--screen` to check suitability before fitting:

```bash
hotspots train . --screen   # check only, no model written
```

### Ridge vs. RandomForest: automatic model class selection

`hotspots train` doesn't always fit a RandomForest. Before training, it runs a quick
pre-flight comparison — the **regime screener** — to check whether a plain linear
model (Ridge regression) already predicts which functions were touched by fix commits
(the same fix-commit labels described above) as well as a forest does. If so, it fits
Ridge instead and skips RandomForest training entirely.

Why this matters: on repos where the relationship between features (churn, coupling,
LRS, etc.) and bug-proneness is essentially linear, a RandomForest adds complexity
(harder to reason about, slower to train, more prone to overfitting on ties) without
improving ranking quality. The screener catches this automatically so you don't have to
guess.

You'll see the verdict printed during training:

```
Model class: Ridge (regime=LINEAR, Δρ=+0.03) — RandomForest training skipped
```

or, when trees add real lift:

```
Model class: RandomForest (regime=STRONG, Δρ=+0.14)
```

Verdicts:

| Verdict | Meaning | Model used |
|---|---|---|
| `LINEAR` | Δρ < 0.03 — Ridge matches RandomForest | Ridge |
| `WEAK` | 0.03 ≤ Δρ ≤ 0.10 — modest tree advantage | RandomForest |
| `STRONG` | Δρ > 0.10 — trees clearly help | RandomForest |
| `UNRELIABLE` | Too few positive labels to trust the comparison | RandomForest (safe default) |

This selection is fully automatic — there's no flag to control it. The chosen model
class is saved in `ranker.json` and reused every time `hotspots analyze` scores your
repo, so you always know which kind of model produced your rankings.

## AI Integration

```bash
# Agent-optimized snapshot JSON (triage buckets + action text)
hotspots analyze . --mode snapshot --format json

# Flat array for tooling that expects a complete function list
hotspots analyze . --mode snapshot --format json --all-functions

# Delta for PR review context
hotspots analyze . --mode delta --format json

# Pipe critical functions to Claude/Cursor/Copilot
hotspots analyze src/ --format json | jq '.functions[] | select(.lrs > 9)'
```

## Hook Templates

```bash
# Print pre-commit and CI hook templates
hotspots init --hooks
```

Seed a baseline snapshot first:
```bash
hotspots analyze . --mode snapshot
hotspots init --hooks   # copy the hook template, install it
```

## Troubleshooting

**`"snapshot already exists and differs"`** — regenerate with `--force`.

**`"no parent snapshot found"` in delta mode** — run `hotspots analyze . --mode snapshot` on the parent commit first.

**`"failed to extract git context"`** — must be run inside a git repository.

**Snapshot mode text output requires `--explain` or `--level`** — text format in snapshot mode without one of these flags is an error.

**`--no-persist` and `--force` are mutually exclusive** — pick one.

**`--level` and `--explain` are mutually exclusive** — pick one.
