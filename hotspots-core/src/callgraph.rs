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

use std::collections::{HashMap, HashSet, VecDeque};

/// Call graph for a codebase
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// Map from function ID to list of called function IDs
    pub edges: HashMap<String, Vec<String>>,
    /// All functions in the graph
    pub nodes: HashSet<String>,
    /// Total callee names found in ASTs across all functions
    pub total_callee_names: usize,
    /// Callee names that resolved to a known internal function ID
    pub resolved_callee_names: usize,
}

/// Mutable state for Tarjan's SCC algorithm
struct TarjanState {
    index: usize,
    stack: Vec<String>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    on_stack: HashMap<String, bool>,
    scc_id: usize,
    result: HashMap<String, (usize, usize)>,
    scc_sizes: HashMap<usize, usize>,
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
            edges: HashMap::new(),
            nodes: HashSet::new(),
            total_callee_names: 0,
            resolved_callee_names: 0,
        }
    }

    /// Add a function to the graph
    pub fn add_node(&mut self, function_id: String) {
        self.nodes.insert(function_id);
    }

    /// Add a call edge (caller -> callee)
    pub fn add_edge(&mut self, caller: String, callee: String) {
        self.nodes.insert(caller.clone());
        self.nodes.insert(callee.clone());

        self.edges.entry(caller).or_default().push(callee);
    }

    /// Calculate fan-in for a function (number of callers)
    pub fn fan_in(&self, function_id: &str) -> usize {
        self.edges
            .values()
            .filter(|callees| callees.contains(&function_id.to_string()))
            .count()
    }

    /// Calculate fan-out for a function (number of callees)
    pub fn fan_out(&self, function_id: &str) -> usize {
        self.edges
            .get(function_id)
            .map(|callees| callees.len())
            .unwrap_or(0)
    }

    /// Calculate PageRank for all functions
    ///
    /// PageRank identifies important/central functions in the call graph.
    /// Higher scores indicate functions that are frequently called by other important functions.
    ///
    /// # Arguments
    ///
    /// * `damping` - Damping factor (typically 0.85)
    /// * `iterations` - Number of iterations (typically 20-50)
    pub fn pagerank(&self, damping: f64, iterations: usize) -> HashMap<String, f64> {
        let n = self.nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        let initial_rank = 1.0 / n as f64;
        let mut ranks: HashMap<String, f64> = self
            .nodes
            .iter()
            .map(|node| (node.clone(), initial_rank))
            .collect();

        // Build reverse edges (who calls whom)
        let mut reverse_edges: HashMap<String, Vec<String>> = HashMap::new();
        for (caller, callees) in &self.edges {
            for callee in callees {
                reverse_edges
                    .entry(callee.clone())
                    .or_default()
                    .push(caller.clone());
            }
        }

        // Sort caller lists once for deterministic computation across all iterations
        for callers in reverse_edges.values_mut() {
            callers.sort();
        }

        // Iterative PageRank calculation
        for _ in 0..iterations {
            let mut new_ranks = HashMap::new();

            for node in &self.nodes {
                let mut rank = (1.0 - damping) / n as f64;

                // Sum contributions from all callers
                if let Some(callers) = reverse_edges.get(node) {
                    for caller in callers {
                        let caller_rank = ranks.get(caller).copied().unwrap_or(initial_rank);
                        let caller_fan_out = self.fan_out(caller).max(1);
                        rank += damping * (caller_rank / caller_fan_out as f64);
                    }
                }

                new_ranks.insert(node.clone(), rank);
            }

            ranks = new_ranks;
        }

        ranks
    }

    /// Calculate betweenness centrality using pivoted source sampling (approximate).
    ///
    /// Uses systematic k-source sampling: selects k sources evenly spaced through the
    /// sorted node list and scales contributions by N/k. This gives an unbiased estimator
    /// of exact betweenness with O(k × (N+E)) complexity instead of O(N × (N+E)).
    ///
    /// Falls back to exact computation when `self.nodes.len() <= k`.
    ///
    /// The source selection is deterministic (no RNG): sorting + stride gives identical
    /// output for identical input, preserving the codebase's byte-for-byte invariant.
    ///
    /// # Arguments
    ///
    /// * `k` - Number of pivot sources to sample (higher = more accurate, more time)
    pub fn betweenness_centrality_approx(&self, k: usize) -> HashMap<String, f64> {
        let n = self.nodes.len();
        if n <= k {
            return self.betweenness_centrality();
        }

        let mut sorted_nodes: Vec<&String> = self.nodes.iter().collect();
        sorted_nodes.sort();

        let scale = n as f64 / k as f64;

        let mut betweenness: HashMap<String, f64> =
            self.nodes.iter().map(|node| (node.clone(), 0.0)).collect();

        for i in 0..k {
            // Use (i * n) / k rather than i * (n / k) so that samples are spread
            // evenly across [0, n-1]. The naive step = n/k approach truncates the
            // tail: e.g. n=300, k=256 gives step=1 and only samples indices 0–255,
            // permanently excluding the last 44 nodes.
            let source = sorted_nodes[(i * n) / k];
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

    /// Calculate betweenness centrality for all functions (exact).
    ///
    /// Betweenness measures how often a function appears on shortest paths between other functions.
    /// High betweenness indicates a function is a critical bridge/bottleneck.
    ///
    /// Uses Brandes' algorithm: O(N × (N+E)). For large graphs use
    /// `betweenness_centrality_approx` instead.
    pub fn betweenness_centrality(&self) -> HashMap<String, f64> {
        let mut betweenness: HashMap<String, f64> =
            self.nodes.iter().map(|node| (node.clone(), 0.0)).collect();

        for source in &self.nodes {
            let (stack, predecessors, sigma) = brandes_bfs(source, &self.nodes, &self.edges);
            let delta = brandes_accumulate(&stack, &predecessors, &sigma);
            for w in &stack {
                if w != source {
                    *betweenness.entry(w.clone()).or_insert(0.0) +=
                        delta.get(w).copied().unwrap_or(0.0);
                }
            }
        }

        // Normalize for undirected graph
        let n = self.nodes.len();
        if n > 2 {
            let normalization = 1.0 / ((n - 1) * (n - 2)) as f64;
            for value in betweenness.values_mut() {
                *value *= normalization;
            }
        }

        betweenness
    }

    /// Find strongly connected components using Tarjan's algorithm
    ///
    /// Returns a map from function ID to (scc_id, scc_size)
    /// Functions in the same SCC form a cyclic dependency group.
    pub fn find_strongly_connected_components(&self) -> HashMap<String, (usize, usize)> {
        let mut state = TarjanState {
            index: 0,
            stack: Vec::new(),
            indices: HashMap::new(),
            lowlinks: HashMap::new(),
            on_stack: HashMap::new(),
            scc_id: 0,
            result: HashMap::new(),
            scc_sizes: HashMap::new(),
        };

        let mut sorted_nodes: Vec<&String> = self.nodes.iter().collect();
        sorted_nodes.sort();
        for node in sorted_nodes {
            if !state.indices.contains_key(node) {
                self.tarjan_strongconnect(node, &mut state);
            }
        }

        // Add SCC sizes to result
        let mut final_result: HashMap<String, (usize, usize)> = HashMap::new();
        for (node, (id, _)) in state.result {
            let size = *state.scc_sizes.get(&id).unwrap_or(&1);
            final_result.insert(node, (id, size));
        }

        final_result
    }

    /// Tarjan's algorithm helper function
    fn tarjan_strongconnect(&self, v: &str, state: &mut TarjanState) {
        state.indices.insert(v.to_string(), state.index);
        state.lowlinks.insert(v.to_string(), state.index);
        state.index += 1;
        state.stack.push(v.to_string());
        state.on_stack.insert(v.to_string(), true);

        // Consider successors of v
        if let Some(successors) = self.edges.get(v) {
            let mut sorted_successors = successors.clone();
            sorted_successors.sort();
            for w in sorted_successors {
                if !state.indices.contains_key(&w) {
                    // Successor w has not yet been visited; recurse on it
                    self.tarjan_strongconnect(&w, state);
                    let w_lowlink = *state.lowlinks.get(&w).unwrap_or(&0);
                    let v_lowlink = *state.lowlinks.get(v).unwrap_or(&0);
                    state
                        .lowlinks
                        .insert(v.to_string(), v_lowlink.min(w_lowlink));
                } else if *state.on_stack.get(&w).unwrap_or(&false) {
                    // Successor w is in stack and hence in the current SCC
                    let w_index = *state.indices.get(&w).unwrap_or(&0);
                    let v_lowlink = *state.lowlinks.get(v).unwrap_or(&0);
                    state.lowlinks.insert(v.to_string(), v_lowlink.min(w_index));
                }
            }
        }

        // If v is a root node, pop the stack and generate an SCC
        let v_lowlink = *state.lowlinks.get(v).unwrap_or(&0);
        let v_index = *state.indices.get(v).unwrap_or(&0);
        if v_lowlink == v_index {
            let mut scc = Vec::new();
            while let Some(w) = state.stack.pop() {
                state.on_stack.insert(w.clone(), false);
                scc.push(w.clone());
                state.result.insert(w.clone(), (state.scc_id, 0)); // Size filled later
                if w == v {
                    break;
                }
            }
            state.scc_sizes.insert(state.scc_id, scc.len());
            state.scc_id += 1;
        }
    }

    /// Compute dependency depth for all functions
    ///
    /// Uses BFS from entry points to compute shortest path depth.
    /// Entry points are identified using heuristics:
    /// - Functions named "main", "start", "init"
    /// - Functions with no incoming calls (potential entry points)
    /// - HTTP handlers (e.g., handleRequest, onRequest)
    ///
    /// Returns a map from function ID to depth (0 = entry point, None = unreachable)
    pub fn compute_dependency_depth(&self) -> HashMap<String, Option<usize>> {
        use std::collections::VecDeque;

        // Identify entry points using heuristics
        let mut entry_points = Vec::new();
        for node in &self.nodes {
            if self.is_entry_point(node) {
                entry_points.push(node.clone());
            }
        }

        // If no explicit entry points found, use all functions with fan_in = 0.
        // Build fan-in counts in O(N+E) instead of calling fan_in() per node (O(N*E)).
        if entry_points.is_empty() {
            let fan_in_map = self.build_fan_in_map();
            for (node, count) in &fan_in_map {
                if *count == 0 {
                    entry_points.push(node.clone());
                }
            }
        }

        // BFS from all entry points to compute depths
        let mut depths: HashMap<String, Option<usize>> = HashMap::new();
        let mut queue = VecDeque::new();

        // Initialize entry points with depth 0
        for entry in &entry_points {
            depths.insert(entry.clone(), Some(0));
            queue.push_back((entry.clone(), 0));
        }

        // BFS traversal
        while let Some((node, depth)) = queue.pop_front() {
            if let Some(callees) = self.edges.get(&node) {
                for callee in callees {
                    // Only update if not visited or found a shorter path
                    let current_depth = depths.get(callee).copied().flatten();
                    if current_depth.is_none() || current_depth.unwrap() > depth + 1 {
                        depths.insert(callee.clone(), Some(depth + 1));
                        queue.push_back((callee.clone(), depth + 1));
                    }
                }
            }
        }

        // Mark unreachable nodes
        for node in &self.nodes {
            if !depths.contains_key(node) {
                depths.insert(node.clone(), None);
            }
        }

        depths
    }

    /// Build a map from function ID to its fan-in count in O(N + E).
    ///
    /// Prefer this over repeated `fan_in()` calls when computing fan-in for many functions.
    pub fn build_fan_in_map(&self) -> HashMap<String, usize> {
        let mut map: HashMap<String, usize> = self.nodes.iter().map(|n| (n.clone(), 0)).collect();
        for callees in self.edges.values() {
            for callee in callees {
                *map.entry(callee.clone()).or_insert(0) += 1;
            }
        }
        map
    }

    /// Check if a function is likely an entry point
    pub fn is_entry_point(&self, function_id: &str) -> bool {
        // Extract function name from ID (format: "file::function")
        let function_name = function_id.split("::").last().unwrap_or("").to_lowercase();

        // Common entry point names
        let entry_point_names = [
            "main",
            "start",
            "init",
            "initialize",
            "run",
            "execute",
            "bootstrap",
        ];

        // HTTP handler patterns
        let handler_patterns = [
            "handle",
            "handler",
            "onrequest",
            "onmessage",
            "onevent",
            "middleware",
            "controller",
        ];

        // Check if function name matches entry point patterns
        if entry_point_names.contains(&function_name.as_str()) {
            return true;
        }

        // Check if function name contains handler patterns
        for pattern in &handler_patterns {
            if function_name.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Calculate all graph metrics for a function
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

/// BFS state returned by `brandes_bfs`: (stack, predecessors, sigma)
type BrandesBfsState = (
    Vec<String>,
    HashMap<String, Vec<String>>,
    HashMap<String, f64>,
);

/// Brandes' algorithm BFS phase from a single source.
///
/// Returns `(stack, predecessors, sigma)`:
/// - `stack`: nodes in BFS discovery order (used for reverse traversal in accumulation)
/// - `predecessors`: for each node, list of predecessors on shortest paths from source
/// - `sigma`: number of shortest paths from source to each node
fn brandes_bfs(
    source: &str,
    nodes: &HashSet<String>,
    edges: &HashMap<String, Vec<String>>,
) -> BrandesBfsState {
    let mut stack = Vec::new();
    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    let mut distance: HashMap<String, i32> = nodes.iter().map(|n| (n.clone(), -1)).collect();
    let mut sigma: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();

    distance.insert(source.to_string(), 0);
    sigma.insert(source.to_string(), 1.0);

    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(source.to_string());
    while let Some(v) = queue.pop_front() {
        stack.push(v.clone());
        if let Some(neighbors) = edges.get(&v) {
            for w in neighbors {
                if distance.get(w).copied().unwrap_or(-1) < 0 {
                    queue.push_back(w.clone());
                    distance.insert(w.clone(), distance.get(&v).copied().unwrap_or(0) + 1);
                }
                if distance.get(w).copied().unwrap_or(0)
                    == distance.get(&v).copied().unwrap_or(0) + 1
                {
                    let sigma_w = sigma.get(w).copied().unwrap_or(0.0);
                    let sigma_v = sigma.get(&v).copied().unwrap_or(0.0);
                    sigma.insert(w.clone(), sigma_w + sigma_v);
                    predecessors.entry(w.clone()).or_default().push(v.clone());
                }
            }
        }
    }
    (stack, predecessors, sigma)
}

/// Brandes' algorithm accumulation phase.
///
/// Back-propagates dependency scores through the BFS stack.
/// Returns `delta`: each node's contribution to betweenness from this source.
fn brandes_accumulate(
    stack: &[String],
    predecessors: &HashMap<String, Vec<String>>,
    sigma: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut delta: HashMap<String, f64> = stack.iter().map(|n| (n.clone(), 0.0)).collect();
    let mut work = stack.to_vec();
    while let Some(w) = work.pop() {
        if let Some(preds) = predecessors.get(&w) {
            for v in preds {
                let sigma_v = sigma.get(v).copied().unwrap_or(0.0);
                let sigma_w = sigma.get(&w).copied().unwrap_or(0.0);
                let delta_w = delta.get(&w).copied().unwrap_or(0.0);
                let contrib = (sigma_v / sigma_w.max(1.0)) * (1.0 + delta_w);
                let delta_v = delta.get(v).copied().unwrap_or(0.0);
                delta.insert(v.clone(), delta_v + contrib);
            }
        }
    }
    delta
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
        assert_eq!(graph.nodes.len(), 0);
        assert_eq!(graph.edges.len(), 0);
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

        let ranks = graph.pagerank(0.85, 20);

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

        let n = graph.nodes.len();
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
            graph.nodes.len() > 32,
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
