//! C# language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use crate::language::tree_sitter_utils::find_child_by_kind;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

pub struct CSharpParser;

impl CSharpParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_c_sharp::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C# language for parser")?;
        Ok(CSharpParser)
    }
}

impl Default for CSharpParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C# parser")
    }
}

impl LanguageParser for CSharpParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let mut parser = Parser::new();
        let language = tree_sitter_c_sharp::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C# language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse C# file: {}", filename))?;

        Ok(Box::new(CSharpModule {
            tree,
            source: source.to_string(),
        }))
    }
}

struct CSharpModule {
    tree: Tree,
    source: String,
}

impl ParsedModule for CSharpModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        let root = self.tree.root_node();
        let mut functions = Vec::new();
        discover_functions_recursive(root, &self.source, file_index, &mut functions);
        functions.sort_by_key(|f| f.span.start);
        functions
    }
}

fn discover_functions_recursive(
    node: Node,
    source: &str,
    file_index: usize,
    functions: &mut Vec<FunctionNode>,
) {
    match node.kind() {
        "method_declaration"
        | "constructor_declaration"
        | "local_function_statement"
        | "operator_declaration"
        | "conversion_operator_declaration" => {
            if let Some(function_node) = extract_function(node, source, file_index, functions.len())
            {
                functions.push(function_node);
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_functions_recursive(child, source, file_index, functions);
    }
}

fn extract_function(
    node: Node,
    source: &str,
    file_index: usize,
    local_index: usize,
) -> Option<FunctionNode> {
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    let name = extract_function_name(node, source);

    let body_node = find_child_by_kind(node, "block")?;

    let span = SourceSpan::new(
        node.start_byte(),
        node.end_byte(),
        node.start_position().row as u32 + 1,
        node.end_position().row as u32 + 1,
        node.start_position().column as u32,
    );

    let body = FunctionBody::CSharp {
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
        suppression_reason: None,
    })
}

fn extract_function_name(node: Node, source: &str) -> Option<String> {
    // method_declaration and local_function_statement use "identifier"
    // constructor_declaration uses "identifier"
    // operator_declaration uses "operator" keyword child — fall back to raw text slice
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
        assert!(CSharpParser::new().is_ok());
    }

    #[test]
    fn test_parse_simple_method() {
        let parser = CSharpParser::new().unwrap();
        let source = r#"
public class Simple {
    public int Add(int x, int y) {
        return x + y;
    }
}
"#;
        let module = parser.parse(source, "test.cs").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("Add".to_string()));
    }

    #[test]
    fn test_parse_constructor() {
        let parser = CSharpParser::new().unwrap();
        let source = r#"
public class MyClass {
    private int value;
    public MyClass(int value) {
        this.value = value;
    }
}
"#;
        let module = parser.parse(source, "test.cs").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("MyClass".to_string()));
    }

    #[test]
    fn test_parse_multiple_methods() {
        let parser = CSharpParser::new().unwrap();
        let source = r#"
public class Calc {
    public int Add(int a, int b) { return a + b; }
    public int Sub(int a, int b) { return a - b; }
    public int Mul(int a, int b) { return a * b; }
}
"#;
        let module = parser.parse(source, "test.cs").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, Some("Add".to_string()));
        assert_eq!(functions[1].name, Some("Sub".to_string()));
        assert_eq!(functions[2].name, Some("Mul".to_string()));
    }

    #[test]
    fn test_parse_empty_file() {
        let parser = CSharpParser::new().unwrap();
        let module = parser.parse("", "test.cs").unwrap();
        assert_eq!(module.discover_functions(0, "").len(), 0);
    }

    #[test]
    fn test_parse_static_method() {
        let parser = CSharpParser::new().unwrap();
        let source = r#"
public class Utils {
    public static string Format(int n) {
        return n.ToString();
    }
}
"#;
        let module = parser.parse(source, "test.cs").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("Format".to_string()));
    }

    #[test]
    fn test_parse_nested_class_methods() {
        let parser = CSharpParser::new().unwrap();
        let source = r#"
public class Outer {
    public void OuterMethod() { }
    private class Inner {
        public void InnerMethod() { }
    }
}
"#;
        let module = parser.parse(source, "test.cs").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 2);
    }
}
