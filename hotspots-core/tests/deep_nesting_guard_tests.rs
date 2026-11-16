//! Very deeply nested input must be skipped with a warning, not abort the process.
//!
//! The Rust (`syn`), ECMAScript (`swc`) and Go paths recurse and overflow a thread stack at some
//! depth. `analyze_file_with_config` skips a file whose bracket nesting is beyond a per-language
//! limit. Each case runs on a small stack, so a missing guard aborts the test process.

use hotspots_core::analysis::analyze_file_with_config;
use hotspots_core::AnalysisOptions;
use std::path::PathBuf;
use swc_common::{sync::Lrc, SourceMap};

const STACK: usize = 4 * 1024 * 1024;

fn analyze(name: &str, source: String) -> usize {
    let dir = std::env::temp_dir().join(format!("hotspots-deep-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let handle = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let cm: Lrc<SourceMap> = Default::default();
            let options = AnalysisOptions {
                min_lrs: None,
                top_n: None,
            };
            analyze_file_with_config(&path, &cm, 0, &options, None, None)
                .expect("deep input must not be an error")
                .len()
        })
        .unwrap();
    let n = handle.join().expect("analysis overflowed or panicked");
    let _ = std::fs::remove_dir_all(&dir);
    n
}

#[test]
fn rust_blocks_deeper_than_the_limit_are_skipped() {
    let (o, c) = ("{".repeat(20_000), "}".repeat(20_000));
    assert_eq!(analyze("deep.rs", format!("fn f() {{ {o}{c} }}\n")), 0);
}

#[test]
fn typescript_parentheses_deeper_than_the_limit_are_skipped() {
    let (o, c) = ("(".repeat(20_000), ")".repeat(20_000));
    assert_eq!(
        analyze("deep.ts", format!("function f() {{ return {o}1{c}; }}\n")),
        0
    );
}

#[test]
fn go_blocks_deeper_than_the_limit_are_skipped() {
    let (o, c) = ("{".repeat(20_000), "}".repeat(20_000));
    assert_eq!(
        analyze("deep.go", format!("package p\nfunc f() {{ {o}{c} }}\n")),
        0
    );
}

#[test]
fn ordinary_nesting_is_still_analyzed() {
    let rust = "fn f(x: i32) -> i32 { if x > 0 { match x { 1 => 1, _ => { (x + 1) * 2 } } } else { 0 } }\n";
    assert_eq!(analyze("ok.rs", rust.to_string()), 1);
    let ts = "function f(x: number) { if (x > 0) { return ((x + 1) * 2); } return 0; }\n";
    assert_eq!(analyze("ok.ts", ts.to_string()), 1);
}
