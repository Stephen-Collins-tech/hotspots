//! Java language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// Java parser using tree-sitter
pub struct JavaParser;

impl JavaParser {
    /// Create a new Java parser
    pub fn new() -> Result<Self> {
        // Just validate that we can create a parser
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Java language for parser")?;
        Ok(JavaParser)
    }
}

impl Default for JavaParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Java parser")
    }
}

impl LanguageParser for JavaParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        // Need to make parser mutable, so we can't use &self directly
        // This is a limitation of tree-sitter's API
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set Java language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse Java file: {}", filename))?;

        Ok(Box::new(JavaModule {
            tree,
            source: source.to_string(),
        }))
    }
}

/// Parsed Java module
struct JavaModule {
    tree: Tree,
    source: String,
}

impl ParsedModule for JavaModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        let root = self.tree.root_node();
        let mut functions = Vec::new();

        // Walk the tree to find method and constructor declarations
        discover_functions_recursive(root, &self.source, file_index, &mut functions);

        // Sort by source position for determinism
        functions.sort_by_key(|f| f.span.start);

        functions
    }
}

/// Recursively discover function declarations in the Java AST
fn discover_functions_recursive(
    node: Node,
    source: &str,
    file_index: usize,
    functions: &mut Vec<FunctionNode>,
) {
    // Check if this node is a method or constructor declaration
    // Java has:
    // - "method_declaration" for regular and static methods
    // - "constructor_declaration" for constructors
    if node.kind() == "method_declaration" || node.kind() == "constructor_declaration" {
        if let Some(function_node) = extract_function(node, source, file_index, functions.len()) {
            functions.push(function_node);
        }
    }

    // Recurse into children (this will find methods in classes, inner classes, interfaces, etc.)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_functions_recursive(child, source, file_index, functions);
    }
}

/// Extract a FunctionNode from a tree-sitter method_declaration or constructor_declaration
fn extract_function(
    node: Node,
    source: &str,
    file_index: usize,
    local_index: usize,
) -> Option<FunctionNode> {
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    // Get function/constructor name
    let name = extract_function_name(node, source);

    // Get function body (block node or constructor_body)
    // Constructors use "constructor_body", methods use "block"
    let body_node = find_child_by_kind(node, "block")
        .or_else(|| find_child_by_kind(node, "constructor_body"))?;

    // Create SourceSpan from tree-sitter node
    let span = SourceSpan::new(
        node.start_byte(),
        node.end_byte(),
        node.start_position().row as u32 + 1, // tree-sitter uses 0-indexed rows
        node.end_position().row as u32 + 1,   // tree-sitter uses 0-indexed rows
        node.start_position().column as u32,
    );

    // Create FunctionBody::Java variant
    let body = FunctionBody::Java {
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

/// Extract function name from a method_declaration or constructor_declaration node
fn extract_function_name(node: Node, source: &str) -> Option<String> {
    // Java method declarations have an "identifier" child for the method name
    // Constructor declarations also have an "identifier" child
    if let Some(name_node) = find_child_by_kind(node, "identifier") {
        let name = &source[name_node.start_byte()..name_node.end_byte()];
        return Some(name.to_string());
    }
    None
}

/// Find a child node by kind
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor).find(|child| child.kind() == kind);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_parser() {
        let parser = JavaParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_simple_method() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Simple {
    public int simpleMethod(int x) {
        return x + 1;
    }
}
"#;
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("simpleMethod".to_string()));
    }

    #[test]
    fn test_parse_constructor() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class MyClass {
    private int value;

    public MyClass(int value) {
        this.value = value;
    }
}
"#;
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("MyClass".to_string()));
    }

    #[test]
    fn test_parse_static_method() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Utils {
    public static int staticMethod(int x) {
        return x * 2;
    }
}
"#;
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("staticMethod".to_string()));
    }

    #[test]
    fn test_parse_inner_class_methods() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Outer {
    public void outerMethod() {
        return;
    }

    class Inner {
        public void innerMethod() {
            return;
        }
    }
}
"#;
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("outerMethod".to_string()));
        assert_eq!(functions[1].name, Some("innerMethod".to_string()));
    }

    #[test]
    fn test_parse_interface_methods() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public interface MyInterface {
    default void defaultMethod() {
        System.out.println("default");
    }

    static void staticMethod() {
        System.out.println("static");
    }
}
"#;
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("defaultMethod".to_string()));
        assert_eq!(functions[1].name, Some("staticMethod".to_string()));
    }

    #[test]
    fn test_parse_multiple_methods() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Multiple {
    public void first() {
        return;
    }

    public void second() {
        return;
    }

    public void third() {
        return;
    }
}
"#;
        let module = parser.parse(source, "test.java");
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
        let parser = JavaParser::new().unwrap();
        let source = "";
        let module = parser.parse(source, "test.java");
        assert!(module.is_ok());

        let functions = module.unwrap().discover_functions(0, source);
        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        let parser = JavaParser::new().unwrap();
        // Java with syntax error (incomplete method)
        let source = "public class Broken { public void broken(int x) }";
        // tree-sitter is error-tolerant, so parsing should still succeed
        let result = parser.parse(source, "test.java");
        // Parser should still work even with errors in the source
        assert!(result.is_ok());
    }
}
