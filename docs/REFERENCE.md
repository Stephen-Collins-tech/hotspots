# Reference

## CLI Commands

### `hotspots analyze <path>`

Core analysis command. Scans source files, computes metrics, scores functions.

```
hotspots analyze <PATH> [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--format` | `text` | `text`, `json`, `jsonl`, `html`, `sarif` |
| `--mode` | — | `snapshot`, `delta`, `models` |
| `--top N` | none | Show top N functions by LRS |
| `--min-lrs F` | `0.0` | Filter functions below this LRS |
| `--config PATH` | auto | Path to config file |
| `--output PATH` | `.hotspots/report.html` | Output file (HTML/SARIF) |
| `--explain` | off | Per-function risk breakdown (snapshot+text only) |
| `--explain-patterns` | off | Show pattern trigger conditions |
| `--level` | — | `file` or `module` aggregate view (snapshot+text only) |
| `--policy` | off | Evaluate policies; exit 1 on blocking violations (delta only) |
| `--force` | off | Overwrite existing snapshot |
| `--no-persist` | off | Skip writing snapshot to disk |
| `--per-function-touches` | off | Use `git log -L` for precise touch counts (slow cold start) |
| `--no-per-function-touches` | off | Force file-level touch batching |
| `--skip-touch-metrics` | off | Skip all git log I/O (touch counts reported as 0) |
| `--all-functions` | off | Output flat array instead of triage buckets (snapshot JSON only) |
| `--include-models` | off | Add model risk map to JSON/HTML (snapshot only) |
| `--callgraph-skip-above N` | 50000 | Skip betweenness centrality if call graph > N edges |
| `--skip-gate` | off | Disable suppression gate P@10 check |
| `-j N` / `--jobs N` | CPU count | Parallel worker threads |

**Notes:**
- `--explain` and `--level` are mutually exclusive
- `--force` and `--no-persist` are mutually exclusive
- Snapshot mode text output requires `--explain` or `--level`
- SARIF requires `--mode snapshot`; HTML requires `--mode snapshot` or `--mode delta`
- `--policy` requires `--mode delta`

### `hotspots diff <base> <head>`

Compare snapshots between any two git refs. Both must have existing snapshots.

```
hotspots diff <BASE> <HEAD> [OPTIONS]
```

Accepts: branch names, tags, full/short SHAs, `HEAD~N` relative refs.

| Flag | Description |
|---|---|
| `--format` | `text` (default), `json`, `jsonl`, `html` |
| `--output PATH` | Write output to file |
| `--policy` | Evaluate policies; exit 1 on blocking violations |
| `--top N` | Limit to N changed functions by \|ΔLRS\| |
| `--config PATH` | Config file |
| `--auto-analyze` | Generate missing snapshots via git worktrees |

Exit codes: 0 = success, 1 = policy failure, 2 = auto-analysis failed, 3 = snapshot missing.

`--top` applies after policy evaluation — violations outside the top N are still detected.

### `hotspots train [PATH]`

Fit a RandomForest ranker from fix-commit history. Model saved to `.hotspots/ranker.json` and auto-loaded by `hotspots analyze`.

Before training, the command prints an estimate and (for repos with > 1,000 functions) prompts for confirmation:

```
hotspots train: 12914 functions · 200 trees · 365 days of git history (file-level labels) · estimated ~4m 30s
Proceed? [y/N]
  [10/200]  ~4m 5s remaining
  [100/200]  ~2m 1s remaining
  [200/200]
Trained: 200 trees × depth 6 | 12914 samples | elapsed 4m 25s
```

| Flag | Default | Description |
|---|---|---|
| `--blame` | off | Blame-based function-level labels (slower, more precise) |
| `--label-window DAYS` | `365` | Days of history to scan |
| `--n-estimators N` | `200` | Trees in RandomForest |
| `--max-depth N` | `6` | Maximum tree depth |
| `--output PATH` | `.hotspots/ranker.json` | Model output path |
| `--eval` | off | Print Precision@K table after training |
| `--screen` | off | Pre-flight check; aborts when mean hotspots score is too flat |
| `--yes` / `-y` | off | Skip confirmation prompt (CI / non-interactive) |
| `--quiet` / `-q` | off | Suppress per-tree progress lines; estimate and completion still shown |

Requires: ≥ 50 functions in snapshot, ≥ 5 positive and ≥ 10 negative labels. Fix keywords: `fix:`, `bug`, `patch`, `hotfix`, `regression`, `defect`.

The trained model (`model_version 5`) uses 10 features: `lrs`, `cc`, `nd`, `loc`, `fo`, `fan_in`, `total_churn`, `authors_90d`, `directed_coupling`, `convention_bug_fix_count`. Models trained with an older version are rejected on load with a retrain message.

### `hotspots prune`

Remove unreachable snapshots (after force-push or branch deletion).

```
hotspots prune --unreachable [--older-than DAYS] [--dry-run]
```

`--unreachable` is required. Only prunes snapshots unreachable from `refs/heads/*`.

### `hotspots compact`

Set compaction level for snapshot storage.

```
hotspots compact --level 0
```

Level 0 = full snapshots (current). Levels 1–2 are not yet implemented.

### `hotspots trends [PATH]`

Analyze complexity trends across snapshot history.

```
hotspots trends . [--window N] [--top K] [--format text|json|html]
```

| Flag | Default | Description |
|---|---|---|
| `--window N` | `10` | Number of snapshots to analyze |
| `--top K` | `5` | Top K functions to track |
| `--format` | `json` | Output format |

Reports: risk velocities (LRS change per snapshot), hotspot stability (consistent top-K), refactor effectiveness (sustained LRS reduction).

### `hotspots config`

```bash
hotspots config show              # show resolved config (merged defaults + file)
hotspots config show --path FILE  # show specific file
hotspots config validate          # validate auto-discovered config (exit 1 on failure)
hotspots config validate --path FILE
```

### `hotspots init`

```bash
hotspots init --hooks   # print pre-commit and CI hook templates to stdout
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
Physical line count. Used for pattern detection only, not the LRS score.

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

### Activity Risk Score (snapshot mode)

Extends LRS with git history and call graph signals:

```
Activity Risk = LRS
  + (lines_added + lines_deleted) / 100 × 0.5   # churn
  + min(touch_count_30d / 10, 5.0) × 0.3         # touch frequency
  + max(0, 5.0 − days_since_change / 7) × 0.2    # recency
  + min(fan_in / 5, 10.0) × 0.4                  # call graph fan-in
  + (scc_size if in cycle else 0) × 0.3           # cyclic dependency
  + min(dependency_depth / 3, 5.0) × 0.1         # depth from entrypoints
  + neighbor_churn / 500 × 0.2                    # churn in callees
```

Activity Risk is always ≥ LRS. When no git data is available, Activity Risk = LRS.

### Call graph metrics (snapshot mode)

- **Fan-in** — functions that call this function (blast radius)
- **PageRank** — importance/centrality based on call graph topology
- **Betweenness centrality** — fraction of shortest paths that pass through this function (hub detection); exact for graphs < 2000 nodes, approximate (k=256 pivots) for larger
- **SCC size** — strongly connected component size; > 1 = part of a dependency cycle
- **Dependency depth** — longest acyclic path from entrypoints to this function
- **Neighbor churn** — sum of churn in directly-called functions

### Quadrant assignment

| | Low activity | High activity |
|---|---|---|
| **High/Critical band** | `debt` | `fire` |
| **Low/Moderate band** | `ok` | `watch` |

Activity is "high" if: 30-day touch count above population median, OR changed within last 30 days.

`fire` = live regression risk (refactor now). `debt` = structural debt (schedule proactively). `watch` = monitor. `ok` = no action.

### Driver labels

Each function gets a single primary diagnosis, checked in priority order:

| Label | Condition | Action |
|---|---|---|
| `cyclic_dep` | Part of dependency cycle (SCC > 1) | Break the cycle before adding callers |
| `high_complexity` | CC above P75 | Schedule refactor; extract sub-functions |
| `deep_nesting` | ND above P75 | Flatten with early returns or guard clauses |
| `high_fanout_churning` | FO above P75 AND touches above P50 | Extract interface boundary |
| `high_fanin_complex` | Fan-in above P75 AND CC above P50 | Extract and stabilize; wide blast radius |
| `high_churn_low_cc` | Touches above P75 AND CC below P25 | Add regression tests before next change |
| `composite` | No single dimension clearly dominates | Address the highest dimension first |

Thresholds are percentile-relative (default P=75, configurable via `driver_threshold_percentile`). `cyclic_dep` is the sole absolute check.

`driver_detail` (JSON): for `composite` functions, lists up to 3 near-miss dimensions with their percentile rank (e.g. `"cc (P72), nd (P68)"` — notable but below P75 threshold). Omitted when null.

### Pattern detection

Patterns are informational labels. A function can have multiple. They do not affect LRS.

**Tier 1 — structural (all modes):**

| Pattern | Trigger |
|---|---|
| `complex_branching` | CC ≥ 10 AND ND ≥ 4 |
| `deeply_nested` | ND ≥ 5 |
| `exit_heavy` | NS ≥ 5 |
| `god_function` | LOC ≥ 60 AND FO ≥ 10 |
| `long_function` | LOC ≥ 80 |

**Tier 2 — enriched (snapshot mode, requires call graph + git data):**

| Pattern | Trigger |
|---|---|
| `churn_magnet` | churn ≥ 200 lines AND CC ≥ 8 |
| `cyclic_hub` | SCC size ≥ 2 AND fan-in ≥ 6 |
| `hub_function` | fan-in ≥ 10 AND CC ≥ 8 |
| `middle_man` | fan-in ≥ 8 AND FO ≥ 8 AND CC ≤ 4 |
| `neighbor_risk` | neighbor churn ≥ 400 AND FO ≥ 8 |
| `shotgun_target` | fan-in ≥ 8 AND churn ≥ 150 lines |
| `stale_complex` | CC ≥ 10 AND LOC ≥ 60 AND days since change ≥ 180 |
| `volatile_god` | Derived: `god_function` AND `churn_magnet` |

All thresholds configurable in `.hotspotsrc.json`. Use `--explain-patterns` to see which conditions triggered each pattern.

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
  "include": ["src/**/*.ts"],
  "exclude": [
    "**/*.test.ts", "**/*.spec.ts",
    "**/node_modules/**", "**/__tests__/**", "**/__mocks__/**",
    "**/dist/**", "**/build/**", "**/vendor/**",
    "**/*.pb.go", "**/zz_generated*.go"
  ],
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 0.8,
    "fo": 0.6,
    "ns": 0.7
  },
  "warning_thresholds": {
    "watch_min": 2.5,
    "watch_max": 3.0,
    "attention_min": 5.5,
    "attention_max": 6.0,
    "rapid_growth_percent": 50.0
  },
  "min_lrs": 0.0,
  "top": null,
  "co_change_window_days": 90,
  "co_change_min_count": 3,
  "driver_threshold_percentile": 75,
  "per_function_touches": true,
  "policy": {
    "critical_introduction": "warn",
    "critical_introduction_reason": "eval/ scripts are one-shot research code reviewed case-by-case, not shipped services — approved by @stephenc222 2026-07-06",
    "excessive_risk_regression": "block"
  }
}
```

**Validation rules:**
- `moderate < high < critical` (all positive)
- `watch_min < watch_max ≤ moderate < attention_min < attention_max ≤ high`
- All weights non-negative; at least one positive; none > 10.0
- `policy.*` values must be one of `"block"`, `"warn"`, `"off"`
- `policy.<name>_reason` is **required** (non-empty) whenever `policy.<name>` is not `"block"`
- Unknown fields are rejected (to catch typos)

**`policy`:** severity overrides for the two blocking CI policies. Both default to
`"block"`. `critical-introduction` fires identically whether a function is brand-new or
an existing function that regressed to Critical — a Critical function needs review
either way, so there is no separate new-vs-regressed knob, only overall severity.
Set to `"warn"` to report without failing CI (e.g. a research repo where new
one-shot scripts are expected to score high on introduction), or `"off"` to disable
the policy entirely. Raising `thresholds.critical` instead changes what counts as
Critical repo-wide (affecting reporting too); `policy` only changes what happens
once something *is* Critical.

Downgrading a policy below `"block"` requires a `<name>_reason` string — mirroring the
`// hotspots-ignore: <reason>` convention for per-function suppression — so that anyone
reviewing a `.hotspotsrc.json` diff sees *why* a blocking gate was weakened, not just
that it was. `hotspots config validate` rejects a downgrade with a missing or
whitespace-only reason. This does not make the override unbypassable — anyone with
commit access to the config can still weaken it, the same as anyone with access to a CI
workflow file can remove a required check — but it does mean the change can't be silent.

**`driver_threshold_percentile`:** default 75 means a function must be in the top 25% of its metric to receive a specific driver label. Lower (50–60) for small/uniform repos; higher (85–90) for large repos with high median complexity.

**`co_change_window_days`:** days of git history to mine for file co-change pairs. Increase for repos with slow commit cadence.

**`per_function_touches`:** `true` = use cached `git log -L` per-function counts; `false` = file-level batching always (useful in CI without persistent cache).

---

## JSON Schema

### Schema versions

| Version | Structure | When |
|---|---|---|
| v4 (default snapshot JSON) | `fire`/`debt`/`watch`/`ok` triage buckets + per-function `action` + `architecture` aggregates | `hotspots analyze --mode snapshot` |
| v2 (full snapshot) | Flat `functions` array + enriched `aggregates` | `--all-functions` |
| v1 (delta) | `deltas` array with before/after | `--mode delta` |

Always check `schema_version` before consuming output in tooling.

### Function fields (v2 / `--all-functions`)

```json
{
  "function_id": "src/api/billing.ts::processPlanUpgrade",
  "file": "src/api/billing.ts",
  "line": 142,
  "language": "TypeScript",
  "lrs": 12.4,
  "band": "critical",
  "quadrant": "fire",
  "driver": "high_complexity",
  "driver_detail": null,
  "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 },
  "risk": { "r_cc": 4.0, "r_nd": 4.0, "r_fo": 3.0, "r_ns": 3.0 },
  "patterns": ["complex_branching", "churn_magnet"],
  "pattern_details": null,
  "suppression_reason": null,
  "churn": { "lines_added": 156, "lines_deleted": 89, "net_change": 67 },
  "touch_count_30d": 12,
  "days_since_last_change": 3,
  "activity_risk": 18.5,
  "callgraph": {
    "fan_in": 8, "fan_out": 8,
    "pagerank": 0.0042, "betweenness": 127.3,
    "scc_id": 0, "scc_size": 1, "dependency_depth": 5
  }
}
```

`pattern_details` is populated only with `--explain-patterns`. `suppression_reason` is omitted (not null) when no suppression is present.

### Aggregates (`--all-functions`)

**`aggregates.file_risk`** — per-file ranked by `file_risk_score`:
```
file_risk_score = max_cc×0.4 + avg_cc×0.3 + log2(fn_count+1)×0.2 + churn_factor×0.1
```

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

**`aggregates.modules`** — directory-level instability:
```json
{
  "module": "hotspots-core/src",
  "afferent": 8,
  "efferent": 3,
  "instability": 0.27,
  "module_risk": "high"
}
```
Instability near 0 = everything depends on it (risky to change). Instability near 1 = depends on others (safe to change).

**`aggregates.models`** / **`architecture.models`** — present with `--include-models`:
```json
{
  "items": [{
    "name": "Snapshot", "file": "...", "line": 219,
    "kind": "struct", "score": 52.11,
    "critical": 4, "high": 15, "moderate": 17,
    "functions": [...]
  }],
  "links": [{ "source": 0, "target": 2, "shared_functions": 15, "shared_risk": 83.53 }]
}
```

### Delta output (v1)

```json
{
  "schema_version": 1,
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
  }
}
```

Delta statuses: `new`, `deleted`, `modified`, `unchanged` (unchanged omitted by default).

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

All languages have full parity across all metrics and features.

**JSX note:** `.jsx` and `.tsx` files support JSX syntax. Plain `.js` files also enable JSX parsing (React webpack convention). JSX elements do not add CC; control flow in JSX (`&&`, ternary) does.

---

## Scoring Changelog

All changes to formulas, weights, thresholds, or ranking rules are tracked in git commit history. The LRS formula and default weights have been stable since v1.0. The trained ranker feature was introduced in a later release; the current model is `model_version 5` (10 features). Check `CHANGELOG.md` for version-specific details.
