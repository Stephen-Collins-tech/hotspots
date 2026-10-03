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
    /// Level 1: number of snapshots whose delta encoding would not actually
    /// reduce (or did not reduce) on-disk size and so were left as full
    /// snapshots rather than converted. See #225.
    pub skipped_no_benefit: usize,
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
            skipped_no_benefit: 0,
            dry_run,
        });
    }

    // Indices of snapshots to keep as full: oldest (index 0) + last `keep_recent`.
    let mut keep_full: HashSet<usize> =
        (0..keep_recent.min(total)).map(|i| total - 1 - i).collect();
    keep_full.insert(0);

    let mut converted_count = 0usize;
    let mut bytes_freed = 0u64;
    let mut skipped_no_benefit = 0usize;

    // Process oldest → newest so that when we compute delta for snapshot i,
    // snapshot i-1 is already the authoritative source (full or delta).
    for i in 1..total {
        if keep_full.contains(&i) {
            continue;
        }
        match convert_one_to_delta(repo_root, &commits[i].sha, &commits[i - 1].sha, dry_run)? {
            Some(ConvertOutcome::Converted(freed)) => {
                bytes_freed += freed;
                converted_count += 1;
            }
            Some(ConvertOutcome::NotWorthwhile) => {
                skipped_no_benefit += 1;
            }
            None => {}
        }
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
        skipped_no_benefit,
        dry_run,
    })
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
            skipped_no_benefit: 0,
            dry_run,
        });
    }

    let loaded = load_all_snapshots(repo_root, &commits)?;
    let keep_shas = select_keep_shas(&loaded, total);
    let drop_shas: Vec<String> = commits
        .iter()
        .map(|e| e.sha.clone())
        .filter(|sha| !keep_shas.contains(sha))
        .collect();

    let bytes_freed = delete_snapshot_files(repo_root, &drop_shas, dry_run)?;

    if !dry_run && !drop_shas.is_empty() {
        let dropped_set: HashSet<&str> = drop_shas.iter().map(|s| s.as_str()).collect();
        fix_orphaned_deltas(repo_root, &commits, &dropped_set, &loaded)?;
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
        skipped_no_benefit: 0,
        dry_run,
    })
}

// ── private helpers ───────────────────────────────────────────────────────────

/// Outcome of attempting to convert one snapshot to a delta.
enum ConvertOutcome {
    /// Converted (or would convert, in dry-run): carries bytes actually/estimated freed.
    Converted(u64),
    /// Skipped because delta+metadata overhead would not reduce (or did not reduce)
    /// on-disk size for this snapshot.
    NotWorthwhile,
}

/// Try to convert snapshot `sha` to a delta against `prev_sha`.
///
/// Always computes the real compressed delta size (even in dry-run mode) so
/// the size comparison reflects reality rather than assuming the full
/// snapshot's entire size is "freed" — at small scale, delta+metadata
/// overhead can outweigh the content saved (see #225). A conversion that
/// would not actually shrink on-disk size is skipped rather than applied.
fn convert_one_to_delta(
    repo_root: &Path,
    sha: &str,
    prev_sha: &str,
    dry_run: bool,
) -> Result<Option<ConvertOutcome>> {
    // Already a delta — nothing to do.
    if snapshot::delta_snapshot_path(repo_root, sha).exists() {
        return Ok(None);
    }
    let full_path = match snapshot::snapshot_path_existing(repo_root, sha) {
        Some(p) => p,
        None => return Ok(None),
    };
    let current = match snapshot::load_snapshot(repo_root, sha)? {
        Some(s) => s,
        None => return Ok(None),
    };
    let base = match snapshot::load_snapshot(repo_root, prev_sha)? {
        Some(s) => s,
        None => return Ok(None),
    };

    let full_size = std::fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);

    // Compute the real compressed delta size in memory first, regardless of
    // dry-run, so the estimate (and the real-run decision) are both based on
    // actual bytes rather than an optimistic formula.
    let delta = snapshot::compute_delta(&base, &current);
    let json = serde_json::to_string_pretty(&delta).context("failed to serialize delta")?;
    let compressed = zstd::encode_all(json.as_bytes(), 3)
        .context("failed to compress delta for size estimate")?;
    let delta_size = compressed.len() as u64;

    if delta_size >= full_size {
        // Converting would not actually reduce storage for this snapshot.
        return Ok(Some(ConvertOutcome::NotWorthwhile));
    }

    if !dry_run {
        snapshot::persist_delta(repo_root, &delta)?;
        std::fs::remove_file(&full_path)
            .with_context(|| format!("failed to remove full snapshot: {}", full_path.display()))?;
    }

    Ok(Some(ConvertOutcome::Converted(full_size - delta_size)))
}

/// Load every snapshot in `commits` into memory (full and delta handled transparently).
fn load_all_snapshots(
    repo_root: &Path,
    commits: &[crate::snapshot::IndexEntry],
) -> Result<Vec<(String, Option<Snapshot>)>> {
    let mut loaded = Vec::with_capacity(commits.len());
    for entry in commits {
        let snap = snapshot::load_snapshot(repo_root, &entry.sha)?;
        loaded.push((entry.sha.clone(), snap));
    }
    Ok(loaded)
}

/// Decide which SHAs to keep: oldest + newest always kept; others only if a
/// band change occurred since the last kept snapshot.
fn select_keep_shas(loaded: &[(String, Option<Snapshot>)], total: usize) -> HashSet<String> {
    let mut keep: HashSet<String> = HashSet::new();
    keep.insert(loaded[0].0.clone());
    keep.insert(loaded[total - 1].0.clone());

    let mut last_kept_idx = 0usize;
    for i in 1..total {
        let is_last = i == total - 1;
        let should_keep = is_last
            || match (&loaded[i].1, &loaded[last_kept_idx].1) {
                (Some(curr), Some(prev)) => has_band_change(prev, curr),
                _ => true,
            };
        if should_keep {
            keep.insert(loaded[i].0.clone());
            last_kept_idx = i;
        }
    }
    keep
}

/// Delete on-disk files (full + delta) for each SHA in `drop_shas`.
/// Returns total bytes freed; skips file ops when `dry_run` is true.
fn delete_snapshot_files(repo_root: &Path, drop_shas: &[String], dry_run: bool) -> Result<u64> {
    let mut bytes_freed = 0u64;
    for sha in drop_shas {
        if let Some(p) = snapshot::snapshot_path_existing(repo_root, sha) {
            bytes_freed += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if !dry_run {
                std::fs::remove_file(&p)
                    .with_context(|| format!("failed to remove snapshot: {}", p.display()))?;
            }
        }
        let dp = snapshot::delta_snapshot_path(repo_root, sha);
        if dp.exists() {
            bytes_freed += std::fs::metadata(&dp).map(|m| m.len()).unwrap_or(0);
            if !dry_run {
                std::fs::remove_file(&dp)
                    .with_context(|| format!("failed to remove delta: {}", dp.display()))?;
            }
        }
    }
    Ok(bytes_freed)
}

/// For any kept delta whose base SHA was dropped, rewrite it as a full snapshot
/// using the already-reconstructed in-memory copy.
fn fix_orphaned_deltas(
    repo_root: &Path,
    commits: &[crate::snapshot::IndexEntry],
    dropped_set: &HashSet<&str>,
    loaded: &[(String, Option<Snapshot>)],
) -> Result<()> {
    let reconstructed: HashMap<&str, &Snapshot> = loaded
        .iter()
        .filter_map(|(sha, s)| s.as_ref().map(|snap| (sha.as_str(), snap)))
        .collect();

    for entry in commits {
        if dropped_set.contains(entry.sha.as_str()) {
            continue;
        }
        let delta_path = snapshot::delta_snapshot_path(repo_root, &entry.sha);
        if !delta_path.exists() {
            continue;
        }
        let base_sha = read_delta_base_sha(&delta_path)?;
        if !dropped_set.contains(base_sha.as_str()) {
            continue;
        }
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
    Ok(())
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

    curr_bands
        .iter()
        .any(|(id, band)| prev_bands.get(id).map(|pb| pb != band).unwrap_or(true))
        || prev_bands.keys().any(|id| !curr_bands.contains_key(id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitContext;
    use crate::language::Language;
    use crate::report::{FunctionRiskReport, MetricsReport, RiskReport};
    use crate::risk::RiskBand;
    use crate::snapshot::{append_to_index, persist_snapshot};

    fn tiny_snapshot(sha: &str, parent: &str, timestamp: i64, func_name: &str) -> Snapshot {
        let git_context = GitContext {
            head_sha: sha.to_string(),
            parent_shas: vec![parent.to_string()],
            timestamp,
            branch: Some("main".to_string()),
            is_detached: false,
            message: Some("test commit".to_string()),
            author: Some("Test Author".to_string()),
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };
        let report = FunctionRiskReport {
            file: "src/foo.ts".to_string(),
            function: func_name.to_string(),
            line: 1,
            language: Language::TypeScript,
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 3,
            },
            risk: RiskReport {
                r_cc: 1.0,
                r_nd: 1.0,
                r_fo: 1.0,
                r_ns: 1.0,
            },
            lrs: 1.0,
            band: RiskBand::Low,
            suppression_reason: None,
            patterns: vec![],
            pattern_details: None,
            callees: vec![],
            explanation: None,
        };
        Snapshot::new(git_context, vec![report])
    }

    /// Regression test for #225: converting a tiny snapshot to a delta can
    /// cost more bytes (delta + metadata overhead) than it saves. Level-1
    /// compaction must not apply such a conversion, and must not make total
    /// on-disk snapshot size worse than before.
    #[test]
    fn compact_level1_skips_conversions_that_would_not_shrink_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path();
        std::fs::create_dir_all(snapshot::snapshots_dir(repo_root)).unwrap();

        let snapshots = [
            tiny_snapshot("sha1", "sha0", 1000, "a"),
            tiny_snapshot("sha2", "sha1", 2000, "b"),
            tiny_snapshot("sha3", "sha2", 3000, "c"),
        ];
        for snap in &snapshots {
            persist_snapshot(repo_root, snap, false).unwrap();
            append_to_index(repo_root, snap).unwrap();
        }

        let size_before: u64 = std::fs::read_dir(snapshot::snapshots_dir(repo_root))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum();

        // keep_recent=1 forces the middle snapshot (sha2) to be a delta
        // candidate; it's tiny, so delta+metadata overhead should exceed
        // any savings and it should be skipped.
        let result = compact_to_level1(repo_root, false, 1).unwrap();

        let size_after: u64 = std::fs::read_dir(snapshot::snapshots_dir(repo_root))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum();

        assert_eq!(
            result.converted_count, 0,
            "tiny snapshot should not be converted: {:?}",
            result
        );
        assert_eq!(result.skipped_no_benefit, 1);
        assert!(
            size_after <= size_before,
            "compaction must never increase total snapshot storage: before={size_before} after={size_after}"
        );
    }

    #[test]
    fn compact_level1_dry_run_reports_same_skip_as_real_run() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path();
        std::fs::create_dir_all(snapshot::snapshots_dir(repo_root)).unwrap();

        let snapshots = [
            tiny_snapshot("sha1", "sha0", 1000, "a"),
            tiny_snapshot("sha2", "sha1", 2000, "b"),
            tiny_snapshot("sha3", "sha2", 3000, "c"),
        ];
        for snap in &snapshots {
            persist_snapshot(repo_root, snap, false).unwrap();
            append_to_index(repo_root, snap).unwrap();
        }

        let dry = compact_to_level1(repo_root, true, 1).unwrap();
        assert_eq!(dry.converted_count, 0);
        assert_eq!(dry.skipped_no_benefit, 1);
        assert_eq!(dry.bytes_freed, 0);

        // Dry run must not have modified anything on disk.
        assert!(!snapshot::delta_snapshot_path(repo_root, "sha2").exists());
        assert!(snapshot::snapshot_path_existing(repo_root, "sha2").is_some());
    }
}
