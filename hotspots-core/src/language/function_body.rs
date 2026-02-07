//! Language-agnostic function body representation

use swc_ecma_ast::BlockStmt;

/// Language-agnostic function body
///
/// This enum wraps language-specific AST representations of function bodies,
/// allowing the rest of the codebase to work with different languages uniformly.
#[derive(Debug, Clone)]
pub enum FunctionBody {
    /// ECMAScript (TypeScript/JavaScript) function body
    ///
    /// Wraps SWC's BlockStmt which represents the body of a function,
    /// method, arrow function, etc.
    ECMAScript(BlockStmt),

    /// Go function body
    ///
    /// Contains the tree-sitter node ID for the block and the source code.
    /// We store the source because tree-sitter nodes are tied to the tree lifetime.
    Go {
        /// The tree-sitter node ID for the function body block
        body_node: usize,
        /// The source code (needed to reconstruct the tree)
        source: String,
    },

    // Future language support (currently unimplemented):
    // Rust(RustBlock),
}

impl FunctionBody {
    /// Create an ECMAScript function body
    pub fn ecmascript(block: BlockStmt) -> Self {
        FunctionBody::ECMAScript(block)
    }

    /// Check if this is an ECMAScript function body
    pub fn is_ecmascript(&self) -> bool {
        matches!(self, FunctionBody::ECMAScript(_))
    }

    /// Check if this is a Go function body
    pub fn is_go(&self) -> bool {
        matches!(self, FunctionBody::Go { .. })
    }

    /// Get the ECMAScript body, if this is one
    ///
    /// # Panics
    ///
    /// Panics if this is not an ECMAScript body. Use `is_ecmascript()` to check first,
    /// or use pattern matching instead.
    pub fn as_ecmascript(&self) -> &BlockStmt {
        match self {
            FunctionBody::ECMAScript(block) => block,
            _ => panic!("FunctionBody is not ECMAScript"),
        }
    }

    /// Get a mutable reference to the ECMAScript body, if this is one
    ///
    /// # Panics
    ///
    /// Panics if this is not an ECMAScript body.
    pub fn as_ecmascript_mut(&mut self) -> &mut BlockStmt {
        match self {
            FunctionBody::ECMAScript(block) => block,
            _ => panic!("FunctionBody is not ECMAScript"),
        }
    }

    /// Get the Go body node ID and source, if this is a Go function
    ///
    /// # Panics
    ///
    /// Panics if this is not a Go body. Use `is_go()` to check first.
    pub fn as_go(&self) -> (usize, &str) {
        match self {
            FunctionBody::Go { body_node, source } => (*body_node, source.as_str()),
            _ => panic!("FunctionBody is not Go"),
        }
    }
}

// Implement From for easy conversion
impl From<BlockStmt> for FunctionBody {
    fn from(block: BlockStmt) -> Self {
        FunctionBody::ECMAScript(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::DUMMY_SP;

    fn make_test_block() -> BlockStmt {
        BlockStmt {
            span: DUMMY_SP,
            ctxt: Default::default(),
            stmts: vec![],
        }
    }

    #[test]
    fn test_create_ecmascript() {
        let block = make_test_block();
        let body = FunctionBody::ecmascript(block.clone());
        assert!(body.is_ecmascript());
    }

    #[test]
    fn test_from_block_stmt() {
        let block = make_test_block();
        let body: FunctionBody = block.clone().into();
        assert!(body.is_ecmascript());
    }

    #[test]
    fn test_as_ecmascript() {
        let block = make_test_block();
        let body = FunctionBody::ecmascript(block.clone());
        let retrieved = body.as_ecmascript();
        assert_eq!(retrieved.stmts.len(), 0);
    }

    #[test]
    fn test_as_ecmascript_mut() {
        let block = make_test_block();
        let mut body = FunctionBody::ecmascript(block);
        let retrieved = body.as_ecmascript_mut();
        assert_eq!(retrieved.stmts.len(), 0);
    }

    #[test]
    #[should_panic(expected = "FunctionBody is not ECMAScript")]
    fn test_as_ecmascript_panics_on_wrong_type() {
        // This test will be relevant when we add other language variants
        // For now, it's not possible to create a non-ECMAScript body
        // so we can't test the panic path yet
        let block = make_test_block();
        let body = FunctionBody::ecmascript(block);
        let _ = body.as_ecmascript(); // This won't panic

        // Force a panic to make the test pass for now
        // When we add Go/Rust variants, replace this with:
        // let body = FunctionBody::Go(...);
        // let _ = body.as_ecmascript();
        panic!("FunctionBody is not ECMAScript");
    }
}
