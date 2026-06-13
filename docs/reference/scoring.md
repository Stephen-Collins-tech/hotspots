# Scoring Methodology

This document describes the complete pipeline from raw source code to the ranked output Hotspots produces. Each stage builds on the previous one.

---

## Pipeline overview

```
Source Code
  ↓
Raw Metrics  (CC, ND, FO, NS, LOC)
  ↓
Risk Components  (log-scaled, bounded)
  ↓
Local Risk Score  (LRS) + Risk Band
  ↓
Pattern Classification  (Tier 1: structural, Tier 2: enriched)
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

Stages above the enrichment line run on source code alone. Stages below require `--git-dir` or a connected git repository.

---

## 1. Raw metrics

Hotspots measures four structural dimensions per function:

| Metric | Meaning |
|--------|---------|
| **CC** | Cyclomatic complexity — independent paths through the control flow graph |
| **ND** | Nesting depth — maximum depth of nested control structures |
| **FO** | Fan-out — distinct functions called from this function |
| **NS** | Non-structured exits — early returns, throws, breaks, continues (excluding tail return) |

Plus **LOC** (physical line count), used only for pattern detection.

See [Metrics Reference](/reference/metrics) for full definitions and per-language counting rules.

---

## 2. Local Risk Score (LRS)

Each raw metric is first passed through a monotonic, bounded transform:

```
R_cc = min(log2(CC + 1), 6.0)    # logarithmic, cap 6
R_nd = min(ND, 8.0)              # linear, cap 8
R_fo = min(log2(FO + 1), 6.0)   # logarithmic, cap 6
R_ns = min(NS, 6.0)              # linear, cap 6
```

Logarithmic scaling for CC and FO means the jump from 1 to 4 counts more than the jump from 20 to 24 — early growth signals risk more reliably than marginal increases at high values.

The four components combine into LRS:

```
LRS = 1.0 × R_cc  +  0.8 × R_nd  +  0.6 × R_fo  +  0.7 × R_ns
```

**Risk bands:**

| Band     | LRS range | Interpretation |
|----------|-----------|----------------|
| Critical | ≥ 9.0     | Refactor now — highest-probability bug sources |
| High     | 6.0–8.9   | Refactor the next time you touch this function |
| Moderate | 3.0–5.9   | Monitor; block increases in CI |
| Low      | < 3.0     | Not worth the risk of touching without a reason |

Theoretical maximum LRS is **22.0** (all four metrics pegged at their caps). In practice, even the most complex real-world functions rarely exceed 15.

See [LRS Specification](/reference/lrs-spec) for the complete formula derivation, worked examples, and precision notes.

---

## 3. Pattern classification

Patterns are named code-smell labels assigned before any git data is needed (Tier 1) or after enrichment (Tier 2). They appear as `patterns: [...]` in JSON output and `--format text`.

### Tier 1 — structural (raw metrics only)

| Pattern | Trigger |
|---------|---------|
| `complex_branching` | CC ≥ 10 **and** ND ≥ 4 |
| `deeply_nested` | ND ≥ 5 |
| `exit_heavy` | NS ≥ 5 |
| `god_function` | LOC ≥ 60 **and** FO ≥ 10 |
| `long_function` | LOC ≥ 80 |

### Tier 2 — enriched (call graph + git)

| Pattern | Trigger |
|---------|---------|
| `churn_magnet` | file churn ≥ 200 lines **and** CC ≥ 8 |
| `cyclic_hub` | SCC size ≥ 2 **and** fan-in ≥ 6 |
| `hub_function` | fan-in ≥ 10 **and** CC ≥ 8 |
| `middle_man` | fan-in ≥ 8 **and** FO ≥ 8 **and** CC ≤ 4 |
| `neighbor_risk` | neighbor churn ≥ 400 **and** FO ≥ 8 |
| `shotgun_target` | fan-in ≥ 8 **and** file churn ≥ 150 |
| `stale_complex` | CC ≥ 10 **and** LOC ≥ 60 **and** days since change ≥ 180 |

**Derived pattern:** `volatile_god` fires only when **both** `god_function` and `churn_magnet` are true.

All thresholds are configurable in `.hotspotsrc.json`. See [Configuration](/guide/configuration).

---

## 4. Activity Risk Score

When git history is available, Hotspots computes an activity-weighted risk score that combines LRS with change signals:

```
Activity Risk = LRS
             + (lines_added + lines_deleted) / 100 × 0.5   # churn
             + min(touch_count / 10, 5.0) × 0.3            # touches in 30 days
             + max(0, 5.0 − days_since_change / 7) × 0.2   # recency
             + min(fan_in / 5, 10.0) × 0.4                 # call-graph fan-in
             + (scc_size if > 1 else 0) × 0.3              # cycle membership
             + min(dependency_depth / 3, 5.0) × 0.1        # call-chain depth
             + neighbor_churn / 500 × 0.2                  # churn in callers/callees
```

**Weights at a glance:**

| Signal | Weight | Rationale |
|--------|--------|-----------|
| Churn | 0.5 | High change volume is a leading defect predictor |
| Fan-in | 0.4 | Wide blast radius amplifies impact of any change |
| Touch count | 0.3 | Frequent touches correlate with instability |
| SCC membership | 0.3 | Cycles make reasoning about ordering impossible |
| Recency | 0.2 | Recent changes haven't had time to stabilize |
| Neighbor churn | 0.2 | Dependencies in flux transfer risk |
| Dependency depth | 0.1 | Deep chains amplify cascading failures |

Activity Risk is always ≥ LRS (all modifiers are non-negative). When no git data is available, Activity Risk equals LRS.

---

## 5. Driver labels

Each function receives a primary **driver** label that names which dimension dominates its risk. This appears as `driver` in JSON output and is shown in the triage view.

Labels are assigned by comparing each dimension against population percentiles (default: 75th percentile threshold):

| Label | Condition |
|-------|-----------|
| `cyclic_dep` | SCC size > 1 (absolute, no percentile) |
| `high_complexity` | CC above P75 |
| `deep_nesting` | ND above P75 |
| `high_fanout_churning` | FO above P75 **and** touch count above P50 |
| `high_fanin_complex` | fan-in above P75 **and** CC above P50 |
| `high_churn_low_cc` | touches above P75 **and** CC below P25 |
| `composite` | no single dimension clearly dominates |

Percentiles are computed independently per dimension across all functions in the current analysis scope, so thresholds adapt to each codebase.

---

## 6. Quadrant assignment

Every function is placed in one of four quadrants using a 2-D matrix of complexity vs. activity:

|  | Low activity | High activity |
|--|---|---|
| **High risk (High or Critical band)** | `debt` | `fire` |
| **Low risk (Low or Moderate band)** | `ok` | `watch` |

**Activity** is considered high when any of the following is true:
- `touch_count` is above the population median for the analysis scope, **or**
- `days_since_change` ≤ 30, **or**
- Activity Risk is in the top 30% of the population (when a ranker has been applied)

**Interpreting the quadrants:**

| Quadrant | Signal | Recommended action |
|----------|--------|--------------------|
| `fire` | Complex **and** actively changing | Highest priority — live regression risk |
| `debt` | Complex but dormant | Schedule refactor; don't defer indefinitely |
| `watch` | Active but not yet complex | Monitor; block LRS increases in CI |
| `ok` | Simple and quiet | Leave it alone |

A high Activity Risk score alone does **not** mean a function is in `fire`. Always check `quadrant` and `touches_30d` — a complex function that hasn't been touched in a year is `debt`, not `fire`.

---

## 7. File risk score

Hotspots also aggregates function-level data into a per-file score for the file-risk view:

```
File Risk Score = max_cc × 0.4
               + avg_cc × 0.3
               + log2(function_count + 1) × 0.2
               + min(file_churn / 100, 10.0) × 0.1
```

Files are ranked descending by this score. Ties break alphabetically by path.

---

## 8. Ranking

**Default ranking (no ranker applied):**

1. LRS descending (highest structural risk first)
2. File path ascending (alphabetical tiebreak)
3. Line number ascending
4. Function name ascending

**With `--mode snapshot` triage view:**

Functions are grouped by quadrant (`fire` → `debt` → `watch` → `ok`), then sorted by Activity Risk descending within each group.

**With a trained ranker (`hotspots rank`):**

The ML ranker re-scores functions using a RandomForest model trained on your repo's bug-fix history. Output is sorted by the ranker's predicted bug probability descending. LRS and quadrant remain in the output for context. See [Training a Ranker](/guide/usage#training-a-ranker) for details.

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
