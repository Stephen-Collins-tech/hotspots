//! ECMAScript (TypeScript/JavaScript) parser and CFG builder implementation

use super::cfg_builder::CfgBuilder;
use super::parser::{LanguageParser, ParsedModule};
use crate::ast::FunctionNode;
use crate::cfg::Cfg;
use anyhow::Result;
use regex::Regex;
use swc_common::{sync::Lrc, SourceMap};
use swc_ecma_ast::Module;

/// ECMAScript parser using SWC
///
/// Parses TypeScript and JavaScript files using the SWC compiler infrastructure.
pub struct ECMAScriptParser {
    source_map: Lrc<SourceMap>,
}

impl ECMAScriptParser {
    /// Create a new ECMAScript parser with the given source map
    pub fn new(source_map: Lrc<SourceMap>) -> Self {
        ECMAScriptParser { source_map }
    }
}

impl LanguageParser for ECMAScriptParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let module = crate::parser::parse_source(source, &self.source_map, filename)?;

        Ok(Box::new(ECMAScriptModule {
            module,
            source_map: self.source_map.clone(),
        }))
    }
}

/// Parsed ECMAScript module
struct ECMAScriptModule {
    module: Module,
    source_map: Lrc<SourceMap>,
}

impl ParsedModule for ECMAScriptModule {
    fn discover_functions(&self, file_index: usize, source: &str) -> Vec<FunctionNode> {
        crate::discover::discover_functions(&self.module, file_index, source, &self.source_map)
    }
}

/// ECMAScript CFG builder
///
/// Builds control flow graphs from ECMAScript (TypeScript/JavaScript) function bodies.
pub struct ECMAScriptCfgBuilder;

impl CfgBuilder for ECMAScriptCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        // Delegate to the existing cfg::builder::build_cfg function
        // which already knows how to build CFGs from ECMAScript functions
        crate::cfg::builder::build_cfg(function)
    }
}

/// Extracted script block from a Vue SFC
struct ScriptBlock {
    /// The raw content between `<script ...>` and `</script>`
    content: String,
    /// 1-indexed line number of the first line of `content` in the original file
    start_line: u32,
    /// Whether `lang="ts"` (or `lang='ts'`) was detected on the script tag
    is_typescript: bool,
}

/// Extract the first `<script>` or `<script setup>` block from a Vue SFC source.
///
/// Returns `None` if no script block is found.
fn extract_script_block(source: &str) -> Option<ScriptBlock> {
    let open_re = Regex::new(r"(?i)<script(\s[^>]*)?>").unwrap();
    let close_tag = "</script>";

    let open_m = open_re.find(source)?;
    let attrs = open_re
        .captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or("");

    let is_typescript = {
        let lang_re = Regex::new(r#"lang\s*=\s*['"]ts['"]"#).unwrap();
        lang_re.is_match(attrs)
    };

    let content_start = open_m.end();
    let close_pos = source[content_start..].find(close_tag)?;
    let content = source[content_start..content_start + close_pos].to_string();

    // Count lines before content_start to get the 1-indexed line of the first
    // content line (the newline after the opening tag puts content on the next line).
    let start_line = source[..content_start]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32
        + 1;

    Some(ScriptBlock {
        content,
        start_line,
        is_typescript,
    })
}

/// Vue SFC parser — extracts the `<script>` block and delegates to ECMAScriptParser.
pub struct VueParser {
    inner: ECMAScriptParser,
}

impl VueParser {
    pub fn new(source_map: Lrc<SourceMap>) -> Self {
        VueParser {
            inner: ECMAScriptParser::new(source_map),
        }
    }
}

impl LanguageParser for VueParser {
    fn parse(&self, source: &str, filename: &str) -> Result<Box<dyn ParsedModule>> {
        let block = extract_script_block(source)
            .ok_or_else(|| anyhow::anyhow!("No <script> block found in Vue SFC: {}", filename))?;

        // Pick a synthetic filename so SWC uses the right syntax
        let synthetic = if block.is_typescript {
            "__vue_script__.ts"
        } else {
            "__vue_script__.js"
        };

        let inner_module = self.inner.parse(&block.content, synthetic)?;

        Ok(Box::new(VueParsedModule {
            inner: inner_module,
            line_offset: block.start_line.saturating_sub(1),
        }))
    }
}

/// Wraps an ECMAScript ParsedModule and shifts all span line numbers by `line_offset`.
struct VueParsedModule {
    inner: Box<dyn ParsedModule>,
    /// Lines to add to each reported start_line / end_line
    line_offset: u32,
}

impl ParsedModule for VueParsedModule {
    fn discover_functions(&self, file_index: usize, source: &str) -> Vec<FunctionNode> {
        let mut functions = self.inner.discover_functions(file_index, source);
        let offset = self.line_offset;
        for f in &mut functions {
            f.span.start_line += offset;
            f.span.end_line += offset;
        }
        functions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecmascript_parser_simple_function() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = "function foo() { return 42; }";
        let module = parser.parse(source, "test.ts").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("foo".to_string()));
    }

    #[test]
    fn test_ecmascript_parser_multiple_functions() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = r#"
            function foo() { return 1; }
            function bar() { return 2; }
            const baz = () => 3;
        "#;
        let module = parser.parse(source, "test.ts").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 3);
    }

    #[test]
    fn test_ecmascript_parser_typescript() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = r#"
            function typed(x: number): string {
                return x.toString();
            }
        "#;
        let module = parser.parse(source, "test.ts").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("typed".to_string()));
    }

    #[test]
    fn test_ecmascript_parser_jsx() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = r#"
            function Component() {
                return <div>Hello</div>;
            }
        "#;
        let module = parser.parse(source, "test.tsx").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("Component".to_string()));
    }

    #[test]
    fn test_ecmascript_parser_class_methods() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = r#"
            class MyClass {
                method1() { return 1; }
                method2() { return 2; }
            }
        "#;
        let module = parser.parse(source, "test.ts").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 2);
    }

    #[test]
    fn test_ecmascript_parser_parse_error() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = "function foo() { return }}}"; // Invalid syntax
        let result = parser.parse(source, "test.ts");

        assert!(result.is_err());
    }

    #[test]
    fn test_ecmascript_parser_empty_file() {
        let source_map: Lrc<SourceMap> = Default::default();
        let parser = ECMAScriptParser::new(source_map);

        let source = "";
        let module = parser.parse(source, "test.ts").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_ecmascript_parser_deterministic() {
        let source = r#"
            function zzz() { return 3; }
            function aaa() { return 1; }
            function mmm() { return 2; }
        "#;

        // Parse twice with fresh source maps
        let source_map1: Lrc<SourceMap> = Default::default();
        let parser1 = ECMAScriptParser::new(source_map1);
        let module1 = parser1.parse(source, "test.ts").unwrap();
        let functions1 = module1.discover_functions(0, source);

        let source_map2: Lrc<SourceMap> = Default::default();
        let parser2 = ECMAScriptParser::new(source_map2);
        let module2 = parser2.parse(source, "test.ts").unwrap();
        let functions2 = module2.discover_functions(0, source);

        // Functions should be discovered in the same order (deterministic)
        assert_eq!(functions1.len(), functions2.len());
        assert_eq!(functions1.len(), 3);

        // Check that names are in the same order
        for (f1, f2) in functions1.iter().zip(functions2.iter()) {
            assert_eq!(f1.name, f2.name);
            assert_eq!(f1.span.start_line, f2.span.start_line);
        }

        // Verify functions are sorted by source position, not alphabetically
        assert_eq!(functions1[0].name, Some("zzz".to_string()));
        assert_eq!(functions1[1].name, Some("aaa".to_string()));
        assert_eq!(functions1[2].name, Some("mmm".to_string()));
    }
}
