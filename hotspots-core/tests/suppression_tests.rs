//! Integration tests for suppression comments
//!
//! These tests go through the full `analyze_file_with_config` pipeline (not
//! `discover::discover_functions` directly) because suppression extraction is
//! wired in once, uniformly, at the language-agnostic call site in
//! `analysis.rs` — every parser's `discover_functions` leaves
//! `suppression_reason: None` and relies on that later pass.

use hotspots_core::analysis::analyze_file_with_config;
use hotspots_core::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use hotspots_core::policy::{evaluate_policies, PolicyId, PolicySeverity};
use hotspots_core::risk::RiskBand;
use hotspots_core::{AnalysisOptions, ResolvedConfig};
use swc_common::{sync::Lrc, SourceMap};
use tempfile::Builder;

fn analyze_source(source: &str, suffix: &str) -> Vec<hotspots_core::report::FunctionRiskReport> {
    let file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("create temp file");
    std::fs::write(file.path(), source).expect("write temp source");

    let cm: Lrc<SourceMap> = Default::default();
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    analyze_file_with_config(file.path(), &cm, 0, &options, None, None)
        .expect("analysis should succeed")
}

#[test]
fn test_suppression_comment_extraction() {
    let source = r#"
// hotspots-ignore: legacy code
function suppressed() {
  if (true) {
    return 1;
  }
  return 0;
}

function notSuppressed() {
  if (true) {
    return 1;
  }
  return 0;
}
"#;

    let functions = analyze_source(source, ".ts");

    assert_eq!(functions.len(), 2);
    assert_eq!(
        functions[0].suppression_reason,
        Some("legacy code".to_string())
    );
    assert_eq!(functions[1].suppression_reason, None);
}

#[test]
fn test_suppression_without_reason() {
    let source = r#"
// hotspots-ignore:
function suppressed() {
  return 42;
}
"#;

    let functions = analyze_source(source, ".ts");

    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].suppression_reason, Some(String::new()));
}

#[test]
fn test_suppression_blank_line_ignored() {
    let source = r#"
// hotspots-ignore: should not work

function notSuppressed() {
  return 42;
}
"#;

    let functions = analyze_source(source, ".ts");

    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].suppression_reason, None);
}

/// Regression test: suppression previously only worked for TypeScript/
/// JavaScript, because `extract_suppression` was called solely from the
/// swc-specific `discover.rs` path. Every other language's parser set
/// `suppression_reason: None` unconditionally and nothing ever filled it in
/// — `// hotspots-ignore` was a silent no-op on Python, Go, Java, C#, Rust,
/// and C. This test exercises a non-JS language end-to-end.
#[test]
fn test_suppression_works_for_python() {
    let source = "# hotspots-ignore: legacy code\ndef suppressed():\n    if True:\n        return 1\n    return 0\n\n\ndef not_suppressed():\n    if True:\n        return 1\n    return 0\n";

    let functions = analyze_source(source, ".py");

    assert_eq!(functions.len(), 2);
    assert_eq!(
        functions[0].suppression_reason,
        Some("legacy code".to_string())
    );
    assert_eq!(functions[1].suppression_reason, None);
}

/// PHP uses `//` suppression comments like the C-family languages.
#[test]
fn test_suppression_works_for_php() {
    let source = "<?php\n// hotspots-ignore: legacy code\nfunction suppressed($a) {\n    if ($a) { return 1; }\n    return 0;\n}\n\nfunction not_suppressed($a) {\n    if ($a) { return 1; }\n    return 0;\n}\n";

    let functions = analyze_source(source, ".php");

    assert_eq!(functions.len(), 2);
    assert_eq!(
        functions[0].suppression_reason,
        Some("legacy code".to_string())
    );
    assert_eq!(functions[1].suppression_reason, None);
}

/// Regression test: a Go file previously never got suppression either,
/// since `Language::Go`'s parser also left `suppression_reason: None` with
/// no follow-up extraction step.
#[test]
fn test_suppression_works_for_go() {
    let source = "// hotspots-ignore: legacy code\nfunc Suppressed() int {\n\tif true {\n\t\treturn 1\n\t}\n\treturn 0\n}\n\nfunc NotSuppressed() int {\n\tif true {\n\t\treturn 1\n\t}\n\treturn 0\n}\n";

    let functions = analyze_source(source, ".go");

    assert_eq!(functions.len(), 2);
    assert_eq!(
        functions[0].suppression_reason,
        Some("legacy code".to_string())
    );
    assert_eq!(functions[1].suppression_reason, None);
}

#[test]
fn test_suppressed_function_excluded_from_critical_introduction() {
    use hotspots_core::delta::FunctionState;
    use hotspots_core::report::MetricsReport;

    // Create a function that would normally trigger critical introduction
    let critical_entry = FunctionDeltaEntry {
        function_id: "test.ts::critical".to_string(),
        status: FunctionStatus::New,
        before: None,
        after: Some(FunctionState {
            metrics: MetricsReport {
                cc: 20,
                nd: 10,
                fo: 5,
                ns: 3,
                loc: 50,
                missing_docstring: false,
            },
            lrs: 50.0,
            band: RiskBand::Critical,
        }),
        delta: None,
        band_transition: None,
        suppression_reason: Some("legacy code, will refactor".to_string()), // Suppressed with reason
        rename_hint: None,
    };

    let delta = Delta {
        commit: hotspots_core::delta::DeltaCommitInfo {
            sha: "abc123".to_string(),
            parent: "parent123".to_string(),
        },
        baseline: false,
        deltas: vec![critical_entry],
        policy: None,
        aggregates: None,
    };

    let config = ResolvedConfig::defaults().unwrap();

    let results = evaluate_policies(&delta, &config).unwrap().unwrap();

    // Should have NO blocking failures because function is suppressed
    assert_eq!(results.failed.len(), 0);
    assert_eq!(results.warnings.len(), 0); // No warnings because reason is provided
}

#[test]
fn test_unsuppressed_function_triggers_critical_introduction() {
    use hotspots_core::delta::FunctionState;
    use hotspots_core::report::MetricsReport;

    // Create a function that would trigger critical introduction (not suppressed)
    let critical_entry = FunctionDeltaEntry {
        function_id: "test.ts::critical".to_string(),
        status: FunctionStatus::New,
        before: None,
        after: Some(FunctionState {
            metrics: MetricsReport {
                cc: 20,
                nd: 10,
                fo: 5,
                ns: 3,
                loc: 50,
                missing_docstring: false,
            },
            lrs: 50.0,
            band: RiskBand::Critical,
        }),
        delta: None,
        band_transition: None,
        suppression_reason: None, // NOT suppressed
        rename_hint: None,
    };

    let delta = Delta {
        commit: hotspots_core::delta::DeltaCommitInfo {
            sha: "abc123".to_string(),
            parent: "parent123".to_string(),
        },
        baseline: false,
        deltas: vec![critical_entry],
        policy: None,
        aggregates: None,
    };

    let config = ResolvedConfig::defaults().unwrap();

    let results = evaluate_policies(&delta, &config).unwrap().unwrap();

    // Should have ONE blocking failure for critical introduction
    assert_eq!(results.failed.len(), 1);
    assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
    assert_eq!(results.failed[0].severity, PolicySeverity::Blocking);
}
