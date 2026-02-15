//! Go language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// Go parser using tree-sitter
pub struct GoParser;

impl GoParser {
    /// Create a new Go parser
    pub fn new() -> Result<Self> {
        // Just validate that we can create a parser
        let mut parser = Parser::new();
        let language = tree_sitter_go::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Go language for parser")?;
        Ok(GoParser)
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Go parser")
    }
}

impl LanguageParser for GoParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        // Need to make parser mutable, so we can't use &self directly
        // This is a limitation of tree-sitter's API
        let mut parser = Parser::new();
        let language = tree_sitter_go::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Go language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse Go file: {}", filename))?;

        Ok(Box::new(GoModule {
            tree,
            source: source.to_string(),
        }))
    }
}

/// Parsed Go module
struct GoModule {
    tree: Tree,
    source: String,
}

impl ParsedModule for GoModule {
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

/// Recursively discover function declarations in the Go AST
fn discover_functions_recursive(
    node: Node,
    source: &str,
    file_index: usize,
    functions: &mut Vec<FunctionNode>,
) {
    // Check if this node is a function declaration
    if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
        if let Some(function_node) = extract_function(node, source, file_index, functions.len()) {
            functions.push(function_node);
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_functions_recursive(child, source, file_index, functions);
    }
}

/// Extract a FunctionNode from a tree-sitter function_declaration or method_declaration
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

    // Create FunctionBody::Go variant (placeholder for now)
    let body = FunctionBody::Go {
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

/// Extract function name from a function_declaration or method_declaration node
fn extract_function_name(node: Node, source: &str) -> Option<String> {
    // For function_declaration: look for "identifier" child
    // For method_declaration: look for "field_identifier" child
    if let Some(name_node) = find_child_by_kind(node, "identifier")
        .or_else(|| find_child_by_kind(node, "field_identifier"))
    {
        let name = &source[name_node.start_byte()..name_node.end_byte()];
        return Some(name.to_string());
    }
    None
}

/// Find a child node by kind
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_parser_simple_function() {
        let parser = GoParser::new().unwrap();
        let source = r#"
package main

func add(a int, b int) int {
    return a + b
}
"#;
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("add".to_string()));
    }

    #[test]
    fn test_go_parser_multiple_functions() {
        let parser = GoParser::new().unwrap();
        let source = r#"
package main

func foo() {
    println("foo")
}

func bar() {
    println("bar")
}
"#;
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("foo".to_string()));
        assert_eq!(functions[1].name, Some("bar".to_string()));
    }

    #[test]
    fn test_go_parser_method() {
        let parser = GoParser::new().unwrap();
        let source = r#"
package main

type MyStruct struct{}

func (m MyStruct) Method() {
    println("method")
}
"#;
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("Method".to_string()));
    }

    #[test]
    fn test_go_parser_empty_file() {
        let parser = GoParser::new().unwrap();
        let source = "package main\n";
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_go_parser_deterministic_ordering() {
        let parser = GoParser::new().unwrap();
        let source = r#"
package main

func zzz() {}
func aaa() {}
func mmm() {}
"#;

        // Parse twice
        let module1 = parser.parse(source, "test.go").unwrap();
        let functions1 = module1.discover_functions(0, source);

        let module2 = parser.parse(source, "test.go").unwrap();
        let functions2 = module2.discover_functions(0, source);

        // Should be in source order, not alphabetical
        assert_eq!(functions1.len(), 3);
        assert_eq!(functions2.len(), 3);
        assert_eq!(functions1[0].name, Some("zzz".to_string()));
        assert_eq!(functions1[1].name, Some("aaa".to_string()));
        assert_eq!(functions1[2].name, Some("mmm".to_string()));

        // Should be deterministic
        for (f1, f2) in functions1.iter().zip(functions2.iter()) {
            assert_eq!(f1.name, f2.name);
            assert_eq!(f1.span.start, f2.span.start);
        }
    }

    #[test]
    fn test_go_parser_parse_error() {
        let parser = GoParser::new().unwrap();
        let source = "func foo() { invalid syntax }}}}";

        // tree-sitter is error-tolerant and will still produce a tree
        // We can still parse, but the tree will have error nodes
        let result = parser.parse(source, "test.go");
        assert!(result.is_ok(), "tree-sitter should handle syntax errors");
    }
}
