//! Go CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::Cfg;
use crate::language::cfg_builder::CfgBuilder;

/// Go CFG builder
///
/// Builds control flow graphs from Go function bodies parsed with tree-sitter.
pub struct GoCfgBuilder;

impl CfgBuilder for GoCfgBuilder {
    fn build(&self, _function: &FunctionNode) -> Cfg {
        // TODO: Implement full Go CFG building
        // For now, create a simple entry->exit CFG
        let mut cfg = Cfg::new();
        // Cfg::new() already creates entry and exit nodes
        // Just connect them
        cfg.add_edge(cfg.entry, cfg.exit);
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunctionId, FunctionNode};
    use crate::language::{FunctionBody, SourceSpan};

    fn make_test_go_function() -> FunctionNode {
        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test".to_string()),
            span: SourceSpan::new(0, 10, 1, 0),
            body: FunctionBody::Go {
                body_node: 0,
                source: "func test() {}".to_string(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_go_cfg_builder() {
        let function = make_test_go_function();
        let builder = GoCfgBuilder;
        let cfg = builder.build(&function);

        // Simple placeholder CFG should have entry and exit
        assert_eq!(cfg.node_count(), 2);
        assert_eq!(cfg.edge_count(), 1);
    }
}
