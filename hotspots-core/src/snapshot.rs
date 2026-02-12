//! Snapshot container and persistence
//!
//! Wraps analysis output in immutable, commit-scoped snapshots and persists them.
//!
//! Global invariants enforced:
//! - Snapshots are immutable (never overwrite existing snapshots)
//! - Commit hash is the sole identity (filename equals commit SHA)
//! - Byte-for-byte deterministic serialization
//! - Paths normalized to `/` (forward slashes only)
//! - ASCII lexical ordering (not locale-aware)

use crate::git::GitContext;
use crate::report::{FunctionRiskReport, MetricsReport};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::report::RiskReport;

/// Schema version for snapshots
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Schema version for index
const INDEX_SCHEMA_VERSION: u32 = 1;

/// Commit information in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CommitInfo {
    pub sha: String,
    pub parents: Vec<String>,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fix_commit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_revert_commit: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ticket_ids: Vec<String>,
}

/// Analysis metadata in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AnalysisInfo {
    pub scope: String,
    #[serde(rename = "tool_version")]
    pub tool_version: String,
}

/// Churn metrics for a file/function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChurnMetrics {
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub net_change: i64,
}

/// Function entry in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionSnapshot {
    pub function_id: String,
    pub file: String,
    pub line: u32,
    pub language: String,
    pub metrics: MetricsReport,
    pub lrs: f64,
    pub band: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn: Option<ChurnMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_count_30d: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_since_last_change: Option<u32>,
}

/// Complete snapshot for a commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Snapshot {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    pub commit: CommitInfo,
    pub analysis: AnalysisInfo,
    pub functions: Vec<FunctionSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<crate::aggregates::SnapshotAggregates>,
}

/// Index entry for a commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct IndexEntry {
    pub sha: String,
    pub parents: Vec<String>,
    pub timestamp: i64,
}

/// Index containing all tracked commits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Index {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    /// Compaction level (0 = full snapshots, 1 = deltas only, 2 = band transitions only)
    /// None means Level 0 (full snapshots) - for backward compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_level: Option<u32>,
    pub commits: Vec<IndexEntry>,
}

impl Snapshot {
    /// Create a new snapshot from git context and function reports
    ///
    /// # Arguments
    ///
    /// * `git_context` - Git context for the current commit
    /// * `reports` - Function risk reports from analysis
    ///
    /// # Function ID Format
    ///
    /// Function ID is `<relative_file_path>::<symbol>` where:
    /// - `relative_file_path` is normalized to use `/` separators
    /// - `symbol` is the function name (or `<anonymous>` for anonymous functions)
    pub fn new(git_context: GitContext, reports: Vec<FunctionRiskReport>) -> Self {
        // Normalize paths and build function snapshots
        let mut functions: Vec<FunctionSnapshot> = reports
            .into_iter()
            .map(|report| {
                // Normalize file path to use `/` separators
                let normalized_file = report.file.replace('\\', "/");

                // Extract function name for function_id
                // Use the function name from report, or derive from file/line if needed
                let function_symbol = if report.function.starts_with("<anonymous>") {
                    "<anonymous>"
                } else {
                    &report.function
                };

                // Build function_id: <relative_file_path>::<symbol>
                let function_id = format!("{}::{}", normalized_file, function_symbol);

                FunctionSnapshot {
                    function_id,
                    file: normalized_file,
                    line: report.line,
                    language: report.language,
                    metrics: report.metrics,
                    lrs: report.lrs,
                    band: report.band,
                    suppression_reason: report.suppression_reason,
                    churn: None, // Churn will be populated separately if available
                    touch_count_30d: None, // Touch count will be populated separately if available
                    days_since_last_change: None, // Days since last change will be populated separately if available
                }
            })
            .collect();

        // Sort functions deterministically by function_id (ASCII lexical ordering)
        functions.sort_by(|a, b| a.function_id.cmp(&b.function_id));

        Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            commit: CommitInfo {
                sha: git_context.head_sha,
                parents: git_context.parent_shas,
                timestamp: git_context.timestamp,
                branch: git_context.branch,
                message: git_context.message,
                author: git_context.author,
                is_fix_commit: git_context.is_fix_commit,
                is_revert_commit: git_context.is_revert_commit,
                ticket_ids: git_context.ticket_ids,
            },
            analysis: AnalysisInfo {
                scope: "full".to_string(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            functions,
            aggregates: None, // Aggregates are computed on-demand, not stored
        }
    }

    /// Populate churn metrics from git data
    ///
    /// Maps file-level churn to all functions in each file.
    /// Files not in the churn map will have churn remain as None.
    ///
    /// # Arguments
    ///
    /// * `file_churns` - Map from file path to churn metrics
    pub fn populate_churn(&mut self, file_churns: &std::collections::HashMap<String, crate::git::FileChurn>) {
        for function in &mut self.functions {
            // Normalize file path for lookup (already normalized in constructor)
            let file_path = &function.file;

            if let Some(file_churn) = file_churns.get(file_path) {
                let net_change = file_churn.lines_added as i64 - file_churn.lines_deleted as i64;
                function.churn = Some(ChurnMetrics {
                    lines_added: file_churn.lines_added,
                    lines_deleted: file_churn.lines_deleted,
                    net_change,
                });
            }
        }
    }

    /// Populate touch count and recency metrics from git data
    ///
    /// For each file, computes:
    /// - touch_count_30d: number of commits in last 30 days
    /// - days_since_last_change: days since last modification
    ///
    /// These are computed at the file level and applied to all functions in the file.
    ///
    /// # Arguments
    ///
    /// * `repo_root` - Path to repository root (for git operations)
    pub fn populate_touch_metrics(&mut self, repo_root: &std::path::Path) -> anyhow::Result<()> {
        use std::collections::HashMap;

        // Build set of unique files to avoid duplicate git operations
        let mut unique_files: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, function) in self.functions.iter().enumerate() {
            unique_files
                .entry(function.file.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        // For each unique file, compute metrics once
        for (file_path, function_indices) in unique_files {
            // Convert absolute path back to relative path for git
            let relative_path = if let Ok(rel) = std::path::Path::new(&file_path).strip_prefix(repo_root) {
                rel.to_string_lossy().to_string()
            } else {
                // If can't make relative, use as-is
                file_path.clone()
            };

            // Compute touch count (may fail if file is new or git operation fails)
            let touch_count = crate::git::count_file_touches_30d(&relative_path, self.commit.timestamp).ok();

            // Compute days since last change
            let days_since = crate::git::days_since_last_change(&relative_path, self.commit.timestamp).ok();

            // Apply to all functions in this file
            for &idx in &function_indices {
                self.functions[idx].touch_count_30d = touch_count;
                self.functions[idx].days_since_last_change = days_since;
            }
        }

        Ok(())
    }

    /// Serialize snapshot to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        // Use serde_json with pretty printing for readability
        // Keys are automatically sorted by serde when using BTreeMap-like structures
        // For deterministic ordering, we rely on serde's default behavior with sorted keys
        serde_json::to_string_pretty(self).context("failed to serialize snapshot to JSON")
    }

    /// Deserialize snapshot from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let snapshot: Snapshot =
            serde_json::from_str(json).context("failed to deserialize snapshot from JSON")?;

        // Validate schema version
        if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
            anyhow::bail!(
                "schema version mismatch: expected {}, got {}",
                SNAPSHOT_SCHEMA_VERSION,
                snapshot.schema_version
            );
        }

        Ok(snapshot)
    }

    /// Get the commit SHA for this snapshot
    pub fn commit_sha(&self) -> &str {
        &self.commit.sha
    }
}

impl Index {
    /// Create a new empty index (default compaction level 0 - full snapshots)
    pub fn new() -> Self {
        Index {
            schema_version: INDEX_SCHEMA_VERSION,
            compaction_level: Some(0), // Default: full snapshots
            commits: Vec::new(),
        }
    }

    /// Get compaction level (defaults to 0 if not set for backward compatibility)
    pub fn compaction_level(&self) -> u32 {
        self.compaction_level.unwrap_or(0)
    }

    /// Set compaction level
    pub fn set_compaction_level(&mut self, level: u32) {
        self.compaction_level = Some(level);
    }

    /// Load index from JSON file, or create new if file doesn't exist
    pub fn load_or_new(path: &Path) -> Result<Self> {
        if path.exists() {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read index file: {}", path.display()))?;
            Self::from_json(&json)
        } else {
            Ok(Self::new())
        }
    }

    /// Deserialize index from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let index: Index =
            serde_json::from_str(json).context("failed to deserialize index from JSON")?;

        // Validate schema version
        if index.schema_version != INDEX_SCHEMA_VERSION {
            anyhow::bail!(
                "index schema version mismatch: expected {}, got {}",
                INDEX_SCHEMA_VERSION,
                index.schema_version
            );
        }

        Ok(index)
    }

    /// Serialize index to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize index to JSON")
    }

    /// Add or update a commit entry in the index
    ///
    /// If the commit already exists, this is idempotent (no-op).
    /// Entries are kept sorted deterministically by timestamp, then SHA.
    pub fn add_commit(&mut self, entry: IndexEntry) {
        // Check if entry already exists
        if !self.commits.iter().any(|c| c.sha == entry.sha) {
            self.commits.push(entry);

            // Sort deterministically: timestamp ascending, then SHA ASCII ascending
            self.commits.sort_by(|a, b| {
                a.timestamp
                    .cmp(&b.timestamp)
                    .then_with(|| a.sha.cmp(&b.sha))
            });
        }
    }

    /// Remove a commit entry by SHA
    pub fn remove_commit(&mut self, sha: &str) {
        self.commits.retain(|c| c.sha != sha);
    }

    /// Check if index contains a commit
    pub fn contains(&self, sha: &str) -> bool {
        self.commits.iter().any(|c| c.sha == sha)
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the path to the `.hotspots` directory in the repository root
pub fn hotspots_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".hotspots")
}

/// Get the path to the snapshots directory
pub fn snapshots_dir(repo_root: &Path) -> PathBuf {
    hotspots_dir(repo_root).join("snapshots")
}

/// Get the path to the index file
pub fn index_path(repo_root: &Path) -> PathBuf {
    hotspots_dir(repo_root).join("index.json")
}

/// Get the path to a snapshot file for a given commit SHA
pub fn snapshot_path(repo_root: &Path, commit_sha: &str) -> PathBuf {
    snapshots_dir(repo_root).join(format!("{}.json", commit_sha))
}

/// Write data to file atomically using temp file + rename
pub fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    use std::fs;
    use std::io::Write;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Create temp file in same directory
    let temp_path = path.with_extension("tmp");

    // Write to temp file
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("failed to create temp file: {}", temp_path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write to temp file: {}", temp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temp file: {}", temp_path.display()))?;
    drop(file);

    // Atomic rename
    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to rename temp file to: {}", path.display()))?;

    Ok(())
}

/// Persist a snapshot to disk
///
/// # Atomic Writes
///
/// Uses temp file + rename pattern for atomic writes.
/// Never overwrites existing snapshots (fails if snapshot already exists).
///
/// # Errors
///
/// Returns error if:
/// - Snapshot file already exists (immutability enforced)
/// - Schema version mismatch (if reading existing file)
/// - I/O errors during write
pub fn persist_snapshot(repo_root: &Path, snapshot: &Snapshot) -> Result<()> {
    let snapshot_path = snapshot_path(repo_root, snapshot.commit_sha());

    // Never overwrite existing snapshots (immutability)
    if snapshot_path.exists() {
        // Verify existing snapshot matches (idempotency check)
        let existing_json = std::fs::read_to_string(&snapshot_path).with_context(|| {
            format!(
                "failed to read existing snapshot: {}",
                snapshot_path.display()
            )
        })?;
        let existing_snapshot = Snapshot::from_json(&existing_json).with_context(|| {
            format!(
                "existing snapshot has invalid schema: {}",
                snapshot_path.display()
            )
        })?;

        // If it's byte-for-byte identical, this is idempotent (ok)
        if existing_snapshot.to_json()? == snapshot.to_json()? {
            return Ok(());
        }

        anyhow::bail!(
            "snapshot already exists and differs: {} (snapshots are immutable)",
            snapshot_path.display()
        );
    }

    // Serialize snapshot
    let json = snapshot.to_json()?;

    // Atomic write
    atomic_write(&snapshot_path, &json)
        .with_context(|| format!("failed to persist snapshot: {}", snapshot_path.display()))?;

    Ok(())
}

/// Append snapshot entry to index
///
/// Loads existing index, adds entry, and persists atomically.
pub fn append_to_index(repo_root: &Path, snapshot: &Snapshot) -> Result<()> {
    let index_path = index_path(repo_root);

    // Load existing index or create new
    let mut index = Index::load_or_new(&index_path)?;

    // Add commit entry
    index.add_commit(IndexEntry {
        sha: snapshot.commit.sha.clone(),
        parents: snapshot.commit.parents.clone(),
        timestamp: snapshot.commit.timestamp,
    });

    // Serialize and write atomically
    let json = index.to_json()?;
    atomic_write(&index_path, &json)
        .with_context(|| format!("failed to update index: {}", index_path.display()))?;

    Ok(())
}

/// Rebuild index from snapshots directory
///
/// Scans `.hotspots/snapshots/` and rebuilds `index.json` with deterministic ordering.
/// Useful for recovery if index is corrupted or missing.
///
/// # Ordering
///
/// Index entries are sorted by timestamp (ascending), then SHA (ASCII ascending),
/// ensuring byte-for-byte deterministic output.
pub fn rebuild_index(repo_root: &Path) -> Result<Index> {
    let snapshots_dir = snapshots_dir(repo_root);

    if !snapshots_dir.exists() {
        return Ok(Index::new());
    }

    let mut index = Index::new();

    // Read all snapshot files
    let entries = std::fs::read_dir(&snapshots_dir).with_context(|| {
        format!(
            "failed to read snapshots directory: {}",
            snapshots_dir.display()
        )
    })?;

    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();

        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        // Read and parse snapshot
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read snapshot: {}", path.display()))?;

        let snapshot = match Snapshot::from_json(&json) {
            Ok(s) => s,
            Err(e) => {
                // Log error but continue (some snapshots may be corrupted)
                eprintln!(
                    "Warning: failed to parse snapshot {}: {}",
                    path.display(),
                    e
                );
                continue;
            }
        };

        // Add to index
        index.add_commit(IndexEntry {
            sha: snapshot.commit.sha,
            parents: snapshot.commit.parents,
            timestamp: snapshot.commit.timestamp,
        });
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::MetricsReport;

    fn create_test_snapshot() -> Snapshot {
        let git_context = GitContext {
            head_sha: "abc123".to_string(),
            parent_shas: vec!["def456".to_string()],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
        };

        let report = FunctionRiskReport {
            file: "src/foo.ts".to_string(),
            function: "handler".to_string(),
            line: 42,
            language: "TypeScript".to_string(),
            metrics: MetricsReport {
                cc: 5,
                nd: 2,
                fo: 3,
                ns: 1,
                loc: 10,
            },
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

        Snapshot::new(git_context, vec![report])
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = create_test_snapshot();

        // Serialize
        let json = snapshot.to_json().expect("should serialize");
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"sha\": \"abc123\""));
        assert!(json.contains("\"function_id\""));

        // Deserialize
        let deserialized = Snapshot::from_json(&json).expect("should deserialize");
        assert_eq!(deserialized.commit.sha, snapshot.commit.sha);
        assert_eq!(deserialized.functions.len(), snapshot.functions.len());
    }

    #[test]
    fn test_function_id_format() {
        let snapshot = create_test_snapshot();
        assert_eq!(snapshot.functions[0].function_id, "src/foo.ts::handler");
    }

    #[test]
    fn test_index_ordering() {
        let mut index = Index::new();

        index.add_commit(IndexEntry {
            sha: "zzz".to_string(),
            parents: vec![],
            timestamp: 2000,
        });

        index.add_commit(IndexEntry {
            sha: "aaa".to_string(),
            parents: vec![],
            timestamp: 1000,
        });

        index.add_commit(IndexEntry {
            sha: "mmm".to_string(),
            parents: vec![],
            timestamp: 2000,
        });

        // Should be sorted by timestamp, then SHA
        assert_eq!(index.commits[0].sha, "aaa");
        assert_eq!(index.commits[1].sha, "mmm");
        assert_eq!(index.commits[2].sha, "zzz");
    }
}
