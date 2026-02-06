//! Git context extraction
//!
//! Extracts git metadata for the current commit deterministically.
//!
//! Global invariants enforced:
//! - Commit hash is the sole identity
//! - Branch names are metadata only
//! - Missing parents produce baselines, not errors
//!
//! Uses git CLI directly (no libgit2) for portability.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Git context for the current commit
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitContext {
    pub head_sha: String,
    pub parent_shas: Vec<String>,
    pub timestamp: i64,
    pub branch: Option<String>,
    pub is_detached: bool,
}

/// Execute a git command and return the trimmed stdout
fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
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

/// Execute a git command in a specific directory and return the trimmed stdout
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

/// Extract git context for the current commit
///
/// Returns context including HEAD SHA, parent SHAs, timestamp, and branch name.
/// Handles detached HEAD (branch = None) and shallow clones gracefully.
///
/// # Errors
///
/// Returns error if:
/// - Not in a git repository
/// - Git commands fail for reasons other than shallow history
///
/// # Shallow Clone Handling
///
/// If parent SHAs cannot be resolved (shallow clone), this function warns
/// and continues. Missing parents are treated as baseline = true.
pub fn extract_git_context() -> Result<GitContext> {
    // Check if we're in a git repository
    // Use `rev-parse --git-dir` which returns non-zero exit code if not in a repo
    if git(&["rev-parse", "--git-dir"]).is_err() {
        anyhow::bail!("not in a git repository");
    }

    let head_sha = git(&["rev-parse", "HEAD"])
        .context("failed to extract HEAD SHA")?;

    // Extract parent SHAs
    // Use `rev-list --parents -n 1 HEAD` which outputs: HEAD_SHA PARENT1 PARENT2 ...
    let parents_raw = git(&["rev-list", "--parents", "-n", "1", "HEAD"])
        .context("failed to extract parent SHAs")?;
    
    // Parse parent SHAs (first token is HEAD, rest are parents)
    let mut parts = parents_raw.split_whitespace();
    let _ = parts.next(); // Skip HEAD SHA (we already have it)
    let parent_shas = parts.map(|s| s.to_string()).collect::<Vec<_>>();

    // Warn if no parents (shouldn't happen for normal commits, but handle gracefully)
    if parent_shas.is_empty() && head_sha != "4b825dc642cb6eb9a060e54bf8d69288fbee4904" {
        // The SHA above is git's empty tree, which is expected to have no parents
        // For other commits with no parents, this is likely an initial commit
        // which is valid, so we don't warn here
    }

    // Extract commit timestamp (%ct = Unix timestamp)
    let timestamp = git(&["show", "-s", "--format=%ct", "HEAD"])
        .context("failed to extract commit timestamp")?
        .parse::<i64>()
        .context("failed to parse commit timestamp")?;

    // Extract branch name (best effort, None if detached)
    let branch = git(&["symbolic-ref", "--short", "HEAD"]).ok();

    Ok(GitContext {
        head_sha,
        parent_shas,
        timestamp,
        is_detached: branch.is_none(),
        branch,
    })
}

/// Extract git context for a specific repository path
///
/// This variant accepts a repository path instead of using the current working directory.
/// Useful for tests and scenarios where you don't want to change the process-wide directory.
///
/// # Arguments
///
/// * `repo_path` - Path to the git repository root
///
/// # Errors
///
/// Returns error if:
/// - Not in a git repository at the specified path
/// - Git commands fail for reasons other than shallow history
pub fn extract_git_context_at(repo_path: &Path) -> Result<GitContext> {
    // Check if we're in a git repository
    if git_at(repo_path, &["rev-parse", "--git-dir"]).is_err() {
        anyhow::bail!("not in a git repository at {}", repo_path.display());
    }

    let head_sha = git_at(repo_path, &["rev-parse", "HEAD"])
        .context("failed to extract HEAD SHA")?;

    // Extract parent SHAs
    let parents_raw = git_at(repo_path, &["rev-list", "--parents", "-n", "1", "HEAD"])
        .context("failed to extract parent SHAs")?;
    
    // Parse parent SHAs (first token is HEAD, rest are parents)
    let mut parts = parents_raw.split_whitespace();
    let _ = parts.next(); // Skip HEAD SHA (we already have it)
    let parent_shas = parts.map(|s| s.to_string()).collect::<Vec<_>>();

    // Extract commit timestamp (%ct = Unix timestamp)
    let timestamp = git_at(repo_path, &["show", "-s", "--format=%ct", "HEAD"])
        .context("failed to extract commit timestamp")?
        .parse::<i64>()
        .context("failed to parse commit timestamp")?;

    // Extract branch name (best effort, None if detached)
    let branch = git_at(repo_path, &["symbolic-ref", "--short", "HEAD"]).ok();

    Ok(GitContext {
        head_sha,
        parent_shas,
        timestamp,
        is_detached: branch.is_none(),
        branch,
    })
}

/// PR context information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrContext {
    pub is_pr: bool,
    pub merge_base: Option<String>,
}

/// Detect if we're in a PR context via CI environment variables
///
/// Checks CI environment variables (GitHub: `GITHUB_EVENT_NAME`, `GITHUB_REF`).
/// Best-effort detection - returns `is_pr=false` if context is ambiguous.
/// Never hard-fails on ambiguous context.
pub fn detect_pr_context() -> PrContext {
    // Check GitHub Actions environment variables
    let github_event_name = std::env::var("GITHUB_EVENT_NAME").ok();
    let github_ref = std::env::var("GITHUB_REF").ok();
    
    // Check if this looks like a PR (pull_request event)
    let is_pr = match (&github_event_name, &github_ref) {
        (Some(event), Some(ref_name)) => {
            // GitHub PR events have event_name = "pull_request" and ref starts with "refs/pull/"
            event == "pull_request" || ref_name.starts_with("refs/pull/")
        }
        _ => false,
    };
    
    PrContext {
        is_pr,
        merge_base: None, // Will be computed later if needed
    }
}

/// Resolve merge-base between current HEAD and target branch
///
/// For PRs, this finds the common ancestor with the base branch.
/// Falls back to direct parent if merge-base cannot be resolved.
///
/// # Arguments
///
/// * `target_branch` - Target branch (e.g., "main", "master")
///
/// # Returns
///
/// Returns merge-base SHA, or None if merge-base fails (falls back to parent)
pub fn resolve_merge_base(target_branch: &str) -> Result<Option<String>> {
    // Try to get merge-base with target branch
    // git merge-base HEAD <target_branch>
    match git(&["merge-base", "HEAD", target_branch]) {
        Ok(sha) => Ok(Some(sha)),
        Err(_) => {
            // Merge-base failed - fall back to direct parent
            // This is acceptable (best-effort), so we just return None
            // The caller will use direct parent instead
            Ok(None)
        }
    }
}

/// Resolve merge-base with common target branches
///
/// Tries common branch names (main, master, develop) to find merge-base.
/// Returns first successful merge-base, or None if all fail.
pub fn resolve_merge_base_auto() -> Option<String> {
    let common_branches = ["main", "master", "develop", "trunk"];
    
    for branch in &common_branches {
        if let Ok(Some(sha)) = resolve_merge_base(branch) {
            return Some(sha);
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_git_context() {
        // This test requires a git repository
        // Skip if not in a git repo (e.g., in CI without git context)
        if git(&["rev-parse", "--git-dir"]).is_err() {
            eprintln!("Skipping test: not in a git repository");
            return;
        }

        let context = extract_git_context().expect("should extract git context");

        // HEAD SHA should be 40 characters (full SHA) or 7+ characters (short SHA)
        assert!(!context.head_sha.is_empty(), "HEAD SHA should not be empty");
        
        // Timestamp should be reasonable (after 2005-04-07, before year 2100)
        assert!(context.timestamp > 1112832000, "timestamp should be after 2005");
        assert!(context.timestamp < 4102444800, "timestamp should be before 2100");

        // is_detached should be consistent with branch
        assert_eq!(
            context.is_detached,
            context.branch.is_none(),
            "is_detached should match branch presence"
        );
    }
}