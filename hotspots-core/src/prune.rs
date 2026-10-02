//! Reachability pruning for snapshot history
//!
//! Bounds storage by removing snapshots that are unreachable from tracked refs.
//!
//! Global invariants enforced:
//! - Never prune reachable snapshots
//! - Index.json stays in sync with on-disk snapshots
//! - CI-friendly (no interactive prompts)

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::snapshot::{self, Index};

/// Pruning options
#[derive(Debug, Clone)]
pub struct PruneOptions {
    /// Tracked ref patterns (default: ["refs/heads/*", "refs/tags/*", "refs/remotes/*"])
    pub ref_patterns: Vec<String>,
    /// Only prune commits older than this many days (None = no age filter)
    pub older_than_days: Option<u64>,
    /// Dry-run mode (report what would be pruned without actually deleting)
    pub dry_run: bool,
}

impl Default for PruneOptions {
    fn default() -> Self {
        PruneOptions {
            ref_patterns: vec![
                "refs/heads/*".to_string(),
                "refs/tags/*".to_string(),
                "refs/remotes/*".to_string(),
            ],
            older_than_days: None,
            dry_run: false,
        }
    }
}

/// Pruning result
#[derive(Debug, Clone)]
pub struct PruneResult {
    /// Number of snapshots that would be pruned (or were pruned if not dry-run)
    pub pruned_count: usize,
    /// SHAs of pruned snapshots
    pub pruned_shas: Vec<String>,
    /// Number of snapshots that are reachable (kept)
    pub reachable_count: usize,
    /// Number of snapshots that are unreachable but not pruned (due to age filter)
    pub unreachable_kept_count: usize,
    /// Of `pruned_count`, how many referenced a commit SHA that no longer
    /// exists in the repository's git history at all (e.g. after a rebase or
    /// force-push rewrote history). These are always eligible for pruning
    /// regardless of `--older-than`, since they can never become reachable
    /// again. See #226.
    pub orphaned_count: usize,
}

/// Environment variables git uses to locate a repository, bypassing normal
/// cwd-based discovery. If the calling process inherited one of these (e.g.
/// `GIT_DIR`, set by git itself for hook subprocesses), passing only
/// `current_dir` below is not enough to sandbox `git` to `repo_path` — a
/// var like `GIT_DIR` takes priority and silently redirects the command at
/// the *caller's* repository instead. Every call site here operates on a
/// repo identified explicitly by `repo_path`, so none of these should ever
/// be inherited.
const GIT_ENV_VARS_TO_CLEAR: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_CEILING_DIRECTORIES",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// Execute a git command in a specific directory
fn git_at(repo_path: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command.current_dir(repo_path).args(args);
    for var in GIT_ENV_VARS_TO_CLEAR {
        command.env_remove(var);
    }
    let output = command.output().context("failed to invoke git")?;

    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Enumerate tracked refs (default: refs/heads/*, refs/tags/*, refs/remotes/*)
///
/// Returns a list of commit SHAs pointed to by the tracked refs.
fn enumerate_tracked_refs(repo_path: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let mut ref_shas = Vec::new();

    for pattern in patterns {
        // Use `git for-each-ref` to list refs matching the pattern
        let refs_output = git_at(repo_path, &["for-each-ref", "--format=%(refname)", pattern])?;

        for ref_line in refs_output.lines() {
            let ref_name = ref_line.trim();
            if ref_name.is_empty() {
                continue;
            }

            // Resolve ref to commit SHA
            match git_at(repo_path, &["rev-parse", ref_name]) {
                Ok(sha) => ref_shas.push(sha),
                Err(_) => {
                    // Skip refs that don't resolve (orphaned refs)
                    continue;
                }
            }
        }
    }

    Ok(ref_shas)
}

/// Compute reachable commit set from starting SHAs
///
/// Uses `git rev-list` to traverse commit graph from all starting points.
fn compute_reachable_commits(
    repo_path: &Path,
    starting_shas: &[String],
) -> Result<HashSet<String>> {
    if starting_shas.is_empty() {
        return Ok(HashSet::new());
    }

    // Use `git rev-list --all` filtered to commits reachable from starting points
    // This is more efficient than calling rev-list for each ref separately
    let mut reachable = HashSet::new();

    for sha in starting_shas {
        let rev_list_output = git_at(repo_path, &["rev-list", sha])?;

        for line in rev_list_output.lines() {
            let commit_sha = line.trim();
            if !commit_sha.is_empty() {
                reachable.insert(commit_sha.to_string());
            }
        }
    }

    Ok(reachable)
}

/// Get commit timestamp for a commit SHA
fn get_commit_timestamp(repo_path: &Path, sha: &str) -> Result<i64> {
    let output = git_at(repo_path, &["show", "-s", "--format=%ct", sha])?;
    output
        .parse::<i64>()
        .with_context(|| format!("failed to parse commit timestamp for {}", sha))
}

/// Compute the Unix timestamp cutoff for age-based pruning
fn compute_cutoff_timestamp(older_than_days: Option<u64>) -> Option<i64> {
    older_than_days.map(|days| {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now - (days as i64) * 24 * 60 * 60
    })
}

/// Returns true if `sha` identifies a commit object that actually exists in
/// the repository's object database — false if history was rewritten (e.g.
/// rebase, force-push) and the commit is gone entirely, not merely
/// unreachable from tracked refs.
fn commit_exists(repo_path: &Path, sha: &str) -> bool {
    let mut command = Command::new("git");
    command
        .current_dir(repo_path)
        .args(["cat-file", "-e", &format!("{sha}^{{commit}}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for var in GIT_ENV_VARS_TO_CLEAR {
        command.env_remove(var);
    }
    command.status().map(|s| s.success()).unwrap_or(false)
}

/// Discover every SHA that has a snapshot on disk, whether or not it's
/// recorded in `index.json`. A file can exist without an index entry if the
/// index was manually edited/recovered, or a snapshot file was placed
/// directly — such orphaned entries would otherwise be invisible to pruning
/// since classification only ever walked `index.commits`. See #226.
fn discover_all_snapshot_shas(repo_path: &Path, index: &Index) -> HashSet<String> {
    let mut shas: HashSet<String> = index.commits.iter().map(|e| e.sha.clone()).collect();

    let dir = snapshot::snapshots_dir(repo_path);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(sha) = name
                .strip_suffix(".delta.json.zst")
                .or_else(|| name.strip_suffix(".json.zst"))
                .or_else(|| name.strip_suffix(".json"))
            {
                shas.insert(sha.to_string());
            }
        }
    }

    shas
}

/// Classify every known snapshot SHA into pruned / reachable / unreachable-kept
/// buckets. `all_shas` is the union of index entries and on-disk snapshot
/// files, so orphaned files with no index entry are still considered.
fn classify_snapshots(
    repo_path: &Path,
    all_shas: &HashSet<String>,
    reachable_shas: &HashSet<String>,
    cutoff_timestamp: Option<i64>,
) -> (Vec<String>, usize, usize, usize) {
    let mut pruned_shas = Vec::new();
    let mut reachable_count = 0;
    let mut unreachable_kept_count = 0;
    let mut orphaned_count = 0;

    let mut shas: Vec<&String> = all_shas.iter().collect();
    shas.sort();

    for sha in shas {
        let has_full = snapshot::snapshot_path_existing(repo_path, sha).is_some();
        let has_delta = snapshot::delta_snapshot_path(repo_path, sha).exists();
        if !has_full && !has_delta {
            continue;
        }

        if reachable_shas.contains(sha) {
            reachable_count += 1;
            continue;
        }

        // A commit absent from the repository's history entirely (rebase,
        // force-push, history rewrite) is unreachable by definition and can
        // never become reachable again — always eligible for pruning,
        // regardless of --older-than.
        if !commit_exists(repo_path, sha) {
            pruned_shas.push(sha.clone());
            orphaned_count += 1;
            continue;
        }

        let should_prune = if let Some(cutoff) = cutoff_timestamp {
            match get_commit_timestamp(repo_path, sha) {
                Ok(timestamp) => timestamp < cutoff,
                Err(_) => false,
            }
        } else {
            true
        };

        if should_prune {
            pruned_shas.push(sha.clone());
        } else {
            unreachable_kept_count += 1;
        }
    }

    (
        pruned_shas,
        reachable_count,
        unreachable_kept_count,
        orphaned_count,
    )
}

/// Delete snapshot files and update the index for pruned SHAs
fn delete_pruned_snapshots(
    repo_path: &Path,
    pruned_shas: &[String],
    index: &mut Index,
    index_path: &Path,
) -> Result<()> {
    for sha in pruned_shas {
        if let Some(path) = snapshot::snapshot_path_existing(repo_path, sha) {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove snapshot: {}", path.display()))?;
        }
        let delta_path = snapshot::delta_snapshot_path(repo_path, sha);
        if delta_path.exists() {
            std::fs::remove_file(&delta_path).with_context(|| {
                format!("failed to remove delta snapshot: {}", delta_path.display())
            })?;
        }
    }
    for sha in pruned_shas {
        index.remove_commit(sha);
    }
    let index_json = index.to_json()?;
    snapshot::atomic_write(index_path, &index_json)?;
    Ok(())
}

/// Prune unreachable snapshots
///
/// # Arguments
///
/// * `repo_path` - Repository root path
/// * `options` - Pruning options
///
/// # Errors
///
/// Returns error if:
/// - Git commands fail
/// - Snapshot files cannot be read/written
/// - Index cannot be updated
pub fn prune_unreachable(repo_path: &Path, options: PruneOptions) -> Result<PruneResult> {
    let index_path = snapshot::index_path(repo_path);
    let mut index = if index_path.exists() {
        Index::load_or_new(&index_path)?
    } else {
        Index::new()
    };

    let tracked_ref_shas = enumerate_tracked_refs(repo_path, &options.ref_patterns)
        .context("failed to enumerate tracked refs")?;
    let reachable_shas = compute_reachable_commits(repo_path, &tracked_ref_shas)
        .context("failed to compute reachable commits")?;
    let cutoff_timestamp = compute_cutoff_timestamp(options.older_than_days);

    let all_shas = discover_all_snapshot_shas(repo_path, &index);
    let (pruned_shas, reachable_count, unreachable_kept_count, orphaned_count) =
        classify_snapshots(repo_path, &all_shas, &reachable_shas, cutoff_timestamp);

    if !options.dry_run {
        delete_pruned_snapshots(repo_path, &pruned_shas, &mut index, &index_path)?;
    }

    Ok(PruneResult {
        pruned_count: pruned_shas.len(),
        pruned_shas,
        reachable_count,
        unreachable_kept_count,
        orphaned_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Initializes a temp git repo with one commit, tags it `v1.0`, then
    /// deletes the local branch pointing at it so the commit is only
    /// reachable via the tag (simulating a detached-HEAD checkout or a
    /// deleted release branch). Returns (tempdir, commit sha).
    fn repo_with_tag_only_commit() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let repo_path = dir.path();

        git_at(repo_path, &["init", "-q"]).expect("git init failed");
        git_at(repo_path, &["config", "user.email", "test@example.com"]).unwrap();
        git_at(repo_path, &["config", "user.name", "Test"]).unwrap();

        std::fs::write(repo_path.join("file.txt"), "hello").unwrap();
        git_at(repo_path, &["add", "file.txt"]).unwrap();
        git_at(repo_path, &["commit", "-q", "-m", "initial"]).unwrap();

        let sha = git_at(repo_path, &["rev-parse", "HEAD"]).unwrap();
        git_at(repo_path, &["tag", "v1.0"]).unwrap();

        // Detach HEAD, then delete every local branch so the commit is only
        // reachable via the tag.
        git_at(repo_path, &["checkout", "-q", "--detach"]).unwrap();
        let branches = git_at(
            repo_path,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/*"],
        )
        .unwrap();
        for branch in branches.lines().filter(|l| !l.is_empty()) {
            git_at(repo_path, &["branch", "-D", branch]).unwrap();
        }

        (dir, sha)
    }

    #[test]
    fn test_git_at_ignores_inherited_git_dir() {
        // Regression test: git_at is called from within `cargo test`, which
        // may itself be a child of `git commit`'s pre-commit hook — a
        // context where git sets GIT_DIR/GIT_WORK_TREE in the environment
        // for hook subprocesses. Before this fix, `git_at` only passed
        // `current_dir`, so an inherited GIT_DIR silently overrode it and
        // every "isolated" tempdir git command (git init, git config, git
        // commit) actually operated on whatever GIT_DIR pointed at instead
        // — this is exactly how a prune test once corrupted this repo's own
        // real .git/config with `user.email=test@example.com`.
        let real_repo = tempfile::tempdir().unwrap();
        git_at(real_repo.path(), &["init", "-q"]).unwrap();
        let real_git_dir = real_repo.path().join(".git");
        assert!(real_git_dir.exists());

        // SAFETY: single-threaded test process section; no other test reads
        // these vars. Simulate a hook-inherited GIT_DIR/GIT_WORK_TREE
        // pointing at `real_repo`, then run git_at against a *different*,
        // unrelated tempdir the way `repo_with_tag_only_commit` does.
        unsafe {
            std::env::set_var("GIT_DIR", real_git_dir.to_str().unwrap());
            std::env::set_var("GIT_WORK_TREE", real_repo.path().to_str().unwrap());
        }
        let result = std::panic::catch_unwind(|| {
            let victim = tempfile::tempdir().unwrap();
            git_at(victim.path(), &["init", "-q"]).unwrap();
            git_at(
                victim.path(),
                &["config", "user.email", "victim@example.com"],
            )
            .unwrap();
            victim
        });
        unsafe {
            std::env::remove_var("GIT_DIR");
            std::env::remove_var("GIT_WORK_TREE");
        }
        let victim = result.expect("git_at must not panic under an inherited GIT_DIR");

        // The victim tempdir must have its own independent .git with the
        // config write actually applied there...
        let victim_email = git_at(victim.path(), &["config", "user.email"]).unwrap();
        assert_eq!(victim_email, "victim@example.com");

        // ...and the "real" repo (simulating this actual project's .git)
        // must be completely untouched by the victim's git init/config.
        let real_config = std::fs::read_to_string(real_git_dir.join("config")).unwrap();
        assert!(
            !real_config.contains("victim@example.com"),
            "git_at leaked a nested test's git config into the inherited GIT_DIR's repo"
        );
    }

    #[test]
    fn test_default_ref_patterns_include_tags_and_remotes() {
        let patterns = PruneOptions::default().ref_patterns;
        assert!(patterns.contains(&"refs/heads/*".to_string()));
        assert!(patterns.contains(&"refs/tags/*".to_string()));
        assert!(patterns.contains(&"refs/remotes/*".to_string()));
    }

    #[test]
    fn test_heads_only_pattern_misses_tag_only_commit() {
        // Regression guard for hotspots#142: with the OLD default
        // (refs/heads/* only), a commit reachable only via a tag or a
        // deleted-locally-but-still-tagged branch is invisible to
        // reachability tracking — this is the exact silent-data-loss bug.
        let (dir, sha) = repo_with_tag_only_commit();
        let shas = enumerate_tracked_refs(dir.path(), &["refs/heads/*".to_string()]).unwrap();
        assert!(!shas.contains(&sha));
    }

    #[test]
    fn test_default_ref_patterns_find_tag_only_commit() {
        let (dir, sha) = repo_with_tag_only_commit();
        let shas =
            enumerate_tracked_refs(dir.path(), &PruneOptions::default().ref_patterns).unwrap();
        assert!(shas.contains(&sha));
    }

    #[test]
    fn test_prune_unreachable_keeps_tag_only_snapshot_by_default() {
        let (dir, sha) = repo_with_tag_only_commit();
        let repo_path = dir.path();

        // Write a snapshot for the tag-only commit directly to disk.
        let snapshot_path = snapshot::snapshot_path(repo_path, &sha);
        std::fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(&snapshot_path, "{}").unwrap();

        let mut index = Index::new();
        index.add_commit(snapshot::IndexEntry {
            sha: sha.clone(),
            parents: Vec::new(),
            timestamp: 0,
        });
        let index_json = index.to_json().unwrap();
        snapshot::atomic_write(&snapshot::index_path(repo_path), &index_json).unwrap();

        let result = prune_unreachable(repo_path, PruneOptions::default()).unwrap();

        assert_eq!(
            result.pruned_count, 0,
            "tag-only snapshot must not be pruned"
        );
        assert_eq!(result.reachable_count, 1);
        assert!(snapshot_path.exists());
    }

    /// Regression test for #226: a snapshot whose filename/index entry
    /// references a commit SHA that no longer exists anywhere in the
    /// repository's git history (simulating a rebase/force-push) must be
    /// treated as unreachable and pruned by `--unreachable`, not silently
    /// kept forever.
    #[test]
    fn test_prune_unreachable_removes_snapshot_for_nonexistent_commit() {
        let (dir, real_sha) = repo_with_tag_only_commit();
        let repo_path = dir.path();

        // A bogus SHA that was never a real commit in this repo (simulating
        // a snapshot recorded before a rebase/force-push rewrote history).
        let bogus_sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

        let real_snapshot_path = snapshot::snapshot_path(repo_path, &real_sha);
        std::fs::create_dir_all(real_snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(&real_snapshot_path, "{}").unwrap();

        let bogus_snapshot_path = snapshot::snapshot_path(repo_path, bogus_sha);
        std::fs::write(&bogus_snapshot_path, "{}").unwrap();

        let mut index = Index::new();
        index.add_commit(snapshot::IndexEntry {
            sha: real_sha.clone(),
            parents: Vec::new(),
            timestamp: 0,
        });
        index.add_commit(snapshot::IndexEntry {
            sha: bogus_sha.to_string(),
            parents: Vec::new(),
            timestamp: 0,
        });
        let index_json = index.to_json().unwrap();
        snapshot::atomic_write(&snapshot::index_path(repo_path), &index_json).unwrap();

        let result = prune_unreachable(repo_path, PruneOptions::default()).unwrap();

        assert_eq!(result.pruned_count, 1, "only the bogus sha should prune");
        assert!(result.pruned_shas.contains(&bogus_sha.to_string()));
        assert_eq!(result.orphaned_count, 1);
        assert_eq!(result.reachable_count, 1, "tag-only commit stays reachable");
        assert!(
            !bogus_snapshot_path.exists(),
            "snapshot for nonexistent commit must be deleted"
        );
        assert!(
            real_snapshot_path.exists(),
            "snapshot for the real, tag-reachable commit must survive"
        );
    }

    /// A snapshot file that exists on disk but has no corresponding entry in
    /// index.json must still be discovered and classified (see #226) rather
    /// than being invisible to pruning entirely.
    #[test]
    fn test_prune_discovers_orphan_snapshot_file_with_no_index_entry() {
        let dir = tempfile::tempdir().unwrap();
        let repo_path = dir.path();
        git_at(repo_path, &["init", "-q"]).unwrap();
        git_at(repo_path, &["config", "user.email", "test@example.com"]).unwrap();
        git_at(repo_path, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(repo_path.join("file.txt"), "hello").unwrap();
        git_at(repo_path, &["add", "file.txt"]).unwrap();
        git_at(repo_path, &["commit", "-q", "-m", "initial"]).unwrap();

        let bogus_sha = "feedfacefeedfacefeedfacefeedfacefeedface";
        let bogus_snapshot_path = snapshot::snapshot_path(repo_path, bogus_sha);
        std::fs::create_dir_all(bogus_snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(&bogus_snapshot_path, "{}").unwrap();
        // Deliberately no index.json entry for bogus_sha.

        let result = prune_unreachable(repo_path, PruneOptions::default()).unwrap();

        assert_eq!(result.pruned_count, 1);
        assert_eq!(result.orphaned_count, 1);
        assert!(!bogus_snapshot_path.exists());
    }
}
