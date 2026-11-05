//! C++ language parser using tree-sitter

use crate::ast::FunctionNode;
use crate::language::parser::{LanguageParser, ParsedModule};
use crate::language::tree_sitter_utils::find_child_by_kind;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tree_sitter::{Node, ParseOptions, Parser, Tree};

/// Bound on how long a single C++ parse may run before we give up.
///
/// tree-sitter's GLR error-recovery can blow up on unusual input (deeply nested
/// templates, unexpanded macros) instead of failing fast. The timeout turns that
/// hang into a skipped file.
const PARSE_TIMEOUT: Duration = Duration::from_secs(5);

/// C++ parser using tree-sitter
pub struct CppParser;

impl CppParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_cpp::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C++ language for parser")?;
        Ok(CppParser)
    }
}

impl Default for CppParser {
    fn default() -> Self {
        Self::new().expect("Failed to create C parser")
    }
}

impl LanguageParser for CppParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let mut parser = Parser::new();
        let language = tree_sitter_cpp::LANGUAGE;
        parser
            .set_language(&language.into())
            .context("Failed to set C++ language")?;

        let start = Instant::now();
        let mut cancel = |_state: &tree_sitter::ParseState| start.elapsed() > PARSE_TIMEOUT;
        let options = ParseOptions::new().progress_callback(&mut cancel);
        let bytes = source.as_bytes();
        let tree = parser
            .parse_with_options(
                &mut |byte_offset, _point| bytes.get(byte_offset..).unwrap_or(&[]),
                None,
                Some(options),
            )
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to parse C++ file: {} (timed out or produced no tree)",
                    filename
                )
            })?;

        Ok(Box::new(CppModule {
            tree,
            source: Arc::from(source),
        }))
    }
}

struct CppModule {
    tree: Tree,
    source: Arc<str>,
}

impl ParsedModule for CppModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        let root = self.tree.root_node();
        let mut found = Vec::new();
        collect_functions(root, &self.source, &mut found);
        found.sort_by_key(|f| f.node.start_byte());
        disambiguate_overloads(&mut found);
        found
            .into_iter()
            .enumerate()
            .map(|(local_index, f)| build_function(f, &self.source, file_index, local_index))
            .collect()
    }
}

/// A discovered `function_definition` with its qualified name and signature.
struct Found<'t> {
    node: Node<'t>,
    body: usize,
    qualified: Option<String>,
    signature: String,
    name: Option<String>,
}

fn collect_functions<'t>(node: Node<'t>, source: &str, out: &mut Vec<Found<'t>>) {
    // Iterative, so deeply nested input cannot overflow the thread stack.
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "function_definition" {
            if let Some(body) = function_body(n) {
                let (qualified, signature) = qualified_name_and_signature(n, source);
                out.push(Found {
                    node: n,
                    body: body.id(),
                    name: qualified.clone(),
                    qualified,
                    signature,
                });
            }
        }
        let mut cursor = n.walk();
        let children: Vec<Node<'t>> = n.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
}

/// The braced body of a function. `= default` / `= delete` definitions have none.
fn function_body(node: Node) -> Option<Node> {
    node.child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement")
        .or_else(|| find_child_by_kind(node, "compound_statement"))
}

/// Overloads share a qualified name. Keep names plain unless two functions in the
/// same file collide; then append the parameter-type signature, and the start line
/// if even that is not unique.
fn disambiguate_overloads(found: &mut [Found]) {
    use std::collections::HashMap;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in found.iter() {
        if let Some(q) = &f.qualified {
            *counts.entry(q.clone()).or_default() += 1;
        }
    }
    let mut with_sig: HashMap<String, usize> = HashMap::new();
    for f in found.iter() {
        if let Some(q) = &f.qualified {
            if counts[q] > 1 {
                *with_sig.entry(format!("{q}{}", f.signature)).or_default() += 1;
            }
        }
    }
    for f in found.iter_mut() {
        let Some(q) = &f.qualified else { continue };
        if counts[q] > 1 {
            let named = format!("{q}{}", f.signature);
            f.name = Some(if with_sig[&named] > 1 {
                format!("{named}@{}", f.node.start_position().row + 1)
            } else {
                named
            });
        }
    }
}

fn build_function(
    f: Found,
    source: &Arc<str>,
    file_index: usize,
    local_index: usize,
) -> FunctionNode {
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    let node = f.node;
    FunctionNode {
        id: FunctionId {
            file_index,
            local_index,
        },
        name: f.name,
        span: SourceSpan::new(
            node.start_byte(),
            node.end_byte(),
            node.start_position().row as u32 + 1,
            node.end_position().row as u32 + 1,
            node.start_position().column as u32,
        ),
        body: FunctionBody::Cpp {
            body_node: f.body,
            source: Arc::clone(source),
        },
        suppression_reason: None,
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Namespace / class names enclosing `node`, outermost first (anonymous ones skipped).
fn enclosing_scope(node: Node, source: &str) -> Vec<String> {
    let mut scopes = Vec::new();
    let mut cur = node.parent();
    while let Some(p) = cur {
        if matches!(
            p.kind(),
            "namespace_definition" | "class_specifier" | "struct_specifier" | "union_specifier"
        ) {
            if let Some(name) = p.child_by_field_name("name") {
                scopes.push(collapse_ws(text(name, source)));
            }
        }
        cur = p.parent();
    }
    scopes.reverse();
    scopes
}

/// The qualified name (`ns::Class::method`, `Class::~Class`, `operator==`) and the
/// signature suffix (`(int,std::string) const`) of a function definition.
fn qualified_name_and_signature(node: Node, source: &str) -> (Option<String>, String) {
    // Descend through pointer/reference/parenthesized declarators to the function_declarator.
    let mut decl = node.child_by_field_name("declarator");
    while let Some(d) = decl {
        if d.kind() == "function_declarator" {
            break;
        }
        decl = d
            .child_by_field_name("declarator")
            .or_else(|| find_child_by_kind(d, "function_declarator"));
    }
    let Some(func_decl) = decl else {
        return (None, String::new());
    };
    let Some(name_node) = func_decl.child_by_field_name("declarator") else {
        return (None, String::new());
    };

    let mut parts = enclosing_scope(node, source);
    parts.push(collapse_ws(text(name_node, source)));
    let qualified = parts.join("::");

    let mut types = Vec::new();
    if let Some(params) = func_decl.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for p in params.children(&mut cursor) {
            match p.kind() {
                "parameter_declaration" | "optional_parameter_declaration" => {
                    let ty = p
                        .child_by_field_name("type")
                        .map(|t| collapse_ws(text(t, source)))
                        .unwrap_or_default();
                    let decl_text = p
                        .child_by_field_name("declarator")
                        .map(|d| text(d, source))
                        .unwrap_or("");
                    let marks: String = decl_text
                        .chars()
                        .filter(|c| matches!(c, '*' | '&'))
                        .collect();
                    types.push(format!("{ty}{marks}"));
                }
                "variadic_parameter_declaration" | "variadic_parameter" => {
                    types.push("...".to_string())
                }
                _ => {}
            }
        }
    }
    let mut signature = format!("({})", types.join(","));
    let mut cursor = func_decl.walk();
    for c in func_decl.children(&mut cursor) {
        if c.kind() == "type_qualifier" {
            signature.push(' ');
            signature.push_str(text(c, source));
        }
    }
    (Some(qualified), signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<Option<String>> {
        let module = CppParser::new().unwrap().parse(source, "t.cpp").unwrap();
        module
            .discover_functions(0, source)
            .into_iter()
            .map(|f| f.name)
            .collect()
    }

    fn n(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn test_create_parser() {
        assert!(CppParser::new().is_ok());
    }

    #[test]
    fn free_functions_keep_plain_names() {
        let got = names("int add(int a, int b) { return a + b; }\nstatic void helper() {}\n");
        assert_eq!(got, vec![n("add"), n("helper")]);
    }

    #[test]
    fn pointer_and_reference_return_types() {
        let got =
            names("char *get(int i) { return 0; }\nconst int &ref() { static int x; return x; }\n");
        assert_eq!(got, vec![n("get"), n("ref")]);
    }

    #[test]
    fn namespaces_and_inline_members_are_qualified() {
        let got = names(
            "namespace a { namespace b { class C { public: int m() { return 1; } void n() {} }; } }\n",
        );
        assert_eq!(got, vec![n("a::b::C::m"), n("a::b::C::n")]);
    }

    #[test]
    fn out_of_line_members_constructors_destructors_operators() {
        let got = names(
            "Foo::Foo() : x(1) {}\nFoo::~Foo() {}\nbool Foo::operator==(const Foo &o) const { return true; }\nint Foo::get() { return 1; }\n",
        );
        assert_eq!(
            got,
            vec![
                n("Foo::Foo"),
                n("Foo::~Foo"),
                n("Foo::operator=="),
                n("Foo::get")
            ]
        );
    }

    #[test]
    fn templates_are_one_function_each() {
        let got = names("template <typename T> T id(T x) { return x; }\n");
        assert_eq!(got, vec![n("id")]);
    }

    #[test]
    fn overloads_get_signature_suffix_only_on_collision() {
        let got = names(
            "void f(int a) {}\nvoid f(const std::string &s) {}\nvoid g() {}\nvoid f(int a, char *p) const {}\n",
        );
        assert_eq!(
            got,
            vec![
                n("f(int)"),
                n("f(std::string&)"),
                n("g"),
                n("f(int,char*) const")
            ]
        );
    }

    #[test]
    fn identical_signatures_fall_back_to_line() {
        // e.g. two definitions under different #if branches
        let got = names("#if A\nvoid f(int a) {}\n#else\nvoid f(int b) {}\n#endif\n");
        assert_eq!(got, vec![n("f(int)@2"), n("f(int)@4")]);
    }

    #[test]
    fn declarations_and_defaulted_members_are_ignored() {
        let got = names(
            "int add(int a, int b);\nstruct S { S() = default; S(const S&) = delete; int real() { return 0; } };\n",
        );
        assert_eq!(got, vec![n("S::real")]);
    }

    #[test]
    fn range_for_try_catch_and_lambda_parse_cleanly() {
        let source = "int f(std::vector<int> v) { for (auto x : v) { if (x) break; } try { throw 1; } catch (int e) { return e; } auto l = [&](int y) { return y; }; return 0; }";
        let module = CppParser::new().unwrap().parse(source, "t.cpp").unwrap();
        let fns = module.discover_functions(0, source);
        assert_eq!(fns.len(), 1, "lambda must not be a separate function");
    }

    #[test]
    fn test_parse_empty_file() {
        let module = CppParser::new().unwrap().parse("", "t.cpp").unwrap();
        assert_eq!(module.discover_functions(0, "").len(), 0);
    }
}
