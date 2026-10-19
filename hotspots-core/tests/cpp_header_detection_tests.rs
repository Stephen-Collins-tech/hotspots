//! Regression test for issue #204: `.h` files containing real C++ (range-based
//! `for` + `break`, member functions, templates) must be cleanly skipped with
//! one clear warning instead of either producing a confusing "invalid CFG"
//! error or silently dropping functions.

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
fn test_cpp_content_in_header_is_skipped_not_errored_or_dropped() {
    let path = fixture_path("range_for_break.h");
    let cm: Lrc<SourceMap> = Default::default();
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let result = analyze_file_with_config(&path, &cm, 0, &options, None, None, None);

    // Before this fix: this file's `first_non1` produced a confusing
    // "invalid CFG: Nodes not reachable from entry" warning, and `Foo::bar`
    // and `add<T>` were silently dropped with no warning at all. Now the
    // whole file is detected as C++ content and rejected up front with one
    // clear, specific error instead.
    let err = result.expect_err("C++ content in a .h file should be rejected, not analyzed as C");
    let msg = err.to_string();
    assert!(
        msg.contains("C++ content") && msg.contains("#204"),
        "expected a clear C++-detection message, got: {msg}"
    );
}
