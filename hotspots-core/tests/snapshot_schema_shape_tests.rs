//! Schema shape golden tests.
//!
//! `golden_tests.rs` pins per-function metric correctness (CC/ND/FO values per
//! language construct) against the bare `analyze <path>` array output. Nothing
//! pinned the *full* `--mode snapshot` envelope shape (`schema_version`,
//! `commit`, `analysis`, `functions`, `summary`, `aggregates` and its nested
//! `file_risk`/`modules`/`directories`/`co_change` objects) until this file.
//!
//! That gap let real drift go unnoticed in production: the GitHub Action
//! (`action/src/main.ts`) reads `result.violations[]` and
//! `policy.{failed,warnings}`, neither of which exists in current snapshot
//! output, and silently degrades to "no violations" instead of erroring.
//!
//! These tests assert the *set of keys present*, not exact values (which vary
//! with the analyzed repo) — so a field rename or removal fails loudly here
//! instead of drifting silently into a downstream consumer. When a schema
//! change is intentional (e.g. the 2.0 master-schema unification), update the
//! expected key sets below in the same PR as the schema change — that pairing
//! is the point of this test.

use hotspots_core::aggregates::{compute_agent_sections, compute_snapshot_aggregates_with_models};
use hotspots_core::language::Language;
use hotspots_core::report::MetricsReport;
use hotspots_core::risk::RiskBand;
use hotspots_core::snapshot::{
    AnalysisInfo, CommitInfo, FunctionSnapshot, Snapshot, SNAPSHOT_SCHEMA_VERSION,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn keys_of(value: &serde_json::Value) -> BTreeSet<String> {
    match value {
        serde_json::Value::Object(map) => map.keys().cloned().collect(),
        _ => panic!("expected a JSON object, got: {value}"),
    }
}

fn test_function(file: &str, line: u32, cc: u32) -> FunctionSnapshot {
    FunctionSnapshot {
        function_id: format!("{file}::f_{line}"),
        file: file.to_string(),
        line,
        language: Language::Rust,
        metrics: MetricsReport {
            cc,
            nd: 2,
            fo: 1,
            ns: 1,
            loc: 20,
        },
        lrs: cc as f64,
        band: RiskBand::Moderate,
        suppression_reason: None,
        churn: None,
        touch_count: None,
        days_since_last_change: None,
        callgraph: None,
        activity_risk: None,
        risk_factors: None,
        percentile: None,
        driver: None,
        driver_detail: None,
        quadrant: None,
        patterns: vec![],
        pattern_details: None,
        subsystem: None,
        authors_90d: None,
        directed_coupling: None,
        jaccard_label_stability: None,
        convention_bug_fix_count: None,
        burst_score: None,
        commit_count: None,
        author_count: None,
        author_entropy: None,
        isolation_rate: None,
        age_days: None,
        last_touch_days: None,
        newcomer_rate: None,
        explanation: None,
    }
}

fn build_test_snapshot() -> serde_json::Value {
    let repo_root = fixtures_dir();

    // Two functions in two different directories so compute_module_instability
    // has something to group, and compute_file_risk_views has >1 file.
    let functions = vec![
        test_function("rust/simple.rs", 1, 12),
        test_function("rust/loops.rs", 1, 3),
    ];

    let mut snapshot = Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        commit: CommitInfo {
            sha: "0".repeat(40),
            parents: vec![],
            timestamp: 0,
            branch: None,
            message: None,
            author: None,
            is_fix_commit: None,
            is_revert_commit: None,
            ticket_ids: vec![],
        },
        analysis: AnalysisInfo {
            scope: "full".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            formula_version: 1,
            touch_window_days: 365,
        },
        functions,
        summary: None,
        aggregates: None,
        triage: None,
        architecture: None,
        co_change: None,
    };

    let aggregates = compute_snapshot_aggregates_with_models(&snapshot, &repo_root, 90, 2, None);
    snapshot.aggregates = Some(aggregates);

    serde_json::to_value(&snapshot).expect("Snapshot must serialize to JSON")
}

#[test]
fn snapshot_envelope_top_level_shape() {
    let json = build_test_snapshot();
    let expected: BTreeSet<String> = [
        "schema_version",
        "commit",
        "analysis",
        "functions",
        "aggregates",
        // `summary` is `None` here (only populated by the CLI's own summary
        // builder, not this library-level call) and is skipped by serde's
        // `skip_serializing_if` — correctly absent, not a gap in this test.
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(
        keys_of(&json),
        expected,
        "Snapshot envelope's top-level keys changed. If this is an intentional \
         schema change (e.g. a master-schema version bump), update every \
         downstream consumer (hotspots-content's shared/snapshot-schema.json, \
         hotspots-cloud's docs/CLI-CONTRACT.md and scripts/pipeline/snapshot.py, \
         hotspots-research's field reads) in the same release, then update the \
         expected set here."
    );
}

#[test]
fn snapshot_aggregates_file_risk_shape() {
    let json = build_test_snapshot();
    let aggregates = &json["aggregates"];

    // file_risk: per-file composite view. `file_churn` was removed in 1.42.1
    // (F170) -- its absence here is a regression guard against reintroducing
    // dead fields, not an oversight.
    let file_risk = aggregates["file_risk"]
        .as_array()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| panic!("expected non-empty aggregates.file_risk, got: {aggregates}"));
    let expected_file_risk: BTreeSet<String> = [
        "file",
        "function_count",
        "loc",
        "max_cc",
        "avg_cc",
        "critical_count",
        "file_risk_score",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        keys_of(&file_risk[0]),
        expected_file_risk,
        "aggregates.file_risk[] shape changed"
    );
}

#[test]
fn snapshot_aggregates_modules_shape() {
    let json = build_test_snapshot();
    let aggregates = &json["aggregates"];

    // modules: module-level risk view. module_risk gates on avg_complexity alone
    // as of 1.42.1 (F170); `instability` carried no measured signal (rho≈-0.002)
    // and was removed entirely in hotspots 2.0 (issue #247) -- its absence here
    // is a regression guard, not an oversight. `afferent`/`efferent` remain as
    // descriptive facts. This fixture has no real import edges, so modules may
    // be empty -- only assert shape when present; a separate,
    // repo-root-with-real-imports test would be needed to guarantee non-empty
    // modules, which is out of scope for a pure shape check.
    if let Some(modules) = aggregates.get("modules").and_then(|m| m.as_array()) {
        if let Some(first) = modules.first() {
            let expected_module: BTreeSet<String> = [
                "module",
                "file_count",
                "function_count",
                "avg_complexity",
                "afferent",
                "efferent",
                "module_risk",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            assert_eq!(
                keys_of(first),
                expected_module,
                "aggregates.modules[] shape changed"
            );
        }
    }
}

#[test]
fn snapshot_aggregates_optional_sections_absent_when_not_requested() {
    let json = build_test_snapshot();
    let aggregates = &json["aggregates"];

    // co_change / models: both optional/empty-skipped. This fixture has no git
    // repo and no model_source_root, so both must be absent or empty -- if
    // either starts appearing unconditionally, that's a real behavior change.
    let co_change_ok = aggregates
        .get("co_change")
        .map(|v| v.as_array().map(|a| a.is_empty()).unwrap_or(false))
        .unwrap_or(true);
    assert!(
        co_change_ok,
        "co_change should be absent or empty with no git repo"
    );

    assert!(
        aggregates.get("models").is_none(),
        "models were not requested (model_source_root=None); should be absent, not null"
    );
}

/// Pins the *other* half of the hotspots 2.0 envelope merge: the slimmed
/// default `analyze --mode snapshot --format json` output (no
/// `--all-functions`), which used to be an entirely separate struct
/// (`AgentSnapshotOutput`, schema_version 4) with no serde-level relationship
/// to `Snapshot`. Both now serialize through the same type and
/// `SNAPSHOT_SCHEMA_VERSION` — this test is the regression guard for that
/// merge, mirroring `snapshot_envelope_top_level_shape`'s role for the
/// `--all-functions` path above.
#[test]
fn snapshot_triage_envelope_shape() {
    let repo_root = fixtures_dir();
    let functions = vec![
        test_function("rust/simple.rs", 1, 12),
        test_function("rust/loops.rs", 1, 3),
    ];

    let mut snapshot = Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        commit: CommitInfo {
            sha: "0".repeat(40),
            parents: vec![],
            timestamp: 0,
            branch: None,
            message: None,
            author: None,
            is_fix_commit: None,
            is_revert_commit: None,
            ticket_ids: vec![],
        },
        analysis: AnalysisInfo {
            scope: "full".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            formula_version: 1,
            touch_window_days: 365,
        },
        functions,
        summary: None,
        aggregates: None,
        triage: None,
        architecture: None,
        co_change: None,
    };

    let aggregates = compute_snapshot_aggregates_with_models(&snapshot, &repo_root, 90, 2, None);
    let (triage, architecture, co_change) =
        compute_agent_sections(&snapshot, &aggregates, &repo_root);
    // Mirrors `emit_json_output`'s non-`--all-functions` branch in
    // `hotspots-cli/src/cmd/analyze.rs`: clear `functions`, leave `aggregates`
    // unset, populate the three agent sections instead.
    snapshot.functions.clear();
    snapshot.triage = Some(triage);
    snapshot.architecture = architecture;
    snapshot.co_change = Some(co_change);

    let json = serde_json::to_value(&snapshot).expect("Snapshot must serialize to JSON");

    // Both fixture files share a directory (`rust/`), so `aggregates.modules`
    // is non-empty and `architecture` is present. `functions` and `aggregates`
    // must still be absent: this is the slimmed shape, not the
    // `--all-functions` one.
    let expected: BTreeSet<String> = [
        "schema_version",
        "commit",
        "analysis",
        "triage",
        "architecture",
        "co_change",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(
        keys_of(&json),
        expected,
        "Triage-path Snapshot envelope's top-level keys changed. This is the \
         merged counterpart of snapshot_envelope_top_level_shape (the \
         `--all-functions` path) — update both together on any intentional \
         schema change, and update the same downstream consumers named in \
         that test's failure message."
    );

    let triage_json = &json["triage"];
    let expected_triage: BTreeSet<String> = ["fire", "debt", "watch", "ok"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        keys_of(triage_json),
        expected_triage,
        "triage shape changed"
    );

    let co_change_json = &json["co_change"];
    let expected_co_change: BTreeSet<String> = ["hidden_coupling", "hidden_count", "total_pairs"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        keys_of(co_change_json),
        expected_co_change,
        "co_change shape changed"
    );

    let architecture_json = &json["architecture"];
    let expected_architecture: BTreeSet<String> = ["file_risk", "modules"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(
        keys_of(architecture_json),
        expected_architecture,
        "architecture shape changed (this fixture has no model_source_root, so \
         `models` is expected to be absent — skip_serializing_if-omitted when None)"
    );
}
