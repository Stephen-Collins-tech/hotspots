//! Call graph extraction and analysis
//!
//! Extracts function call relationships and computes graph-based metrics:
//! - Fan-in/fan-out (structural coupling)
//! - PageRank (importance/centrality)
//! - Betweenness centrality (critical paths)
//!
//! ## Limitations (by design)
//!
//! This implementation tracks **internal function calls only** (functions defined
//! in the analyzed codebase). External calls are intentionally excluded:
//!
//! - ❌ External library calls (npm packages, standard libraries)
//! - ❌ Dynamic/runtime calls (callbacks, reflection, dynamic imports)
//! - ❌ Indirect calls through function pointers or event handlers
//!
//! This keeps analysis fast, deterministic, and focused on the codebase's internal
//! architecture. Advanced call tracking (including external dependencies and runtime
//! analysis) is reserved for future cloud/pro versions.

use std::collections::{HashMap, VecDeque};

/// Call graph for a codebase.
///
/// Uses an index-based representation: node strings are interned into a `Vec<String>`
/// and all adjacency, BFS, and PageRank structures operate on `u32` indices.
/// This avoids per-BFS-call HashMap<String, ...> allocations that otherwise grow
/// unbounded under approximate betweenness (k=256 calls × ~400 MB/call).
#[derive(Debug, Clone)]
pub struct CallGraph {
    ids: Vec<String>,
    id_to_idx: HashMap<String, u32>,
    adj: Vec<Vec<u32>>,
    /// Total callee names found in ASTs across all functions
    pub total_callee_names: usize,
    /// Callee names that resolved to a known internal function ID
    pub resolved_callee_names: usize,
}

/// Graph metrics for a single function
#[derive(Debug, Clone, PartialEq)]
pub struct GraphMetrics {
    /// Fan-in: number of functions calling this function
    pub fan_in: usize,
    /// Fan-out: number of functions this function calls
    pub fan_out: usize,
    /// PageRank score (importance/centrality)
    pub pagerank: f64,
    /// Betweenness centrality (criticality on paths)
    pub betweenness: f64,
}

impl CallGraph {
    /// Create an empty call graph
    pub fn new() -> Self {
        CallGraph {
            ids: Vec::new(),
            id_to_idx: HashMap::new(),
            adj: Vec::new(),
            total_callee_names: 0,
            resolved_callee_names: 0,
        }
    }

    /// Intern a node string, returning its u32 index (allocated if new).
    pub fn intern(&mut self, id: String) -> u32 {
        if let Some(&idx) = self.id_to_idx.get(&id) {
            return idx;
        }
        let idx = self.ids.len() as u32;
        self.id_to_idx.insert(id.clone(), idx);
        self.ids.push(id);
        self.adj.push(Vec::new());
        idx
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.ids.len()
    }

    /// Total number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.adj.iter().map(|v| v.len()).sum()
    }

    /// Returns true if `id` is a node in this graph.
    pub fn contains(&self, id: &str) -> bool {
        self.id_to_idx.contains_key(id)
    }

    /// Returns an iterator over callee IDs for the given function, or None if not found.
    pub fn callees_of<'a>(&'a self, id: &str) -> Option<impl Iterator<Item = &'a str>> {
        let idx = *self.id_to_idx.get(id)? as usize;
        Some(self.adj[idx].iter().map(|&i| self.ids[i as usize].as_str()))
    }

    /// Add a directed edge from `caller_idx` to `callee_idx` (index-based, no interning).
    ///
    /// Both indices must already be interned. Used by `lib.rs` during fast graph construction
    /// to avoid redundant string lookups after `intern` has already been called.
    pub fn add_adj(&mut self, caller_idx: u32, callee_idx: u32) {
        self.adj[caller_idx as usize].push(callee_idx);
    }

    /// Add a function to the graph (interning its ID).
    pub fn add_node(&mut self, function_id: String) {
        self.intern(function_id);
    }

    /// Add a call edge (caller -> callee), interning both nodes.
    pub fn add_edge(&mut self, caller: String, callee: String) {
        let caller_idx = self.intern(caller);
        let callee_idx = self.intern(callee);
        self.adj[caller_idx as usize].push(callee_idx);
    }

    /// Calculate fan-in for a function (number of callers).
    pub fn fan_in(&self, function_id: &str) -> usize {
        match self.id_to_idx.get(function_id) {
            None => 0,
            Some(&target) => self.adj.iter().filter(|c| c.contains(&target)).count(),
        }
    }

    /// Calculate fan-out for a function (number of callees).
    pub fn fan_out(&self, function_id: &str) -> usize {
        match self.id_to_idx.get(function_id) {
            None => 0,
            Some(&idx) => self.adj[idx as usize].len(),
        }
    }

    /// Calculate PageRank for all functions.
    ///
    /// Uses Vec<f64> indexed by node index with swap-buffer iteration — no per-iteration
    /// HashMap allocations.
    ///
    /// # Arguments
    ///
    /// * `damping` - Damping factor (typically 0.85)
    /// * `max_iterations` - Upper bound on iterations (typically 20-50)
    /// * `epsilon` - Convergence threshold; stop early when max delta < epsilon (e.g. 1e-6)
    pub fn pagerank(
        &self,
        damping: f64,
        max_iterations: usize,
        epsilon: f64,
    ) -> HashMap<String, f64> {
        let n = self.ids.len();
        if n == 0 {
            return HashMap::new();
        }

        // Build reverse adjacency once
        let mut rev_adj: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (caller_idx, callees) in self.adj.iter().enumerate() {
            for &callee_idx in callees {
                rev_adj[callee_idx as usize].push(caller_idx as u32);
            }
        }
        // Sort caller lists for deterministic computation
        for callers in rev_adj.iter_mut() {
            callers.sort();
        }

        // fan_out per node (clamped to 1 to avoid divide-by-zero)
        let fan_out: Vec<f64> = self.adj.iter().map(|v| v.len().max(1) as f64).collect();

        let initial_rank = 1.0 / n as f64;
        let mut ranks = vec![initial_rank; n];
        let mut new_ranks = vec![0.0f64; n];

        for _ in 0..max_iterations {
            for i in 0..n {
                let mut rank = (1.0 - damping) / n as f64;
                for &caller_idx in &rev_adj[i] {
                    rank += damping * ranks[caller_idx as usize] / fan_out[caller_idx as usize];
                }
                new_ranks[i] = rank;
            }

            let max_delta = ranks
                .iter()
                .zip(new_ranks.iter())
                .map(|(old, new)| (new - old).abs())
                .fold(0.0_f64, f64::max);

            std::mem::swap(&mut ranks, &mut new_ranks);

            if max_delta < epsilon {
                break;
            }
        }

        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), ranks[i]))
            .collect()
    }

    /// Calculate betweenness centrality using pivoted source sampling (approximate).
    ///
    /// Uses systematic k-source sampling: selects k sources evenly spaced through the
    /// sorted node list and scales contributions by N/k. This gives an unbiased estimator
    /// of exact betweenness with O(k × (N+E)) complexity instead of O(N × (N+E)).
    ///
    /// Falls back to exact computation when `self.node_count() <= k`.
    ///
    /// BFS working buffers (stack, pred, sigma, dist, delta) are pre-allocated once and
    /// reused across all k iterations via `.clear()` / `.fill()` — RSS is flat regardless
    /// of k.
    ///
    /// # Arguments
    ///
    /// * `k` - Number of pivot sources to sample (higher = more accurate, more time)
    pub fn betweenness_centrality_approx(&self, k: usize) -> HashMap<String, f64> {
        let n = self.ids.len();
        if n <= k {
            return self.betweenness_centrality();
        }

        // Sort node indices by string ID for deterministic stride-based sampling
        let mut sorted_indices: Vec<u32> = (0..n as u32).collect();
        sorted_indices.sort_by_key(|&i| &self.ids[i as usize]);

        let scale = n as f64 / k as f64;
        let mut betweenness = vec![0.0f64; n];

        // Pre-allocate BFS buffers — reused every iteration (no dealloc between calls)
        let mut stack: Vec<u32> = Vec::with_capacity(n);
        let mut pred: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut sigma = vec![0.0f64; n];
        let mut dist = vec![-1i32; n];
        let mut delta = vec![0.0f64; n];
        let mut queue: VecDeque<u32> = VecDeque::with_capacity(n);

        for i in 0..k {
            let source_idx = sorted_indices[(i * n) / k];
            brandes_bfs_inplace(
                source_idx, &self.adj, &mut stack, &mut pred, &mut sigma, &mut dist, &mut queue,
            );
            brandes_accumulate_inplace(&stack, &pred, &sigma, &mut delta);
            for &w in &stack {
                if w != source_idx {
                    betweenness[w as usize] += delta[w as usize] * scale;
                }
            }
        }

        if n > 2 {
            let normalization = 1.0 / ((n - 1) * (n - 2)) as f64;
            for v in betweenness.iter_mut() {
                *v *= normalization;
            }
        }

        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), betweenness[i]))
            .collect()
    }

    /// Calculate betweenness centrality for all functions (exact).
    ///
    /// Uses Brandes' algorithm: O(N × (N+E)). For large graphs use
    /// `betweenness_centrality_approx` instead.
    ///
    /// BFS working buffers are pre-allocated once and reused across all N iterations.
    pub fn betweenness_centrality(&self) -> HashMap<String, f64> {
        let n = self.ids.len();
        let mut betweenness = vec![0.0f64; n];

        if n == 0 {
            return HashMap::new();
        }

        let mut stack: Vec<u32> = Vec::with_capacity(n);
        let mut pred: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut sigma = vec![0.0f64; n];
        let mut dist = vec![-1i32; n];
        let mut delta = vec![0.0f64; n];
        let mut queue: VecDeque<u32> = VecDeque::with_capacity(n);

        for source_idx in 0..n as u32 {
            brandes_bfs_inplace(
                source_idx, &self.adj, &mut stack, &mut pred, &mut sigma, &mut dist, &mut queue,
            );
            brandes_accumulate_inplace(&stack, &pred, &sigma, &mut delta);
            for &w in &stack {
                if w != source_idx {
                    betweenness[w as usize] += delta[w as usize];
                }
            }
        }

        if n > 2 {
            let normalization = 1.0 / ((n - 1) * (n - 2)) as f64;
            for v in betweenness.iter_mut() {
                *v *= normalization;
            }
        }

        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), betweenness[i]))
            .collect()
    }

    /// Find strongly connected components using iterative Tarjan's algorithm.
    ///
    /// Returns a map from function ID to (scc_id, scc_size).
    /// Functions in the same SCC form a cyclic dependency group.
    pub fn find_strongly_connected_components(&self) -> HashMap<String, (usize, usize)> {
        let n = self.ids.len();

        // Pre-sort adjacency lists by node ID string for determinism
        let sorted_adj: Vec<Vec<u32>> = self
            .adj
            .iter()
            .map(|v| {
                let mut s = v.clone();
                s.sort_by_key(|&i| &self.ids[i as usize]);
                s
            })
            .collect();

        let mut node_index: Vec<i64> = vec![-1; n]; // -1 = unvisited
        let mut lowlink: Vec<u32> = vec![0; n];
        let mut on_stack: Vec<bool> = vec![false; n];
        let mut tarjan_stack: Vec<u32> = Vec::new();
        let mut index_counter: u32 = 0;
        let mut scc_id: usize = 0;
        let mut node_scc: Vec<usize> = vec![0; n];
        let mut scc_sizes: Vec<usize> = Vec::new();

        // Process nodes in sorted order for determinism
        let mut sorted_nodes: Vec<u32> = (0..n as u32).collect();
        sorted_nodes.sort_by_key(|&i| &self.ids[i as usize]);

        // Work stack: (node_idx, next_successor_position)
        let mut work: Vec<(u32, usize)> = Vec::new();

        for &start in &sorted_nodes {
            if node_index[start as usize] >= 0 {
                continue;
            }

            work.push((start, 0));
            node_index[start as usize] = index_counter as i64;
            lowlink[start as usize] = index_counter;
            index_counter += 1;
            tarjan_stack.push(start);
            on_stack[start as usize] = true;

            while !work.is_empty() {
                let (v, si) = *work.last().unwrap();
                let vi = v as usize;

                if si < sorted_adj[vi].len() {
                    let w = sorted_adj[vi][si];
                    work.last_mut().unwrap().1 += 1;
                    let wi = w as usize;

                    if node_index[wi] < 0 {
                        // Not yet visited: push and initialize
                        work.push((w, 0));
                        node_index[wi] = index_counter as i64;
                        lowlink[wi] = index_counter;
                        index_counter += 1;
                        tarjan_stack.push(w);
                        on_stack[wi] = true;
                    } else if on_stack[wi] {
                        lowlink[vi] = lowlink[vi].min(node_index[wi] as u32);
                    }
                } else {
                    work.pop();
                    // Update parent's lowlink
                    if let Some(&(parent, _)) = work.last() {
                        lowlink[parent as usize] = lowlink[parent as usize].min(lowlink[vi]);
                    }
                    // If v is SCC root, pop the SCC
                    if lowlink[vi] == node_index[vi] as u32 {
                        let mut size = 0;
                        loop {
                            let w = tarjan_stack.pop().unwrap();
                            on_stack[w as usize] = false;
                            node_scc[w as usize] = scc_id;
                            size += 1;
                            if w == v {
                                break;
                            }
                        }
                        scc_sizes.push(size);
                        scc_id += 1;
                    }
                }
            }
        }

        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let sid = node_scc[i];
                let size = scc_sizes.get(sid).copied().unwrap_or(1);
                (id.clone(), (sid, size))
            })
            .collect()
    }

    /// Compute dependency depth for all functions.
    ///
    /// Returns a map from function ID to depth (0 = entry point, None = unreachable).
    pub fn compute_dependency_depth(&self) -> HashMap<String, Option<usize>> {
        let n = self.ids.len();
        let mut depths: Vec<Option<usize>> = vec![None; n];
        let mut queue: VecDeque<(u32, usize)> = VecDeque::new();

        // Identify entry points
        let mut entry_indices: Vec<u32> = (0..n as u32)
            .filter(|&i| self.is_entry_point(&self.ids[i as usize]))
            .collect();

        if entry_indices.is_empty() {
            let mut fan_in = vec![0usize; n];
            for callees in &self.adj {
                for &c in callees {
                    fan_in[c as usize] += 1;
                }
            }
            entry_indices = (0..n as u32).filter(|&i| fan_in[i as usize] == 0).collect();
        }

        for entry in entry_indices {
            depths[entry as usize] = Some(0);
            queue.push_back((entry, 0));
        }

        while let Some((node_idx, depth)) = queue.pop_front() {
            for &callee_idx in &self.adj[node_idx as usize] {
                let ci = callee_idx as usize;
                let current = depths[ci];
                if current.is_none() || current.unwrap() > depth + 1 {
                    depths[ci] = Some(depth + 1);
                    queue.push_back((callee_idx, depth + 1));
                }
            }
        }

        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), depths[i]))
            .collect()
    }

    /// Build a map from function ID to its fan-in count in O(N + E).
    pub fn build_fan_in_map(&self) -> HashMap<String, usize> {
        let n = self.ids.len();
        let mut counts = vec![0usize; n];
        for callees in &self.adj {
            for &callee in callees {
                counts[callee as usize] += 1;
            }
        }
        self.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), counts[i]))
            .collect()
    }

    /// Check if a function is likely an entry point.
    pub fn is_entry_point(&self, function_id: &str) -> bool {
        let function_name = function_id.split("::").last().unwrap_or("").to_lowercase();

        let entry_point_names = [
            "main",
            "start",
            "init",
            "initialize",
            "run",
            "execute",
            "bootstrap",
        ];

        let handler_patterns = [
            "handle",
            "handler",
            "onrequest",
            "onmessage",
            "onevent",
            "middleware",
            "controller",
        ];

        if entry_point_names.contains(&function_name.as_str()) {
            return true;
        }

        for pattern in &handler_patterns {
            if function_name.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Calculate all graph metrics for a function.
    pub fn metrics_for(
        &self,
        function_id: &str,
        pagerank_scores: &HashMap<String, f64>,
        betweenness_scores: &HashMap<String, f64>,
    ) -> GraphMetrics {
        GraphMetrics {
            fan_in: self.fan_in(function_id),
            fan_out: self.fan_out(function_id),
            pagerank: pagerank_scores.get(function_id).copied().unwrap_or(0.0),
            betweenness: betweenness_scores.get(function_id).copied().unwrap_or(0.0),
        }
    }
}

/// Brandes' BFS phase from a single source, operating on pre-allocated Vec buffers.
///
/// `stack` enters holding the previous call's visited nodes (used for cleanup) and exits
/// holding the current BFS order. This avoids a separate `touched` tracker while keeping
/// cleanup O(visited) rather than O(N).
fn brandes_bfs_inplace(
    source: u32,
    adj: &[Vec<u32>],
    stack: &mut Vec<u32>,
    pred: &mut [Vec<u32>],
    sigma: &mut [f64],
    dist: &mut [i32],
    queue: &mut VecDeque<u32>,
) {
    // Clear state for nodes visited in the previous call (stack still holds them)
    for &i in stack.iter() {
        pred[i as usize].clear();
        sigma[i as usize] = 0.0;
        dist[i as usize] = -1;
    }
    stack.clear();
    queue.clear();

    let s = source as usize;
    dist[s] = 0;
    sigma[s] = 1.0;
    queue.push_back(source);

    while let Some(v) = queue.pop_front() {
        let vi = v as usize;
        stack.push(v);
        for &w in &adj[vi] {
            let wi = w as usize;
            if dist[wi] < 0 {
                queue.push_back(w);
                dist[wi] = dist[vi] + 1;
            }
            if dist[wi] == dist[vi] + 1 {
                sigma[wi] += sigma[vi];
                pred[wi].push(v);
            }
        }
    }
}

/// Brandes' accumulation phase. Resets and fills `delta` for nodes on `stack`.
fn brandes_accumulate_inplace(stack: &[u32], pred: &[Vec<u32>], sigma: &[f64], delta: &mut [f64]) {
    for &w in stack {
        delta[w as usize] = 0.0;
    }
    for &w in stack.iter().rev() {
        let wi = w as usize;
        for &v in &pred[wi] {
            let vi = v as usize;
            delta[vi] += (sigma[vi] / sigma[wi].max(1e-300)) * (1.0 + delta[wi]);
        }
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = CallGraph::new();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_fan_in_fan_out() {
        let mut graph = CallGraph::new();

        // A -> B
        // A -> C
        // B -> C
        graph.add_edge("A".to_string(), "B".to_string());
        graph.add_edge("A".to_string(), "C".to_string());
        graph.add_edge("B".to_string(), "C".to_string());

        assert_eq!(graph.fan_in("A"), 0); // No one calls A
        assert_eq!(graph.fan_out("A"), 2); // A calls B and C

        assert_eq!(graph.fan_in("B"), 1); // A calls B
        assert_eq!(graph.fan_out("B"), 1); // B calls C

        assert_eq!(graph.fan_in("C"), 2); // A and B call C
        assert_eq!(graph.fan_out("C"), 0); // C calls nothing
    }

    #[test]
    fn test_pagerank() {
        let mut graph = CallGraph::new();

        // Simple chain: A -> B -> C
        graph.add_edge("A".to_string(), "B".to_string());
        graph.add_edge("B".to_string(), "C".to_string());

        let ranks = graph.pagerank(0.85, 20, 1e-6);

        // C should have highest rank (called by B, which is called by A)
        // A should have lowest rank (not called by anyone)
        assert!(ranks.get("C").copied().unwrap_or(0.0) > ranks.get("B").copied().unwrap_or(0.0));
        assert!(ranks.get("B").copied().unwrap_or(0.0) > ranks.get("A").copied().unwrap_or(0.0));
    }

    #[test]
    fn test_build_fan_in_map() {
        let mut graph = CallGraph::new();
        // A -> B, A -> C, B -> C
        graph.add_edge("A".to_string(), "B".to_string());
        graph.add_edge("A".to_string(), "C".to_string());
        graph.add_edge("B".to_string(), "C".to_string());

        let fan_in = graph.build_fan_in_map();
        assert_eq!(fan_in.get("A").copied().unwrap_or(0), 0); // nobody calls A
        assert_eq!(fan_in.get("B").copied().unwrap_or(0), 1); // A calls B
        assert_eq!(fan_in.get("C").copied().unwrap_or(0), 2); // A and B call C
    }

    #[test]
    fn test_betweenness_linear_chain() {
        // a -> b -> c: b is the only intermediary on the a→c shortest path.
        // Normalized betweenness for b = 1 / ((3-1)(3-2)) = 0.5
        let mut graph = CallGraph::new();
        graph.add_edge("a".to_string(), "b".to_string());
        graph.add_edge("b".to_string(), "c".to_string());

        let scores = graph.betweenness_centrality();
        assert!(
            (scores["b"] - 0.5).abs() < 1e-10,
            "b betweenness should be 0.5"
        );
        assert!(scores["a"].abs() < 1e-10, "a betweenness should be 0.0");
        assert!(scores["c"].abs() < 1e-10, "c betweenness should be 0.0");
    }

    #[test]
    fn test_approx_betweenness_equals_exact_when_k_geq_n() {
        // When k >= n the n<=k guard forces exact computation; approx and exact must match.
        let mut graph = CallGraph::new();
        graph.add_edge("a".to_string(), "b".to_string());
        graph.add_edge("b".to_string(), "c".to_string());
        graph.add_edge("c".to_string(), "d".to_string());

        let exact = graph.betweenness_centrality();
        let approx = graph.betweenness_centrality_approx(100); // k >> n=4

        for (node, &exact_val) in &exact {
            let approx_val = approx.get(node).copied().unwrap_or(0.0);
            assert!(
                (exact_val - approx_val).abs() < 1e-10,
                "node {node}: exact={exact_val}, approx={approx_val}"
            );
        }
    }

    #[test]
    fn test_approx_betweenness_identifies_bridge() {
        // "bridge" is the only node connecting callers to callees, so it must have
        // the highest betweenness even when approximation is used (k=2 < n=4).
        //
        //   a ──► bridge ──► y
        //                └──► z
        let mut graph = CallGraph::new();
        graph.add_edge("a".to_string(), "bridge".to_string());
        graph.add_edge("bridge".to_string(), "y".to_string());
        graph.add_edge("bridge".to_string(), "z".to_string());

        let approx = graph.betweenness_centrality_approx(2);
        let bridge_score = approx.get("bridge").copied().unwrap_or(0.0);
        for (node, &score) in &approx {
            if node != "bridge" {
                assert!(
                    bridge_score >= score,
                    "bridge ({bridge_score}) should dominate {node} ({score})"
                );
            }
        }
    }

    #[test]
    fn test_approx_betweenness_pivot_covers_tail() {
        // Regression for the (i*n)/k sampling fix.
        //
        // "z_source" sorts last and has outgoing paths through "hub" to several
        // destinations.  With the old `step = n/k` formula, z_source would be
        // excluded as a pivot when k is small (tail nodes are never sampled).
        // We verify:
        //   1. approx(k=n) matches exact exactly (the n<=k fallback).
        //   2. hub has the highest betweenness in the exact result — confirming
        //      the graph structure is meaningful.
        //
        //   a_in ──┐
        //   b_in ──┤──► hub ──► x_out
        //  z_source┘        └──► y_out
        let mut graph = CallGraph::new();
        graph.add_edge("a_in".to_string(), "hub".to_string());
        graph.add_edge("b_in".to_string(), "hub".to_string());
        graph.add_edge("z_source".to_string(), "hub".to_string());
        graph.add_edge("hub".to_string(), "x_out".to_string());
        graph.add_edge("hub".to_string(), "y_out".to_string());

        let n = graph.node_count();
        let exact = graph.betweenness_centrality();

        // k=n must be byte-for-byte identical to exact
        let approx_full = graph.betweenness_centrality_approx(n);
        for (node, &val) in &exact {
            let av = approx_full.get(node).copied().unwrap_or(0.0);
            assert!(
                (val - av).abs() < 1e-10,
                "k=n mismatch for {node}: exact={val}, approx={av}"
            );
        }

        // hub must be the top-betweenness node in exact
        let hub_score = exact.get("hub").copied().unwrap_or(0.0);
        assert!(hub_score > 0.0, "hub should have non-zero betweenness");
        for (node, &val) in &exact {
            if node != "hub" {
                assert!(
                    hub_score >= val,
                    "hub ({hub_score}) should have highest betweenness, but {node}={val}"
                );
            }
        }
    }

    #[test]
    fn test_approx_betweenness_top_hubs_rank_preserved() {
        // Core invariant: approximate betweenness surfaces the biggest structural
        // offenders in the right order, even at k << n.
        //
        // Three dumbbell clusters, each with a single hub bridging its in- and
        // out-nodes. Different cluster sizes give separated exact betweenness so
        // we can assert strict rank ordering, not just set membership.
        //
        //   in_hub_a_0..49 ──► hub_a ──► out_hub_a_0..49   (50×50 = 2500 paths)
        //   in_hub_b_0..29 ──► hub_b ──► out_hub_b_0..29   (30×30 =  900 paths)
        //   in_hub_c_0..14 ──► hub_c ──► out_hub_c_0..14   (15×15 =  225 paths)
        //
        // ~193 nodes total, k=32. The three hubs are the only non-leaf nodes so
        // they must be the top-3 in both exact and approximate rankings.
        let mut graph = CallGraph::new();
        for (hub, size) in [("hub_a", 50usize), ("hub_b", 30), ("hub_c", 15)] {
            for i in 0..size {
                graph.add_edge(format!("in_{hub}_{i}"), hub.to_string());
                graph.add_edge(hub.to_string(), format!("out_{hub}_{i}"));
            }
        }

        assert!(
            graph.node_count() > 32,
            "graph must be large enough that k=32 is a real approximation"
        );

        let exact = graph.betweenness_centrality();
        let approx = graph.betweenness_centrality_approx(32);

        // Exact ranking must be hub_a > hub_b > hub_c (structural guarantee from cluster sizes)
        let ex_a = exact.get("hub_a").copied().unwrap_or(0.0);
        let ex_b = exact.get("hub_b").copied().unwrap_or(0.0);
        let ex_c = exact.get("hub_c").copied().unwrap_or(0.0);
        assert!(
            ex_a > ex_b && ex_b > ex_c,
            "exact: hub_a={ex_a} hub_b={ex_b} hub_c={ex_c}"
        );

        // Approximate ranking must preserve hub_a > hub_b > hub_c
        let ap_a = approx.get("hub_a").copied().unwrap_or(0.0);
        let ap_b = approx.get("hub_b").copied().unwrap_or(0.0);
        let ap_c = approx.get("hub_c").copied().unwrap_or(0.0);
        assert!(
            ap_a > ap_b && ap_b > ap_c,
            "approx rank broken: hub_a={ap_a} hub_b={ap_b} hub_c={ap_c}"
        );

        // All three hubs must appear in the top-3 — no leaf node should outrank them
        let mut ranked: Vec<(&str, f64)> = approx.iter().map(|(k, &v)| (k.as_str(), v)).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top3: Vec<&str> = ranked.iter().take(3).map(|(name, _)| *name).collect();
        assert!(
            top3.contains(&"hub_a"),
            "hub_a missing from top-3: {top3:?}"
        );
        assert!(
            top3.contains(&"hub_b"),
            "hub_b missing from top-3: {top3:?}"
        );
        assert!(
            top3.contains(&"hub_c"),
            "hub_c missing from top-3: {top3:?}"
        );
    }
}
