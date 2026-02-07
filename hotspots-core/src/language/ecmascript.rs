//! ECMAScript (TypeScript/JavaScript) parser implementation

use super::parser::{LanguageParser, ParsedModule};
use crate::ast::FunctionNode;
use anyhow::Result;
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
