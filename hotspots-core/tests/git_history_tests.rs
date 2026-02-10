//! Git history integration tests - verify correctness under real git mutations
//!
//! Tests rebase, merge, cherry-pick, and revert operations to ensure
//! snapshots and deltas remain correct.
//!
//! Global test rules:
//! - Real git repos
//! - Temp directories
//! - No fixed SHAs
//! - Assert relationships only
//! - Fail loudly on invariant violation

use hotspots_core::{analyze, delta, git, snapshot, AnalysisOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a temporary git repository for testing
fn create_temp_git_repo() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("failed to create temp directory");
    let repo_path = temp_dir.path();

    // Initialize git repo with explicit branch name for portability
    git_command(repo_path, &["init", "--initial-branch=main"]);
    git_command(repo_path, &["config", "user.name", "Test User"]);
    git_command(repo_path, &["config", "user.email", "test@example.com"]);
    // Disable commit signing (may be configured globally in some environments)
    git_command(repo_path, &["config", "commit.gpgsign", "false"]);

    // Ensure .hotspots/ is git-ignored (important for force-push tests)
    let gitignore_path = repo_path.join(".gitignore");
    if !gitignore_path.exists() {
        std::fs::write(&gitignore_path, ".hotspots/\n").expect("failed to write .gitignore");
    }

    temp_dir
}

/// Run a git command in the repository
fn git_command(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .unwrap_or_else(|_| panic!("failed to run git {:?}", args));

    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Create a TypeScript file with content
fn create_ts_file(repo_path: &Path, path: &str, content: &str) {
    let file_path = repo_path.join(path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).expect("failed to create directory");
    }
    fs::write(&file_path, content).expect("failed to write file");
}

/// Commit changes to git repo
fn git_commit(repo_path: &Path, message: &str) -> String {
    git_command(repo_path, &["add", "."]);
    git_command(repo_path, &["commit", "-m", message]);
    git_command(repo_path, &["rev-parse", "HEAD"])
}

/// Get commit SHA
fn get_commit_sha(repo_path: &Path, ref_name: &str) -> String {
    git_command(repo_path, &["rev-parse", ref_name])
}

/// Verify snapshot exists for a commit
fn verify_snapshot_exists(repo_path: &Path, commit_sha: &str) -> bool {
    let snapshot_path = repo_path
        .join(".hotspots")
        .join("snapshots")
        .join(format!("{}.json", commit_sha));
    snapshot_path.exists()
}

/// Create snapshot for current commit in the specified repo
fn create_snapshot_for_commit(repo_path: &Path) -> snapshot::Snapshot {
    // Use extract_git_context_at to avoid changing the process-wide directory
    // This allows tests to run in parallel without interfering with each other
    let git_context =
        git::extract_git_context_at(repo_path).expect("failed to extract git context");

    // Create a simple TypeScript file if none exists
    let ts_files: Vec<PathBuf> = std::fs::read_dir(repo_path)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ts"))
                .collect()
        })
        .unwrap_or_default();

    let test_file = if ts_files.is_empty() {
        let test_file_path = repo_path.join("test.ts");
        fs::write(&test_file_path, "function test() { return 1; }")
            .expect("failed to write test file");
        test_file_path
    } else {
        ts_files[0].clone()
    };

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&test_file, options).expect("failed to analyze");

    snapshot::Snapshot::new(git_context, reports)
}

#[test]
fn test_rebase_creates_new_snapshots() {
    let temp_repo = create_temp_git_repo();
    let repo_path = temp_repo.path();

    // Create initial commit
    create_ts_file(repo_path, "file.ts", "function foo() { return 1; }");
    let _commit1 = git_commit(repo_path, "Initial commit");

    // Create snapshot for commit1
    let _snapshot1 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &_snapshot1).expect("failed to persist snapshot1");

    // Create branch and make changes
    git_command(repo_path, &["checkout", "-b", "feature"]);
    create_ts_file(repo_path, "feature.ts", "function feature() { return 2; }");
    let commit2 = git_commit(repo_path, "Feature change");

    // Create snapshot for commit2
    let _snapshot2 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &_snapshot2).expect("failed to persist snapshot2");

    // Rebase onto main (use different file to avoid conflicts)
    git_command(repo_path, &["checkout", "main"]);
    create_ts_file(repo_path, "main.ts", "function main() { return 10; }");
    let _commit3 = git_commit(repo_path, "Main change");
    git_command(repo_path, &["checkout", "feature"]);
    git_command(repo_path, &["rebase", "main"]);

    // After rebase, commit2 should be gone (new SHA)
    let new_commit2_sha = git_command(repo_path, &["rev-parse", "HEAD"]);

    // Verify original commit2 snapshot still exists (rebases create new history, not edits)
    assert!(
        verify_snapshot_exists(repo_path, &commit2),
        "Original commit snapshot should still exist after rebase"
    );

    // New commit (after rebase) should be different SHA
    assert_ne!(
        commit2, new_commit2_sha,
        "Rebased commit should have different SHA (rebases create new history)"
    );
}

#[test]
fn test_merge_uses_parent0() {
    let temp_repo = create_temp_git_repo();
    let repo_path = temp_repo.path();

    // Create initial commit
    create_ts_file(repo_path, "base.ts", "function base() { return 1; }");
    let _commit1 = git_commit(repo_path, "Initial commit");

    // Create snapshot for commit1
    let snapshot1 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &snapshot1).expect("failed to persist snapshot1");

    // Create branch and make changes (different file to avoid conflicts)
    git_command(repo_path, &["checkout", "-b", "feature"]);
    create_ts_file(repo_path, "feature.ts", "function feature() { return 2; }");
    let _commit2 = git_commit(repo_path, "Feature change");

    // Create snapshot for commit2
    let _snapshot2 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &_snapshot2).expect("failed to persist snapshot2");

    // Create merge commit (different file on main to avoid conflicts but ensure non-fast-forward)
    git_command(repo_path, &["checkout", "main"]);
    create_ts_file(repo_path, "main.ts", "function main() { return 10; }");
    let _commit3 = git_commit(repo_path, "Main change");

    // Force non-fast-forward merge with --no-ff
    git_command(
        repo_path,
        &["merge", "--no-ff", "feature", "-m", "Merge feature"],
    );
    let _merge_commit = git_command(repo_path, &["rev-parse", "HEAD"]);

    // Create snapshot for merge commit
    let snapshot_merge = create_snapshot_for_commit(repo_path);

    // Verify merge commit has multiple parents (non-fast-forward merge)
    let parent_count = snapshot_merge.commit.parents.len();
    assert!(
        parent_count >= 2,
        "Merge commit should have multiple parents (got {} parents)",
        parent_count
    );

    // Delta should use parent[0] only
    let delta = delta::compute_delta(repo_path, &snapshot_merge).expect("failed to compute delta");

    // Verify delta uses parent[0] (commit3, not commit2)
    assert_eq!(
        delta.commit.parent, snapshot_merge.commit.parents[0],
        "Delta should use parent[0] only for merge commits"
    );
}

#[test]
fn test_cherry_pick_creates_new_snapshot() {
    let temp_repo = create_temp_git_repo();
    let repo_path = temp_repo.path();

    // Create initial commit
    create_ts_file(repo_path, "base.ts", "function base() { return 1; }");
    let _commit1 = git_commit(repo_path, "Initial commit");

    // Create branch and make changes (different file to avoid conflicts)
    git_command(repo_path, &["checkout", "-b", "feature"]);
    create_ts_file(repo_path, "feature.ts", "function feature() { return 2; }");
    let commit2 = git_commit(repo_path, "Feature change");

    // Create snapshot for commit2
    let snapshot2 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &snapshot2).expect("failed to persist snapshot2");

    // Cherry-pick commit2 onto another branch (use different file to avoid conflicts)
    git_command(repo_path, &["checkout", "main"]);
    create_ts_file(repo_path, "main.ts", "function main() { return 10; }");
    let _commit3 = git_commit(repo_path, "Main change");

    git_command(repo_path, &["cherry-pick", &commit2]);
    let cherry_pick_commit = git_command(repo_path, &["rev-parse", "HEAD"]);

    // Cherry-pick creates new commit with different SHA
    assert_ne!(
        commit2, cherry_pick_commit,
        "Cherry-picked commit should have different SHA (creates new history)"
    );

    // Both snapshots should exist (original commit2 and new cherry-pick)
    assert!(
        verify_snapshot_exists(repo_path, &commit2),
        "Original commit snapshot should still exist"
    );

    // Create snapshot for cherry-pick commit
    let snapshot_cherry = create_snapshot_for_commit(repo_path);

    // Verify cherry-pick snapshot has correct parent (commit3, not commit2's parent)
    assert_eq!(
        snapshot_cherry.commit.parents[0],
        get_commit_sha(repo_path, "HEAD^"),
        "Cherry-pick should have new parent (not original commit's parent)"
    );
}

#[test]
fn test_revert_produces_negative_deltas() {
    let temp_repo = create_temp_git_repo();
    let repo_path = temp_repo.path();

    // Create initial commit
    create_ts_file(repo_path, "simple.ts", "function simple() { return 1; }");
    let _commit1 = git_commit(repo_path, "Initial commit");

    // Create snapshot for commit1
    let snapshot1 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &snapshot1).expect("failed to persist snapshot1");

    // Make change that increases complexity (more nesting = higher complexity)
    create_ts_file(
        repo_path,
        "simple.ts",
        "function simple() { if (true) { if (true) { if (true) { return 1; } } } return 1; }",
    );
    let commit2 = git_commit(repo_path, "Increase complexity");

    // Create snapshot for commit2
    let snapshot2 = create_snapshot_for_commit(repo_path);
    snapshot::persist_snapshot(repo_path, &snapshot2).expect("failed to persist snapshot2");

    // Revert commit2 (this should reduce complexity back)
    git_command(repo_path, &["revert", "--no-edit", "HEAD"]);
    let _revert_commit = git_command(repo_path, &["rev-parse", "HEAD"]);

    // Create snapshot for revert commit
    let snapshot_revert = create_snapshot_for_commit(repo_path);

    // Compute delta for revert (revert's parent is commit2, so we compare revert vs commit2)
    let delta = delta::compute_delta(repo_path, &snapshot_revert).expect("failed to compute delta");

    // Verify revert produces negative deltas (reverts complexity increase from commit2)
    // The revert reduces complexity back to the original simple state
    let has_negative_delta = delta.deltas.iter().any(|d| {
        d.delta
            .as_ref()
            .map(|del| del.cc < 0 || del.nd < 0 || del.lrs < 0.0)
            .unwrap_or(false)
    });

    assert!(
        has_negative_delta || delta.deltas.is_empty(),
        "Revert should produce negative deltas or no deltas if reverted to original state (delta: {:?})",
        delta
    );

    // Verify revert commit's parent is commit2 (not commit1)
    assert_eq!(
        snapshot_revert.commit.parents[0], commit2,
        "Revert commit's parent should be the reverted commit"
    );
}

#[test]
fn test_force_push_does_not_corrupt_history() {
    let temp_repo = create_temp_git_repo();
    let repo_path = temp_repo.path();

    // Create initial commit
    create_ts_file(repo_path, "simple.ts", "function simple() { return 1; }");
    let commit1 = git_commit(repo_path, "Initial commit");

    // Create snapshot for commit1
    let snapshot1 = create_snapshot_for_commit(repo_path);
    let snapshot1_sha = snapshot1.commit_sha().to_string();

    // Verify snapshot SHA matches commit1 (should be the same since both get HEAD)
    assert_eq!(
        snapshot1_sha, commit1,
        "snapshot1 SHA ({}) should match commit1 SHA ({})",
        snapshot1_sha, commit1
    );

    snapshot::persist_snapshot(repo_path, &snapshot1).expect("failed to persist snapshot1");

    // Verify snapshot file exists using snapshot's SHA (which should match commit1)
    let snapshot_path1 = snapshot::snapshot_path(repo_path, &snapshot1_sha);
    assert!(
        snapshot_path1.exists(),
        "snapshot1 should exist after persist: {}",
        snapshot_path1.display()
    );

    // Read snapshot1 content before reset for comparison
    let content1_before =
        std::fs::read_to_string(&snapshot_path1).expect("failed to read snapshot1 before reset");

    // Create new commit
    create_ts_file(repo_path, "simple.ts", "function simple() { return 2; }");
    let _commit2 = git_commit(repo_path, "Second commit");

    // Create snapshot for commit2
    let snapshot2 = create_snapshot_for_commit(repo_path);
    let snapshot2_sha = snapshot2.commit_sha().to_string();
    snapshot::persist_snapshot(repo_path, &snapshot2).expect("failed to persist snapshot2");

    // Verify snapshot2 exists
    let snapshot_path2 = snapshot::snapshot_path(repo_path, &snapshot2_sha);
    assert!(
        snapshot_path2.exists(),
        "snapshot2 should exist after persist: {}",
        snapshot_path2.display()
    );

    // Force-push to reset HEAD to commit1 (removes commit2 from git history, but not snapshots)
    // git reset --hard does not affect .hotspots/ directory (which is git-ignored)
    git_command(repo_path, &["reset", "--hard", &commit1]);

    // After reset, verify snapshot files still exist by reconstructing paths
    // (This ensures we're checking the actual file system state, not cached PathBuf)
    let snapshot_path1_check = snapshot::snapshot_path(repo_path, &snapshot1_sha);
    let snapshot_path2_check = snapshot::snapshot_path(repo_path, &snapshot2_sha);

    // Check if .hotspots directory still exists
    let faultline_dir = repo_path.join(".hotspots");
    assert!(
        faultline_dir.exists(),
        ".hotspots directory should still exist after reset: {}",
        faultline_dir.display()
    );

    // Check if snapshots directory still exists
    let snapshots_dir = snapshot::snapshots_dir(repo_path);
    assert!(
        snapshots_dir.exists(),
        "snapshots directory should still exist after reset: {}",
        snapshots_dir.display()
    );

    // Verify snapshot files exist after reset
    assert!(
        snapshot_path1_check.exists(),
        "snapshot1 should still exist after reset (snapshots are immutable): {}",
        snapshot_path1_check.display()
    );
    assert!(
        snapshot_path2_check.exists(),
        "snapshot2 should still exist after reset (history not corrupted by git operations): {}",
        snapshot_path2_check.display()
    );

    // Verify snapshot content is unchanged (immutability)
    let content1_after = std::fs::read_to_string(&snapshot_path1_check)
        .expect("failed to read snapshot1 after reset");
    assert_eq!(
        content1_before, content1_after,
        "snapshot1 content must be unchanged after reset (immutability)"
    );
}
