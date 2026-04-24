//! Snapshot compaction — reduce storage by converting full snapshots to deltas
//! or discarding snapshots with no risk-band changes.
//!
//! Level 1: replace old full snapshots with parent-relative compact deltas;
//!          keep the last N snapshots as full for fast random access.
//! Level 2: discard snapshots where no function's risk band changed;
//!          retain only band-transition points.

use crate::snapshot::{
    self, AnalysisInfo, CommitInfo, FunctionSnapshot, Index, Snapshot, SnapshotSummary,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const COMPACT_DELTA_SCHEMA_VERSION: u32 = 1;

/// A compacted snapshot that stores only the diff relative to its git parent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompactedDelta {
    pub schema_version: u32,
    pub sha: String,
    pub parent_sha: String,
    pub commit: CommitInfo,
    pub analysis: AnalysisInfo,
    /// Functions that are new or whose content changed relative to the parent.
    pub upserted: Vec<FunctionSnapshot>,
    /// Function IDs that existed in the parent but are absent here.
    pub deleted_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SnapshotSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<crate::aggregates::SnapshotAggregates>,
}

/// Outcome of a compaction run.
#[derive(Debug, Default)]
pub struct CompactStats {
    pub snapshots_compacted: usize,
    pub snapshots_removed: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

impl CompactStats {
    pub fn bytes_saved(&self) -> u64 {
        self.bytes_before.saturating_sub(self.bytes_after)
    }
}

/// Return the path for the compacted delta file for a given SHA.
pub fn compact_delta_path(repo_root: &Path, sha: &str) -> std::path::PathBuf {
    snapshot::snapshots_dir(repo_root).join(format!("{}.delta.json.zst", sha))
}

/// Return true if a compacted delta file exists for this SHA.
pub fn compact_delta_exists(repo_root: &Path, sha: &str) -> bool {
    compact_delta_path(repo_root, sha).exists()
}

/// Build a CompactedDelta from a current snapshot and its parent.
fn make_compacted_delta(current: &Snapshot, parent: &Snapshot) -> CompactedDelta {
    let parent_map: HashMap<&str, &FunctionSnapshot> = parent
        .functions
        .iter()
        .map(|f| (f.function_id.as_str(), f))
        .collect();

    let current_ids: HashSet<&str> = current
        .functions
        .iter()
        .map(|f| f.function_id.as_str())
        .collect();

    let upserted: Vec<FunctionSnapshot> = current
        .functions
        .iter()
        .filter(|f| {
            parent_map
                .get(f.function_id.as_str())
                .map(|p| *p != *f)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    let deleted_ids: Vec<String> = parent
        .functions
        .iter()
        .filter(|f| !current_ids.contains(f.function_id.as_str()))
        .map(|f| f.function_id.clone())
        .collect();

    CompactedDelta {
        schema_version: COMPACT_DELTA_SCHEMA_VERSION,
        sha: current.commit.sha.clone(),
        parent_sha: parent.commit.sha.clone(),
        commit: current.commit.clone(),
        analysis: current.analysis.clone(),
        upserted,
        deleted_ids,
        summary: current.summary.clone(),
        aggregates: current.aggregates.clone(),
    }
}

/// Reconstruct a full Snapshot from a CompactedDelta and its parent Snapshot.
fn reconstruct(delta: &CompactedDelta, parent: &Snapshot) -> Snapshot {
    let deleted: HashSet<&str> = delta.deleted_ids.iter().map(|s| s.as_str()).collect();

    let upserted_map: HashMap<&str, &FunctionSnapshot> = delta
        .upserted
        .iter()
        .map(|f| (f.function_id.as_str(), f))
        .collect();

    let parent_ids: HashSet<&str> = parent
        .functions
        .iter()
        .map(|f| f.function_id.as_str())
        .collect();

    // Start from parent functions, apply modifications and deletions.
    let mut functions: Vec<FunctionSnapshot> = parent
        .functions
        .iter()
        .filter(|f| !deleted.contains(f.function_id.as_str()))
        .map(|f| {
            upserted_map
                .get(f.function_id.as_str())
                .map(|u| (*u).clone())
                .unwrap_or_else(|| f.clone())
        })
        .collect();

    // Append new functions (in upserted but not in parent).
    for f in &delta.upserted {
        if !parent_ids.contains(f.function_id.as_str()) {
            functions.push(f.clone());
        }
    }

    functions.sort_by(|a, b| a.function_id.cmp(&b.function_id));

    Snapshot {
        schema_version: snapshot::SNAPSHOT_SCHEMA_VERSION,
        commit: delta.commit.clone(),
        analysis: delta.analysis.clone(),
        functions,
        summary: delta.summary.clone(),
        aggregates: delta.aggregates.clone(),
    }
}

fn read_compact_delta(path: &Path) -> Result<CompactedDelta> {
    let compressed = std::fs::read(path)
        .with_context(|| format!("failed to read compact delta: {}", path.display()))?;
    let bytes = zstd::decode_all(compressed.as_slice())
        .with_context(|| format!("failed to decompress compact delta: {}", path.display()))?;
    let json = String::from_utf8(bytes).context("compact delta contains invalid UTF-8")?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse compact delta: {}", path.display()))
}

fn write_compact_delta(path: &Path, delta: &CompactedDelta) -> Result<()> {
    let json = serde_json::to_string_pretty(delta).context("failed to serialize compact delta")?;
    let compressed =
        zstd::encode_all(json.as_bytes(), 3).context("failed to compress compact delta")?;
    snapshot::atomic_write_bytes(path, &compressed)
        .with_context(|| format!("failed to write compact delta: {}", path.display()))?;
    Ok(())
}

/// Load a snapshot by following the delta chain from disk, up to `depth` hops.
/// Returns `None` if no delta file exists for the given SHA.
pub fn load_via_delta_chain(repo_root: &Path, sha: &str, depth: usize) -> Result<Option<Snapshot>> {
    if depth > 1000 {
        anyhow::bail!("compact delta chain too deep (>1000) resolving SHA {}", sha);
    }

    let delta_path = compact_delta_path(repo_root, sha);
    if !delta_path.exists() {
        return Ok(None);
    }

    let delta = read_compact_delta(&delta_path)?;

    let parent = snapshot::load_snapshot(repo_root, &delta.parent_sha)?.with_context(|| {
        format!(
            "compact delta for {} requires parent {} which is not available",
            sha, delta.parent_sha
        )
    })?;

    Ok(Some(reconstruct(&delta, &parent)))
}

/// Level-1 compaction: convert old full snapshots to parent-relative deltas.
///
/// Keeps the last `keep_full` snapshots as full snapshots on disk.
/// Snapshots whose parent is not in the index (orphaned roots) are left as-is.
pub fn compact_level1(repo_root: &Path, keep_full: usize, dry_run: bool) -> Result<CompactStats> {
    let index_path = snapshot::index_path(repo_root);
    let mut index = Index::load_or_new(&index_path)?;

    let entry_count = index.commits.len();
    let mut stats = CompactStats::default();

    if entry_count <= keep_full {
        return Ok(stats);
    }

    // Commits are sorted ascending; compact all except the last keep_full.
    let to_compact: Vec<_> = index.commits[..entry_count - keep_full].to_vec();

    for entry in &to_compact {
        let parent_sha = match entry.parents.first() {
            Some(p) => p.clone(),
            None => continue, // root commit — no parent to delta against
        };
        if !index.contains(&parent_sha) {
            continue; // parent not tracked — can't build delta chain
        }
        if compact_delta_exists(repo_root, &entry.sha) {
            continue; // already compacted
        }

        let snap_path = match snapshot::snapshot_path_existing(repo_root, &entry.sha) {
            Some(p) => p,
            None => continue, // no full snapshot to compact
        };

        let snap_size = std::fs::metadata(&snap_path).map(|m| m.len()).unwrap_or(0);
        stats.bytes_before += snap_size;

        let current = snapshot::load_snapshot(repo_root, &entry.sha)?
            .with_context(|| format!("failed to load snapshot {}", entry.sha))?;
        let parent = snapshot::load_snapshot(repo_root, &parent_sha)?
            .with_context(|| format!("failed to load parent snapshot {}", parent_sha))?;

        let compact = make_compacted_delta(&current, &parent);
        let delta_path = compact_delta_path(repo_root, &entry.sha);

        if dry_run {
            let json = serde_json::to_string_pretty(&compact)?;
            let compressed = zstd::encode_all(json.as_bytes(), 3)?;
            stats.bytes_after += compressed.len() as u64;
        } else {
            write_compact_delta(&delta_path, &compact)?;
            stats.bytes_after += std::fs::metadata(&delta_path).map(|m| m.len()).unwrap_or(0);
            std::fs::remove_file(&snap_path).with_context(|| {
                format!("failed to remove full snapshot: {}", snap_path.display())
            })?;
        }

        stats.snapshots_compacted += 1;
    }

    if !dry_run && stats.snapshots_compacted > 0 {
        let current_level = index.compaction_level();
        if current_level < 1 {
            index.set_compaction_level(1);
        }
        let index_json = index.to_json()?;
        snapshot::atomic_write(&index_path, &index_json)?;
    }

    Ok(stats)
}

/// Level-2 compaction: discard snapshots where no function's risk band changed.
///
/// Walks the snapshot history in chronological order. Keeps a snapshot only if
/// at least one function changed band (including new or deleted functions).
/// Discards files AND removes index entries for no-change snapshots.
pub fn compact_level2(repo_root: &Path, dry_run: bool) -> Result<CompactStats> {
    let index_path = snapshot::index_path(repo_root);
    let mut index = Index::load_or_new(&index_path)?;

    let entries = index.commits.clone();
    let mut stats = CompactStats::default();

    if entries.is_empty() {
        return Ok(stats);
    }

    let mut last_bands: HashMap<String, String> = HashMap::new();
    let mut to_remove: Vec<String> = Vec::new();
    let mut first = true;

    for entry in &entries {
        let snap = match snapshot::load_snapshot(repo_root, &entry.sha)? {
            Some(s) => s,
            None => continue,
        };

        if first {
            // Always keep the first snapshot (it's the baseline).
            for f in &snap.functions {
                last_bands.insert(f.function_id.clone(), f.band.clone());
            }
            first = false;
            continue;
        }

        let current_ids: HashSet<&str> = snap
            .functions
            .iter()
            .map(|f| f.function_id.as_str())
            .collect();

        let has_band_change = snap.functions.iter().any(|f| {
            last_bands
                .get(&f.function_id)
                .map(|b| b.as_str() != f.band.as_str())
                .unwrap_or(true) // new function counts as a change
        }) || last_bands
            .keys()
            .any(|id| !current_ids.contains(id.as_str())); // deleted function

        if has_band_change {
            last_bands.clear();
            for f in &snap.functions {
                last_bands.insert(f.function_id.clone(), f.band.clone());
            }
        } else {
            to_remove.push(entry.sha.clone());
        }
    }

    for sha in &to_remove {
        // Accumulate sizes for both full and delta files.
        if let Some(p) = snapshot::snapshot_path_existing(repo_root, sha) {
            stats.bytes_before += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        }
        let delta_p = compact_delta_path(repo_root, sha);
        if delta_p.exists() {
            stats.bytes_before += std::fs::metadata(&delta_p).map(|m| m.len()).unwrap_or(0);
        }

        stats.snapshots_removed += 1;

        if !dry_run {
            if let Some(p) = snapshot::snapshot_path_existing(repo_root, sha) {
                std::fs::remove_file(&p).ok();
            }
            if delta_p.exists() {
                std::fs::remove_file(&delta_p).ok();
            }
            index.remove_commit(sha);
        }
    }

    if !dry_run && stats.snapshots_removed > 0 {
        index.set_compaction_level(2);
        let index_json = index.to_json()?;
        snapshot::atomic_write(&index_path, &index_json)?;
    }

    Ok(stats)
}
