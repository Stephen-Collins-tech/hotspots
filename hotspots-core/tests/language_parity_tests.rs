//! Tests to verify TypeScript and JavaScript produce identical metrics
//!
//! Critical invariant: Functions with identical structure should yield
//! identical complexity metrics regardless of whether they're written
//! in TypeScript or JavaScript.

use hotspots_core::{analyze, AnalysisOptions};
use std::path::PathBuf;

/// Test that TypeScript and JavaScript versions of the same function yield identical metrics
#[test]
fn test_typescript_javascript_parity() {
    let fixtures = vec![
        ("simple.ts", "js/simple.js"),
        ("nested-branching.ts", "js/nested-branching.js"),
        ("loop-breaks.ts", "js/loop-breaks.js"),
        ("try-catch-finally.ts", "js/try-catch-finally.js"),
        ("pathological.ts", "js/pathological.js"),
    ];

    for (ts_file, js_file) in fixtures {
        let ts_path = PathBuf::from("tests/fixtures").join(ts_file);
        let js_path = PathBuf::from("tests/fixtures").join(js_file);

        let options = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };

        // Analyze TypeScript version
        let ts_reports = analyze(&ts_path, options)
            .unwrap_or_else(|_| panic!("Failed to analyze {}", ts_file));

        // Analyze JavaScript version
        let options = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };
        let js_reports = analyze(&js_path, options)
            .unwrap_or_else(|_| panic!("Failed to analyze {}", js_file));

        // Both should have same number of functions
        assert_eq!(
            ts_reports.len(),
            js_reports.len(),
            "TypeScript and JavaScript versions of {} should have same number of functions",
            ts_file
        );

        // Compare each function's metrics
        for (ts_report, js_report) in ts_reports.iter().zip(js_reports.iter()) {
            // Function names should match
            assert_eq!(
                ts_report.function, js_report.function,
                "Function names should match in {}/{}",
                ts_file, js_file
            );

            // Raw metrics should be identical
            assert_eq!(
                ts_report.metrics.cc, js_report.metrics.cc,
                "Cyclomatic Complexity should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.metrics.nd, js_report.metrics.nd,
                "Nesting Depth should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.metrics.fo, js_report.metrics.fo,
                "Fan-Out should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.metrics.ns, js_report.metrics.ns,
                "Non-Structured Exits should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            // Risk components should be identical
            assert_eq!(
                ts_report.risk.r_cc, js_report.risk.r_cc,
                "Risk CC should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.risk.r_nd, js_report.risk.r_nd,
                "Risk ND should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.risk.r_fo, js_report.risk.r_fo,
                "Risk FO should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            assert_eq!(
                ts_report.risk.r_ns, js_report.risk.r_ns,
                "Risk NS should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );

            // LRS should be identical (within floating point precision)
            let lrs_diff = (ts_report.lrs - js_report.lrs).abs();
            assert!(
                lrs_diff < 1e-10,
                "LRS should match for function {} in {}/{}: TS={}, JS={}, diff={}",
                ts_report.function,
                ts_file,
                js_file,
                ts_report.lrs,
                js_report.lrs,
                lrs_diff
            );

            // Risk bands should match
            assert_eq!(
                ts_report.band, js_report.band,
                "Risk band should match for function {} in {}/{}",
                ts_report.function, ts_file, js_file
            );
        }

        println!("✓ Parity verified: {} ↔ {}", ts_file, js_file);
    }
}

/// Test that .mjs and .cjs files are also analyzed correctly
#[test]
fn test_javascript_module_extensions() {
    // Create temporary .mjs and .cjs files
    let temp_dir = std::env::temp_dir().join("hotspots_test_extensions");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mjs_file = temp_dir.join("test.mjs");
    let cjs_file = temp_dir.join("test.cjs");

    let src = "function test() { return 42; }";
    std::fs::write(&mjs_file, src).unwrap();
    std::fs::write(&cjs_file, src).unwrap();

    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    // Both should parse and analyze successfully
    let mjs_reports = analyze(&mjs_file, options1)
        .expect("Should analyze .mjs files");
    let cjs_reports = analyze(&cjs_file, options2)
        .expect("Should analyze .cjs files");

    assert_eq!(mjs_reports.len(), 1, "Should find function in .mjs file");
    assert_eq!(cjs_reports.len(), 1, "Should find function in .cjs file");

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}

/// Test that .mts and .cts files are also analyzed correctly
#[test]
fn test_typescript_module_extensions() {
    // Create temporary .mts and .cts files
    let temp_dir = std::env::temp_dir().join("hotspots_test_ts_extensions");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mts_file = temp_dir.join("test.mts");
    let cts_file = temp_dir.join("test.cts");

    let src = "function test(): number { return 42; }";
    std::fs::write(&mts_file, src).unwrap();
    std::fs::write(&cts_file, src).unwrap();

    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    // Both should parse and analyze successfully
    let mts_reports = analyze(&mts_file, options1)
        .expect("Should analyze .mts files");
    let cts_reports = analyze(&cts_file, options2)
        .expect("Should analyze .cts files");

    assert_eq!(mts_reports.len(), 1, "Should find function in .mts file");
    assert_eq!(cts_reports.len(), 1, "Should find function in .cts file");

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}
