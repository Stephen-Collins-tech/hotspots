//! Python language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use crate::language::tree_sitter_utils::find_child_by_kind;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// Python parser using tree-sitter
pub struct PythonParser;

impl PythonParser {
    /// Create a new Python parser
    pub fn new() -> Result<Self> {
        // Just validate that we can create a parser
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Python language for parser")?;
        Ok(PythonParser)
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Python parser")
    }
}

impl LanguageParser for PythonParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        // Need to make parser mutable, so we can't use &self directly
        // This is a limitation of tree-sitter's API
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Python language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse Python file: {}", filename))?;

        Ok(Box::new(PythonModule {
            tree,
            source: source.to_string(),
        }))
    }
}

/// Parsed Python module
struct PythonModule {
    tree: Tree,
    source: String,
}

impl ParsedModule for PythonModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        let root = self.tree.root_node();
        let mut functions = Vec::new();

        // Walk the tree to find function declarations
        discover_functions_recursive(root, &self.source, file_index, &mut functions);

        // Sort by source position for determinism
        functions.sort_by_key(|f| f.span.start);

        functions
    }
}

/// Recursively discover function declarations in the Python AST
fn discover_functions_recursive(
    node: Node,
    source: &str,
    file_index: usize,
    functions: &mut Vec<FunctionNode>,
) {
    // Check if this node is a function declaration
    // Python has "function_definition" for regular functions and "async_function_definition" for async functions
    if node.kind() == "function_definition" || node.kind() == "async_function_definition" {
        if let Some(function_node) = extract_function(node, source, file_index, functions.len()) {
            functions.push(function_node);
        }
    }

    // Recurse into children (this will find nested functions and methods)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_functions_recursive(child, source, file_index, functions);
    }
}

/// Extract a FunctionNode from a tree-sitter function_definition or async_function_definition
fn extract_function(
    node: Node,
    source: &str,
    file_index: usize,
    local_index: usize,
) -> Option<FunctionNode> {
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    // Get function name
    let name = extract_function_name(node, source);

    // Get function body (block node)
    let body_node = find_child_by_kind(node, "block")?;

    // Create SourceSpan from tree-sitter node
    let span = SourceSpan::new(
        node.start_byte(),
        node.end_byte(),
        node.start_position().row as u32 + 1, // tree-sitter uses 0-indexed rows
        node.end_position().row as u32 + 1,   // tree-sitter uses 0-indexed rows
        node.start_position().column as u32,
    );

    // Create FunctionBody::Python variant
    let body = FunctionBody::Python {
        body_node: body_node.id(),
        source: source.to_string(),
    };

    Some(FunctionNode {
        id: FunctionId {
            file_index,
            local_index,
        },
        name,
        span,
        body,
        suppression_reason: None, // Will be extracted separately
    })
}

/// Extract function name from a function_definition or async_function_definition node
fn extract_function_name(node: Node, source: &str) -> Option<String> {
    // Python function definitions have an "identifier" child for the function name
    if let Some(name_node) = find_child_by_kind(node, "identifier") {
        let name = &source[name_node.start_byte()..name_node.end_byte()];
        return Some(name.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_parser() {
        let parser = PythonParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def simple_function(x):
    return x + 1
"#;
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("simple_function".to_string()));
    }

    #[test]
    fn test_parse_async_function() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
async def async_function():
    return await something()
"#;
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("async_function".to_string()));
    }

    #[test]
    fn test_parse_class_methods() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
class MyClass:
    def method_one(self, x):
        return x + 1

    def method_two(self):
        return 42
"#;
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("method_one".to_string()));
        assert_eq!(functions[1].name, Some("method_two".to_string()));
    }

    #[test]
    fn test_parse_nested_functions() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def outer_function(x):
    def inner_function(y):
        return y * 2
    return inner_function(x)
"#;
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("outer_function".to_string()));
        assert_eq!(functions[1].name, Some("inner_function".to_string()));
    }

    #[test]
    fn test_parse_multiple_functions() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def first():
    return 1

def second():
    return 2

def third():
    return 3
"#;
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 3);
        // Verify deterministic ordering (sorted by source position)
        assert_eq!(functions[0].name, Some("first".to_string()));
        assert_eq!(functions[1].name, Some("second".to_string()));
        assert_eq!(functions[2].name, Some("third".to_string()));
    }

    #[test]
    fn test_parse_empty_file() {
        let parser = PythonParser::new().unwrap();
        let source = "";
        let module = parser.parse(source, "test.py");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        let parser = PythonParser::new().unwrap();
        // Python with syntax error (incomplete function)
        let source = "def broken(x)";
        // tree-sitter is error-tolerant, so parsing should still succeed
        let result = parser.parse(source, "test.py");
        // Parser should still work even with errors in the source
        assert!(result.is_ok());
    }
}
