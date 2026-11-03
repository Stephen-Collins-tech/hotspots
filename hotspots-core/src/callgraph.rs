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

use std::collections::HashMap;

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

    /// Iterate over all interned function IDs in the graph.
    pub fn all_ids(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(|s| s.as_str())
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
}
