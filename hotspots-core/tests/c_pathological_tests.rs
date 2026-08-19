//! Regression zoo for the C-parser GLR-hang fix (bounds tree-sitter-c error
//! recovery on C++ syntax leaking into `.h` files). Each fixture is a
//! deliberately adversarial header; the assertion is simply that parsing
//! completes well within the parser's internal timeout and never panics.
//!
//! These fixtures do not reproduce the original unbounded blowup seen against
//! `llvm-project`'s `clang/lib` (RSS climbing past 2GB, never isolated to a
//! specific file) -- they're the closest synthetic approximations found while
//! stress-testing the fix, kept here to catch future regressions in parse time.

use hotspots_core::language::c::CParser;
use hotspots_core::language::parser::LanguageParser;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// Comfortably above the parser's internal 5s cancellation bound: a fixture
// that takes longer than this indicates the timeout itself failed to fire.
const MAX_ALLOWED: Duration = Duration::from_secs(8);

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("c")
        .join(name)
}

fn assert_parses_within_bound(name: &str) {
    let path = fixture_path(name);
    let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));

    let parser = CParser::new().unwrap();
    let start = Instant::now();
    let result = parser.parse(&source, name);
    let elapsed = start.elapsed();

    assert!(
        elapsed < MAX_ALLOWED,
        "{name} took {elapsed:?}, expected under {MAX_ALLOWED:?} (internal timeout should have fired)"
    );
    // Either outcome is acceptable: a clean parse, or a timeout-triggered Err.
    // The only failure mode under test is hanging past MAX_ALLOWED above.
    let _ = result;
}

#[test]
fn test_cpp_syntax_leak_does_not_hang() {
    assert_parses_within_bound("cpp_syntax_leak.h");
}

#[test]
fn test_deep_nested_chain_does_not_hang() {
    assert_parses_within_bound("deep_nested_chain.h");
}
