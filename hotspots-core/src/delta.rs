//! Parent-relative delta computation
//!
//! Computes deterministic deltas between a snapshot and its parent.
//!
//! Global invariants enforced:
//! - Deltas are parent-relative (use parents[0] only)
//! - Missing parents produce baselines, not errors
//! - Function matching by function_id (file moves are delete + add)
//! - Status based on metrics/LRS/band changes, not file/line movements

use crate::policy::PolicyResults;
use crate::report::MetricsReport;
use crate::snapshot::{FunctionSnapshot, Snapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Schema version for deltas
const DELTA_SCHEMA_VERSION: u32 = 1;

/// Function change status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FunctionStatus {
    New,
    Deleted,
    Modified,
    Unchanged,
}

/// Function state in delta (before or after)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionState {
    pub metrics: MetricsReport,
    pub lrs: f64,
    pub band: String,
}

/// Numeric delta for a function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDelta {
    pub cc: i64,
    pub nd: i64,
    pub fo: i64,
    pub ns: i64,
    pub lrs: f64,
}

/// Band transition information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub struct BandTransition {
    pub from: String,
    pub to: String,
}

/// Single function delta entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDeltaEntry {
    pub function_id: String,
    pub status: FunctionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<FunctionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<FunctionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<FunctionDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band_transition: Option<BandTransition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    /// Fuzzy-matched new function_id when this Deleted entry is likely a rename/move.
    /// Set by second-pass heuristic; absent when exact match was found or no match possible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_hint: Option<String>,
}

/// Commit info in delta
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeltaCommitInfo {
    pub sha: String,
    pub parent: String,
}

/// Complete delta between two snapshots
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Delta {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    pub commit: DeltaCommitInfo,
    pub baseline: bool,
    pub deltas: Vec<FunctionDeltaEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyResults>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<crate::aggregates::DeltaAggregates>,
}

impl Delta {
    /// Create a delta between current and parent snapshots
    ///
    /// # Arguments
    ///
    /// * `current` - Current snapshot
    /// * `parent` - Parent snapshot (None if baseline)
    ///
    /// # Baseline Handling
    ///
    /// If `parent` is None, all functions in `current` are marked as `new`
    /// and `baseline` is set to `true`.
    pub fn new(current: &Snapshot, parent: Option<&Snapshot>) -> Result<Self> {
        validate_snapshot_versions(current, parent)?;
        // Get parent SHA (use parents[0] only for delta computation)
        let parent_sha = current.commit.parents.first().cloned().unwrap_or_default();
        if parent.is_none() {
            return Ok(build_baseline_delta(current, parent_sha));
        }
        let parent_snap = parent.unwrap();
        let parent_funcs: HashMap<&str, &FunctionSnapshot> = parent_snap
            .functions
            .iter()
            .map(|f| (f.function_id.as_str(), f))
            .collect();
        let current_funcs: HashMap<&str, &FunctionSnapshot> = current
            .functions
            .iter()
            .map(|f| (f.function_id.as_str(), f))
            .collect();
        // Collect all function_ids (union of parent and current), sorted deterministically
        let mut all_ids: Vec<&str> = parent_funcs
            .keys()
            .chain(current_funcs.keys())
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        all_ids.sort();
        let mut deltas = compute_function_deltas(&all_ids, &parent_funcs, &current_funcs);
        apply_rename_hints(&mut deltas, &parent_funcs, &current_funcs);
        Ok(Delta {
            schema_version: DELTA_SCHEMA_VERSION,
            commit: DeltaCommitInfo {
                sha: current.commit.sha.clone(),
                parent: parent_sha,
            },
            baseline: false,
            deltas,
            policy: None,
            aggregates: None,
        })
    }

    /// Serialize delta to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize delta to JSON")
    }

    /// Deserialize delta from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let delta: Delta =
            serde_json::from_str(json).context("failed to deserialize delta from JSON")?;

        // Validate schema version
        if delta.schema_version != DELTA_SCHEMA_VERSION {
            anyhow::bail!(
                "delta schema version mismatch: expected {}, got {}",
                DELTA_SCHEMA_VERSION,
                delta.schema_version
            );
        }

        Ok(delta)
    }
}

fn validate_snapshot_versions(current: &Snapshot, parent: Option<&Snapshot>) -> Result<()> {
    if current.schema_version != crate::snapshot::SNAPSHOT_SCHEMA_VERSION {
        anyhow::bail!(
            "current snapshot schema version mismatch: expected {}, got {}",
            crate::snapshot::SNAPSHOT_SCHEMA_VERSION,
            current.schema_version
        );
    }
    if let Some(p) = parent {
        if p.schema_version != crate::snapshot::SNAPSHOT_SCHEMA_VERSION {
            anyhow::bail!(
                "parent snapshot schema version mismatch: expected {}, got {}",
                crate::snapshot::SNAPSHOT_SCHEMA_VERSION,
                p.schema_version
            );
        }
    }
    Ok(())
}

fn build_baseline_delta(current: &Snapshot, parent_sha: String) -> Delta {
    let deltas = current
        .functions
        .iter()
        .map(|func| FunctionDeltaEntry {
            function_id: func.function_id.clone(),
            status: FunctionStatus::New,
            before: None,
            after: Some(FunctionState {
                metrics: func.metrics.clone(),
                lrs: func.lrs,
                band: func.band.clone(),
            }),
            delta: None,
            band_transition: None,
            suppression_reason: func.suppression_reason.clone(),
            rename_hint: None,
        })
        .collect();
    Delta {
        schema_version: DELTA_SCHEMA_VERSION,
        commit: DeltaCommitInfo {
            sha: current.commit.sha.clone(),
            parent: parent_sha,
        },
        baseline: true,
        deltas,
        policy: None,
        aggregates: None,
    }
}

fn compute_function_deltas(
    all_ids: &[&str],
    parent_funcs: &HashMap<&str, &FunctionSnapshot>,
    current_funcs: &HashMap<&str, &FunctionSnapshot>,
) -> Vec<FunctionDeltaEntry> {
    let mut deltas = Vec::new();
    for function_id in all_ids {
        let parent_func = parent_funcs.get(function_id);
        let current_func = current_funcs.get(function_id);
        match (parent_func, current_func) {
            (Some(parent), Some(current)) => {
                let status = if functions_differ(parent, current) {
                    FunctionStatus::Modified
                } else {
                    FunctionStatus::Unchanged
                };
                let delta = if status == FunctionStatus::Modified {
                    Some(compute_function_delta(parent, current))
                } else {
                    None
                };
                let band_transition = if parent.band != current.band {
                    Some(BandTransition {
                        from: parent.band.clone(),
                        to: current.band.clone(),
                    })
                } else {
                    None
                };
                deltas.push(FunctionDeltaEntry {
                    function_id: function_id.to_string(),
                    status,
                    before: Some(FunctionState {
                        metrics: parent.metrics.clone(),
                        lrs: parent.lrs,
                        band: parent.band.clone(),
                    }),
                    after: Some(FunctionState {
                        metrics: current.metrics.clone(),
                        lrs: current.lrs,
                        band: current.band.clone(),
                    }),
                    delta,
                    band_transition,
                    suppression_reason: current.suppression_reason.clone(),
                    rename_hint: None,
                });
            }
            (Some(parent), None) => {
                deltas.push(FunctionDeltaEntry {
                    function_id: function_id.to_string(),
                    status: FunctionStatus::Deleted,
                    before: Some(FunctionState {
                        metrics: parent.metrics.clone(),
                        lrs: parent.lrs,
                        band: parent.band.clone(),
                    }),
                    after: None,
                    delta: Some(compute_delete_delta(parent)),
                    band_transition: None,
                    suppression_reason: parent.suppression_reason.clone(),
                    rename_hint: None,
                });
            }
            (None, Some(current)) => {
                deltas.push(FunctionDeltaEntry {
                    function_id: function_id.to_string(),
                    status: FunctionStatus::New,
                    before: None,
                    after: Some(FunctionState {
                        metrics: current.metrics.clone(),
                        lrs: current.lrs,
                        band: current.band.clone(),
                    }),
                    delta: None,
                    band_transition: None,
                    suppression_reason: current.suppression_reason.clone(),
                    rename_hint: None,
                });
            }
            (None, None) => {
                unreachable!("function_id should exist in at least one snapshot");
            }
        }
    }
    deltas
}

/// Second pass: fuzzy match Deleted+New pairs as likely renames/moves.
///
/// Heuristics (applied in order, first match wins):
///   1. Same function name, different file → likely file rename
///   2. Same file, start line within ±10 → likely function move within file
///
/// Only sets `rename_hint` on the Deleted entry; does not change status.
fn apply_rename_hints(
    deltas: &mut [FunctionDeltaEntry],
    parent_funcs: &HashMap<&str, &FunctionSnapshot>,
    current_funcs: &HashMap<&str, &FunctionSnapshot>,
) {
    let deleted_ids: Vec<String> = deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Deleted)
        .map(|e| e.function_id.clone())
        .collect();
    let new_ids: Vec<String> = deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::New)
        .map(|e| e.function_id.clone())
        .collect();
    if deleted_ids.is_empty() || new_ids.is_empty() {
        return;
    }
    let mut matched_new: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut hints: Vec<(String, String)> = Vec::new();
    'outer: for del_id in &deleted_ids {
        let del_func = match parent_funcs.get(del_id.as_str()) {
            Some(f) => f,
            None => continue,
        };
        let del_name = del_id
            .strip_prefix(&format!("{}::", del_func.file))
            .unwrap_or(del_id.as_str());
        for new_id in &new_ids {
            if matched_new.contains(new_id) {
                continue;
            }
            let new_func = match current_funcs.get(new_id.as_str()) {
                Some(f) => f,
                None => continue,
            };
            let new_name = new_id
                .strip_prefix(&format!("{}::", new_func.file))
                .unwrap_or(new_id.as_str());
            // Case 1: same name, different file → file rename
            if del_name == new_name && del_func.file != new_func.file {
                hints.push((del_id.clone(), new_id.clone()));
                matched_new.insert(new_id.clone());
                continue 'outer;
            }
            // Case 2: same file, line within ±10 → function move
            if del_func.file == new_func.file && del_func.line.abs_diff(new_func.line) <= 10 {
                hints.push((del_id.clone(), new_id.clone()));
                matched_new.insert(new_id.clone());
                continue 'outer;
            }
        }
    }
    for (del_id, new_id) in hints {
        for entry in deltas.iter_mut() {
            if entry.function_id == del_id {
                entry.rename_hint = Some(new_id);
                break;
            }
        }
    }
}

/// Check if two functions differ (based on metrics, LRS, or band)
///
/// Ignores file/line changes - only structural changes matter.
fn functions_differ(parent: &FunctionSnapshot, current: &FunctionSnapshot) -> bool {
    parent.metrics != current.metrics
        || (parent.lrs - current.lrs).abs() > f64::EPSILON
        || parent.band != current.band
}

/// Compute numeric delta between two functions
///
/// Returns deltas for metrics and LRS. Negative deltas are allowed
/// (valid for reverts, refactors).
fn compute_function_delta(parent: &FunctionSnapshot, current: &FunctionSnapshot) -> FunctionDelta {
    FunctionDelta {
        cc: current.metrics.cc as i64 - parent.metrics.cc as i64,
        nd: current.metrics.nd as i64 - parent.metrics.nd as i64,
        fo: current.metrics.fo as i64 - parent.metrics.fo as i64,
        ns: current.metrics.ns as i64 - parent.metrics.ns as i64,
        lrs: current.lrs - parent.lrs,
    }
}

/// Compute delta for a deleted function (all values negative)
fn compute_delete_delta(parent: &FunctionSnapshot) -> FunctionDelta {
    FunctionDelta {
        cc: -(parent.metrics.cc as i64),
        nd: -(parent.metrics.nd as i64),
        fo: -(parent.metrics.fo as i64),
        ns: -(parent.metrics.ns as i64),
        lrs: -parent.lrs,
    }
}

/// Load parent snapshot for delta computation
///
/// Loads the snapshot for `parent_sha` from the repository.
/// Returns None if the snapshot doesn't exist (baseline case).
///
/// # Arguments
///
/// * `repo_root` - Repository root path
/// * `parent_sha` - Parent commit SHA (from parents[0])
///
/// # Errors
///
/// Returns error if snapshot exists but cannot be read/parsed.
pub fn load_parent_snapshot(repo_root: &Path, parent_sha: &str) -> Result<Option<Snapshot>> {
    let snapshot_path = crate::snapshot::snapshot_path(repo_root, parent_sha);

    if !snapshot_path.exists() {
        // Missing parent snapshot - baseline case
        return Ok(None);
    }

    // Load and parse snapshot
    let json = std::fs::read_to_string(&snapshot_path).with_context(|| {
        format!(
            "failed to read parent snapshot: {}",
            snapshot_path.display()
        )
    })?;

    let snapshot = Snapshot::from_json(&json).with_context(|| {
        format!(
            "failed to parse parent snapshot: {}",
            snapshot_path.display()
        )
    })?;

    Ok(Some(snapshot))
}

/// Compute delta for a snapshot against its parent
///
/// Loads parent snapshot and computes delta. If parent is missing,
/// returns baseline delta (baseline=true).
///
/// # Arguments
///
/// * `repo_root` - Repository root path
/// * `current` - Current snapshot
///
/// # Errors
///
/// Returns error if:
/// - Parent snapshot exists but cannot be loaded
/// - Parent snapshot has wrong schema version
/// - Delta computation fails
pub fn compute_delta(repo_root: &Path, current: &Snapshot) -> Result<Delta> {
    // Get parent SHA (use parents[0] only)
    let parent_sha = current.commit.parents.first();

    let parent = if let Some(sha) = parent_sha {
        load_parent_snapshot(repo_root, sha)?
    } else {
        None
    };

    Delta::new(current, parent.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitContext;
    use crate::report::{FunctionRiskReport, MetricsReport};
    use crate::snapshot::Snapshot;

    fn create_test_snapshot(
        sha: &str,
        parent_sha: &str,
        cc: usize,
        lrs: f64,
        band: &str,
    ) -> Snapshot {
        let git_context = GitContext {
            head_sha: sha.to_string(),
            parent_shas: vec![parent_sha.to_string()],
            timestamp: 1705600000,
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
            function: "handler".to_string(),
            line: 42,
            language: "TypeScript".to_string(),
            metrics: MetricsReport {
                cc,
                nd: 2,
                fo: 3,
                ns: 1,
                loc: 10,
            },
            risk: crate::report::RiskReport {
                r_cc: 2.0,
                r_nd: 1.0,
                r_fo: 1.0,
                r_ns: 1.0,
            },
            lrs,
            band: band.to_string(),
            suppression_reason: None,
            callees: vec![],
        };

        Snapshot::new(git_context, vec![report])
    }

    #[test]
    fn test_baseline_delta() {
        let current = create_test_snapshot("abc123", "", 5, 4.8, "moderate");

        // No parent - should be baseline
        let delta = Delta::new(&current, None).expect("should create baseline delta");

        assert!(delta.baseline);
        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].status, FunctionStatus::New);
    }

    #[test]
    fn test_modified_delta() {
        let parent = create_test_snapshot("parent123", "grandparent", 4, 3.9, "moderate");
        let current = create_test_snapshot("current123", "parent123", 6, 6.2, "high");

        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        assert!(!delta.baseline);
        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].status, FunctionStatus::Modified);

        let delta_values = delta.deltas[0].delta.as_ref().unwrap();
        assert_eq!(delta_values.cc, 2); // 6 - 4 = 2
        assert!((delta_values.lrs - 2.3).abs() < 0.01); // 6.2 - 3.9 ≈ 2.3

        // Check band transition
        let transition = delta.deltas[0].band_transition.as_ref().unwrap();
        assert_eq!(transition.from, "moderate");
        assert_eq!(transition.to, "high");
    }

    #[test]
    fn test_unchanged_delta() {
        let parent = create_test_snapshot("parent123", "grandparent", 5, 4.8, "moderate");
        let current = create_test_snapshot("current123", "parent123", 5, 4.8, "moderate");

        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].status, FunctionStatus::Unchanged);
        assert!(delta.deltas[0].delta.is_none());
        assert!(delta.deltas[0].band_transition.is_none());
    }

    #[test]
    fn test_negative_deltas() {
        let parent = create_test_snapshot("parent123", "grandparent", 6, 6.2, "high");
        let current = create_test_snapshot("current123", "parent123", 4, 3.9, "moderate");

        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        let delta_values = delta.deltas[0].delta.as_ref().unwrap();
        assert_eq!(delta_values.cc, -2); // 4 - 6 = -2 (negative allowed)
        assert!(delta_values.lrs < 0.0); // Negative LRS delta allowed
    }

    #[test]
    fn test_deleted_function() {
        let parent = create_test_snapshot("parent123", "grandparent", 5, 4.8, "moderate");

        // Current has no functions (empty)
        let git_context = GitContext {
            head_sha: "current123".to_string(),
            parent_shas: vec!["parent123".to_string()],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
            message: Some("test commit".to_string()),
            author: Some("Test Author".to_string()),
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };
        let current = Snapshot::new(git_context, vec![]);

        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].status, FunctionStatus::Deleted);
        assert!(delta.deltas[0].before.is_some());
        assert!(delta.deltas[0].after.is_none());
    }
}
