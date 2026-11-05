//! Adversarial C++ inputs: parsing must finish within the parser's internal
//! timeout (plus slack) and never panic. Either a clean parse or a timeout Err is fine.

use hotspots_core::language::cpp::CppParser;
use hotspots_core::language::parser::LanguageParser;
use std::time::{Duration, Instant};

const MAX_ALLOWED: Duration = Duration::from_secs(8);

fn assert_parses_within_bound(label: &str, source: &str) {
    let parser = CppParser::new().unwrap();
    let start = Instant::now();
    let result = parser.parse(source, label);
    let elapsed = start.elapsed();
    assert!(
        elapsed < MAX_ALLOWED,
        "{label} took {elapsed:?}, expected under {MAX_ALLOWED:?}"
    );
    if let Ok(module) = result {
        // discovery and CFG building must not panic either
        let _ = module.discover_functions(0, source);
    }
}

#[test]
fn deeply_nested_templates_do_not_hang() {
    let depth = 400;
    let src = format!(
        "{}int{} x;\nint f() {{ return 0; }}\n",
        "std::vector<".repeat(depth),
        ">".repeat(depth)
    );
    assert_parses_within_bound("nested_templates.cpp", &src);
}

#[test]
fn deeply_nested_blocks_do_not_overflow() {
    let depth = 2000;
    let src = format!(
        "int f(int x) {{ {} return x; {} }}\n",
        "if (x) {".repeat(depth),
        "}".repeat(depth)
    );
    assert_parses_within_bound("nested_blocks.cpp", &src);
}

#[test]
fn unbalanced_macro_soup_does_not_hang() {
    let src = "#define BEGIN namespace x {\nBEGIN\nstruct S { void f() { try { } catch (...) \n"
        .repeat(300);
    assert_parses_within_bound("macro_soup.cpp", &src);
}
