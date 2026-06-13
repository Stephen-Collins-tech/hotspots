# Scoring Methodology

This document describes every step of the pipeline from raw source code to the ranked output Hotspots produces. Each stage builds on the previous one.

---

## Pipeline overview

```
Source Code
  ↓
Raw Metrics  (CC, ND, FO, NS, LOC)
  ↓
Risk Components  (log-scaled, bounded transforms)
  ↓
Local Risk Score  (LRS) + Risk Band
  ↓
Pattern Classification  (Tier 1: structural · Tier 2: enriched)
  ↓
[optional enrichment: call graph, git churn, touch counts]
  ↓
Activity Risk Score  (LRS + activity modifiers)
  ↓
Driver Label  (primary dimension diagnosis)
  ↓
Quadrant Assignment  (2-D: complexity × activity)
  ↓
Ranked Output
```

Stages above the enrichment line run on source code alone and are always computed. Stages below require a git repository.

---

## Step 1 — Collect raw metrics

Hotspots parses each source file and visits every function, extracting four structural measurements:

**CC — Cyclomatic Complexity**
The number of independent decision paths through the function. Every `if`, `else if`, loop, `case`, `catch`, `&&`, `||`, and ternary adds one path. A function with no branches has CC 1.

**ND — Nesting Depth**
The maximum depth of nested control structures — how many layers of `if`/loop/`try` are present at the deepest point.

**FO — Fan-Out**
The number of distinct functions called from within this function.

**NS — Non-Structured Exits**
The count of early returns, throws, breaks, and continues inside the function body, excluding the final tail return.

**LOC — Lines of Code**
Physical line count. Used only for pattern detection (see Step 4), not for the risk score itself.

See [Metrics Reference](/reference/metrics) for exact counting rules per language.

---

## Step 2 — Transform to risk components

Raw metric values are passed through bounded, monotonic transforms before being combined:

```
R_cc = min(log2(CC + 1), 6.0)    # logarithmic, capped at 6
R_nd = min(ND, 8.0)              # linear, capped at 8
R_fo = min(log2(FO + 1), 6.0)   # logarithmic, capped at 6
R_ns = min(NS, 6.0)              # linear, capped at 6
```

**Logarithmic scaling for CC and FO** gives more weight to early growth than to increases at already-high values — the marginal risk of going from CC 1 to CC 4 is larger than going from CC 40 to CC 44. Fan-out follows the same reasoning.

**Linear scaling for ND and NS** reflects that each additional nesting level or exit point contributes more uniformly to complexity in practice.

**Caps** prevent a single extreme metric from dominating the score. Each dimension is bounded independently so the combined score reflects overall structural complexity.

---

## Step 3 — Compute the Local Risk Score (LRS)

The four risk components are combined into a single score using a weighted sum:

```
LRS = 1.0 × R_cc  +  0.8 × R_nd  +  0.6 × R_fo  +  0.7 × R_ns
```

**Weight rationale:**
- **CC (1.0)** — highest weight; control-flow complexity is the primary correlate of defect density and testing difficulty.
- **ND (0.8)** — nesting depth captures a dimension of complexity that CC alone can miss; a function can have moderate CC but still be hard to follow due to deep nesting.
- **NS (0.7)** — non-structured exits increase the number of implicit exit conditions and make postconditions harder to reason about.
- **FO (0.6)** — fan-out represents external coupling rather than internal complexity; weighted lower because some degree of fan-out is expected in most functions.

LRS is always ≥ 1.0. The theoretical maximum is **20.2** (all four components at their caps: 1.0×6 + 0.8×8 + 0.6×6 + 0.7×6). The theoretical minimum for a trivial single-path function with no nesting, calls, or exits is 1.0.

**Risk bands:**

| Band | LRS range | Meaning |
|------|-----------|---------|
| Critical | ≥ 9.0 | High structural risk |
| High | 6.0–8.9 | Elevated structural risk |
| Moderate | 3.0–5.9 | Moderate structural risk |
| Low | < 3.0 | Low structural risk |

See [LRS Specification](/reference/lrs-spec) for the complete formula derivation, worked examples, and precision notes.

---

## Step 4 — Classify patterns

Patterns are named labels that identify specific structural combinations. They complement LRS by describing *what kind* of issue a function has, not just its overall score. A function can match multiple patterns simultaneously.

### Tier 1 — structural (source code only)

Detected from raw metrics alone; always computed:

| Pattern | Trigger | Description |
|---------|---------|-------------|
| `complex_branching` | CC ≥ 10 **and** ND ≥ 4 | High branching combined with deep nesting |
| `deeply_nested` | ND ≥ 5 | Maximum nesting depth at or above threshold |
| `exit_heavy` | NS ≥ 5 | High number of non-structured exits |
| `god_function` | LOC ≥ 60 **and** FO ≥ 10 | Long function with high fan-out |
| `long_function` | LOC ≥ 80 | High physical line count |

### Tier 2 — enriched (call graph + git data)

Require git history and the call graph; computed only when that data is available:

| Pattern | Trigger | Description |
|---------|---------|-------------|
| `churn_magnet` | file churn ≥ 200 lines **and** CC ≥ 8 | High complexity combined with high change volume |
| `cyclic_hub` | SCC size ≥ 2 **and** fan-in ≥ 6 | Part of a dependency cycle with many callers |
| `hub_function` | fan-in ≥ 10 **and** CC ≥ 8 | High fan-in with high complexity |
| `middle_man` | fan-in ≥ 8 **and** FO ≥ 8 **and** CC ≤ 4 | High fan-in and fan-out with low internal complexity |
| `neighbor_risk` | neighbor churn ≥ 400 **and** FO ≥ 8 | High fan-out into frequently changing functions |
| `shotgun_target` | fan-in ≥ 8 **and** file churn ≥ 150 | Many callers in a frequently changed file |
| `stale_complex` | CC ≥ 10 **and** LOC ≥ 60 **and** days since change ≥ 180 | High complexity with no recent changes |

**Derived pattern:** `volatile_god` fires only when **both** `god_function` and `churn_magnet` are true.

All thresholds are configurable in `.hotspotsrc.json`. See [Configuration](/guide/configuration).

---

## Step 5 — Compute the Activity Risk Score

When git history is available, Hotspots extends LRS with activity signals:

```
Activity Risk = LRS
             + (lines_added + lines_deleted) / 100 × 0.5
             + min(touch_count_30d / 10, 5.0) × 0.3
             + max(0, 5.0 − days_since_change / 7) × 0.2
             + min(fan_in / 5, 10.0) × 0.4
             + (scc_size, if in cycle, else 0) × 0.3
             + min(dependency_depth / 3, 5.0) × 0.1
             + neighbor_churn / 500 × 0.2
```

Each modifier is non-negative, so Activity Risk is always ≥ LRS. When no git data is available, Activity Risk equals LRS.

| Signal | Weight | What it captures |
|--------|--------|-----------------|
| Churn (lines added + deleted) | 0.5 | Volume of recent change |
| Fan-in (call-graph callers) | 0.4 | Number of functions that depend on this one |
| Touch count (30-day commits) | 0.3 | Frequency of recent modification |
| SCC membership | 0.3 | Presence in a dependency cycle |
| Recency (days since last change) | 0.2 | How recently the function was last modified |
| Neighbor churn | 0.2 | Change volume in called functions |
| Dependency depth | 0.1 | Depth in the call graph from entry points |

---

## Step 6 — Assign driver labels

Every function gets a single **driver** label identifying which dimension contributes most to its risk. Labels are assigned using population-relative percentile thresholds, computed independently per dimension across all functions in the current scope.

The label is assigned by checking dimensions in the following priority order:

| Label | Condition | Interpretation |
|-------|-----------|----------------|
| `cyclic_dep` | Function is part of a dependency cycle | Risk is primarily structural — a cycle in the call graph |
| `high_complexity` | CC above P75 | Cyclomatic complexity is the dominant dimension |
| `deep_nesting` | ND above P75 | Nesting depth is the dominant dimension |
| `high_fanout_churning` | FO above P75 **and** touches above P50 | High fan-out combined with active change |
| `high_fanin_complex` | Fan-in above P75 **and** CC above P50 | High caller count combined with elevated complexity |
| `high_churn_low_cc` | Touches above P75 **and** CC below P25 | High activity relative to structural complexity |
| `composite` | No single dimension clearly dominates | Multiple dimensions are elevated |

Because percentiles are codebase-relative, the absolute metric value that triggers a label varies across repos.

---

## Step 7 — Assign quadrants

Every function is placed in one of four quadrants by combining its risk band with its activity level:

|  | Low activity | High activity |
|--|---|---|
| **High or Critical band** | `debt` | `fire` |
| **Low or Moderate band** | `ok` | `watch` |

**Activity** is considered high if either of the following is true:
- 30-day touch count is above the population median, **or**
- Function was changed within the last 30 days

| Quadrant | Signal | Typical action |
|----------|--------|----------------|
| `fire` | High complexity **and** high activity | Prioritize for review or refactoring |
| `debt` | High complexity, low activity | Schedule for future refactoring |
| `watch` | Low complexity, high activity | Monitor for complexity increases |
| `ok` | Low complexity, low activity | No immediate action indicated |

Note: a high Activity Risk score does not by itself place a function in `fire`. Quadrant is determined by band (from LRS) and activity independently. Always check `quadrant` alongside `touches_30d` for context.

---

## Step 8 — Rank output

**Default ranking (no trained ranker):**

1. LRS descending
2. File path ascending (tiebreak)
3. Line number ascending (tiebreak)
4. Function name ascending (tiebreak)

**With `--mode snapshot` triage view:**

Functions are grouped by quadrant (`fire` → `debt` → `watch` → `ok`), then sorted by Activity Risk descending within each group.

**With a trained ranker (`hotspots train`):**

Functions are re-scored using a RandomForest model trained on the repo's bug-fix history and sorted by predicted probability descending. LRS, band, and quadrant remain in the output.

---

## File risk score

In addition to per-function scoring, Hotspots computes a per-file score for the file-risk view:

```
File Risk Score = max_cc × 0.4
               + avg_cc × 0.3
               + log2(function_count + 1) × 0.2
               + min(file_churn / 100, 10.0) × 0.1
```

The score weights the highest-complexity function most heavily, incorporates the average complexity distribution, accounts for file size by function count, and includes recent change volume. Files are ranked descending by this score.

---

## Version history

Every change to a formula, weight, threshold, or ranking rule is recorded in the [Scoring Changelog](/reference/scoring-changelog).

---

## Coming soon: ranker scoring

The trained ranker layer is currently in active development. Once complete, the ranker will:

- Assign a `rank_score` (predicted probability this function appears in a future bug-fix commit)
- Surface functions that are statistically over-represented in past defects, even when LRS is moderate
- Blend structural risk with historical signal rather than treating them as separate steps

The heuristic pipeline above will remain the default for repos without training data.
