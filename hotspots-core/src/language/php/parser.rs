//! PHP language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use anyhow::{Context, Result};
use std::sync::Arc;
use tree_sitter::{Node, Parser, Tree};

/// Node kinds that define an analyzable function.
///
/// Abstract and interface methods have no body and are skipped.
pub(crate) const FUNCTION_KINDS: &[&str] = &[
    "function_definition",
    "method_declaration",
    "anonymous_function",
    "arrow_function",
];

/// PHP parser using tree-sitter
pub struct PhpParser;

impl PhpParser {
    /// Create a new PHP parser
    pub fn new() -> Result<Self> {
        // Just validate that we can create a parser
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .context("Failed to set PHP language for parser")?;
        Ok(PhpParser)
    }
}

impl Default for PhpParser {
    fn default() -> Self {
        Self::new().expect("Failed to create PHP parser")
    }
}

impl LanguageParser for PhpParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .context("Failed to set PHP language")?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse PHP file: {}", filename))?;

        Ok(Box::new(PhpModule {
            tree,
            source: Arc::from(source),
        }))
    }
}

/// Parsed PHP module
struct PhpModule {
    tree: Tree,
    source: Arc<str>,
}

impl ParsedModule for PhpModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        let mut functions = Vec::new();

        // Iterative walk (explicit stack) so deeply nested expressions can't
        // overflow the thread stack.
        let mut stack = vec![self.tree.root_node()];
        while let Some(node) = stack.pop() {
            if FUNCTION_KINDS.contains(&node.kind()) {
                if let Some(function_node) =
                    extract_function(node, &self.source, file_index, functions.len())
                {
                    functions.push(function_node);
                }
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }

        // Sort by source position for determinism (the stack walk visits in
        // reverse order)
        functions.sort_by_key(|f| f.span.start);
        for (i, f) in functions.iter_mut().enumerate() {
            f.id.local_index = i;
        }

        functions
    }
}

/// Extract a FunctionNode from a PHP function-like node. Returns `None` when
/// there is no body (abstract/interface methods).
fn extract_function(
    node: Node,
    source: &Arc<str>,
    file_index: usize,
    local_index: usize,
) -> Option<FunctionNode> {
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    let body_node = node.child_by_field_name("body")?;

    // Anonymous functions and arrow functions have no `name`
    let name = node
        .child_by_field_name("name")
        .map(|n| source[n.start_byte()..n.end_byte()].to_string());

    let span = SourceSpan::new(
        node.start_byte(),
        node.end_byte(),
        node.start_position().row as u32 + 1, // tree-sitter uses 0-indexed rows
        node.end_position().row as u32 + 1,
        node.start_position().column as u32,
    );

    Some(FunctionNode {
        id: FunctionId {
            file_index,
            local_index,
        },
        name,
        span,
        body: FunctionBody::Php {
            body_node: body_node.id(),
            source: Arc::clone(source),
        },
        suppression_reason: None, // Will be extracted separately
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discover(source: &str) -> Vec<FunctionNode> {
        let parser = PhpParser::new().unwrap();
        let module = parser.parse(source, "test.php").unwrap();
        module.discover_functions(0, source)
    }

    #[test]
    fn test_php_parser_simple_function() {
        let functions = discover("<?php\nfunction add($a, $b) { return $a + $b; }\n");
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("add".to_string()));
    }

    #[test]
    fn test_php_parser_methods_and_abstract() {
        let source = r#"<?php
abstract class Shape {
    abstract public function area(): float;
    public function describe() { return "shape"; }
    public static function make() { return new static(); }
}
interface Named { public function name(); }
trait Greets { public function hi() { echo "hi"; } }
enum Suit { case Hearts; public function label() { return "h"; } }
"#;
        let functions = discover(source);
        let names: Vec<_> = functions.iter().map(|f| f.name.clone().unwrap()).collect();
        assert_eq!(names, vec!["describe", "make", "hi", "label"]);
    }

    #[test]
    fn test_php_parser_closures() {
        let source = r#"<?php
function outer() {
    $a = function ($x) { return $x; };
    $b = fn($y) => $y * 2;
}
"#;
        let functions = discover(source);
        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, Some("outer".to_string()));
        assert_eq!(functions[1].name, None);
        assert_eq!(functions[2].name, None);
    }

    #[test]
    fn test_php_parser_anonymous_class_method() {
        let source = "<?php\n$o = new class { public function q() { return 1; } };\n";
        let functions = discover(source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("q".to_string()));
    }

    #[test]
    fn test_php_parser_namespaced_and_mixed_html() {
        let source = "<html>\n<?php\nnamespace App;\nfunction f() { return 1; }\n?>\n<div></div>\n<?php function g() {} ?>\n";
        let functions = discover(source);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("f".to_string()));
        assert_eq!(functions[1].name, Some("g".to_string()));
    }

    #[test]
    fn test_php_parser_empty_file() {
        assert_eq!(discover("<?php\n").len(), 0);
    }

    #[test]
    fn test_php_parser_deterministic_ordering() {
        let source = "<?php\nfunction zzz() {}\nfunction aaa() {}\nfunction mmm() {}\n";
        let f1 = discover(source);
        let f2 = discover(source);
        assert_eq!(f1.len(), 3);
        assert_eq!(f1[0].name, Some("zzz".to_string()));
        assert_eq!(f1[1].name, Some("aaa".to_string()));
        assert_eq!(f1[2].name, Some("mmm".to_string()));
        for (i, (a, b)) in f1.iter().zip(f2.iter()).enumerate() {
            assert_eq!(a.span.start, b.span.start);
            assert_eq!(a.id.local_index, i);
        }
    }

    #[test]
    fn test_php_parser_parse_error() {
        let parser = PhpParser::new().unwrap();
        let result = parser.parse("<?php function foo( { }}}}", "test.php");
        assert!(result.is_ok(), "tree-sitter should handle syntax errors");
    }
}
