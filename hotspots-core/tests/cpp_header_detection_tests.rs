//! Regression test for issue #204: `.h` files containing real C++ (range-based
//! `for` + `break`, member functions, templates) must be parsed with the C++
//! grammar, not skipped, mis-parsed as C, or silently stripped of functions.

use hotspots_core::analysis::analyze_file_with_config;
use hotspots_core::AnalysisOptions;
use std::path::PathBuf;
use swc_common::{sync::Lrc, SourceMap};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("c")
        .join(name)
}

#[test]
fn test_cpp_content_in_header_is_analyzed_as_cpp() {
    let path = fixture_path("range_for_break.h");
    let cm: Lrc<SourceMap> = Default::default();
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze_file_with_config(&path, &cm, 0, &options, None, None)
        .expect("a .h file with C++ content should be analyzed");

    let names: Vec<String> = reports.iter().map(|r| r.function.clone()).collect();
    // Before: `first_non1` gave "invalid CFG" (range-for `break` had no target), and the
    // member function and template were silently dropped.
    assert!(names.iter().any(|n| n == "first_non1"), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("bar")), "{names:?}");
    assert!(names.iter().any(|n| n == "add"), "{names:?}");
}
