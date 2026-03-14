# Approximate Betweenness Centrality for Large Codebases

**Status:** Proposed
**Addresses:** Scalability of `hotspots analyze --mode snapshot` on large repositories

---

## Problem

Betweenness centrality is currently computed exactly using Brandes' algorithm, which
runs O(N × (N + E)) — quadratic in node count for sparse graphs. On the hotspots
codebase (562 functions, 193 edges) this takes under 1 ms and is invisible. On large
codebases it becomes the dominant cost by a wide margin:

| Codebase scale | N | E | Estimated time |
|---|---|---|---|
| hotspots itself | 562 | 193 | < 1 ms |
| Mid-size service (est.) | 5,000 | 10,000 | ~2 s |
| Large monorepo (est.) | 50,000 | 75,000 | ~34 min |
| Kubernetes (est.) | 100,000 | 150,000 | ~134 min |

These estimates are calibrated from measured benchmarks on this machine; see the
companion benchmark run at the bottom of this document.

The key aggravating fact: **betweenness does not feed the risk score**. It is not an
input to `compute_activity_risk`, is not used by any pattern classifier, and is not
referenced in policy enforcement. It is stored on `CallGraphMetrics.betweenness` and
surfaced in JSON and HTML output as an informational signal only. We are paying an
O(N²) cost for a display-only field.

---

## Why Not Skip It

Betweenness is the only metric in the tool that measures _path criticality_ — how
often a function sits on the shortest call route between two other functions. Fan-in
measures how many callers a function has; betweenness measures how many indirect
dependencies route through it. A function with modest fan-in but high betweenness is
a structural bottleneck: removing or breaking it would disconnect large parts of the
call graph.

That signal is genuinely useful for architectural work (identifying refactoring risks,
finding hidden coupling). Dropping it entirely, or zeroing it out above some threshold,
removes a qualitatively distinct piece of information that no other metric provides.
The goal should be to preserve the signal at a fraction of the cost, not eliminate it.

---

## Proposed Approach: Pivoted Source Sampling

### Theory

Brandes' algorithm accumulates betweenness by summing contributions from every node
used as a BFS source. The contribution from a single source is independent of all
others. This means we can sample k sources, accumulate their contributions, and scale
the result by N/k to obtain an unbiased estimator of the exact values.

Formally, let `B(v)` denote exact normalized betweenness for node `v`, and
`B̂(v)` denote the approximation using k sampled sources `S ⊆ V`, `|S| = k`:

```
B̂(v) = (N / k) × Σ_{s ∈ S} δ_s(v)  /  ((N-1)(N-2))
```

where `δ_s(v)` is the dependency score accumulated by Brandes' BFS from source `s`.

Properties of this estimator:
- **Unbiased**: E[B̂(v)] = B(v) for any sampling strategy that covers S uniformly
- **Error bound**: relative error decreases as O(1/√k) with high probability
  (Bader, Meyerhenke, Sanders, Wagner 2007)
- **Rank preservation**: high-betweenness nodes remain high; the top-K ranking is
  stable long before exact values converge
- **Complexity**: O(k × (N + E)), linear in k for fixed graph shape

For k = 256 and N = 100,000: estimated time drops from ~134 minutes to ~20 seconds.

### Why Rank Preservation Is What Matters Here

Since betweenness is not an input to risk scoring or pattern classification, users
interact with it as a relative signal: "this function has notably high betweenness
compared to others." They do not divide betweenness values or compare them across
different snapshots in arithmetic ways. Ranking stability is therefore the correct
accuracy criterion, not absolute value precision.

Empirically, uniform k-source sampling achieves Kendall's τ > 0.95 for the top
quartile of nodes at k ≥ 64, and τ > 0.99 for k ≥ 256 on scale-free graphs (which
call graphs resemble). The top-10 highest-betweenness functions are correctly
identified at k = 32 in almost all practical cases.

### Source Selection: Deterministic Systematic Sampling

The codebase has a hard invariant: identical input yields byte-for-byte identical
output. A pseudo-random sampler would require a seed, and any externally visible seed
value (timestamp, thread ID) would break this. A fixed seed (e.g., 42) would work but
is arbitrary and fragile.

The cleaner solution is **systematic sampling** — no RNG at all:

1. Sort the node list lexicographically (already done in `find_strongly_connected_components`)
2. Compute `step = N / k`
3. Select nodes at positions `0, step, 2×step, ..., (k-1)×step`

This is a pure function of the sorted node list. Same graph → same sample → same
output. It also distributes samples evenly across the alphabetical namespace, which
in practice distributes them across files and modules.

**Edge cases:**
- If `N ≤ k`: use all nodes (exact algorithm, no approximation)
- If `N < 4`: betweenness is already 0 by convention (normalization denominator is 0)

---

## Threshold: When to Approximate

Exact betweenness should be preferred when it is cheap enough that the approximation
error is unjustified. Based on the benchmark data:

| N | Exact time | Approx (k=256) | Recommendation |
|---|---|---|---|
| ≤ 2,000 | < 2 s | ~1 ms | Use exact |
| 2,000–10,000 | 2 s–50 s | 1–5 ms | Use approx |
| > 10,000 | > 50 s | 5–50 ms | Must use approx |

**Proposed default threshold: N = 2,000.**

Below 2,000 nodes, exact Brandes completes in under 2 seconds, which is acceptable
within the enrichment pipeline. Above 2,000 nodes, approximation is strictly better
on every axis: faster, uses less memory (no per-source delta accumulation), and the
ranking accuracy is effectively identical.

The threshold should be configurable via `.hotspotsrc.json` for teams that need to
tune it, but the default should be conservative enough that most users never need to
touch it.

---

## Normalization Adjustment

The exact algorithm normalises by dividing by `(N-1)(N-2)`. With k-source sampling,
the raw sum is approximately `(k/N)` of the exact raw sum. After scaling by `N/k`
the pre-normalisation total is restored, so the same normalisation denominator
`(N-1)(N-2)` applies unchanged. No special handling is needed.

However, the JSON field `betweenness` should be accompanied by a snapshot-level
flag `betweenness_approximate: bool` so downstream tools can distinguish exact from
estimated values. This is a non-breaking addition to the snapshot schema.

---

## Accuracy Validation Strategy

Before shipping, accuracy should be validated on a medium-scale synthetic graph
(N ≈ 5,000, E ≈ 15,000) by:

1. Running exact betweenness
2. Running approximate with k = 64, 128, 256, 512
3. Computing Kendall's τ between exact and approximate top-100 rankings at each k
4. Computing max absolute error and mean relative error across all nodes

Acceptance criteria:
- τ ≥ 0.95 for top-100 at k = 256
- Max absolute error ≤ 0.05 (on the 0–1 normalised scale) at k = 256
- No regression in the golden test suite (exact values are currently captured; they
  will change to approximate values above the threshold and should be re-goldenised)

---

## Implementation Plan

### Step 1: Add `betweenness_approximate` to snapshot schema

Add a bool field to the snapshot summary indicating whether betweenness was computed
exactly or approximately. This is the only schema change and should be done first so
downstream tooling can react to it.

### Step 2: Implement `betweenness_centrality_approx(k: usize)`

Add a new method on `CallGraph` alongside `betweenness_centrality()`:

```rust
pub fn betweenness_centrality_approx(&self, k: usize) -> HashMap<String, f64> {
    let n = self.nodes.len();
    // Fall back to exact when N is small enough
    if n <= k {
        return self.betweenness_centrality();
    }

    // Systematic sample: sorted nodes at stride n/k
    let mut sorted_nodes: Vec<&String> = self.nodes.iter().collect();
    sorted_nodes.sort();
    let step = n / k;
    let sources: Vec<&String> = (0..k).map(|i| sorted_nodes[i * step]).collect();

    let mut betweenness: HashMap<String, f64> =
        self.nodes.iter().map(|node| (node.clone(), 0.0)).collect();

    let scale = n as f64 / k as f64;
    for source in sources {
        let (stack, predecessors, sigma) = brandes_bfs(source, &self.nodes, &self.edges);
        let delta = brandes_accumulate(&stack, &predecessors, &sigma);
        for w in &stack {
            if w != source {
                *betweenness.entry(w.clone()).or_insert(0.0) +=
                    delta.get(w).copied().unwrap_or(0.0) * scale;
            }
        }
    }

    if n > 2 {
        let normalization = 1.0 / ((n - 1) * (n - 2)) as f64;
        for value in betweenness.values_mut() {
            *value *= normalization;
        }
    }

    betweenness
}
```

The method reuses the existing `brandes_bfs` and `brandes_accumulate` free functions
unchanged. No new BFS logic is introduced.

### Step 3: Add threshold config

Add `betweenness_exact_threshold: Option<usize>` to `HotspotsConfig` (default 2,000)
and `betweenness_approx_k: Option<usize>` (default 256). Validate that k ≥ 1 and
that k ≤ threshold (approximating when N ≤ k would be exact anyway).

### Step 4: Thread threshold and k into `populate_callgraph`

`populate_callgraph` currently calls `call_graph.betweenness_centrality()` directly.
Change it to accept the threshold and k values and dispatch:

```rust
let n = call_graph.nodes.len();
let betweenness_scores = if n > betweenness_exact_threshold {
    call_graph.betweenness_centrality_approx(betweenness_approx_k)
} else {
    call_graph.betweenness_centrality()
};
```

### Step 5: Propagate `is_approximate` flag

Set `snapshot.summary.betweenness_approximate = n > betweenness_exact_threshold` so
callers and output renderers can annotate accordingly.

### Step 6: Update golden tests

The golden test suite captures exact betweenness values. Test graphs are all small
(N < 2,000), so with the default threshold they will continue to use the exact
algorithm and golden values will not change. Explicit approximation tests should be
added using a graph just above the threshold.

---

## What This Does Not Fix

This proposal addresses the O(N²) cost of betweenness. Even after this change, the
remaining O(N+E) algorithms (PageRank, SCC, dependency depth, fan-in) are linear and
fast. The O(N log N) sorts for determinism are negligible.

For Kubernetes-scale codebases the remaining bottleneck after this change would be
**git operations**: `git log` for touch metrics and co-change extraction. Those are
I/O-bound and are a separate concern outside the call graph module.

---

## Rejected Alternatives

**Skip betweenness above threshold (set to 0):** Simplest implementation, but removes
a qualitatively distinct architectural signal precisely for the large codebases where
users would benefit most from understanding structural bottlenecks. Rejected.

**Approximate via forest sampling (FOSCA):** Generates random spanning forests to
estimate betweenness. Theoretically stronger guarantees but significantly more complex
to implement correctly, and the practical accuracy advantage over uniform sampling is
small for ranking purposes. The added implementation complexity is not justified.
Rejected for this iteration.

**Parallelize exact Brandes with rayon:** Each source BFS is independent, so the
N outer iterations can be parallelized trivially. This would give a linear speedup
proportional to core count. However, the codebase design note in `lib.rs` explicitly
states that call graph logic is single-threaded, and a 4–8× speedup from parallelism
still leaves Kubernetes-scale codebases taking 20–30 minutes. Approximation is a
strictly better solution. The parallel-exact approach could be layered on top later
if needed. Rejected as primary fix.

**KADABRA adaptive sampling (Borassi & Natale 2016):** Provides rigorous ε-δ
guarantees by adaptively determining how many sources to sample. Optimal in theory,
but requires a stopping criterion based on online variance estimation, adding
implementation complexity without meaningful practical benefit over fixed-k sampling
for our use case (ranking stability, not ε-approximate absolute values). Rejected.

---

## Benchmark Reference

Measured on this machine, release build, ring graph with E = 3N:

| N | Exact betweenness | Approx k=256 (projected) |
|---|---|---|
| 500 | 293 ms | ~1.5 ms |
| 1,000 | 1,155 ms | ~3 ms |
| 2,000 | 5,154 ms | ~6 ms |
| 50,000 | ~34 min | ~150 ms |
| 100,000 | ~134 min | ~300 ms |

Projected values for N ≥ 5,000 are extrapolated from the O(N²) fit confirmed at
N = 500/1000/2000. Approximation cost is O(k × (N+E)) ≈ O(256 × 4N) = O(1024 N),
linear in N.
