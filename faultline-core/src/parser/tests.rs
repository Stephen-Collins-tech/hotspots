//! Tests for TypeScript parser

#[cfg(test)]
mod tests {
    use crate::parser;
    use swc_common::{sync::Lrc, SourceMap};

    fn parse_test(src: &str) -> Result<swc_ecma_ast::Module, anyhow::Error> {
        let cm: Lrc<SourceMap> = Default::default();
        parser::parse_typescript(src, &cm, "test.ts")
    }

    #[test]
    fn test_parse_simple_function() {
        let src = "function foo() { return 42; }";
        let result = parse_test(src);
        assert!(result.is_ok(), "Should parse simple function");
    }

    #[test]
    fn test_parse_typescript_types() {
        let src = "function foo(x: number): number { return x * 2; }";
        let result = parse_test(src);
        assert!(result.is_ok(), "Should parse TypeScript types");
    }

    #[test]
    fn test_parse_rejects_jsx() {
        let src = "function foo() { return <div>hello</div>; }";
        let result = parse_test(src);
        // JSX syntax should cause a parse error (since tsx=false in parser config)
        // The error may or may not explicitly mention JSX, but it must fail to parse
        assert!(
            result.is_err(),
            "JSX syntax should cause parse error when tsx=false"
        );
    }

    #[test]
    fn test_parse_interface_ignored() {
        // Interfaces should parse but won't be analyzed
        let src = "interface Foo { bar: string; }";
        let result = parse_test(src);
        assert!(result.is_ok(), "Should parse interface (but ignore in analysis)");
    }

    #[test]
    fn test_parse_multiple_functions() {
        let src = r#"
            function foo() { return 1; }
            function bar() { return 2; }
        "#;
        let result = parse_test(src);
        assert!(result.is_ok(), "Should parse multiple functions");
    }
}
