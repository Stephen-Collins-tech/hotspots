//! Integration tests for the diff pipeline.
//!
//! Covers Delta computation with two persisted snapshots, filtering,
//! --top sort order, JSONL serialization, and aggregate attachment.

use hotspots_core::delta::{Delta, FunctionStatus};
use hotspots_core::git::GitContext;
use hotspots_core::language::Language;
use hotspots_core::report::{FunctionRiskReport, MetricsReport, RiskReport};
use hotspots_core::risk::RiskBand;
use hotspots_core::snapshot::{self, Snapshot};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn git_ctx(sha: &str, parent: &str) -> GitContext {
    GitContext {
        head_sha: sha.to_string(),
        parent_shas: vec![parent.to_string()],
        timestamp: 1_705_600_000,
        branch: Some("main".to_string()),
        is_detached: false,
        message: Some("test".to_string()),
        author: Some("tester".to_string()),
        is_fix_commit: Some(false),
        is_revert_commit: Some(false),
        ticket_ids: vec![],
    }
}

fn make_report(file: &str, func: &str, cc: u32, lrs: f64, band: &str) -> FunctionRiskReport {
    FunctionRiskReport {
        file: file.to_string(),
        function: func.to_string(),
        line: 1,
        language: Language::TypeScript,
        metrics: MetricsReport {
            cc,
            nd: 1,
            fo: 1,
            ns: 1,
            loc: 20,
        },
        risk: RiskReport {
            r_cc: 1.0,
            r_nd: 1.0,
            r_fo: 1.0,
            r_ns: 1.0,
        },
        lrs,
        band: RiskBand::parse(band).unwrap_or(RiskBand::Low),
        suppression_reason: None,
        patterns: vec![],
        pattern_details: None,
        callees: vec![],
    }
}

fn persist_and_load(repo: &std::path::Path, snapshot: &Snapshot) -> Snapshot {
    snapshot::persist_snapshot(repo, snapshot, false).expect("persist failed");
    snapshot::load_snapshot(repo, snapshot.commit_sha())
        .expect("load failed")
        .expect("snapshot not found")
}

fn init_repo(dir: &std::path::Path) {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(["init"])
        .output()
        .expect("git init failed");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_diff_modified_function() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(
        git_ctx("base000", "root000"),
        vec![make_report("src/a.ts", "handler", 5, 3.0, "low")],
    );
    let head = Snapshot::new(
        git_ctx("head000", "base000"),
        vec![make_report("src/a.ts", "handler", 12, 7.5, "high")],
    );

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let delta = Delta::new(&head, Some(&base)).expect("delta failed");

    assert_eq!(delta.deltas.len(), 1);
    let entry = &delta.deltas[0];
    assert_eq!(entry.status, FunctionStatus::Modified);
    let d = entry.delta.as_ref().unwrap();
    assert!(d.cc > 0, "CC delta should be positive");
    assert!(d.lrs > 0.0, "LRS delta should be positive");
    let bt = entry.band_transition.as_ref().unwrap();
    assert_eq!(bt.from, "low");
    assert_eq!(bt.to, "high");
}

#[test]
fn test_diff_new_function() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(git_ctx("base001", "root001"), vec![]);
    let head = Snapshot::new(
        git_ctx("head001", "base001"),
        vec![make_report("src/b.ts", "newFn", 8, 5.0, "moderate")],
    );

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let delta = Delta::new(&head, Some(&base)).expect("delta failed");

    assert_eq!(delta.deltas.len(), 1);
    assert_eq!(delta.deltas[0].status, FunctionStatus::New);
    assert!(delta.deltas[0].before.is_none());
    assert!(delta.deltas[0].after.is_some());
}

#[test]
fn test_diff_deleted_function() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(
        git_ctx("base002", "root002"),
        vec![make_report("src/c.ts", "goneFunc", 6, 4.0, "moderate")],
    );
    let head = Snapshot::new(git_ctx("head002", "base002"), vec![]);

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let delta = Delta::new(&head, Some(&base)).expect("delta failed");

    assert_eq!(delta.deltas.len(), 1);
    assert_eq!(delta.deltas[0].status, FunctionStatus::Deleted);
    assert!(delta.deltas[0].before.is_some());
    assert!(delta.deltas[0].after.is_none());
}

#[test]
fn test_diff_unchanged_not_present_when_filtered() {
    // Unchanged functions should be filterable; Delta::new produces them but
    // the diff command removes them. Verify they appear in the raw delta so
    // the filter has something to act on.
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let report = make_report("src/d.ts", "stable", 4, 2.0, "low");
    let base = Snapshot::new(git_ctx("base003", "root003"), vec![report.clone()]);
    let head = Snapshot::new(git_ctx("head003", "base003"), vec![report]);

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let delta = Delta::new(&head, Some(&base)).expect("delta failed");

    // Raw delta contains the Unchanged entry
    assert_eq!(delta.deltas.len(), 1);
    assert_eq!(delta.deltas[0].status, FunctionStatus::Unchanged);

    // After filtering (as the diff command does), it's gone
    let mut filtered = delta.deltas.clone();
    filtered.retain(|e| e.status != FunctionStatus::Unchanged);
    assert!(filtered.is_empty(), "filtered delta should be empty");
}

#[test]
fn test_diff_top_sort_new_high_lrs_above_modified_small_delta() {
    // A newly-introduced function with LRS 9.0 should rank above a modified
    // function whose |ΔLRS| is only 0.5, even though modified functions rank
    // by delta magnitude and new functions rank by absolute LRS.
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(
        git_ctx("base004", "root004"),
        vec![make_report("src/e.ts", "tweaked", 5, 3.0, "low")],
    );
    let head = Snapshot::new(
        git_ctx("head004", "base004"),
        vec![
            make_report("src/e.ts", "tweaked", 5, 3.5, "low"), // ΔLRS = 0.5
            make_report("src/f.ts", "bigNew", 20, 9.0, "critical"), // new, LRS 9.0
        ],
    );

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let mut delta = Delta::new(&head, Some(&base)).expect("delta failed");
    delta
        .deltas
        .retain(|e| e.status != FunctionStatus::Unchanged);

    // Sort: New by after.lrs, Modified by |Δlrs|
    delta.deltas.sort_by(|a, b| {
        let score = |e: &hotspots_core::delta::FunctionDeltaEntry| match e.status {
            FunctionStatus::New => e.after.as_ref().map(|s| s.lrs).unwrap_or(0.0),
            FunctionStatus::Deleted => e.before.as_ref().map(|s| s.lrs).unwrap_or(0.0),
            FunctionStatus::Modified => e.delta.as_ref().map(|d| d.lrs.abs()).unwrap_or(0.0),
            FunctionStatus::Unchanged => 0.0,
        };
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    assert_eq!(delta.deltas.len(), 2);
    assert_eq!(
        delta.deltas[0].status,
        FunctionStatus::New,
        "new critical function should rank first"
    );
    assert!(
        delta.deltas[0].function_id.contains("bigNew"),
        "bigNew should be first"
    );
}

#[test]
fn test_diff_to_jsonl() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(
        git_ctx("base005", "root005"),
        vec![make_report("src/g.ts", "alpha", 3, 2.0, "low")],
    );
    let head = Snapshot::new(
        git_ctx("head005", "base005"),
        vec![
            make_report("src/g.ts", "alpha", 8, 5.0, "moderate"), // modified
            make_report("src/h.ts", "beta", 5, 3.0, "low"),       // new
        ],
    );

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let mut delta = Delta::new(&head, Some(&base)).expect("delta failed");
    delta
        .deltas
        .retain(|e| e.status != FunctionStatus::Unchanged);

    let jsonl = delta.to_jsonl().expect("to_jsonl failed");
    let lines: Vec<&str> = jsonl.lines().collect();

    assert_eq!(
        lines.len(),
        2,
        "JSONL should have one line per changed function"
    );

    // Each line should be valid JSON containing a function_id
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("line should be valid JSON");
        assert!(
            v.get("function_id").is_some(),
            "each JSONL entry should have function_id"
        );
        assert!(
            v.get("status").is_some(),
            "each JSONL entry should have status"
        );
    }
}

#[test]
fn test_diff_delta_aggregates_attached() {
    // Verify compute_delta_aggregates produces a non-None result when
    // called with empty co-change slices (the common case for diff).
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let base = Snapshot::new(
        git_ctx("base006", "root006"),
        vec![make_report("src/i.ts", "fn1", 4, 2.5, "low")],
    );
    let head = Snapshot::new(
        git_ctx("head006", "base006"),
        vec![make_report("src/i.ts", "fn1", 9, 6.0, "moderate")],
    );

    let base = persist_and_load(tmp.path(), &base);
    let head = persist_and_load(tmp.path(), &head);

    let mut delta = Delta::new(&head, Some(&base)).expect("delta failed");
    delta.aggregates = Some(hotspots_core::aggregates::compute_delta_aggregates(
        &delta,
        &[],
        &[],
    ));

    assert!(
        delta.aggregates.is_some(),
        "delta aggregates should be attached"
    );
    let agg = delta.aggregates.as_ref().unwrap();
    assert_eq!(
        agg.files.len(),
        1,
        "one file should appear in file-level aggregates"
    );
    assert_eq!(agg.files[0].file, "src/i.ts");
}
