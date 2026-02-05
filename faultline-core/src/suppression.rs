//! Suppression comment extraction
//!
//! Parses `// faultline-ignore: reason` comments from source code.
//!
//! Global invariants enforced:
//! - Deterministic extraction (pure function of source, span, source_map)
//! - Comment must be on the line immediately before the function
//! - Returns None (no suppression), Some("") (no reason), or Some("reason")

use swc_common::{SourceMap, Span};

/// Extract suppression comment for a function
///
/// Returns:
/// - `None` if no suppression comment found
/// - `Some("")` if suppression comment found but no reason provided
/// - `Some("reason")` if suppression comment found with reason
///
/// # Arguments
///
/// * `source` - The complete source code
/// * `span` - The function's span
/// * `source_map` - SWC source map for position lookups
///
/// # Comment Format
///
/// The suppression comment must be on the line immediately before the function:
/// ```typescript
/// // faultline-ignore: reason for suppression
/// function foo() { ... }
/// ```
///
/// Blank lines between the comment and function will cause the comment to be ignored.
pub fn extract_suppression(source: &str, span: Span, source_map: &SourceMap) -> Option<String> {
    // Get the line number of the function start
    let func_pos = source_map.lookup_char_pos(span.lo);
    let func_line = func_pos.line;

    // Edge case: function is on first line, no previous line exists
    if func_line <= 1 {
        return None;
    }

    // Get the previous line (line numbers are 1-indexed)
    let prev_line_num = func_line - 1;

    // Split source into lines and get the previous line
    let lines: Vec<&str> = source.lines().collect();

    // Check if prev_line_num is valid (convert to 0-indexed)
    if prev_line_num == 0 || prev_line_num > lines.len() {
        return None;
    }

    let prev_line = lines[prev_line_num - 1].trim();

    // Check if the line contains the suppression comment
    if !prev_line.starts_with("// faultline-ignore") {
        return None;
    }

    // Extract the reason after the colon
    if let Some(colon_pos) = prev_line.find(':') {
        let reason = prev_line[colon_pos + 1..].trim();
        if reason.is_empty() {
            Some(String::new()) // Suppression without reason
        } else {
            Some(reason.to_string()) // Suppression with reason
        }
    } else {
        // No colon found - treat as suppression without reason
        Some(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::{sync::Lrc, FileName, SourceMap};
    use swc_ecma_ast::EsVersion;
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};

    fn parse_and_extract(source: &str) -> Option<String> {
        let source_map = SourceMap::default();
        let source_file = source_map.new_source_file(
            Lrc::new(FileName::Custom("test.ts".to_string())),
            source.to_string(),
        );

        let lexer = Lexer::new(
            Syntax::Typescript(Default::default()),
            EsVersion::Es2022,
            StringInput::from(&*source_file),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        // Get the first function declaration
        let function_span = module
            .body
            .iter()
            .find_map(|item| {
                if let swc_ecma_ast::ModuleItem::Stmt(swc_ecma_ast::Stmt::Decl(
                    swc_ecma_ast::Decl::Fn(fn_decl),
                )) = item
                {
                    Some(fn_decl.function.span)
                } else {
                    None
                }
            })
            .expect("no function found");

        extract_suppression(source, function_span, &source_map)
    }

    #[test]
    fn test_no_suppression() {
        let source = r#"
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), None);
    }

    #[test]
    fn test_suppression_with_reason() {
        let source = r#"
// faultline-ignore: legacy code, will refactor later
function foo() {
  return 42;
}
"#;
        assert_eq!(
            parse_and_extract(source),
            Some("legacy code, will refactor later".to_string())
        );
    }

    #[test]
    fn test_suppression_without_reason() {
        let source = r#"
// faultline-ignore:
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), Some(String::new()));
    }

    #[test]
    fn test_suppression_no_colon() {
        let source = r#"
// faultline-ignore
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), Some(String::new()));
    }

    #[test]
    fn test_blank_line_between() {
        let source = r#"
// faultline-ignore: should not be recognized

function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), None);
    }

    #[test]
    fn test_function_on_first_line() {
        let source = "function foo() { return 42; }";
        assert_eq!(parse_and_extract(source), None);
    }

    #[test]
    fn test_suppression_with_whitespace() {
        let source = r#"
  // faultline-ignore:   whitespace test
function foo() {
  return 42;
}
"#;
        assert_eq!(
            parse_and_extract(source),
            Some("whitespace test".to_string())
        );
    }

    #[test]
    fn test_different_comment() {
        let source = r#"
// This is just a regular comment
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), None);
    }
}
