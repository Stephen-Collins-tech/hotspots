//! Tests for Control Flow Graph

#[cfg(test)]
mod cfg_tests {
    use crate::cfg::{Cfg, NodeId, NodeKind};

    #[test]
    fn test_empty_cfg_construction() {
        let cfg = Cfg::new();
        assert_eq!(cfg.nodes.len(), 2, "Should have entry and exit nodes");
        assert_eq!(cfg.edge_count(), 0, "Empty CFG should have no edges");
        assert_eq!(cfg.node_count(), 2, "Should have exactly 2 nodes");
    }

    #[test]
    fn test_empty_cfg_validation() {
        let cfg = Cfg::new();
        let result = cfg.validate();
        assert!(
            result.is_ok(),
            "Empty CFG should validate (entry can reach exit via direct edge)"
        );
    }

    #[test]
    fn test_cfg_has_entry_and_exit() {
        let cfg = Cfg::new();
        let entry_nodes: Vec<_> = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Entry))
            .collect();
        let exit_nodes: Vec<_> = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Exit))
            .collect();

        assert_eq!(entry_nodes.len(), 1, "Should have exactly one entry node");
        assert_eq!(exit_nodes.len(), 1, "Should have exactly one exit node");
        assert_eq!(cfg.entry, entry_nodes[0].id);
        assert_eq!(cfg.exit, exit_nodes[0].id);
    }

    #[test]
    fn test_cfg_add_node() {
        let mut cfg = Cfg::new();
        let node_id = cfg.add_node(NodeKind::Statement);

        assert_eq!(
            cfg.nodes.len(),
            3,
            "Should have 3 nodes (entry, exit, statement)"
        );
        assert_eq!(node_id, NodeId(2), "Node ID should be 2");
        assert!(matches!(cfg.nodes[2].kind, NodeKind::Statement));
    }

    #[test]
    fn test_cfg_add_edge() {
        let mut cfg = Cfg::new();
        let node_id = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(cfg.entry, node_id);
        cfg.add_edge(node_id, cfg.exit);

        assert_eq!(cfg.edge_count(), 2, "Should have 2 edges");
        assert_eq!(cfg.edges[0].from, cfg.entry);
        assert_eq!(cfg.edges[0].to, node_id);
        assert_eq!(cfg.edges[1].from, node_id);
        assert_eq!(cfg.edges[1].to, cfg.exit);
    }

    #[test]
    fn test_cfg_validation_reachable_from_entry() {
        let mut cfg = Cfg::new();
        let node_id = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(cfg.entry, node_id);
        cfg.add_edge(node_id, cfg.exit);

        let result = cfg.validate();
        assert!(
            result.is_ok(),
            "CFG with path entry -> node -> exit should validate"
        );
    }

    #[test]
    fn test_cfg_validation_unreachable_node() {
        let mut cfg = Cfg::new();
        let _unreachable_id = cfg.add_node(NodeKind::Statement);
        // Don't add edge from entry to unreachable_id
        cfg.add_edge(cfg.entry, cfg.exit); // Entry can reach exit

        let result = cfg.validate();
        assert!(
            result.is_err(),
            "CFG with unreachable node should fail validation"
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("not reachable") || err_msg.contains("unreachable"),
            "Error should mention unreachable nodes, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_cfg_validation_entry_to_exit_direct() {
        let mut cfg = Cfg::new();
        cfg.add_edge(cfg.entry, cfg.exit);

        let result = cfg.validate();
        assert!(result.is_ok(), "CFG with entry -> exit should validate");
    }

    #[test]
    fn test_cfg_reachable_from() {
        let mut cfg = Cfg::new();
        let node1 = cfg.add_node(NodeKind::Statement);
        let node2 = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(cfg.entry, node1);
        cfg.add_edge(node1, node2);
        cfg.add_edge(node2, cfg.exit);

        let reachable = cfg.reachable_from(cfg.entry);
        assert!(reachable.contains(&cfg.entry));
        assert!(reachable.contains(&node1));
        assert!(reachable.contains(&node2));
        assert!(reachable.contains(&cfg.exit));
    }

    #[test]
    fn test_cfg_reachable_to() {
        let mut cfg = Cfg::new();
        let node1 = cfg.add_node(NodeKind::Statement);
        let node2 = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(cfg.entry, node1);
        cfg.add_edge(node1, node2);
        cfg.add_edge(node2, cfg.exit);

        let can_reach_exit = cfg.reachable_to(cfg.exit);
        assert!(can_reach_exit.contains(&cfg.entry));
        assert!(can_reach_exit.contains(&node1));
        assert!(can_reach_exit.contains(&node2));
        assert!(can_reach_exit.contains(&cfg.exit));
    }
}
