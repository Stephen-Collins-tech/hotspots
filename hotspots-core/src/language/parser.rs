//! Language-agnostic parser traits

use crate::ast::FunctionNode;
use anyhow::Result;

/// Language-agnostic parser interface
///
/// Each supported language must implement this trait to parse source code
/// into a ParsedModule representation.
pub trait LanguageParser {
    /// Parse source code into a module
    ///
    /// # Arguments
    ///
    /// * `source` - The source code to parse
    /// * `filename` - The name of the file being parsed (for error messages)
    ///
    /// # Returns
    ///
    /// A boxed ParsedModule trait object that can discover functions
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>>;
}

/// Parsed module interface
///
/// Represents a parsed source file that can be analyzed for functions.
/// This abstraction allows different parsers to produce a common representation.
pub trait ParsedModule {
    /// Discover all functions in this module
    ///
    /// Returns functions sorted deterministically by span start position.
    ///
    /// # Arguments
    ///
    /// * `file_index` - Index of this file in the analysis
    /// * `source` - Original source code (for suppression extraction)
    ///
    /// # Returns
    ///
    /// Vector of function nodes sorted by source position
    fn discover_functions(&self, file_index: usize, source: &str) -> Vec<FunctionNode>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    // Test implementation of ParsedModule
    struct TestModule {
        function_count: usize,
    }

    impl ParsedModule for TestModule {
        fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
            // Create dummy functions for testing
            (0..self.function_count)
                .map(|i| FunctionNode {
                    id: FunctionId {
                        file_index,
                        local_index: i,
                    },
                    name: Some(format!("test_fn_{}", i)),
                    span: SourceSpan::new(i * 10, (i + 1) * 10, (i + 1) as u32, 0),
                    body: FunctionBody::ecmascript(swc_ecma_ast::BlockStmt {
                        span: swc_common::DUMMY_SP,
                        ctxt: Default::default(),
                        stmts: vec![],
                    }),
                    suppression_reason: None,
                })
                .collect()
        }
    }

    // Test implementation of LanguageParser
    struct TestParser {
        function_count: usize,
    }

    impl LanguageParser for TestParser {
        fn parse(&self, _source: &str, _filename: &str) -> Result<Box<dyn ParsedModule>> {
            Ok(Box::new(TestModule {
                function_count: self.function_count,
            }))
        }
    }

    #[test]
    fn test_parser_trait() {
        let parser = TestParser { function_count: 3 };
        let module = parser.parse("test source", "test.ts").unwrap();
        let functions = module.discover_functions(0, "test source");

        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, Some("test_fn_0".to_string()));
        assert_eq!(functions[1].name, Some("test_fn_1".to_string()));
        assert_eq!(functions[2].name, Some("test_fn_2".to_string()));
    }

    #[test]
    fn test_parsed_module_trait() {
        let module = TestModule { function_count: 2 };
        let functions = module.discover_functions(5, "source");

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].id.file_index, 5);
        assert_eq!(functions[0].id.local_index, 0);
        assert_eq!(functions[1].id.local_index, 1);
    }

    #[test]
    fn test_empty_module() {
        let module = TestModule { function_count: 0 };
        let functions = module.discover_functions(0, "");
        assert_eq!(functions.len(), 0);
    }
}
