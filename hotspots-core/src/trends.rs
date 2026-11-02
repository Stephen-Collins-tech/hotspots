//! Snapshot history window, used for the trend chart in the HTML report.

use crate::snapshot::{Index, Snapshot};
use anyhow::{Context, Result};
use std::path::Path;

/// Load snapshots from history using a sliding window
///
/// # Arguments
///
/// * `repo_root` - Repository root path
/// * `window_size` - Maximum number of snapshots to include
///
/// # Returns
///
/// Vector of snapshots ordered by commit timestamp (ascending), then SHA
pub fn load_snapshot_window(repo_root: &Path, window_size: usize) -> Result<Vec<Snapshot>> {
    let index_path = crate::snapshot::index_path(repo_root);
    let index = Index::load_or_new(&index_path).context("failed to load index")?;

    if index.commits.is_empty() {
        return Ok(Vec::new());
    }

    // Get last N commits (index is already sorted by timestamp, then SHA)
    let commits_to_load = if index.commits.len() <= window_size {
        &index.commits[..]
    } else {
        &index.commits[index.commits.len() - window_size..]
    };

    let mut snapshots = Vec::new();
    for entry in commits_to_load {
        if let Some(snapshot) = crate::snapshot::load_snapshot(repo_root, &entry.sha)? {
            snapshots.push(snapshot);
        }
    }

    // Ensure deterministic ordering (by timestamp, then SHA)
    snapshots.sort_by(|a, b| {
        a.commit
            .timestamp
            .cmp(&b.commit.timestamp)
            .then_with(|| a.commit.sha.cmp(&b.commit.sha))
    });

    Ok(snapshots)
}
