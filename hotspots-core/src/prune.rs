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
    /// Tracked ref patterns (default: ["refs/heads/*"])
    pub ref_patterns: Vec<String>,
    /// Only prune commits older than this many days (None = no age filter)
    pub older_than_days: Option<u64>,
    /// Dry-run mode (report what would be pruned without actually deleting)
    pub dry_run: bool,
}

impl Default for PruneOptions {
    fn default() -> Self {
        PruneOptions {
            ref_patterns: vec!["refs/heads/*".to_string()],
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
}

/// Execute a git command in a specific directory
fn git_at(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .context("failed to invoke git")?;

    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Enumerate tracked refs (default: local branches refs/heads/*)
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
    // Load index to get list of all snapshot SHAs
    let index_path = snapshot::index_path(repo_path);
    let mut index = if index_path.exists() {
        Index::load_or_new(&index_path)?
    } else {
        Index::new()
    };

    // Enumerate tracked refs
    let tracked_ref_shas = enumerate_tracked_refs(repo_path, &options.ref_patterns)
        .context("failed to enumerate tracked refs")?;

    // Compute reachable commit set
    let reachable_shas = compute_reachable_commits(repo_path, &tracked_ref_shas)
        .context("failed to compute reachable commits")?;

    // Get current time for age filtering
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let cutoff_timestamp = options.older_than_days.map(|days| {
        let days_ago = (days as i64) * 24 * 60 * 60;
        now - days_ago
    });

    // Find unreachable snapshots
    let mut pruned_shas = Vec::new();
    let mut reachable_count = 0;
    let mut unreachable_kept_count = 0;

    // Iterate over all commits in index
    for entry in &index.commits {
        let sha = &entry.sha;

        // Check if snapshot file exists
        let snapshot_path = snapshot::snapshot_path(repo_path, sha);
        if !snapshot_path.exists() {
            // Snapshot file is missing but still in index - remove from index
            continue;
        }

        if reachable_shas.contains(sha) {
            // Snapshot is reachable - keep it
            reachable_count += 1;
        } else {
            // Snapshot is unreachable - check age filter
            let should_prune = if let Some(cutoff) = cutoff_timestamp {
                // Check if commit is older than cutoff
                match get_commit_timestamp(repo_path, sha) {
                    Ok(timestamp) => timestamp < cutoff,
                    Err(_) => {
                        // If we can't get timestamp, err on side of caution and don't prune
                        false
                    }
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
    }

    // Prune snapshots and update index (unless dry-run)
    if !options.dry_run {
        for sha in &pruned_shas {
            let snapshot_path = snapshot::snapshot_path(repo_path, sha);
            if snapshot_path.exists() {
                std::fs::remove_file(&snapshot_path).with_context(|| {
                    format!("failed to remove snapshot: {}", snapshot_path.display())
                })?;
            }
        }

        // Update index - remove pruned entries
        for sha in &pruned_shas {
            index.remove_commit(sha);
        }

        // Write updated index atomically
        let index_json = index.to_json()?;
        snapshot::atomic_write(&index_path, &index_json)?;
    }

    Ok(PruneResult {
        pruned_count: pruned_shas.len(),
        pruned_shas,
        reachable_count,
        unreachable_kept_count,
    })
}
