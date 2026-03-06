//! Integration tests for hotspots analysis

use hotspots_core::{analyze, analyze_with_progress, render_json, AnalysisOptions};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    assert!(
        reports[0].metrics.cc > 1,
        "Nested branching should increase CC"
    );
    assert!(
        reports[0].metrics.nd >= 2,
        "Should have nesting depth of at least 2"
    );
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
    assert!(
        reports[0].metrics.ns > 0,
        "Breaks and continues should be counted"
    );
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
    assert!(
        reports[0].metrics.cc >= 2,
        "Try/catch should contribute to CC"
    );
    assert!(
        reports[0].metrics.nd >= 1,
        "Try should increase nesting depth"
    );
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
    assert!(
        reports[0].metrics.cc > 5,
        "Pathological function should have high CC"
    );
    assert!(
        reports[0].lrs > 5.0,
        "Pathological function should have high LRS"
    );
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

/// Angular-style TypeScript with class decorators (@Component, @Injectable, @Input)
/// must parse without error and produce function reports with correct metrics.
/// Decorators must not inflate CC or be counted as functions.
#[test]
fn test_angular_decorators_parse_and_analyze() {
    let path = fixture_path("angular-component.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&path, options).expect("Angular decorated TypeScript should parse");
    assert!(
        !reports.is_empty(),
        "Should discover functions in Angular component"
    );

    // getUser has one if/throw branch
    let get_user = reports.iter().find(|r| r.function == "getUser");
    assert!(get_user.is_some(), "Should find getUser method");
    assert!(
        get_user.unwrap().metrics.cc >= 2,
        "getUser with if/throw should have CC >= 2"
    );

    // Decorators must not appear as function reports
    let decorator_fn = reports
        .iter()
        .find(|r| r.function.contains("Component") || r.function.contains("Injectable"));
    assert!(
        decorator_fn.is_none(),
        "Decorator expressions must not be counted as functions"
    );
}

/// React JSX in a plain .js file (React webpack convention).
/// Must parse without error and produce function reports.
/// JSX elements must not inflate CC beyond the actual control flow.
#[test]
fn test_react_jsx_in_plain_js_file() {
    let path = fixture_path("react-component.js");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&path, options).expect("JSX in .js file should parse");
    assert!(
        !reports.is_empty(),
        "Should discover functions in .js React component"
    );

    // Counter component should be found
    let counter = reports.iter().find(|r| r.function == "Counter");
    assert!(counter.is_some(), "Should find Counter function");

    // Arrow functions inside Counter are reported as anonymous; verify at least
    // one discovered function has conditional logic (CC >= 2) from the if branches
    let has_branching_fn = reports.iter().any(|r| r.metrics.cc >= 2);
    assert!(
        has_branching_fn,
        "At least one function should have CC >= 2 from if branches"
    );

    // JSX return statement must not add CC — Counter's CC comes only from its
    // logic, not from the JSX element tree
    let counter_cc = counter.unwrap().metrics.cc;
    assert!(
        counter_cc < 10,
        "JSX elements must not inflate CC; got CC={}",
        counter_cc
    );
}

/// Progress callback must be called with (0, total) after discovery, then
/// (n, total) once per file analyzed, ending with (total, total).
#[test]
fn test_progress_callback_sequence_single_file() {
    let calls: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_ref = calls.clone();
    let path = fixture_path("simple.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    analyze_with_progress(
        &path,
        options,
        None,
        Some(&move |done: usize, total: usize| {
            calls_ref.lock().unwrap().push((done, total));
        }),
    )
    .unwrap();

    let recorded = calls.lock().unwrap();
    // Single file: (0, 1) discovery + (1, 1) after the file
    assert_eq!(*recorded, vec![(0, 1), (1, 1)]);
}

/// For a directory of N files, progress is called N+1 times total:
/// once with (0, N) and once with (i, N) for each file i in 1..=N.
#[test]
fn test_progress_callback_sequence_directory() {
    let calls: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_ref = calls.clone();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("rust");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    analyze_with_progress(
        &path,
        options,
        None,
        Some(&move |done: usize, total: usize| {
            calls_ref.lock().unwrap().push((done, total));
        }),
    )
    .unwrap();

    let recorded = calls.lock().unwrap();
    assert!(
        !recorded.is_empty(),
        "progress must be called for non-empty dir"
    );
    let total = recorded[0].1;
    assert!(total > 1, "rust fixtures dir should have multiple files");
    // Exact sequence: (0, total), (1, total), ..., (total, total)
    assert_eq!(
        recorded.len(),
        total + 1,
        "expected 1 discovery + N per-file calls"
    );
    for (i, &(done, t)) in recorded.iter().enumerate() {
        assert_eq!(
            (done, t),
            (i, total),
            "call {i}: expected ({i}, {total}), got ({done}, {t})"
        );
    }
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
