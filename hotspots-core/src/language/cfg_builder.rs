//! Language-agnostic CFG builder traits

use crate::ast::FunctionNode;
use crate::cfg::Cfg;

/// Language-agnostic CFG builder interface
///
/// Each supported language must implement this trait to build control flow graphs
/// from function bodies.
pub trait CfgBuilder {
    /// Build a control flow graph from a function
    ///
    /// # Arguments
    ///
    /// * `function` - The function node containing the body to analyze
    ///
    /// # Returns
    ///
    /// A CFG representing the control flow of the function
    fn build(&self, function: &FunctionNode) -> Cfg;
}

/// Get the appropriate CFG builder for a function based on its language
///
/// Currently only ECMAScript is supported. Future versions will dispatch
/// to Go, Rust, etc. builders based on the function body variant.
pub fn get_builder_for_function(_function: &FunctionNode) -> Box<dyn CfgBuilder> {
    // For now, always return ECMAScript builder since that's all we support
    // In the future, this will check function.body and dispatch accordingly:
    // match &function.body {
    //     FunctionBody::ECMAScript(_) => Box::new(ECMAScriptCfgBuilder),
    //     FunctionBody::Go(_) => Box::new(GoCfgBuilder),
    //     FunctionBody::Rust(_) => Box::new(RustCfgBuilder),
    // }
    Box::new(super::ecmascript::ECMAScriptCfgBuilder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    fn make_test_function() -> FunctionNode {
        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test".to_string()),
            span: SourceSpan::new(0, 10, 1, 0),
            body: FunctionBody::ecmascript(swc_ecma_ast::BlockStmt {
                span: swc_common::DUMMY_SP,
                ctxt: Default::default(),
                stmts: vec![],
            }),
            suppression_reason: None,
        }
    }

    #[test]
    fn test_get_builder_for_ecmascript() {
        let function = make_test_function();
        let builder = get_builder_for_function(&function);
        let cfg = builder.build(&function);

        // Basic CFG should have entry and exit nodes
        assert!(cfg.node_count() >= 2, "CFG should have at least entry and exit nodes");
    }

    #[test]
    fn test_builder_trait() {
        use super::super::ecmascript::ECMAScriptCfgBuilder;

        let function = make_test_function();
        let builder = ECMAScriptCfgBuilder;
        let cfg = builder.build(&function);

        // Empty function should have simple CFG: entry -> exit
        assert_eq!(cfg.node_count(), 2);
        assert_eq!(cfg.edge_count(), 1);
    }
}
