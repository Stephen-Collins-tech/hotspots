//! CI Invariant Tests
//!
//! These tests explicitly validate critical invariants that must always hold.
//! Run in CI to prevent regressions.

use hotspots_core::{delta, snapshot, git};
use hotspots_core::report::{FunctionRiskReport, MetricsReport, RiskReport};
use tempfile::TempDir;

/// Create a test snapshot with given SHA
fn create_test_snapshot(sha: &str, parent_sha: &str) -> snapshot::Snapshot {
    let git_context = git::GitContext {
        head_sha: sha.to_string(),
        parent_shas: vec![parent_sha.to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
    };
    
    let report = FunctionRiskReport {
        file: "src/foo.ts".to_string(),
        function: "handler".to_string(),
        line: 42,
        metrics: MetricsReport { cc: 5, nd: 2, fo: 3, ns: 1 },
        risk: RiskReport {
            r_cc: 2.0,
            r_nd: 1.0,
            r_fo: 1.0,
            r_ns: 1.0,
        },
        lrs: 4.8,
        band: "moderate".to_string(),
        suppression_reason: None,
    };
    
    snapshot::Snapshot::new(git_context, vec![report])
}

#[test]
fn test_snapshot_immutability() {
    let temp_repo = TempDir::new().expect("failed to create temp directory");
    let repo_path = temp_repo.path();
    
    // Initialize git repo
    std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["init"])
        .output()
        .expect("failed to run git init");
    
    let snapshot = create_test_snapshot("abc123", "def456");
    let snapshot_path = snapshot::snapshot_path(repo_path, snapshot.commit_sha());
    
    // First persist should succeed
    snapshot::persist_snapshot(repo_path, &snapshot)
        .expect("first persist should succeed");
    
    // Read file content after first persist
    let first_content = std::fs::read_to_string(&snapshot_path)
        .expect("failed to read snapshot file");
    
    // Second persist with identical snapshot should succeed (idempotency)
    snapshot::persist_snapshot(repo_path, &snapshot)
        .expect("second persist with identical snapshot should succeed (idempotent)");
    
    // File content should be unchanged (immutability)
    let second_content = std::fs::read_to_string(&snapshot_path)
        .expect("failed to read snapshot file");
    assert_eq!(
        first_content, second_content,
        "snapshot file must not change when persisting identical snapshot (immutability)"
    );
}

#[test]
fn test_snapshot_byte_for_byte_determinism() {
    let snapshot1 = create_test_snapshot("abc123", "def456");
    let snapshot2 = create_test_snapshot("abc123", "def456");
    
    // Serialize both snapshots
    let json1 = snapshot1.to_json().expect("should serialize");
    let json2 = snapshot2.to_json().expect("should serialize");
    
    // Should be byte-for-byte identical
    assert_eq!(
        json1, json2,
        "identical snapshots must serialize to identical JSON (deterministic)"
    );
    
    // Verify deterministic ordering (run multiple times)
    let json3 = snapshot1.to_json().expect("should serialize");
    assert_eq!(
        json1, json3,
        "serialization must be deterministic across multiple calls"
    );
}

#[test]
fn test_snapshot_filename_equals_commit_sha() {
    let temp_repo = TempDir::new().expect("failed to create temp directory");
    let repo_path = temp_repo.path();
    
    // Initialize git repo
    std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["init"])
        .output()
        .expect("failed to run git init");
    
    let commit_sha = "abc123def456";
    let snapshot = create_test_snapshot(commit_sha, "def456");
    
    snapshot::persist_snapshot(repo_path, &snapshot)
        .expect("failed to persist snapshot");
    
    // Verify filename equals commit SHA
    let snapshot_path = snapshot::snapshot_path(repo_path, commit_sha);
    assert!(
        snapshot_path.exists(),
        "snapshot file should exist at path derived from commit SHA"
    );
    
    // Verify filename matches commit SHA exactly
    let expected_filename = format!("{}.json", commit_sha);
    let actual_filename = snapshot_path.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        actual_filename,
        expected_filename,
        "snapshot filename must equal commit SHA (filename is authoritative identity)"
    );
}

#[test]
fn test_delta_single_parent_only() {
    // Create snapshot with multiple parents (merge commit)
    let git_context = git::GitContext {
        head_sha: "merge123".to_string(),
        parent_shas: vec!["parent1".to_string(), "parent2".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
    };
    
    let report = FunctionRiskReport {
        file: "src/foo.ts".to_string(),
        function: "handler".to_string(),
        line: 42,
        metrics: MetricsReport { cc: 5, nd: 2, fo: 3, ns: 1 },
        risk: RiskReport {
            r_cc: 2.0,
            r_nd: 1.0,
            r_fo: 1.0,
            r_ns: 1.0,
        },
        lrs: 4.8,
        band: "moderate".to_string(),
        suppression_reason: None,
    };
    
    let merge_snapshot = snapshot::Snapshot::new(git_context, vec![report]);
    
    // Create parent snapshot for parent[0]
    let parent_snapshot = create_test_snapshot("parent1", "grandparent");
    
    // Compute delta - should use parent[0] only
    let delta = delta::Delta::new(&merge_snapshot, Some(&parent_snapshot))
        .expect("should compute delta");
    
    // Verify delta uses parent[0] only (not parent[1])
    assert_eq!(
        delta.commit.parent,
        "parent1",
        "delta must use parent[0] only, even for merge commits with multiple parents"
    );
}

#[test]
fn test_delta_baseline_handling_correct() {
    let snapshot = create_test_snapshot("abc123", "def456");
    
    // No parent - should be baseline
    let delta = delta::Delta::new(&snapshot, None)
        .expect("should create baseline delta");
    
    assert!(
        delta.baseline,
        "delta with no parent must have baseline=true"
    );
    
    // With parent - should not be baseline
    let parent = create_test_snapshot("def456", "grandparent");
    let delta = delta::Delta::new(&snapshot, Some(&parent))
        .expect("should create delta");
    
    assert!(
        !delta.baseline,
        "delta with parent must have baseline=false"
    );
}

#[test]
fn test_delta_negative_deltas_allowed() {
    let parent = create_test_snapshot("parent123", "grandparent");
    
    // Create current with lower metrics (should produce negative deltas)
    let git_context = git::GitContext {
        head_sha: "current123".to_string(),
        parent_shas: vec!["parent123".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
    };
    
    let report = FunctionRiskReport {
        file: "src/foo.ts".to_string(),
        function: "handler".to_string(),
        line: 42,
        metrics: MetricsReport { cc: 3, nd: 1, fo: 1, ns: 0 }, // Lower than parent
        risk: RiskReport {
            r_cc: 2.0,
            r_nd: 1.0,
            r_fo: 1.0,
            r_ns: 1.0,
        },
        lrs: 2.5, // Lower than parent
        band: "low".to_string(),
        suppression_reason: None,
    };
    
    let current = snapshot::Snapshot::new(git_context, vec![report]);
    
    let delta = delta::Delta::new(&current, Some(&parent))
        .expect("should create delta");
    
    let delta_values = delta.deltas[0].delta.as_ref().unwrap();
    assert!(
        delta_values.cc < 0,
        "negative deltas must be allowed (valid for reverts, refactors)"
    );
    assert!(
        delta_values.lrs < 0.0,
        "negative LRS deltas must be allowed"
    );
}

#[test]
fn test_delta_deleted_functions_explicit() {
    let parent = create_test_snapshot("parent123", "grandparent");
    
    // Current has no functions (all deleted)
    let git_context = git::GitContext {
        head_sha: "current123".to_string(),
        parent_shas: vec!["parent123".to_string()],
        timestamp: 1705600000,
        branch: Some("main".to_string()),
        is_detached: false,
    };
    let current = snapshot::Snapshot::new(git_context, vec![]);
    
    let delta = delta::Delta::new(&current, Some(&parent))
        .expect("should create delta");
    
    assert_eq!(
        delta.deltas.len(), 1,
        "deleted function must appear in delta"
    );
    assert_eq!(
        delta.deltas[0].status,
        delta::FunctionStatus::Deleted,
        "deleted function status must be explicit"
    );
    assert!(
        delta.deltas[0].before.is_some(),
        "deleted function must have 'before' state"
    );
    assert!(
        delta.deltas[0].after.is_none(),
        "deleted function must have no 'after' state"
    );
}
