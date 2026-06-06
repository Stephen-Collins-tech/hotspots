//! C language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use crate::language::tree_sitter_utils::find_child_by_kind;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// C parser using tree-sitter
pub struct CParser;

impl CParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_c::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C language for parser")?;
        Ok(CParser)
    }
}

impl Default for CParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C parser")
    }
}

impl LanguageParser for CParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let mut parser = Parser::new();
        let language = tree_sitter_c::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse C file: {}", filename))?;

        Ok(Box::new(CModule {
            tree,
            source: source.to_string(),
        }))
    }
}

struct CModule {
    tree: Tree,
    source: String,
}

impl ParsedModule for CModule {
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
    if node.kind() == "function_definition" {
        if let Some(function_node) = extract_function(node, source, file_index, functions.len()) {
            functions.push(function_node);
        }
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

    // C function_definition has a declarator child containing the function name
    let name = extract_function_name(node, source);

    // Body is a compound_statement
    let body_node = find_child_by_kind(node, "compound_statement")?;

    let span = SourceSpan::new(
        node.start_byte(),
        node.end_byte(),
        node.start_position().row as u32 + 1,
        node.end_position().row as u32 + 1,
        node.start_position().column as u32,
    );

    let body = FunctionBody::C {
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

/// Extract function name from a C function_definition node.
///
/// C grammar: function_definition → type declarator compound_statement
/// The declarator may be a function_declarator or a pointer_declarator wrapping one.
/// We find the innermost function_declarator and grab its identifier child.
fn extract_function_name(node: Node, source: &str) -> Option<String> {
    // Walk into declarator → function_declarator → identifier
    fn find_identifier<'a>(n: Node<'a>, source: &str) -> Option<String> {
        if n.kind() == "identifier" {
            return Some(source[n.start_byte()..n.end_byte()].to_string());
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if let Some(name) = find_identifier(child, source) {
                return Some(name);
            }
        }
        None
    }

    // The declarator child is the second child (after the type specifier)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind == "function_declarator" || kind == "pointer_declarator" || kind == "declarator" {
            if let Some(name) = find_identifier(child, source) {
                return Some(name);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_parser() {
        assert!(CParser::new().is_ok());
    }

    #[test]
    fn test_parse_simple_function() {
        let parser = CParser::new().unwrap();
        let source = r#"
int add(int a, int b) {
    return a + b;
}
"#;
        let module = parser.parse(source, "test.c").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("add".to_string()));
    }

    #[test]
    fn test_parse_multiple_functions() {
        let parser = CParser::new().unwrap();
        let source = r#"
void init(void) { }
int compute(int x) { return x * 2; }
static void helper(void) { }
"#;
        let module = parser.parse(source, "test.c").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, Some("init".to_string()));
        assert_eq!(functions[1].name, Some("compute".to_string()));
        assert_eq!(functions[2].name, Some("helper".to_string()));
    }

    #[test]
    fn test_parse_pointer_return_type() {
        let parser = CParser::new().unwrap();
        let source = r#"
char *get_name(int id) {
    return names[id];
}
"#;
        let module = parser.parse(source, "test.c").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("get_name".to_string()));
    }

    #[test]
    fn test_parse_empty_file() {
        let parser = CParser::new().unwrap();
        let module = parser.parse("", "test.c").unwrap();
        let functions = module.discover_functions(0, "");
        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_parse_header_declarations_ignored() {
        let parser = CParser::new().unwrap();
        // Declarations (no body) should not be discovered
        let source = r#"
int add(int a, int b);
void cleanup(void);
int real_function(void) { return 0; }
"#;
        let module = parser.parse(source, "test.h").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("real_function".to_string()));
    }
}
