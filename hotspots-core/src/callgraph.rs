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

use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Call graph for a codebase
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// Map from function ID to list of called function IDs
    pub edges: HashMap<String, Vec<String>>,
    /// All functions in the graph
    pub nodes: HashSet<String>,
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

    /// Calculate betweenness centrality for all functions
    ///
    /// Betweenness measures how often a function appears on shortest paths between other functions.
    /// High betweenness indicates a function is a critical bridge/bottleneck.
    ///
    /// Uses Brandes' algorithm for efficient computation.
    pub fn betweenness_centrality(&self) -> HashMap<String, f64> {
        let mut betweenness: HashMap<String, f64> =
            self.nodes.iter().map(|node| (node.clone(), 0.0)).collect();

        // For each node, compute shortest paths using BFS
        for source in &self.nodes {
            let mut stack = Vec::new();
            let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
            let mut distance: HashMap<String, i32> = HashMap::new();
            let mut sigma: HashMap<String, f64> = HashMap::new();

            for node in &self.nodes {
                distance.insert(node.clone(), -1);
                sigma.insert(node.clone(), 0.0);
            }

            distance.insert(source.clone(), 0);
            sigma.insert(source.clone(), 1.0);

            // BFS to find shortest paths
            let mut queue = vec![source.clone()];
            while let Some(v) = queue.pop() {
                stack.push(v.clone());

                if let Some(neighbors) = self.edges.get(&v) {
                    for w in neighbors {
                        // First time we see w?
                        if distance.get(w).copied().unwrap_or(-1) < 0 {
                            queue.insert(0, w.clone());
                            distance.insert(w.clone(), distance.get(&v).copied().unwrap_or(0) + 1);
                        }

                        // Shortest path to w via v?
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

            // Accumulation phase
            let mut delta: HashMap<String, f64> =
                self.nodes.iter().map(|node| (node.clone(), 0.0)).collect();

            while let Some(w) = stack.pop() {
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

                if &w != source {
                    let current = betweenness.get(&w).copied().unwrap_or(0.0);
                    let delta_w = delta.get(&w).copied().unwrap_or(0.0);
                    betweenness.insert(w, current + delta_w);
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
        let mut index = 0;
        let mut stack = Vec::new();
        let mut indices: HashMap<String, usize> = HashMap::new();
        let mut lowlinks: HashMap<String, usize> = HashMap::new();
        let mut on_stack: HashMap<String, bool> = HashMap::new();
        let mut scc_id = 0;
        let mut result: HashMap<String, (usize, usize)> = HashMap::new();
        let mut scc_sizes: HashMap<usize, usize> = HashMap::new();

        for node in &self.nodes {
            if !indices.contains_key(node) {
                self.tarjan_strongconnect(
                    node,
                    &mut index,
                    &mut stack,
                    &mut indices,
                    &mut lowlinks,
                    &mut on_stack,
                    &mut scc_id,
                    &mut result,
                    &mut scc_sizes,
                );
            }
        }

        // Add SCC sizes to result
        let mut final_result: HashMap<String, (usize, usize)> = HashMap::new();
        for (node, (id, _)) in result {
            let size = *scc_sizes.get(&id).unwrap_or(&1);
            final_result.insert(node, (id, size));
        }

        final_result
    }

    /// Tarjan's algorithm helper function
    #[allow(clippy::too_many_arguments)]
    fn tarjan_strongconnect(
        &self,
        v: &str,
        index: &mut usize,
        stack: &mut Vec<String>,
        indices: &mut HashMap<String, usize>,
        lowlinks: &mut HashMap<String, usize>,
        on_stack: &mut HashMap<String, bool>,
        scc_id: &mut usize,
        result: &mut HashMap<String, (usize, usize)>,
        scc_sizes: &mut HashMap<usize, usize>,
    ) {
        indices.insert(v.to_string(), *index);
        lowlinks.insert(v.to_string(), *index);
        *index += 1;
        stack.push(v.to_string());
        on_stack.insert(v.to_string(), true);

        // Consider successors of v
        if let Some(successors) = self.edges.get(v) {
            for w in successors {
                if !indices.contains_key(w) {
                    // Successor w has not yet been visited; recurse on it
                    self.tarjan_strongconnect(
                        w, index, stack, indices, lowlinks, on_stack, scc_id, result, scc_sizes,
                    );
                    let w_lowlink = *lowlinks.get(w).unwrap_or(&0);
                    let v_lowlink = *lowlinks.get(v).unwrap_or(&0);
                    lowlinks.insert(v.to_string(), v_lowlink.min(w_lowlink));
                } else if *on_stack.get(w).unwrap_or(&false) {
                    // Successor w is in stack and hence in the current SCC
                    let w_index = *indices.get(w).unwrap_or(&0);
                    let v_lowlink = *lowlinks.get(v).unwrap_or(&0);
                    lowlinks.insert(v.to_string(), v_lowlink.min(w_index));
                }
            }
        }

        // If v is a root node, pop the stack and generate an SCC
        let v_lowlink = *lowlinks.get(v).unwrap_or(&0);
        let v_index = *indices.get(v).unwrap_or(&0);
        if v_lowlink == v_index {
            let mut scc = Vec::new();
            loop {
                if let Some(w) = stack.pop() {
                    on_stack.insert(w.clone(), false);
                    scc.push(w.clone());
                    result.insert(w.clone(), (*scc_id, 0)); // Size will be filled later
                    if w == v {
                        break;
                    }
                } else {
                    break;
                }
            }
            scc_sizes.insert(*scc_id, scc.len());
            *scc_id += 1;
        }
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

    /// Build a call graph from source files
    ///
    /// Extracts function calls using regex patterns (simple but effective).
    /// More sophisticated AST-based extraction can be added later.
    ///
    /// **Important**: This only tracks internal calls (functions in the analyzed codebase).
    /// External library calls, dynamic calls, and callbacks are intentionally excluded
    /// to keep analysis fast and deterministic.
    ///
    /// # Arguments
    ///
    /// * `files` - Map from file path to (source code, list of function names in that file)
    pub fn from_sources(files: &HashMap<String, (String, Vec<String>)>) -> Self {
        let mut graph = CallGraph::new();

        // Add all functions as nodes first
        for (_source, functions) in files.values() {
            for func in functions {
                graph.add_node(func.clone());
            }
        }

        // Extract calls using regex patterns
        // Pattern matches: functionName(...), object.method(...), etc.
        let call_pattern = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();

        for (source, functions) in files.values() {
            // For each function, find what it calls
            // This is simplified: we assume each function's code is separable
            // A more sophisticated approach would use AST parsing

            for func in functions {
                // Find function calls in the source
                for cap in call_pattern.captures_iter(source) {
                    if let Some(called_func) = cap.get(1) {
                        let called_name = called_func.as_str().to_string();

                        // INTERNAL CALLS ONLY: Only add edge if both caller and callee
                        // are in our graph (i.e., defined in the analyzed codebase).
                        // This excludes external libraries, runtime APIs, etc.
                        if graph.nodes.contains(&called_name) && &called_name != func {
                            graph.add_edge(func.clone(), called_name);
                        }
                    }
                }
            }
        }

        graph
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
