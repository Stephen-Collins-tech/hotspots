# Scoring Methodology

This document explains every step of the pipeline from raw source code to the ranked output Hotspots produces. Each stage builds on the previous one.

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
The number of independent decision paths through the function. Every `if`, `else if`, loop, `case`, `catch`, `&&`, `||`, and ternary adds one path. A function with no branches has CC 1. CC is the strongest single predictor of testing difficulty: CC 10 means at least 10 distinct paths to exercise.

**ND — Nesting Depth**
The maximum depth of nested control structures — how many layers of `if`/loop/`try` a reader must mentally track at once. Each level of nesting multiplies the number of implicit states the function can be in. ND 4 or higher almost always means the function is doing too many things at once.

**FO — Fan-Out**
The number of distinct functions this function calls. High fan-out means the function coordinates many dependencies. Each additional callee is another place for the function's behavior to change unexpectedly — and another thing to mock in tests.

**NS — Non-Structured Exits**
The count of early returns, throws, breaks, and continues inside the function body, excluding the final tail return. Non-structured exits create multiple exit points that are hard to trace and make it difficult to reason about what state the function leaves behind.

**LOC — Lines of Code**
Physical line count. Used only for pattern detection (see Step 4), not for the risk score itself.

See [Metrics Reference](/reference/metrics) for exact counting rules per language.

---

## Step 2 — Transform to risk components

Raw metric values are not summed directly. Each one is first passed through a bounded, monotonic transform that shapes how it contributes to the final score:

```
R_cc = min(log2(CC + 1), 6.0)    # logarithmic, capped at 6
R_nd = min(ND, 8.0)              # linear, capped at 8
R_fo = min(log2(FO + 1), 6.0)   # logarithmic, capped at 6
R_ns = min(NS, 6.0)              # linear, capped at 6
```

**Why logarithmic for CC and FO?**
The difference between CC 1 and CC 4 is enormous — one path versus four. The difference between CC 40 and CC 44 is negligible — already completely untestable either way. Logarithmic scaling captures this: early growth counts a lot, marginal increases at high values contribute little. Fan-out follows the same logic.

**Why linear for ND and NS?**
Nesting depth and exit count grow more uniformly in practice. Each additional nesting level genuinely adds proportional cognitive load, and there is no diminishing-returns region to flatten.

**Why caps?**
Caps prevent a single catastrophically bad metric from drowning out the others. A function with CC 200 and ND 1 should not score higher than a function with CC 30 and ND 8 — both are severe, and the score should reflect overall structural complexity, not just one runaway dimension.

---

## Step 3 — Compute the Local Risk Score (LRS)

The four risk components are combined into a single score using a weighted sum:

```
LRS = 1.0 × R_cc  +  0.8 × R_nd  +  0.6 × R_fo  +  0.7 × R_ns
```

**Why these weights?**
- **CC gets the highest weight (1.0)** because control-flow complexity is the primary driver of defect density across empirical studies. It directly determines how many paths must be tested and how many branch combinations can interact.
- **ND gets the second-highest weight (0.8)** because deep nesting compounds complexity in a way CC alone doesn't capture — a function can have moderate CC but be nearly unreadable due to nesting.
- **NS gets a medium-high weight (0.7)** because non-structured exits create implicit control flow that is hard to reason about and makes postconditions difficult to state or enforce.
- **FO gets the lowest weight (0.6)** because some fan-out is expected and healthy — functions that orchestrate other functions aren't inherently risky unless combined with other signals.

LRS is always ≥ 1.0 (minimum for a trivial single-path function with no nesting, calls, or exits). The theoretical maximum is **22.0** (all four components pegged at their caps). Real-world functions rarely exceed 15.

**Risk bands:**

| Band | LRS range | Meaning |
|------|-----------|---------|
| Critical | ≥ 9.0 | Refactor now — highest-probability bug sources |
| High | 6.0–8.9 | Refactor the next time you touch this function |
| Moderate | 3.0–5.9 | Monitor; block increases in CI |
| Low | < 3.0 | Not worth the risk of touching without a reason |

See [LRS Specification](/reference/lrs-spec) for the complete formula derivation, worked examples, and precision notes.

---

## Step 4 — Classify patterns

Patterns are named labels that identify specific code smells. They complement LRS by naming *what kind* of problem a function has, not just *how bad* it is. A function can match multiple patterns simultaneously.

### Tier 1 — structural (source code only)

These patterns are detected from raw metrics alone and are always computed:

| Pattern | Trigger | What it means |
|---------|---------|---------------|
| `complex_branching` | CC ≥ 10 **and** ND ≥ 4 | High branching *and* deep nesting — two independent complexity signals reinforcing each other |
| `deeply_nested` | ND ≥ 5 | Nesting so deep that the function almost certainly needs to be decomposed |
| `exit_heavy` | NS ≥ 5 | So many exit points that control flow is genuinely hard to trace |
| `god_function` | LOC ≥ 60 **and** FO ≥ 10 | Long *and* calls many things — a function that knows too much and does too much |
| `long_function` | LOC ≥ 80 | Physically long regardless of complexity — a readability and reviewability problem |

### Tier 2 — enriched (call graph + git data)

These patterns require git history and the call graph and are only computed when that data is available:

| Pattern | Trigger | What it means |
|---------|---------|---------------|
| `churn_magnet` | file churn ≥ 200 lines **and** CC ≥ 8 | Complex *and* frequently changed — the highest-risk combination |
| `cyclic_hub` | SCC size ≥ 2 **and** fan-in ≥ 6 | Part of a dependency cycle *and* widely called — hard to reason about ordering |
| `hub_function` | fan-in ≥ 10 **and** CC ≥ 8 | Many callers depend on a complex function — wide blast radius |
| `middle_man` | fan-in ≥ 8 **and** FO ≥ 8 **and** CC ≤ 4 | Called by many, calls many, but does very little itself — often a sign of unnecessary indirection |
| `neighbor_risk` | neighbor churn ≥ 400 **and** FO ≥ 8 | The things this function calls are changing a lot — risk inherited from dependencies |
| `shotgun_target` | fan-in ≥ 8 **and** file churn ≥ 150 | Many callers *and* the file changes frequently — changes here ripple widely |
| `stale_complex` | CC ≥ 10 **and** LOC ≥ 60 **and** days since change ≥ 180 | Complex and old — institutional knowledge risk; the people who understood it may be gone |

**Derived pattern:** `volatile_god` fires only when **both** `god_function` and `churn_magnet` are true — a large, widely-coupled function that is also actively changing.

All thresholds are configurable in `.hotspotsrc.json`. See [Configuration](/guide/configuration).

---

## Step 5 — Compute the Activity Risk Score

When git history is available, Hotspots extends LRS with activity signals to produce a score that reflects not just how complex a function is, but how much risk that complexity is generating *right now*:

```
Activity Risk = LRS
             + (lines_added + lines_deleted) / 100 × 0.5
             + min(touch_count_30d / 10, 5.0) × 0.3
             + max(0, 5.0 − days_since_change / 7) × 0.2
             + min(fan_in / 5, 10.0) × 0.4
             + (scc_size − 1, if in cycle, else 0) × 0.3
             + min(dependency_depth / 3, 5.0) × 0.1
             + neighbor_churn / 500 × 0.2
```

Each modifier adds to LRS — Activity Risk is always ≥ LRS. When no git data is available, Activity Risk equals LRS.

**What each signal captures:**

| Signal | Weight | Why it matters |
|--------|--------|----------------|
| Churn (lines added + deleted) | 0.5 | Volume of recent change is the strongest activity signal — high churn means the function is a live target |
| Fan-in (call-graph callers) | 0.4 | Many callers means changes here have a wide blast radius |
| Touch count (30-day commits) | 0.3 | Frequent commits suggest instability or active development — either way, the function is in motion |
| SCC membership | 0.3 | Being part of a dependency cycle makes any change harder to reason about safely |
| Recency (days since last change) | 0.2 | A function changed this week is more likely to be changed again than one stable for a year |
| Neighbor churn | 0.2 | When the functions this one calls are changing a lot, risk transfers inward |
| Dependency depth | 0.1 | Deep call chains amplify cascading failures — lower weight because structural depth is already captured partially by LRS |

---

## Step 6 — Assign driver labels

Every function gets a single **driver** label that names the primary dimension responsible for its risk. This is computed after enrichment, using population-relative percentile thresholds so the labels adapt to each codebase rather than using fixed cutoffs.

The label is assigned by checking dimensions in priority order:

| Label | Condition | What it tells you |
|-------|-----------|-------------------|
| `cyclic_dep` | Function is part of a dependency cycle | The risk comes from circular dependencies, not just internal complexity |
| `high_complexity` | CC is above the 75th percentile | Control-flow complexity is the dominant driver |
| `deep_nesting` | ND is above the 75th percentile | Nesting depth is the dominant driver |
| `high_fanout_churning` | FO above P75 **and** touches above P50 | Fan-out is high *and* the function is actively changing — dependencies in flux |
| `high_fanin_complex` | Fan-in above P75 **and** CC above P50 | Many callers depend on a complex function |
| `high_churn_low_cc` | Touches above P75 **and** CC below P25 | Unusually active for a simple function — worth investigating why |
| `composite` | No single dimension clearly dominates | Multiple dimensions are elevated together |

Percentiles are computed independently per dimension across all functions in the current analysis scope. A function in a mostly-simple codebase may be labeled `high_complexity` at CC 6; one in a complex codebase may need CC 20 to earn the same label.

---

## Step 7 — Assign quadrants

Every function is placed in one of four quadrants by combining its risk band with its activity level. Quadrant is the primary triage signal — it tells you whether to act now, schedule work, monitor, or leave it alone.

|  | Low activity | High activity |
|--|---|---|
| **High or Critical band** | `debt` | `fire` |
| **Low or Moderate band** | `ok` | `watch` |

**What "active" means:**
A function is considered active if any of the following is true:
- Its 30-day touch count is above the population median, **or**
- It was changed within the last 30 days

**What each quadrant means:**

| Quadrant | Signal | What to do |
|----------|--------|------------|
| `fire` | Complex **and** actively changing right now | Highest priority. Live regression risk — every change to this function is high-stakes. |
| `debt` | Complex but dormant | Schedule a refactor. Not urgent today, but don't let it stay this way indefinitely. |
| `watch` | Active but not yet complex | Monitor. Block LRS increases in CI so it doesn't drift into `fire`. |
| `ok` | Simple and quiet | Leave it alone. Touching it introduces risk with no expected return. |

**Important:** A high Activity Risk score alone does **not** mean a function is in `fire`. Always check `quadrant` and `touches_30d` — a complex function that hasn't been touched in a year is `debt`, not `fire`. The quadrant is the more actionable signal.

---

## Step 8 — Rank output

**Default ranking (no trained ranker):**

Functions are sorted by:
1. LRS descending — highest structural risk first
2. File path ascending — alphabetical tiebreak within the same LRS
3. Line number ascending — earlier in the file wins ties within the same file
4. Function name ascending — final tiebreak

**With `--mode snapshot` triage view:**

Functions are grouped by quadrant in priority order (`fire` → `debt` → `watch` → `ok`), then sorted by Activity Risk descending within each group.

**With a trained ranker (`hotspots train`):**

The ML ranker re-scores functions using a RandomForest model trained on your repo's bug-fix history. Output is sorted by the ranker's predicted bug probability descending. LRS, band, and quadrant remain in the output for context.

---

## File risk score

In addition to per-function scoring, Hotspots aggregates function data into a per-file score for the file-risk view:

```
File Risk Score = max_cc × 0.4
               + avg_cc × 0.3
               + log2(function_count + 1) × 0.2
               + min(file_churn / 100, 10.0) × 0.1
```

This weights the worst function in the file most heavily (max CC), considers the overall complexity distribution (avg CC), penalizes files with many functions (harder to navigate), and factors in recent change volume. Files are ranked descending by this score.

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
