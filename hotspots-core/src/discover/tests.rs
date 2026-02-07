//! Tests for function discovery

#[cfg(test)]
mod discover_tests {
    use crate::discover;
    use crate::parser;
    use swc_common::{sync::Lrc, SourceMap};

    fn parse_and_discover(src: &str, file_index: usize) -> Vec<crate::ast::FunctionNode> {
        let cm: Lrc<SourceMap> = Default::default();
        let module = parser::parse_source(src, &cm, "test.ts").unwrap();
        discover::discover_functions(&module, file_index, src, &cm)
    }

    #[test]
    fn test_discover_single_function() {
        let src = "function foo() { return 42; }";
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should discover one function");
        assert_eq!(functions[0].name, Some("foo".to_string()));
    }

    #[test]
    fn test_discover_multiple_functions() {
        let src = r#"
            function foo() { return 1; }
            function bar() { return 2; }
        "#;
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 2, "Should discover two functions");
    }

    #[test]
    fn test_discover_anonymous_arrow_function() {
        let src = "const foo = () => { return 42; };";
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should discover arrow function");
        assert_eq!(functions[0].name, None, "Arrow function should have no name");
    }

    #[test]
    fn test_discover_class_method() {
        let src = r#"
            class Foo {
                method() { return 42; }
            }
        "#;
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should discover class method");
        assert_eq!(functions[0].name, Some("method".to_string()));
    }

    #[test]
    fn test_discover_deterministic_ordering() {
        let src = r#"
            function zzz() { return 3; }
            function aaa() { return 1; }
            function mmm() { return 2; }
        "#;
        let functions1 = parse_and_discover(src, 0);
        let functions2 = parse_and_discover(src, 0);
        
        // Functions should be in same order (sorted by span.start)
        assert_eq!(functions1.len(), functions2.len());
        for (f1, f2) in functions1.iter().zip(functions2.iter()) {
            assert_eq!(f1.span.start, f2.span.start, "Function order should be deterministic");
        }
    }

    #[test]
    fn test_discover_ignores_interfaces() {
        let src = r#"
            interface Foo {
                bar: string;
            }
            function baz() { return 42; }
        "#;
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should ignore interfaces");
        assert_eq!(functions[0].name, Some("baz".to_string()));
    }

    #[test]
    fn test_discover_ignores_type_aliases() {
        let src = r#"
            type Foo = string;
            function bar() { return 42; }
        "#;
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should ignore type aliases");
    }

    #[test]
    fn test_discover_function_with_arrow_expression_body() {
        let src = "const foo = () => 42;";
        let functions = parse_and_discover(src, 0);
        assert_eq!(functions.len(), 1, "Should discover arrow with expression body");
        // Expression body should be converted to return statement
        assert_eq!(functions[0].body.stmts.len(), 1, "Should have one statement (return)");
    }
}
