//! Function discovery must not recurse on the syntax tree: a generated table or deeply nested input
//! can produce a tree thousands of levels deep. Each case runs on a deliberately small thread stack
//! (256 KiB), so a recursive walk overflows and aborts the test process while the iterative one passes.

use hotspots_core::language::{
    CParser, CSharpParser, GoParser, JavaParser, LanguageParser, PythonParser,
};

const STACK: usize = 256 * 1024;

fn discover_on_small_stack<P: LanguageParser + 'static>(
    make: fn() -> P,
    source: String,
    expect: usize,
) {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let parser = make();
            let module = parser.parse(&source, "deep").expect("parse");
            let found = module.discover_functions(0, &source);
            assert_eq!(found.len(), expect);
        })
        .unwrap()
        .join()
        .expect("discovery overflowed or panicked");
}

fn nested(open: &str, close: &str, n: usize) -> (String, String) {
    (open.repeat(n), close.repeat(n))
}

#[test]
fn c_generated_table_and_nested_blocks() {
    // A file-scope `{ {0x1}, {0x2}, ... }` (no function) parses as a right-nested comma expression.
    let table = format!(
        "{{\n{}\n}}\n",
        (0..20_000)
            .map(|i| format!("{{0x{i:x}}},"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    discover_on_small_stack(|| CParser::new().unwrap(), table, 0);

    let (o, c) = nested("{", "}", 4000);
    discover_on_small_stack(
        || CParser::new().unwrap(),
        format!("void f(void) {{ {o}{c} }}"),
        1,
    );
}

#[test]
fn go_nested_blocks() {
    let (o, c) = nested("{", "}", 4000);
    discover_on_small_stack(
        || GoParser::new().unwrap(),
        format!("package p\nfunc f() {{ {o}{c} }}\n"),
        1,
    );
}

#[test]
fn java_nested_blocks() {
    let (o, c) = nested("{", "}", 4000);
    discover_on_small_stack(
        || JavaParser::new().unwrap(),
        format!("class A {{ void f() {{ {o}{c} }} }}"),
        1,
    );
}

#[test]
fn csharp_nested_blocks() {
    let (o, c) = nested("{", "}", 4000);
    discover_on_small_stack(
        || CSharpParser::new().unwrap(),
        format!("class A {{ void F() {{ {o}{c} }} }}"),
        1,
    );
}

#[test]
fn python_nested_parentheses() {
    let (o, c) = nested("(", ")", 4000);
    discover_on_small_stack(
        || PythonParser::new().unwrap(),
        format!("def f():\n    x = {o}1{c}\n"),
        1,
    );
}
