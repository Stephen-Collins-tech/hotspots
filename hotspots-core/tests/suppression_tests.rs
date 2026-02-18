//! Integration tests for suppression comments

use hotspots_core::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use hotspots_core::discover;
use hotspots_core::parser;
use hotspots_core::policy::{evaluate_policies, PolicyId, PolicySeverity};
use hotspots_core::snapshot::Snapshot;
use hotspots_core::{git::GitContext, ResolvedConfig};
use std::path::Path;
use swc_common::{sync::Lrc, SourceMap};

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

    let cm: Lrc<SourceMap> = Default::default();
    let module = parser::parse_source(source, &cm, "test.ts").unwrap();
    let functions = discover::discover_functions(&module, 0, source, &cm);

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

    let cm: Lrc<SourceMap> = Default::default();
    let module = parser::parse_source(source, &cm, "test.ts").unwrap();
    let functions = discover::discover_functions(&module, 0, source, &cm);

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

    let cm: Lrc<SourceMap> = Default::default();
    let module = parser::parse_source(source, &cm, "test.ts").unwrap();
    let functions = discover::discover_functions(&module, 0, source, &cm);

    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].suppression_reason, None);
}

#[test]
fn test_suppression_missing_reason_policy() {
    use hotspots_core::delta::FunctionState;
    use hotspots_core::report::MetricsReport;

    let delta_entry = FunctionDeltaEntry {
        function_id: "test.ts::foo".to_string(),
        status: FunctionStatus::New,
        before: None,
        after: Some(FunctionState {
            metrics: MetricsReport {
                cc: 1,
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 10,
            },
            lrs: 1.0,
            band: "low".to_string(),
        }),
        delta: None,
        band_transition: None,
        suppression_reason: Some(String::new()), // Empty reason
        rename_hint: None,
    };

    let delta = Delta {
        schema_version: 1,
        commit: hotspots_core::delta::DeltaCommitInfo {
            sha: "abc123".to_string(),
            parent: "parent123".to_string(),
        },
        baseline: false,
        deltas: vec![delta_entry],
        policy: None,
        aggregates: None,
    };

    let git_context = GitContext {
        head_sha: "abc123".to_string(),
        parent_shas: vec!["parent123".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
        message: Some("test commit".to_string()),
        author: Some("Test Author".to_string()),
        is_fix_commit: Some(false),
        is_revert_commit: Some(false),
        ticket_ids: vec![],
    };

    let snapshot = Snapshot::new(git_context, vec![]);
    let config = ResolvedConfig::defaults().unwrap();

    let results = evaluate_policies(&delta, &snapshot, Path::new("."), &config)
        .unwrap()
        .unwrap();

    // Should have one warning for missing reason
    assert_eq!(results.warnings.len(), 1);
    assert_eq!(results.warnings[0].id, PolicyId::SuppressionMissingReason);
    assert_eq!(results.warnings[0].severity, PolicySeverity::Warning);
    assert!(results.warnings[0]
        .message
        .contains("suppressed without reason"));
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
            },
            lrs: 50.0,
            band: "critical".to_string(),
        }),
        delta: None,
        band_transition: None,
        suppression_reason: Some("legacy code, will refactor".to_string()), // Suppressed with reason
        rename_hint: None,
    };

    let delta = Delta {
        schema_version: 1,
        commit: hotspots_core::delta::DeltaCommitInfo {
            sha: "abc123".to_string(),
            parent: "parent123".to_string(),
        },
        baseline: false,
        deltas: vec![critical_entry],
        policy: None,
        aggregates: None,
    };

    let git_context = GitContext {
        head_sha: "abc123".to_string(),
        parent_shas: vec!["parent123".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
        message: Some("test commit".to_string()),
        author: Some("Test Author".to_string()),
        is_fix_commit: Some(false),
        is_revert_commit: Some(false),
        ticket_ids: vec![],
    };

    let snapshot = Snapshot::new(git_context, vec![]);
    let config = ResolvedConfig::defaults().unwrap();

    let results = evaluate_policies(&delta, &snapshot, Path::new("."), &config)
        .unwrap()
        .unwrap();

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
            },
            lrs: 50.0,
            band: "critical".to_string(),
        }),
        delta: None,
        band_transition: None,
        suppression_reason: None, // NOT suppressed
        rename_hint: None,
    };

    let delta = Delta {
        schema_version: 1,
        commit: hotspots_core::delta::DeltaCommitInfo {
            sha: "abc123".to_string(),
            parent: "parent123".to_string(),
        },
        baseline: false,
        deltas: vec![critical_entry],
        policy: None,
        aggregates: None,
    };

    let git_context = GitContext {
        head_sha: "abc123".to_string(),
        parent_shas: vec!["parent123".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
        message: Some("test commit".to_string()),
        author: Some("Test Author".to_string()),
        is_fix_commit: Some(false),
        is_revert_commit: Some(false),
        ticket_ids: vec![],
    };

    let snapshot = Snapshot::new(git_context, vec![]);
    let config = ResolvedConfig::defaults().unwrap();

    let results = evaluate_policies(&delta, &snapshot, Path::new("."), &config)
        .unwrap()
        .unwrap();

    // Should have ONE blocking failure for critical introduction
    assert_eq!(results.failed.len(), 1);
    assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
    assert_eq!(results.failed[0].severity, PolicySeverity::Blocking);
}
