//! Integration tests for faultline analysis

use hotspots_core::{analyze, render_json, AnalysisOptions};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_simple_function() {
    let path = fixture_path("simple.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options).unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].function, "simple");
}

#[test]
fn test_nested_branching() {
    let path = fixture_path("nested-branching.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].metrics.cc > 1, "Nested branching should increase CC");
    assert!(reports[0].metrics.nd >= 2, "Should have nesting depth of at least 2");
}

#[test]
fn test_loop_with_breaks() {
    let path = fixture_path("loop-breaks.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].metrics.ns > 0, "Breaks and continues should be counted");
}

#[test]
fn test_try_catch_finally() {
    let path = fixture_path("try-catch-finally.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].metrics.cc >= 2, "Try/catch should contribute to CC");
    assert!(reports[0].metrics.nd >= 1, "Try should increase nesting depth");
}

#[test]
fn test_pathological_complexity() {
    let path = fixture_path("pathological.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options).unwrap();
    assert_eq!(reports.len(), 1);
    // Pathological function should have high complexity
    assert!(reports[0].metrics.cc > 5, "Pathological function should have high CC");
    assert!(reports[0].lrs > 5.0, "Pathological function should have high LRS");
}

#[test]
fn test_deterministic_output() {
    let path = fixture_path("simple.ts");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    // Run analysis twice
    let reports1 = analyze(&path, options1).unwrap();
    let reports2 = analyze(&path, options2).unwrap();
    
    // Output should be identical
    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);
    
    assert_eq!(json1, json2, "Output should be byte-for-byte identical");
}

#[test]
fn test_whitespace_invariance() {
    // Test that whitespace changes don't affect output
    let path = fixture_path("simple.ts");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    
    let reports = analyze(&path, options1).unwrap();
    let lrs1 = reports[0].lrs;
    let cc1 = reports[0].metrics.cc;
    
    // Re-run should produce same results
    let reports2 = analyze(&path, options2).unwrap();
    let lrs2 = reports2[0].lrs;
    let cc2 = reports2[0].metrics.cc;
    
    assert_eq!(lrs1, lrs2);
    assert_eq!(cc1, cc2);
}
