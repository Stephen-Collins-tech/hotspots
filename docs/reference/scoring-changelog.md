# Scoring Changelog

A versioned record of every change to Hotspots' scoring methodology — transforms, weights, thresholds, patterns, and ranking logic.

**Audience:** `hotspots-research` authors who need to know exactly which formula version the CLI implements when designing experiments or writing promotion briefs.

**Rule:** every promotion that touches a formula, weight, threshold, or ranking order gets an entry here, written by the CLI side as part of the same PR that ships the change.

---

## Entry format

```markdown
### vX.Y.Z — YYYY-MM-DD

**Changed:** short name of what moved (e.g. "LRS weights", "churn_magnet threshold")
**Research finding:** F## — title (link to brief)
**PR:** hotspots#NNN

| | Before | After |
|--|--|--|
| field or formula | old value | new value |

**Notes:** (optional) why, edge cases, what stays the same
```

One entry per released version that contains a scoring change. If a release has no scoring changes, omit it.

---

## Changelog

### v1.24.0 — current

No scoring changes since this changelog was introduced. The formulas below in [Scoring Methodology](/reference/scoring) represent the current baseline.

**Baseline snapshot (as of v1.24.0):**

| Component | Value |
|-----------|-------|
| LRS weights | cc=1.0, nd=0.8, fo=0.6, ns=0.7 |
| R_cc cap | 6.0 (log2 scale) |
| R_nd cap | 8.0 (linear) |
| R_fo cap | 6.0 (log2 scale) |
| R_ns cap | 6.0 (linear) |
| Band: Low | LRS < 3.0 |
| Band: Moderate | 3.0 ≤ LRS < 6.0 |
| Band: High | 6.0 ≤ LRS < 9.0 |
| Band: Critical | LRS ≥ 9.0 |
| Activity: churn weight | 0.5 |
| Activity: fan-in weight | 0.4 |
| Activity: touch weight | 0.3 |
| Activity: SCC weight | 0.3 |
| Activity: recency weight | 0.2 |
| Activity: neighbor-churn weight | 0.2 |
| Activity: depth weight | 0.1 |
| Driver label percentile | P75 |
| Quadrant active threshold | touch > P50 OR days_since ≤ 30 |
| Pattern: complex_branching | CC ≥ 10 AND ND ≥ 4 |
| Pattern: deeply_nested | ND ≥ 5 |
| Pattern: exit_heavy | NS ≥ 5 |
| Pattern: god_function | LOC ≥ 60 AND FO ≥ 10 |
| Pattern: long_function | LOC ≥ 80 |
| Pattern: churn_magnet | churn ≥ 200 AND CC ≥ 8 |
| Pattern: hub_function | fan-in ≥ 10 AND CC ≥ 8 |
| Pattern: middle_man | fan-in ≥ 8 AND FO ≥ 8 AND CC ≤ 4 |
| Pattern: shotgun_target | fan-in ≥ 8 AND churn ≥ 150 |
| Pattern: stale_complex | CC ≥ 10 AND LOC ≥ 60 AND days ≥ 180 |
| Pattern: neighbor_risk | neighbor_churn ≥ 400 AND FO ≥ 8 |
| Pattern: cyclic_hub | SCC ≥ 2 AND fan-in ≥ 6 |

---

## How research uses this

When writing a promotion brief for a scoring change:

1. Look up the current baseline row for the value you are changing.
2. State the **Before** value explicitly in the brief — if it does not match what is here, the brief is stale.
3. State the **After** value as the exact replacement.
4. The CLI author fills in a new changelog entry (using the format above) in the same PR that ships the change.

This keeps the changelog and the code in sync at every release boundary, and gives research a single doc to diff when asking "what formula is the CLI running right now?"
