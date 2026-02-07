//! Tests to verify JSX and TSX produce identical metrics
//!
//! Critical invariant: JSX and TSX versions of the same component
//! should yield identical complexity metrics.

use hotspots_core::{analyze, AnalysisOptions};
use std::path::PathBuf;

/// Test that JSX and TSX versions produce identical metrics
#[test]
fn test_jsx_tsx_parity() {
    let fixtures = vec![
        ("simple-component.tsx", "simple-component.jsx"),
        ("conditional-rendering.tsx", "conditional-rendering.jsx"),
        ("complex-component.tsx", "complex-component.jsx"),
    ];

    for (tsx_file, jsx_file) in fixtures {
        let tsx_path = PathBuf::from("../tests/fixtures/tsx").join(tsx_file);
        let jsx_path = PathBuf::from("../tests/fixtures/jsx").join(jsx_file);

        let options = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };

        // Analyze TSX version
        let tsx_reports = analyze(&tsx_path, options)
            .unwrap_or_else(|_| panic!("Failed to analyze {}", tsx_file));

        // Analyze JSX version
        let options = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };
        let jsx_reports = analyze(&jsx_path, options)
            .unwrap_or_else(|_| panic!("Failed to analyze {}", jsx_file));

        // Both should have same number of functions
        assert_eq!(
            tsx_reports.len(),
            jsx_reports.len(),
            "TSX and JSX versions of {} should have same number of functions",
            tsx_file
        );

        // Sort by line number for comparison (anonymous functions may be in different order)
        let mut tsx_sorted = tsx_reports.clone();
        let mut jsx_sorted = jsx_reports.clone();
        tsx_sorted.sort_by_key(|r| r.line);
        jsx_sorted.sort_by_key(|r| r.line);

        // Compare each function's metrics
        for (tsx_report, jsx_report) in tsx_sorted.iter().zip(jsx_sorted.iter()) {
            // Line numbers should match
            assert_eq!(
                tsx_report.line, jsx_report.line,
                "Function at line {} should exist in both files",
                tsx_report.line
            );

            // Raw metrics should be identical
            assert_eq!(
                tsx_report.metrics.cc, jsx_report.metrics.cc,
                "CC should match for function at line {} in {}/{}",
                tsx_report.line, tsx_file, jsx_file
            );

            assert_eq!(
                tsx_report.metrics.nd, jsx_report.metrics.nd,
                "ND should match for function at line {} in {}/{}",
                tsx_report.line, tsx_file, jsx_file
            );

            assert_eq!(
                tsx_report.metrics.fo, jsx_report.metrics.fo,
                "FO should match for function at line {} in {}/{}",
                tsx_report.line, tsx_file, jsx_file
            );

            assert_eq!(
                tsx_report.metrics.ns, jsx_report.metrics.ns,
                "NS should match for function at line {} in {}/{}",
                tsx_report.line, tsx_file, jsx_file
            );

            // LRS should be identical (within floating point precision)
            let lrs_diff = (tsx_report.lrs - jsx_report.lrs).abs();
            assert!(
                lrs_diff < 1e-10,
                "LRS should match for function at line {} in {}/{}: TSX={}, JSX={}, diff={}",
                tsx_report.line,
                tsx_file,
                jsx_file,
                tsx_report.lrs,
                jsx_report.lrs,
                lrs_diff
            );

            // Risk bands should match
            assert_eq!(
                tsx_report.band, jsx_report.band,
                "Risk band should match for function at line {} in {}/{}",
                tsx_report.line, tsx_file, jsx_file
            );
        }

        println!("✓ JSX/TSX parity verified: {} ↔ {}", tsx_file, jsx_file);
    }
}

/// Test that simple JSX elements don't inflate complexity
#[test]
fn test_jsx_elements_dont_inflate_complexity() {
    let tsx_path = PathBuf::from("../tests/fixtures/tsx/simple-component.tsx");

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&tsx_path, options)
        .expect("Should analyze simple TSX component");

    assert_eq!(reports.len(), 1, "Should find exactly one function");

    let report = &reports[0];
    assert_eq!(report.function, "SimpleComponent");

    // Simple component with JSX should have same complexity as a simple return
    assert_eq!(report.metrics.cc, 1, "JSX elements should not increase CC");
    assert_eq!(report.metrics.nd, 0, "JSX elements should not increase ND");
    assert_eq!(report.metrics.fo, 0, "JSX elements should not increase FO");
    assert_eq!(report.metrics.ns, 0, "JSX elements should not increase NS");
    assert_eq!(report.lrs, 1.0, "Simple JSX component should have LRS of 1.0");
}

/// Test that control flow in JSX expressions IS counted
#[test]
fn test_jsx_control_flow_is_counted() {
    let tsx_path = PathBuf::from("../tests/fixtures/tsx/conditional-rendering.tsx");

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&tsx_path, options)
        .expect("Should analyze conditional rendering component");

    assert_eq!(reports.len(), 1, "Should find exactly one function");

    let report = &reports[0];
    assert_eq!(report.function, "ConditionalComponent");

    // Ternary operator and && operator should increase CC
    // Note: This tests current behavior - ternary counts as 1 branch point
    assert!(
        report.metrics.cc > 1,
        "Control flow in JSX expressions should increase CC, got CC={}",
        report.metrics.cc
    );

    assert!(
        report.lrs > 1.0,
        "Component with conditional rendering should have LRS > 1.0, got LRS={}",
        report.lrs
    );
}

/// Test that multiple functions in a JSX file are all analyzed
#[test]
fn test_multiple_functions_in_jsx_file() {
    let tsx_path = PathBuf::from("../tests/fixtures/tsx/complex-component.tsx");

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&tsx_path, options)
        .expect("Should analyze complex component");

    // Should find: ComplexComponent, handleClick, map callback, onClick handlers
    assert!(
        reports.len() >= 2,
        "Should find at least 2 functions (main component + event handler), found {}",
        reports.len()
    );

    // Verify ComplexComponent exists
    let main_component = reports.iter()
        .find(|r| r.function == "ComplexComponent")
        .expect("Should find ComplexComponent function");

    assert!(
        main_component.metrics.cc >= 1,
        "ComplexComponent should have positive CC"
    );
}
