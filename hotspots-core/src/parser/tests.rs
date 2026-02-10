//! Tests for TypeScript and JavaScript parser

#[cfg(test)]
mod parser_tests {
    use crate::parser;
    use swc_common::{sync::Lrc, SourceMap};

    fn parse_test(src: &str, filename: &str) -> Result<swc_ecma_ast::Module, anyhow::Error> {
        let cm: Lrc<SourceMap> = Default::default();
        parser::parse_source(src, &cm, filename)
    }

    #[test]
    fn test_parse_simple_function_typescript() {
        let src = "function foo() { return 42; }";
        let result = parse_test(src, "test.ts");
        assert!(result.is_ok(), "Should parse simple TypeScript function");
    }

    #[test]
    fn test_parse_simple_function_javascript() {
        let src = "function foo() { return 42; }";
        let result = parse_test(src, "test.js");
        assert!(result.is_ok(), "Should parse simple JavaScript function");
    }

    #[test]
    fn test_parse_typescript_types() {
        let src = "function foo(x: number): number { return x * 2; }";
        let result = parse_test(src, "test.ts");
        assert!(result.is_ok(), "Should parse TypeScript types");
    }

    #[test]
    fn test_parse_rejects_jsx_in_plain_typescript() {
        let src = "function foo() { return <div>hello</div>; }";
        let result = parse_test(src, "test.ts");
        // JSX syntax should cause a parse error in .ts files (use .tsx for JSX)
        assert!(
            result.is_err(),
            "JSX syntax should cause parse error in .ts files (use .tsx instead)"
        );
    }

    #[test]
    fn test_parse_rejects_jsx_in_plain_javascript() {
        let src = "function foo() { return <div>hello</div>; }";
        let result = parse_test(src, "test.js");
        // JSX syntax should cause a parse error in .js files (use .jsx for JSX)
        assert!(
            result.is_err(),
            "JSX syntax should cause parse error in .js files (use .jsx instead)"
        );
    }

    #[test]
    fn test_parse_accepts_jsx_in_tsx_files() {
        let src = "function foo() { return <div>hello</div>; }";
        let result = parse_test(src, "test.tsx");
        // JSX syntax should parse successfully in .tsx files
        assert!(
            result.is_ok(),
            "JSX syntax should parse successfully in .tsx files"
        );
    }

    #[test]
    fn test_parse_accepts_jsx_in_jsx_files() {
        let src = "function foo() { return <div>hello</div>; }";
        let result = parse_test(src, "test.jsx");
        // JSX syntax should parse successfully in .jsx files
        assert!(
            result.is_ok(),
            "JSX syntax should parse successfully in .jsx files"
        );
    }

    #[test]
    fn test_parse_interface_ignored() {
        // Interfaces should parse but won't be analyzed
        let src = "interface Foo { bar: string; }";
        let result = parse_test(src, "test.ts");
        assert!(
            result.is_ok(),
            "Should parse interface (but ignore in analysis)"
        );
    }

    #[test]
    fn test_parse_multiple_functions_typescript() {
        let src = r#"
            function foo() { return 1; }
            function bar() { return 2; }
        "#;
        let result = parse_test(src, "test.ts");
        assert!(result.is_ok(), "Should parse multiple TypeScript functions");
    }

    #[test]
    fn test_parse_multiple_functions_javascript() {
        let src = r#"
            function foo() { return 1; }
            function bar() { return 2; }
        "#;
        let result = parse_test(src, "test.js");
        assert!(result.is_ok(), "Should parse multiple JavaScript functions");
    }

    #[test]
    fn test_parse_javascript_es6_features() {
        let src = r#"
            const arrow = (x) => x * 2;
            async function asyncFn() { await Promise.resolve(42); }
            const destructure = ({a, b}) => a + b;
        "#;
        let result = parse_test(src, "test.js");
        assert!(result.is_ok(), "Should parse modern JavaScript features");
    }
}
