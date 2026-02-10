//! Rust parser implementation using syn

use crate::ast::{FunctionId, FunctionNode};
use crate::language::function_body::FunctionBody;
use crate::language::parser::{LanguageParser, ParsedModule};
use crate::language::span::SourceSpan;
use anyhow::{Context, Result};
use syn::spanned::Spanned;
use syn::{Block, File, ImplItem, ImplItemFn, Item, ItemFn, Signature};

/// Rust parser using syn
pub struct RustParser;

impl LanguageParser for RustParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let file = syn::parse_file(source)
            .with_context(|| format!("Failed to parse Rust file: {}", filename))?;

        Ok(Box::new(RustModule::new(file, source.to_string())))
    }
}

/// Parsed Rust module
struct RustModule {
    file: File,
    source: String,
}

impl RustModule {
    fn new(file: File, source: String) -> Self {
        Self { file, source }
    }

    /// Extract function nodes from the module
    fn extract_functions(&self, file_index: usize) -> Vec<FunctionNode> {
        let mut functions = Vec::new();
        let mut local_index = 0;

        // Visit top-level functions
        for item in &self.file.items {
            self.visit_item(item, None, file_index, &mut local_index, &mut functions);
        }

        // Sort functions by span start for deterministic ordering
        functions.sort_by_key(|f| f.span.start);

        functions
    }

    /// Visit an item and extract functions
    fn visit_item(
        &self,
        item: &Item,
        type_name: Option<&str>,
        file_index: usize,
        local_index: &mut usize,
        functions: &mut Vec<FunctionNode>,
    ) {
        match item {
            Item::Fn(item_fn) => {
                self.extract_item_fn(item_fn, type_name, file_index, local_index, functions);
            }
            Item::Impl(item_impl) => {
                // Get type name for method prefix
                let type_name = if let syn::Type::Path(type_path) = &*item_impl.self_ty {
                    type_path
                        .path
                        .segments
                        .last()
                        .map(|seg| seg.ident.to_string())
                } else {
                    None
                };

                // Visit methods in impl block
                for impl_item in &item_impl.items {
                    if let ImplItem::Fn(method) = impl_item {
                        self.extract_impl_fn(
                            method,
                            type_name.as_deref(),
                            file_index,
                            local_index,
                            functions,
                        );
                    }
                }
            }
            _ => {
                // Ignore other items (structs, traits, etc.)
            }
        }
    }

    /// Extract a function node from ItemFn
    fn extract_item_fn(
        &self,
        item_fn: &ItemFn,
        name_prefix: Option<&str>,
        file_index: usize,
        local_index: &mut usize,
        functions: &mut Vec<FunctionNode>,
    ) {
        self.extract_function_common(
            &item_fn.sig,
            &*item_fn.block,
            item_fn,
            name_prefix,
            file_index,
            local_index,
            functions,
        );
    }

    /// Extract a function node from ImplItemFn (method)
    fn extract_impl_fn(
        &self,
        impl_fn: &ImplItemFn,
        name_prefix: Option<&str>,
        file_index: usize,
        local_index: &mut usize,
        functions: &mut Vec<FunctionNode>,
    ) {
        self.extract_function_common(
            &impl_fn.sig,
            &impl_fn.block,
            impl_fn,
            name_prefix,
            file_index,
            local_index,
            functions,
        );
    }

    /// Common extraction logic for both functions and methods
    fn extract_function_common<S: Spanned>(
        &self,
        sig: &Signature,
        _block: &Block,
        item: &S,
        name_prefix: Option<&str>,
        file_index: usize,
        local_index: &mut usize,
        functions: &mut Vec<FunctionNode>,
    ) {
        let name = if let Some(prefix) = name_prefix {
            format!("{}::{}", prefix, sig.ident)
        } else {
            sig.ident.to_string()
        };

        // Get span start/end from the item
        let full_span = item.span();
        let span_start = full_span.start();
        let span_end = full_span.end();

        // Convert proc_macro2::LineColumn to byte offsets
        let start_byte = self.line_column_to_byte(span_start.line, span_start.column);
        let end_byte = self.line_column_to_byte(span_end.line, span_end.column);

        // Extract source for the function body
        let body_source = if start_byte < self.source.len() && end_byte <= self.source.len() {
            self.source[start_byte..end_byte].to_string()
        } else {
            // Fallback - use the whole function as a string
            format!("fn {}() {{}}", sig.ident)
        };

        let span = SourceSpan::new(
            start_byte,
            end_byte,
            span_start.line as u32,
            span_start.column as u32,
        );

        functions.push(FunctionNode {
            id: FunctionId {
                file_index,
                local_index: *local_index,
            },
            name: Some(name),
            span,
            body: FunctionBody::Rust {
                source: body_source,
            },
            suppression_reason: None,
        });

        *local_index += 1;
    }

    /// Convert line/column to byte offset
    /// syn's spans use 0-indexed columns and 1-indexed lines
    fn line_column_to_byte(&self, line: usize, column: usize) -> usize {
        let mut byte_offset = 0;
        let mut current_line = 1;

        for (i, ch) in self.source.char_indices() {
            if current_line == line {
                // We're on the target line - now count columns
                let line_start = byte_offset;
                let line_text = self.source[line_start..].lines().next().unwrap_or("");

                // Column is 0-indexed in syn
                let mut col_count = 0;
                for (char_idx, _) in line_text.char_indices() {
                    if col_count == column {
                        return line_start + char_idx;
                    }
                    col_count += 1;
                }

                // Column is past end of line, return line end
                return line_start + line_text.len();
            }

            if ch == '\n' {
                current_line += 1;
                byte_offset = i + 1;
            }
        }

        // Line not found, return end of source
        self.source.len()
    }
}

impl ParsedModule for RustModule {
    fn discover_functions(&self, file_index: usize, _source: &str) -> Vec<FunctionNode> {
        self.extract_functions(file_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_parser_simple_function() {
        let source = r#"
fn simple() {
    let x = 1;
}
"#;

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("simple".to_string()));
        assert_eq!(functions[0].id.file_index, 0);
        assert_eq!(functions[0].id.local_index, 0);
    }

    #[test]
    fn test_rust_parser_multiple_functions() {
        let source = r#"
fn first() {
    println!("first");
}

fn second() {
    println!("second");
}
"#;

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("first".to_string()));
        assert_eq!(functions[1].name, Some("second".to_string()));
    }

    #[test]
    fn test_rust_parser_method() {
        let source = r#"
struct Calculator {
    value: i32,
}

impl Calculator {
    fn new() -> Self {
        Calculator { value: 0 }
    }

    fn add(&mut self, x: i32) {
        self.value += x;
    }
}
"#;

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, Some("Calculator::new".to_string()));
        assert_eq!(functions[1].name, Some("Calculator::add".to_string()));
    }

    #[test]
    fn test_rust_parser_async_function() {
        let source = r#"
async fn fetch_data() -> String {
    "data".to_string()
}
"#;

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("fetch_data".to_string()));
    }

    #[test]
    fn test_rust_parser_parse_error() {
        let source = r#"
fn invalid( {
    // Missing closing paren
}
"#;

        let parser = RustParser;
        let result = parser.parse(source, "test.rs");

        assert!(result.is_err());
    }

    #[test]
    fn test_rust_parser_deterministic_ordering() {
        let source = r#"
fn third() {}
fn first() {}
fn second() {}
"#;

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        // Should be sorted by source order (span start)
        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, Some("third".to_string()));
        assert_eq!(functions[1].name, Some("first".to_string()));
        assert_eq!(functions[2].name, Some("second".to_string()));
    }

    #[test]
    fn test_rust_parser_empty_file() {
        let source = "";

        let parser = RustParser;
        let module = parser.parse(source, "test.rs").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 0);
    }
}
