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
    pub message: Option<String>,
    pub author: Option<String>,
    pub is_fix_commit: Option<bool>,
    pub is_revert_commit: Option<bool>,
    pub ticket_ids: Vec<String>,
}

/// File churn metrics (lines added/deleted)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChurn {
    pub file: String,
    pub lines_added: usize,
    pub lines_deleted: usize,
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

    let head_sha = git(&["rev-parse", "HEAD"]).context("failed to extract HEAD SHA")?;

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

    // Extract commit message and author
    let message = git(&["log", "-1", "--format=%B", "HEAD"]).ok();
    let author = git(&["log", "-1", "--format=%an", "HEAD"]).ok();

    // Detect fix and revert commits
    let is_fix_commit = message.as_ref().map(|m| detect_fix_commit(m));
    let is_revert_commit = message.as_ref().map(|m| detect_revert_commit(m));

    // Extract ticket IDs
    let ticket_ids = message.as_ref()
        .map(|m| extract_ticket_ids(m, branch.as_deref()))
        .unwrap_or_default();

    Ok(GitContext {
        head_sha,
        parent_shas,
        timestamp,
        is_detached: branch.is_none(),
        branch,
        message,
        author,
        is_fix_commit,
        is_revert_commit,
        ticket_ids,
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

    let head_sha =
        git_at(repo_path, &["rev-parse", "HEAD"]).context("failed to extract HEAD SHA")?;

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

    // Extract commit message and author
    let message = git_at(repo_path, &["log", "-1", "--format=%B", "HEAD"]).ok();
    let author = git_at(repo_path, &["log", "-1", "--format=%an", "HEAD"]).ok();

    // Detect fix and revert commits
    let is_fix_commit = message.as_ref().map(|m| detect_fix_commit(m));
    let is_revert_commit = message.as_ref().map(|m| detect_revert_commit(m));

    // Extract ticket IDs
    let ticket_ids = message.as_ref()
        .map(|m| extract_ticket_ids(m, branch.as_deref()))
        .unwrap_or_default();

    Ok(GitContext {
        head_sha,
        parent_shas,
        timestamp,
        is_detached: branch.is_none(),
        branch,
        message,
        author,
        is_fix_commit,
        is_revert_commit,
        ticket_ids,
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

/// Extract commit churn (lines added/deleted per file)
///
/// Uses `git show --numstat <sha>` to get churn data for all files in the commit.
/// Binary files (shown as `-\t-\t<file>`) are skipped.
///
/// # Arguments
///
/// * `sha` - Commit SHA to analyze
///
/// # Returns
///
/// Returns vector of FileChurn entries, one per changed file.
/// Returns empty vector for:
/// - Initial commits (no parent)
/// - Merge commits (uses first parent only)
/// - Commits with no file changes
///
/// # Errors
///
/// Returns error if git command fails or output cannot be parsed
pub fn extract_commit_churn(sha: &str) -> Result<Vec<FileChurn>> {
    // Use git show --numstat to get lines added/deleted per file
    // Format: <added>\t<deleted>\t<file>
    // Binary files show: -\t-\t<file>
    let output = match git(&["show", "--numstat", "--format=", sha]) {
        Ok(out) => out,
        Err(_) => {
            // If git show fails (e.g., initial commit with no parent), return empty
            return Ok(Vec::new());
        }
    };

    let mut churns = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue; // Invalid line, skip
        }

        let added_str = parts[0];
        let deleted_str = parts[1];
        let file = parts[2].to_string();

        // Skip binary files (shown as -)
        if added_str == "-" || deleted_str == "-" {
            continue;
        }

        // Parse numbers
        let lines_added = added_str
            .parse::<usize>()
            .with_context(|| format!("Failed to parse lines added: {}", added_str))?;
        let lines_deleted = deleted_str
            .parse::<usize>()
            .with_context(|| format!("Failed to parse lines deleted: {}", deleted_str))?;

        churns.push(FileChurn {
            file,
            lines_added,
            lines_deleted,
        });
    }

    Ok(churns)
}

/// Extract commit churn at a specific repository path
///
/// Like `extract_commit_churn`, but operates on a repository at a specific path.
///
/// # Arguments
///
/// * `repo_path` - Path to git repository
/// * `sha` - Commit SHA to analyze
///
/// # Returns
///
/// Returns vector of FileChurn entries for the commit
pub fn extract_commit_churn_at(repo_path: &Path, sha: &str) -> Result<Vec<FileChurn>> {
    let output = match git_at(repo_path, &["show", "--numstat", "--format=", sha]) {
        Ok(out) => out,
        Err(_) => {
            return Ok(Vec::new());
        }
    };

    let mut churns = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let added_str = parts[0];
        let deleted_str = parts[1];
        let file = parts[2].to_string();

        if added_str == "-" || deleted_str == "-" {
            continue;
        }

        let lines_added = added_str
            .parse::<usize>()
            .with_context(|| format!("Failed to parse lines added: {}", added_str))?;
        let lines_deleted = deleted_str
            .parse::<usize>()
            .with_context(|| format!("Failed to parse lines deleted: {}", deleted_str))?;

        churns.push(FileChurn {
            file,
            lines_added,
            lines_deleted,
        });
    }

    Ok(churns)
}

/// Count how many commits touched a file in the last 30 days
///
/// Counts commits relative to a specific timestamp (typically the commit timestamp),
/// not wall clock time. This allows deterministic analysis of historical commits.
///
/// # Arguments
///
/// * `file` - Relative path to file from repository root
/// * `as_of_timestamp` - Unix timestamp to use as "now" (typically commit timestamp)
///
/// # Returns
///
/// Returns count of commits that modified the file in the 30 days before `as_of_timestamp`.
/// Returns 0 if file has no commits in the window or doesn't exist.
pub fn count_file_touches_30d(file: &str, as_of_timestamp: i64) -> Result<usize> {
    // Calculate 30 days before the reference timestamp
    let thirty_days_ago = as_of_timestamp - (30 * 24 * 60 * 60);

    // Use git log with --since and --until to get commits in the window
    // Format: --since=<timestamp> --until=<timestamp>
    let since_arg = format!("--since={}", thirty_days_ago);
    let until_arg = format!("--until={}", as_of_timestamp);

    let output = match git(&["log", &since_arg, &until_arg, "--oneline", "--", file]) {
        Ok(out) => out,
        Err(_) => {
            // File doesn't exist or no commits in window
            return Ok(0);
        }
    };

    // Count non-empty lines
    let count = output.lines().filter(|line| !line.trim().is_empty()).count();
    Ok(count)
}

/// Count file touches at a specific repository path
pub fn count_file_touches_30d_at(repo_path: &Path, file: &str, as_of_timestamp: i64) -> Result<usize> {
    let thirty_days_ago = as_of_timestamp - (30 * 24 * 60 * 60);
    let since_arg = format!("--since={}", thirty_days_ago);
    let until_arg = format!("--until={}", as_of_timestamp);

    let output = match git_at(repo_path, &["log", &since_arg, &until_arg, "--oneline", "--", file]) {
        Ok(out) => out,
        Err(_) => {
            return Ok(0);
        }
    };

    let count = output.lines().filter(|line| !line.trim().is_empty()).count();
    Ok(count)
}

/// Calculate days since last change to a file
///
/// Returns number of days between the file's last modification and the reference timestamp.
///
/// # Arguments
///
/// * `file` - Relative path to file from repository root
/// * `as_of_timestamp` - Unix timestamp to use as "now" (typically commit timestamp)
///
/// # Returns
///
/// Returns days since last change, or 0 if file was just modified.
/// Returns 0 if file doesn't exist or has no history.
pub fn days_since_last_change(file: &str, as_of_timestamp: i64) -> Result<u32> {
    // Get the timestamp of the most recent commit that modified this file
    // Use: git log -1 --format=%ct -- <file>
    let output = match git(&["log", "-1", "--format=%ct", "--", file]) {
        Ok(out) => out,
        Err(_) => {
            // File doesn't exist or has no history
            return Ok(0);
        }
    };

    let last_change_timestamp = output.trim().parse::<i64>()
        .context("failed to parse last change timestamp")?;

    // Calculate days difference
    let seconds_diff = as_of_timestamp - last_change_timestamp;
    let days = (seconds_diff / (24 * 60 * 60)).max(0) as u32;

    Ok(days)
}

/// Calculate days since last change at a specific repository path
pub fn days_since_last_change_at(repo_path: &Path, file: &str, as_of_timestamp: i64) -> Result<u32> {
    let output = match git_at(repo_path, &["log", "-1", "--format=%ct", "--", file]) {
        Ok(out) => out,
        Err(_) => {
            return Ok(0);
        }
    };

    let last_change_timestamp = output.trim().parse::<i64>()
        .context("failed to parse last change timestamp")?;

    let seconds_diff = as_of_timestamp - last_change_timestamp;
    let days = (seconds_diff / (24 * 60 * 60)).max(0) as u32;

    Ok(days)
}

/// Detect if a commit message indicates a fix/bug fix
///
/// Looks for common keywords: "fix", "bug", "hotfix", "bugfix", etc.
pub fn detect_fix_commit(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("fix") || lower.contains("bug") || lower.contains("hotfix") || lower.contains("bugfix")
}

/// Detect if a commit is a revert
///
/// Looks for "revert" keyword in the message
pub fn detect_revert_commit(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("revert")
}

/// Extract ticket IDs from commit message and branch name
///
/// Supports formats:
/// - JIRA-1234
/// - ABC-123
/// - #123
/// - fixes #123
/// - closes #123
pub fn extract_ticket_ids(message: &str, branch: Option<&str>) -> Vec<String> {
    use regex::Regex;

    let mut tickets = Vec::new();

    // Pattern for JIRA-style tickets: PROJECT-123
    let jira_re = Regex::new(r"([A-Z]+-\d+)").unwrap();
    for cap in jira_re.captures_iter(message) {
        if let Some(ticket) = cap.get(1) {
            let ticket_str = ticket.as_str().to_string();
            if !tickets.contains(&ticket_str) {
                tickets.push(ticket_str);
            }
        }
    }

    // Pattern for GitHub issues: #123, fixes #123, closes #123
    let github_re = Regex::new(r"(?:fixes|closes|fixed|closed)?\s*#(\d+)").unwrap();
    for cap in github_re.captures_iter(message) {
        if let Some(number) = cap.get(1) {
            let ticket_str = format!("#{}", number.as_str());
            if !tickets.contains(&ticket_str) {
                tickets.push(ticket_str);
            }
        }
    }

    // Also extract from branch name if provided
    if let Some(branch_name) = branch {
        for cap in jira_re.captures_iter(branch_name) {
            if let Some(ticket) = cap.get(1) {
                let ticket_str = ticket.as_str().to_string();
                if !tickets.contains(&ticket_str) {
                    tickets.push(ticket_str);
                }
            }
        }

        for cap in github_re.captures_iter(branch_name) {
            if let Some(number) = cap.get(1) {
                let ticket_str = format!("#{}", number.as_str());
                if !tickets.contains(&ticket_str) {
                    tickets.push(ticket_str);
                }
            }
        }
    }

    tickets
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
        assert!(
            context.timestamp > 1112832000,
            "timestamp should be after 2005"
        );
        assert!(
            context.timestamp < 4102444800,
            "timestamp should be before 2100"
        );

        // is_detached should be consistent with branch
        assert_eq!(
            context.is_detached,
            context.branch.is_none(),
            "is_detached should match branch presence"
        );
    }

    #[test]
    fn test_extract_commit_churn() {
        // Skip if not in a git repo
        if git(&["rev-parse", "--git-dir"]).is_err() {
            eprintln!("Skipping test: not in a git repository");
            return;
        }

        let context = match extract_git_context() {
            Ok(ctx) => ctx,
            Err(_) => {
                eprintln!("Skipping test: could not extract git context");
                return;
            }
        };

        // Skip if this is an initial commit (no parents)
        if context.parent_shas.is_empty() {
            eprintln!("Skipping test: initial commit has no churn");
            return;
        }

        // Extract churn for HEAD
        let churns = extract_commit_churn(&context.head_sha).expect("should extract churn");

        // Churn should be a list (may be empty for commits with no changes)
        // We just verify it doesn't error
        println!("Extracted {} file churns for HEAD", churns.len());

        // Validate structure if we have any churns
        for churn in &churns {
            assert!(!churn.file.is_empty(), "file path should not be empty");
            // lines_added and lines_deleted can be 0, so just check they parse
        }
    }
}
