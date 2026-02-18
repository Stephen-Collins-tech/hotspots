//! Control Flow Graph (CFG) construction and analysis
//!
//! Global invariants enforced:
//! - One CFG per function
//! - No cross-function edges
//! - No global graph
//! - Deterministic node and edge ordering

pub mod builder;

use std::collections::BTreeSet;

/// CFG node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// Kind of CFG node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Entry node (always first)
    Entry,
    /// Exit node (always last)
    Exit,
    /// Control-relevant statement
    Statement,
    /// Condition node (if, switch, loop condition)
    Condition,
    /// Loop header node
    LoopHeader,
    /// Join node (convergence point after branches)
    Join,
}

/// A node in the control flow graph
#[derive(Debug, Clone)]
pub struct CfgNode {
    pub id: NodeId,
    pub kind: NodeKind,
}

/// An edge in the control flow graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CfgEdge {
    pub from: NodeId,
    pub to: NodeId,
}

/// Control Flow Graph for a single function
///
/// Rules:
/// - One CFG per function
/// - No cross-function edges
/// - No global graph
/// - Exactly one entry and one exit node
#[derive(Debug, Clone)]
pub struct Cfg {
    pub nodes: Vec<CfgNode>,
    pub edges: Vec<CfgEdge>,
    pub entry: NodeId,
    pub exit: NodeId,
}

impl Cfg {
    /// Create a new empty CFG with entry and exit nodes
    pub fn new() -> Self {
        let entry_node = CfgNode {
            id: NodeId(0),
            kind: NodeKind::Entry,
        };
        let exit_node = CfgNode {
            id: NodeId(1),
            kind: NodeKind::Exit,
        };

        Cfg {
            nodes: vec![entry_node.clone(), exit_node.clone()],
            edges: Vec::new(),
            entry: entry_node.id,
            exit: exit_node.id,
        }
    }

    /// Add a new node to the CFG
    ///
    /// Returns the NodeId of the added node
    pub fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(CfgNode { id, kind });
        id
    }

    /// Add an edge to the CFG
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.edges.push(CfgEdge { from, to });
    }

    /// Validate the CFG structure
    ///
    /// Returns Ok(()) if valid, or an error describing the violation
    pub fn validate(&self) -> Result<(), String> {
        self.validate_entry_exit_counts()?;
        // Empty functions (only entry + exit, no edges) have implicit flow to exit
        let is_empty = self.nodes.len() == 2 && self.edges.is_empty();
        self.validate_forward_reachability(is_empty)?;
        self.validate_backward_reachability(is_empty)?;
        Ok(())
    }

    fn validate_entry_exit_counts(&self) -> Result<(), String> {
        let entry_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Entry))
            .count();
        if entry_count != 1 {
            return Err(format!(
                "Expected exactly 1 entry node, found {}",
                entry_count
            ));
        }
        let exit_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Exit))
            .count();
        if exit_count != 1 {
            return Err(format!(
                "Expected exactly 1 exit node, found {}",
                exit_count
            ));
        }
        Ok(())
    }

    fn validate_forward_reachability(&self, is_empty: bool) -> Result<(), String> {
        let reachable = self.reachable_from(self.entry);
        let all_node_ids: BTreeSet<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        let unreachable: Vec<_> = all_node_ids.difference(&reachable).copied().collect();
        // In empty functions, exit is unreachable from entry — that's expected
        if !(unreachable.is_empty() || is_empty && unreachable == vec![self.exit]) {
            return Err(format!("Nodes not reachable from entry: {:?}", unreachable));
        }
        Ok(())
    }

    fn validate_backward_reachability(&self, is_empty: bool) -> Result<(), String> {
        let can_reach_exit = self.reachable_to(self.exit);
        let all_node_ids: BTreeSet<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        let cannot_reach_exit: Vec<_> = all_node_ids.difference(&can_reach_exit).copied().collect();
        if !can_reach_exit.contains(&self.entry) && !is_empty {
            let has_direct_edge = self
                .edges
                .iter()
                .any(|e| e.from == self.entry && e.to == self.exit);
            if !has_direct_edge {
                return Err("Entry node cannot reach exit node".to_string());
            }
        }
        for node_id in cannot_reach_exit {
            if is_empty && node_id == self.entry {
                continue;
            }
            let has_exit_edge = self
                .edges
                .iter()
                .any(|e| e.from == node_id && e.to == self.exit);
            if !has_exit_edge {
                return Err(format!(
                    "Node {:?} cannot reach exit and has no explicit exit edge",
                    node_id
                ));
            }
        }
        Ok(())
    }

    /// Find all nodes reachable from a given node (forward reachability)
    fn reachable_from(&self, start: NodeId) -> BTreeSet<NodeId> {
        let mut visited = BTreeSet::new();
        let mut stack = vec![start];

        while let Some(node_id) = stack.pop() {
            if visited.insert(node_id) {
                // Add all nodes reachable via outgoing edges
                for edge in &self.edges {
                    if edge.from == node_id {
                        stack.push(edge.to);
                    }
                }
            }
        }

        visited
    }

    /// Find all nodes that can reach a given node (backward reachability)
    fn reachable_to(&self, target: NodeId) -> BTreeSet<NodeId> {
        let mut visited = BTreeSet::new();
        let mut stack = vec![target];

        while let Some(node_id) = stack.pop() {
            if visited.insert(node_id) {
                // Add all nodes that have edges to this node
                for edge in &self.edges {
                    if edge.to == node_id {
                        stack.push(edge.from);
                    }
                }
            }
        }

        visited
    }

    /// Get the number of edges
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for Cfg {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "cfg/tests.rs"]
mod tests;
