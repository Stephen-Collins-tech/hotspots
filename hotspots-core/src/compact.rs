//! Compaction of snapshot history to reduce storage
//!
//! Level 1: convert intermediate full snapshots to delta encoding.
//! Level 2: drop snapshots where no function changed risk band.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::snapshot::{self, DeltaSnapshot, Index, Snapshot};

/// Result of a compaction run (real or dry-run).
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Level 1: number of full snapshots converted to deltas.
    pub converted_count: usize,
    /// Level 2: number of snapshots deleted.
    pub dropped_count: usize,
    /// Bytes freed from disk (0 in dry-run mode).
    pub bytes_freed: u64,
    pub dry_run: bool,
}

/// Level 1: convert all intermediate full snapshots to delta encoding.
///
/// Keeps `keep_recent` most-recent snapshots as full snapshots plus the
/// oldest snapshot as a baseline.  Everything in between is stored as a
/// delta relative to its chronological predecessor.
pub fn compact_to_level1(
    repo_root: &Path,
    dry_run: bool,
    keep_recent: usize,
) -> Result<CompactionResult> {
    let index_path = snapshot::index_path(repo_root);
    let index = Index::load_or_new(&index_path)?;
    let commits = index.commits.clone();
    let total = commits.len();

    if total <= 1 {
        return Ok(CompactionResult {
            converted_count: 0,
            dropped_count: 0,
            bytes_freed: 0,
            dry_run,
        });
    }

    // Indices of snapshots to keep as full: oldest (index 0) + last `keep_recent`.
    let mut keep_full: HashSet<usize> =
        (0..keep_recent.min(total)).map(|i| total - 1 - i).collect();
    keep_full.insert(0);

    let mut converted_count = 0usize;
    let mut bytes_freed = 0u64;

    // Process oldest → newest so that when we compute delta for snapshot i,
    // snapshot i-1 is already the authoritative source (full or delta).
    for i in 1..total {
        if keep_full.contains(&i) {
            continue;
        }

        let entry = &commits[i];
        let prev_entry = &commits[i - 1];

        // Skip if already a delta.
        if snapshot::delta_snapshot_path(repo_root, &entry.sha).exists() {
            continue;
        }

        // Skip if there is no full snapshot on disk for this entry.
        let full_path = match snapshot::snapshot_path_existing(repo_root, &entry.sha) {
            Some(p) => p,
            None => continue,
        };

        // Load this snapshot and its chronological predecessor.
        let current = match snapshot::load_snapshot(repo_root, &entry.sha)? {
            Some(s) => s,
            None => continue,
        };
        let base = match snapshot::load_snapshot(repo_root, &prev_entry.sha)? {
            Some(s) => s,
            None => continue,
        };

        if let Ok(meta) = std::fs::metadata(&full_path) {
            bytes_freed += meta.len();
        }

        if !dry_run {
            let delta = snapshot::compute_delta(&base, &current);
            snapshot::persist_delta(repo_root, &delta)?;
            // Subtract the new delta file size from bytes_freed.
            let delta_path = snapshot::delta_snapshot_path(repo_root, &entry.sha);
            if let Ok(meta) = std::fs::metadata(&delta_path) {
                bytes_freed = bytes_freed.saturating_sub(meta.len());
            }
            std::fs::remove_file(&full_path).with_context(|| {
                format!("failed to remove full snapshot: {}", full_path.display())
            })?;
        }

        converted_count += 1;
    }

    if !dry_run {
        let mut index = Index::load_or_new(&index_path)?;
        index.set_compaction_level(1);
        snapshot::atomic_write(&index_path, &index.to_json()?)?;
    }

    Ok(CompactionResult {
        converted_count,
        dropped_count: 0,
        bytes_freed,
        dry_run,
    })
}

/// Returns true if any function changed its risk band between `prev` and `curr`,
/// or if functions were added or removed.
fn has_band_change(prev: &Snapshot, curr: &Snapshot) -> bool {
    let prev_bands: HashMap<&str, crate::risk::RiskBand> = prev
        .functions
        .iter()
        .map(|f| (f.function_id.as_str(), f.band))
        .collect();
    let curr_bands: HashMap<&str, crate::risk::RiskBand> = curr
        .functions
        .iter()
        .map(|f| (f.function_id.as_str(), f.band))
        .collect();

    // Any function added, removed, or changed band.
    curr_bands
        .iter()
        .any(|(id, band)| prev_bands.get(id).map(|pb| pb != band).unwrap_or(true))
        || prev_bands.keys().any(|id| !curr_bands.contains_key(id))
}

/// Level 2: delete snapshots where no function changed risk band.
///
/// Always retains the oldest and most-recent snapshot.  For all others, keeps
/// the snapshot only if at least one function's band changed since the last kept
/// snapshot.
///
/// After deletion, any delta snapshot whose base was removed is converted to a
/// full snapshot so that remaining chains remain intact.
pub fn compact_to_level2(repo_root: &Path, dry_run: bool) -> Result<CompactionResult> {
    let index_path = snapshot::index_path(repo_root);
    let index = Index::load_or_new(&index_path)?;
    let commits = index.commits.clone();
    let total = commits.len();

    if total <= 1 {
        return Ok(CompactionResult {
            converted_count: 0,
            dropped_count: 0,
            bytes_freed: 0,
            dry_run,
        });
    }

    // Load all snapshots (handles full and delta transparently).
    let mut loaded: Vec<(String, Option<Snapshot>)> = Vec::with_capacity(total);
    for entry in &commits {
        let snap = snapshot::load_snapshot(repo_root, &entry.sha)?;
        loaded.push((entry.sha.clone(), snap));
    }

    // Determine which SHAs to keep via band-change filter.
    let mut keep_shas: HashSet<String> = HashSet::new();
    keep_shas.insert(loaded[0].0.clone()); // always keep oldest
    keep_shas.insert(loaded[total - 1].0.clone()); // always keep newest

    let mut last_kept_idx = 0usize;
    for i in 1..total {
        let (sha, snap_opt) = &loaded[i];
        let is_last = i == total - 1;

        let keep = is_last
            || match (snap_opt, &loaded[last_kept_idx].1) {
                (Some(curr), Some(prev)) => has_band_change(prev, curr),
                _ => true,
            };

        if keep {
            keep_shas.insert(sha.clone());
            last_kept_idx = i;
        }
    }

    let drop_shas: Vec<String> = commits
        .iter()
        .map(|e| e.sha.clone())
        .filter(|sha| !keep_shas.contains(sha))
        .collect();

    let dropped_set: HashSet<String> = drop_shas.iter().cloned().collect();
    let mut bytes_freed = 0u64;

    // Measure and delete snapshot files for dropped SHAs.
    for sha in &drop_shas {
        if let Some(p) = snapshot::snapshot_path_existing(repo_root, sha) {
            if let Ok(meta) = std::fs::metadata(&p) {
                bytes_freed += meta.len();
            }
            if !dry_run {
                std::fs::remove_file(&p)
                    .with_context(|| format!("failed to remove snapshot: {}", p.display()))?;
            }
        }
        let dp = snapshot::delta_snapshot_path(repo_root, sha);
        if dp.exists() {
            if let Ok(meta) = std::fs::metadata(&dp) {
                bytes_freed += meta.len();
            }
            if !dry_run {
                std::fs::remove_file(&dp)
                    .with_context(|| format!("failed to remove delta: {}", dp.display()))?;
            }
        }
    }

    // Fix orphaned delta snapshots: if a kept delta's base was dropped, convert
    // it to a full snapshot using the already-reconstructed in-memory copy.
    if !dry_run && !dropped_set.is_empty() {
        // Build a map sha → reconstructed Snapshot for kept entries.
        let reconstructed: HashMap<&str, &Snapshot> = loaded
            .iter()
            .filter_map(|(sha, s)| s.as_ref().map(|snap| (sha.as_str(), snap)))
            .collect();

        for entry in &commits {
            if dropped_set.contains(&entry.sha) {
                continue;
            }
            let delta_path = snapshot::delta_snapshot_path(repo_root, &entry.sha);
            if !delta_path.exists() {
                continue;
            }
            // Read the delta to check its base_sha.
            let base_sha = read_delta_base_sha(&delta_path)?;
            if dropped_set.contains(&base_sha) {
                // Convert to full snapshot.
                if let Some(&snap) = reconstructed.get(entry.sha.as_str()) {
                    let json = snap
                        .to_json()
                        .context("failed to serialize reconstructed snapshot")?;
                    let compressed = zstd::encode_all(json.as_bytes(), 3)
                        .context("failed to compress reconstructed snapshot")?;
                    let full_path = snapshot::snapshot_path(repo_root, &entry.sha);
                    snapshot::atomic_write_bytes(&full_path, &compressed).with_context(|| {
                        format!(
                            "failed to write reconstructed snapshot: {}",
                            full_path.display()
                        )
                    })?;
                    std::fs::remove_file(&delta_path).with_context(|| {
                        format!("failed to remove orphaned delta: {}", delta_path.display())
                    })?;
                }
            }
        }
    }

    // Update index.
    if !dry_run {
        let mut index = Index::load_or_new(&index_path)?;
        for sha in &drop_shas {
            index.remove_commit(sha);
        }
        index.set_compaction_level(2);
        snapshot::atomic_write(&index_path, &index.to_json()?)?;
    }

    Ok(CompactionResult {
        converted_count: 0,
        dropped_count: drop_shas.len(),
        bytes_freed,
        dry_run,
    })
}

/// Read the `base_sha` field from a `.delta.json.zst` file without fully parsing it.
fn read_delta_base_sha(delta_path: &Path) -> Result<String> {
    let compressed = std::fs::read(delta_path)
        .with_context(|| format!("failed to read delta: {}", delta_path.display()))?;
    let bytes = zstd::decode_all(compressed.as_slice())
        .with_context(|| format!("failed to decompress delta: {}", delta_path.display()))?;
    let json = String::from_utf8(bytes).context("delta contains invalid UTF-8")?;
    let delta: DeltaSnapshot = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse delta: {}", delta_path.display()))?;
    Ok(delta.base_sha)
}
