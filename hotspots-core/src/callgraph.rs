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

use std::collections::{HashMap, HashSet};

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

        // Iterative PageRank calculation
        for _ in 0..iterations {
            let mut new_ranks = HashMap::new();

            for node in &self.nodes {
                let mut rank = (1.0 - damping) / n as f64;

                // Sum contributions from all callers (sorted for determinism)
                if let Some(callers) = reverse_edges.get(node) {
                    let mut sorted_callers = callers.clone();
                    sorted_callers.sort();
                    for caller in &sorted_callers {
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

    /// Calculate betweenness centrality for all functions
    ///
    /// Betweenness measures how often a function appears on shortest paths between other functions.
    /// High betweenness indicates a function is a critical bridge/bottleneck.
    ///
    /// Uses Brandes' algorithm for efficient computation.
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

        // If no explicit entry points found, use all functions with fan_in = 0
        if entry_points.is_empty() {
            for node in &self.nodes {
                if self.fan_in(node) == 0 {
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

    /// Check if a function is likely an entry point
    fn is_entry_point(&self, function_id: &str) -> bool {
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

    // BFS: insert at front, pop from back (FIFO ordering)
    let mut queue = vec![source.to_string()];
    while let Some(v) = queue.pop() {
        stack.push(v.clone());
        if let Some(neighbors) = edges.get(&v) {
            for w in neighbors {
                if distance.get(w).copied().unwrap_or(-1) < 0 {
                    queue.insert(0, w.clone());
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
}
