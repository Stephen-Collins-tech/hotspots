//! Suppression comment extraction
//!
//! Parses `<comment-prefix> hotspots-ignore: reason` comments from source code,
//! e.g. `// hotspots-ignore: reason` or, for Python, `# hotspots-ignore: reason`.
//!
//! Global invariants enforced:
//! - Deterministic extraction (pure function of source, span, comment prefix)
//! - Comment must be on the line immediately before the function
//! - Returns None (no suppression), Some("") (no reason), or Some("reason")

use crate::language::SourceSpan;

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
/// * `span` - The function's source span
/// * `comment_prefix` - The line-comment prefix for this language (e.g. `"//"`
///   or `"#"`), from [`crate::language::Language::suppression_comment_prefix`]
///
/// # Comment Format
///
/// The suppression comment must be on the line immediately before the function:
/// ```typescript
/// // hotspots-ignore: reason for suppression
/// function foo() { ... }
/// ```
///
/// Blank lines between the comment and function will cause the comment to be ignored.
pub fn extract_suppression(source: &str, span: SourceSpan, comment_prefix: &str) -> Option<String> {
    // Get the line number of the function start (1-indexed)
    let func_line = span.start_line;

    // Edge case: function is on first line, no previous line exists
    if func_line <= 1 {
        return None;
    }

    // Get the previous line (line numbers are 1-indexed)
    let prev_line_num = (func_line - 1) as usize;

    // Split source into lines and get the previous line
    let lines: Vec<&str> = source.lines().collect();

    // Check if prev_line_num is valid (convert to 0-indexed)
    if prev_line_num == 0 || prev_line_num > lines.len() {
        return None;
    }

    let prev_line = lines[prev_line_num - 1].trim();

    // Check if the line contains the suppression comment
    let marker = format!("{comment_prefix} hotspots-ignore");
    if !prev_line.starts_with(&marker) {
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

        let source_span = crate::language::span::span_with_location(function_span, &source_map);
        extract_suppression(source, source_span, "//")
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
// hotspots-ignore: legacy code, will refactor later
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
// hotspots-ignore:
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), Some(String::new()));
    }

    #[test]
    fn test_suppression_no_colon() {
        let source = r#"
// hotspots-ignore
function foo() {
  return 42;
}
"#;
        assert_eq!(parse_and_extract(source), Some(String::new()));
    }

    #[test]
    fn test_blank_line_between() {
        let source = r#"
// hotspots-ignore: should not be recognized

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
  // hotspots-ignore:   whitespace test
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

    // Python uses `#` line comments, not `//`. These tests exercise
    // `extract_suppression` directly against a synthetic Python-shaped span
    // rather than going through the swc/TS test harness above.
    fn python_span(func_line: u32) -> SourceSpan {
        SourceSpan {
            start: 0,
            end: 0,
            start_line: func_line,
            end_line: func_line,
            start_col: 0,
        }
    }

    #[test]
    fn test_hash_prefix_suppression_with_reason() {
        let source =
            "# hotspots-ignore: legacy code, will refactor later\ndef foo():\n    return 42\n";
        assert_eq!(
            extract_suppression(source, python_span(2), "#"),
            Some("legacy code, will refactor later".to_string())
        );
    }

    #[test]
    fn test_hash_prefix_no_match_against_slash_comment() {
        let source = "// hotspots-ignore: this is a JS-style comment\ndef foo():\n    return 42\n";
        assert_eq!(extract_suppression(source, python_span(2), "#"), None);
    }

    #[test]
    fn test_slash_prefix_no_match_against_hash_comment() {
        let source = "# hotspots-ignore: this is a Python-style comment\nfunction foo() {}\n";
        assert_eq!(extract_suppression(source, python_span(2), "//"), None);
    }

    #[test]
    fn test_hash_prefix_blank_line_between() {
        let source = "# hotspots-ignore: should not be recognized\n\ndef foo():\n    return 42\n";
        assert_eq!(extract_suppression(source, python_span(3), "#"), None);
    }
}
