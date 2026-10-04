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
use crate::risk::RiskBand;
use crate::snapshot::{FunctionSnapshot, Snapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Schema version for deltas
const DELTA_SCHEMA_VERSION: u32 = 2;

/// Default epsilon below which an LRS delta is treated as floating-point noise, not a real
/// change, when building `ChangeRisk`'s scored function set (hotspots#202). Looser than
/// `functions_differ`'s `f64::EPSILON` by design — this is a separate filter over an
/// already-computed `deltas[]`, not a change to how `status`/`delta` themselves are computed.
pub const CHANGE_RISK_NOISE_EPSILON: f64 = 1e-9;

/// Score formula version for `ChangeRisk.score.kind == "sum_positive_delta_lrs"`. Frozen per
/// hotspots#202's spec — bump only when the formula itself changes, not when inputs do.
const CHANGE_RISK_SCORE_VERSION: &str = "1.0.0";

/// Change-level (commit- or PR-level) risk score, additive to the per-function `deltas[]`.
/// See `compute_change_risk` for the scoring definition. `scope.kind` is an open string
/// (`"commit"`/`"range"` today) rather than an enum, since a hosted API caller may know a
/// more specific scope (`"pull_request"`/`"release"`) without requiring a CLI schema change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeRisk {
    pub scope: ChangeRiskScope,
    pub score: ChangeRiskScoreValue,
    pub components: ChangeRiskComponents,
    pub inputs: ChangeRiskInputs,
    pub tool_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeRiskScope {
    pub kind: String,
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeRiskScoreValue {
    pub kind: String,
    pub version: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeRiskComponents {
    pub max_delta_lrs: f64,
    pub sum_positive_delta_lrs: f64,
    pub new_critical_functions: usize,
    pub changed_functions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeRiskInputs {
    pub functions_scored: usize,
    pub noise_epsilon: f64,
    pub overlap_filter_applied: bool,
}

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
    pub band: RiskBand,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_risk: Option<ChangeRisk>,
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
        // `function_id` embeds each function's absolute file path (`<abs path>::<symbol>`).
        // Two snapshots analyzed from different absolute roots — e.g. `hotspots diff`'s
        // `--auto-analyze`, which builds each ref in its own temp git worktree — never share
        // a common root, so raw function_id equality spuriously fails for every function even
        // when the code is identical, misreporting the whole snapshot as all-new + all-deleted.
        // Pair functions on each snapshot's own root-stripped relative key instead, just for
        // this matching step; `parent_funcs`/`current_funcs` above (absolute-keyed) are the
        // ones `apply_rename_hints` uses, and the real absolute `function_id` is what every
        // built `FunctionDeltaEntry` reports — this is a matching-key fix only, not a format
        // change, so serialized output/downstream consumers are unaffected.
        let parent_root = common_root(&parent_snap.functions);
        let current_root = common_root(&current.functions);
        let parent_by_rel: HashMap<String, &FunctionSnapshot> = parent_snap
            .functions
            .iter()
            .map(|f| (relative_match_key(f, parent_root.as_deref()), f))
            .collect();
        let current_by_rel: HashMap<String, &FunctionSnapshot> = current
            .functions
            .iter()
            .map(|f| (relative_match_key(f, current_root.as_deref()), f))
            .collect();
        // Collect all match keys (union of parent and current), sorted deterministically
        let mut all_rel_ids: Vec<&str> = parent_by_rel
            .keys()
            .chain(current_by_rel.keys())
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        all_rel_ids.sort();
        let mut deltas = compute_function_deltas(&all_rel_ids, &parent_by_rel, &current_by_rel);
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
            change_risk: None,
        })
    }

    /// Serialize delta to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize delta to JSON")
    }

    /// Serialize delta entries as newline-delimited JSON (one entry per line).
    pub fn to_jsonl(&self) -> Result<String> {
        let mut lines = Vec::with_capacity(self.deltas.len());
        for entry in &self.deltas {
            lines.push(
                serde_json::to_string(entry).context("failed to serialize delta entry to JSON")?,
            );
        }
        Ok(lines.join("\n"))
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
                band: func.band,
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
        change_risk: None,
    }
}

fn compute_function_deltas(
    all_rel_ids: &[&str],
    parent_by_rel: &HashMap<String, &FunctionSnapshot>,
    current_by_rel: &HashMap<String, &FunctionSnapshot>,
) -> Vec<FunctionDeltaEntry> {
    let mut deltas = Vec::new();
    for rel_id in all_rel_ids {
        let parent_func = parent_by_rel.get(*rel_id);
        let current_func = current_by_rel.get(*rel_id);
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
                        from: parent.band.as_str().to_string(),
                        to: current.band.as_str().to_string(),
                    })
                } else {
                    None
                };
                // `current`'s own function_id (absolute) is the entry's identity — matches
                // pre-existing behavior when both snapshots share a root (relative key ==
                // suffix of the absolute one in that case) and is correct when they don't.
                deltas.push(FunctionDeltaEntry {
                    function_id: current.function_id.clone(),
                    status,
                    before: Some(FunctionState {
                        metrics: parent.metrics.clone(),
                        lrs: parent.lrs,
                        band: parent.band,
                    }),
                    after: Some(FunctionState {
                        metrics: current.metrics.clone(),
                        lrs: current.lrs,
                        band: current.band,
                    }),
                    delta,
                    band_transition,
                    suppression_reason: current.suppression_reason.clone(),
                    rename_hint: None,
                });
            }
            (Some(parent), None) => {
                deltas.push(FunctionDeltaEntry {
                    function_id: parent.function_id.clone(),
                    status: FunctionStatus::Deleted,
                    before: Some(FunctionState {
                        metrics: parent.metrics.clone(),
                        lrs: parent.lrs,
                        band: parent.band,
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
                    function_id: current.function_id.clone(),
                    status: FunctionStatus::New,
                    before: None,
                    after: Some(FunctionState {
                        metrics: current.metrics.clone(),
                        lrs: current.lrs,
                        band: current.band,
                    }),
                    delta: None,
                    band_transition: None,
                    suppression_reason: current.suppression_reason.clone(),
                    rename_hint: None,
                });
            }
            (None, None) => {
                unreachable!("rel_id should exist in at least one snapshot");
            }
        }
    }
    deltas
}

/// The longest common path-component prefix shared by every function's file in a snapshot,
/// used to strip each snapshot's own analysis root before matching functions across two
/// snapshots (see `Delta::new`'s doc comment on why). `None` for an empty snapshot, or when
/// there's only one distinct file (its own parent directory is used as the "root" in that
/// case, since a bare filename carries no path information to compare against the other side).
fn common_root(functions: &[FunctionSnapshot]) -> Option<std::path::PathBuf> {
    let mut files = functions.iter().map(|f| Path::new(&f.file));
    let first = files.next()?;
    let mut root: Vec<std::path::Component> = first
        .parent()
        .map(|p| p.components().collect())
        .unwrap_or_default();
    for file in files {
        let parent_components: Vec<_> = file
            .parent()
            .map(|p| p.components().collect())
            .unwrap_or_default();
        let common_len = root
            .iter()
            .zip(parent_components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        root.truncate(common_len);
        if root.is_empty() {
            break;
        }
    }
    if root.is_empty() {
        None
    } else {
        Some(root.into_iter().collect())
    }
}

/// `<relative file>::<symbol>` using `root` to strip the snapshot-specific absolute prefix
/// from `f.file`, or the raw (absolute) `function_id` unchanged if `root` is `None` or
/// doesn't actually prefix this file (defensive fallback — matches prior behavior exactly
/// for a snapshot with no discernible common root instead of producing an inconsistent key).
fn relative_match_key(f: &FunctionSnapshot, root: Option<&Path>) -> String {
    match root.and_then(|r| Path::new(&f.file).strip_prefix(r).ok()) {
        Some(rel) => format!("{}::{}", rel.to_string_lossy(), symbol_of(f)),
        None => f.function_id.clone(),
    }
}

/// The `<symbol>` half of `function_id` (everything after the last `<file>::` separator),
/// i.e. what's left once the absolute-path half is stripped off.
fn symbol_of(f: &FunctionSnapshot) -> &str {
    f.function_id
        .strip_prefix(&format!("{}::", f.file))
        .unwrap_or(&f.function_id)
}

/// Find the best rename match for a deleted function among new functions.
///
/// Returns the new function ID if a match is found (first match wins).
fn find_rename_match<'a>(
    del_id: &str,
    del_func: &FunctionSnapshot,
    new_ids: &'a [String],
    current_funcs: &HashMap<&str, &FunctionSnapshot>,
    matched_new: &std::collections::HashSet<String>,
) -> Option<&'a str> {
    let del_name = del_id
        .strip_prefix(&format!("{}::", del_func.file))
        .unwrap_or(del_id);
    for new_id in new_ids {
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
        if del_name == new_name && del_func.file != new_func.file {
            return Some(new_id.as_str());
        }
        if del_func.file == new_func.file && del_func.line.abs_diff(new_func.line) <= 10 {
            return Some(new_id.as_str());
        }
    }
    None
}

/// Write rename_hint onto each Deleted entry that was matched
fn apply_hints(deltas: &mut [FunctionDeltaEntry], hints: &[(String, String)]) {
    for (del_id, new_id) in hints {
        for entry in deltas.iter_mut() {
            if entry.function_id == *del_id {
                entry.rename_hint = Some(new_id.clone());
                break;
            }
        }
    }
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
    let mut deleted_ids: Vec<String> = deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Deleted)
        .map(|e| e.function_id.clone())
        .collect();
    let mut new_ids: Vec<String> = deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::New)
        .map(|e| e.function_id.clone())
        .collect();
    if deleted_ids.is_empty() || new_ids.is_empty() {
        return;
    }
    // Sort candidates by a stable tiebreak key (function_id, which is derived
    // from file path + function name + line) so "first match wins" no longer
    // depends on incidental input/collection ordering.
    deleted_ids.sort();
    new_ids.sort();
    let mut matched_new: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut hints: Vec<(String, String)> = Vec::new();
    for del_id in &deleted_ids {
        let del_func = match parent_funcs.get(del_id.as_str()) {
            Some(f) => f,
            None => continue,
        };
        if let Some(new_id) =
            find_rename_match(del_id, del_func, &new_ids, current_funcs, &matched_new)
        {
            hints.push((del_id.clone(), new_id.to_string()));
            matched_new.insert(new_id.to_string());
        }
    }
    apply_hints(deltas, &hints);
}

/// Identify pure renames: a Deleted entry whose `rename_hint` points at a New
/// entry, where the two sides carry identical metrics/LRS/band (i.e. the
/// rename carried no content change).
///
/// Returns the set of `function_id`s on both sides of each such pair, so
/// callers (PR risk summary, per-file delta aggregates) can net them out
/// instead of counting them as independent new+deleted entries.
pub fn pure_rename_function_ids(
    deltas: &[FunctionDeltaEntry],
) -> std::collections::HashSet<String> {
    let new_by_id: HashMap<&str, &FunctionDeltaEntry> = deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::New)
        .map(|e| (e.function_id.as_str(), e))
        .collect();

    let mut excluded = std::collections::HashSet::new();
    for entry in deltas {
        if entry.status != FunctionStatus::Deleted {
            continue;
        }
        let Some(hint) = &entry.rename_hint else {
            continue;
        };
        let Some(new_entry) = new_by_id.get(hint.as_str()) else {
            continue;
        };
        if entry.before == new_entry.after {
            excluded.insert(entry.function_id.clone());
            excluded.insert(new_entry.function_id.clone());
        }
    }
    excluded
}

/// Compute the change-level `ChangeRisk` block (hotspots#202) for a non-baseline delta.
///
/// `scope.kind` is `"commit"` for `--mode delta` (parent-relative) or `"range"` for
/// `hotspots diff <base> <head>` (arbitrary two refs). `diff_lines` is best-effort — an
/// empty map (shallow clone, failed `git diff`) skips the overlap filter entirely rather
/// than scoring zero functions, with `inputs.overlap_filter_applied` recording which
/// happened. Callers should check `delta.baseline` themselves before calling this (baseline
/// deltas have no base to score against), matching the `fn_changed_lines` call sites'
/// existing pattern.
pub fn compute_change_risk(
    delta: &Delta,
    head_snapshot: &Snapshot,
    base_snapshot: &Snapshot,
    diff_lines: &HashMap<String, crate::git::DiffLineSet>,
    repo_root: &Path,
    scope: ChangeRiskScope,
) -> ChangeRisk {
    let overlap_filter_applied = !diff_lines.is_empty();
    let pure_renames = pure_rename_function_ids(&delta.deltas);

    let mut sum_positive_delta_lrs = 0.0_f64;
    let mut max_delta_lrs = 0.0_f64;
    let mut new_critical_functions = 0usize;
    let mut functions_scored = 0usize;

    for entry in &delta.deltas {
        if pure_renames.contains(&entry.function_id) {
            continue;
        }
        let in_scored_set = match entry.status {
            FunctionStatus::New | FunctionStatus::Modified | FunctionStatus::Deleted => {
                let lrs_changed = entry
                    .delta
                    .as_ref()
                    .map(|d| d.lrs.abs() > CHANGE_RISK_NOISE_EPSILON)
                    .unwrap_or(false);
                let band_changed = entry.band_transition.is_some();
                // New/Deleted entries never populate `delta`/`band_transition` (only the
                // Modified branch in `compute_function_deltas` does) — without this they'd
                // be unconditionally excluded from the scored set regardless of actual risk,
                // contradicting the issue spec's "an added function contributes its full
                // after.lrs." Always include them; the noise-epsilon/band-transition pair is
                // only meaningful as a filter for Modified entries.
                let is_new_or_deleted =
                    matches!(entry.status, FunctionStatus::New | FunctionStatus::Deleted);
                (lrs_changed || band_changed || is_new_or_deleted)
                    && (!overlap_filter_applied
                        || crate::git::function_overlaps_changed_hunk(
                            entry,
                            head_snapshot,
                            base_snapshot,
                            diff_lines,
                            repo_root,
                        ))
            }
            FunctionStatus::Unchanged => false,
        };
        if !in_scored_set {
            continue;
        }

        functions_scored += 1;

        let contribution = match entry.status {
            FunctionStatus::New => entry.after.as_ref().map(|s| s.lrs).unwrap_or(0.0),
            FunctionStatus::Modified => entry.delta.as_ref().map(|d| d.lrs.max(0.0)).unwrap_or(0.0),
            FunctionStatus::Deleted => 0.0,
            FunctionStatus::Unchanged => 0.0,
        };
        sum_positive_delta_lrs += contribution;
        max_delta_lrs = max_delta_lrs.max(contribution);

        if entry.status == FunctionStatus::New {
            if let Some(after) = &entry.after {
                if after.band == RiskBand::Critical {
                    new_critical_functions += 1;
                }
            }
        }
    }

    ChangeRisk {
        scope,
        score: ChangeRiskScoreValue {
            kind: "sum_positive_delta_lrs".to_string(),
            version: CHANGE_RISK_SCORE_VERSION.to_string(),
            value: sum_positive_delta_lrs,
        },
        components: ChangeRiskComponents {
            max_delta_lrs,
            sum_positive_delta_lrs,
            new_critical_functions,
            changed_functions: functions_scored,
        },
        inputs: ChangeRiskInputs {
            functions_scored,
            noise_epsilon: CHANGE_RISK_NOISE_EPSILON,
            overlap_filter_applied,
        },
        tool_version: head_snapshot.analysis.tool_version.clone(),
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
    crate::snapshot::load_snapshot(repo_root, parent_sha)
}

/// Compute delta for a snapshot against its parent
///
/// Loads parent snapshot and computes delta. If parent is missing,
/// returns baseline delta (baseline=true).
///
/// A baseline delta always skips policy evaluation ([`crate::policy::evaluate_policies`])
/// since there is no prior state to compare against. That's expected and safe when the
/// current commit genuinely has no parent (a repo's root commit). It is a silent
/// false-confidence trap when the commit *does* have a parent but that parent's snapshot
/// simply isn't on disk (missing history, a cold cache, a fresh CI runner) — in that case
/// this prints a loud warning so a CI log makes clear that policy checks did not actually
/// run, rather than the run silently reporting "passed".
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
        let loaded = load_parent_snapshot(repo_root, sha)?;
        if loaded.is_none() {
            eprintln!(
                "warning: no snapshot found for parent commit {sha}; treating this analysis \
                 as a baseline and SKIPPING policy evaluation. If parent history is expected \
                 to be available (e.g. in CI), this may hide real policy violations — run \
                 `hotspots analyze` on the parent commit first, or restore its snapshot cache."
            );
        }
        loaded
    } else {
        None
    };

    Delta::new(current, parent.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitContext;
    use crate::language::Language;
    use crate::report::{FunctionRiskReport, MetricsReport};
    use crate::risk::RiskBand;
    use crate::snapshot::Snapshot;

    // ── common_root / relative_match_key (the auto-analyze cross-worktree fix) ──

    #[test]
    fn common_root_finds_shared_ancestor_across_multiple_files() {
        let fns = vec![
            make_fn_snapshot("a", "/tmp/wt1/src/x.rs", 1),
            make_fn_snapshot("b", "/tmp/wt1/src/sub/y.rs", 1),
            make_fn_snapshot("c", "/tmp/wt1/tests/z.rs", 1),
        ];
        assert_eq!(
            common_root(&fns),
            Some(std::path::PathBuf::from("/tmp/wt1"))
        );
    }

    #[test]
    fn common_root_similar_but_distinct_prefixes_dont_false_match() {
        // "/tmp/wtA-old" and "/tmp/wtA-new" share the string prefix "/tmp/wtA" but are
        // different directories — component-wise comparison must not treat them as shared.
        let fns = vec![
            make_fn_snapshot("a", "/tmp/wtA-old/src/x.rs", 1),
            make_fn_snapshot("b", "/tmp/wtA-new/src/x.rs", 1),
        ];
        assert_eq!(common_root(&fns), Some(std::path::PathBuf::from("/tmp")));
    }

    #[test]
    fn common_root_empty_snapshot_is_none() {
        assert_eq!(common_root(&[]), None);
    }

    #[test]
    fn relative_match_key_strips_each_snapshots_own_root() {
        // The actual bug: two snapshots of identical code analyzed from two different
        // temp-worktree roots must produce the SAME match key.
        let f1 = make_fn_snapshot("foo", "/tmp/wt1/src/x.rs", 10);
        let f2 = make_fn_snapshot("foo", "/tmp/wt2/src/x.rs", 10);
        let root1 = std::path::PathBuf::from("/tmp/wt1");
        let root2 = std::path::PathBuf::from("/tmp/wt2");
        assert_eq!(
            relative_match_key(&f1, Some(&root1)),
            relative_match_key(&f2, Some(&root2))
        );
    }

    #[test]
    fn relative_match_key_distinguishes_genuinely_different_files() {
        let root = std::path::PathBuf::from("/tmp/wt1");
        let x = make_fn_snapshot("foo", "/tmp/wt1/src/x.rs", 10);
        let y = make_fn_snapshot("foo", "/tmp/wt1/src/y.rs", 10);
        assert_ne!(
            relative_match_key(&x, Some(&root)),
            relative_match_key(&y, Some(&root))
        );
    }

    #[test]
    fn relative_match_key_falls_back_to_function_id_when_root_is_none() {
        let f = make_fn_snapshot("foo", "/tmp/wt1/src/x.rs", 10);
        assert_eq!(relative_match_key(&f, None), f.function_id);
    }

    #[test]
    fn delta_new_matches_identical_code_across_different_worktree_roots() {
        // End-to-end regression test for the real bug: two snapshots with byte-identical
        // function content, analyzed from two different absolute roots (as --auto-analyze's
        // two temp worktrees would produce), must report Unchanged, not New+Deleted for
        // every function.
        let mk_report = |name: &str, file: String, line: u32| FunctionRiskReport {
            file,
            function: name.to_string(),
            line,
            language: Language::TypeScript,
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 5,
            },
            risk: crate::report::RiskReport {
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
        let mk = |root: &str| {
            let git_context = GitContext {
                head_sha: "sha".to_string(),
                parent_shas: vec!["parent_sha".to_string()],
                timestamp: 1705600000,
                branch: Some("main".to_string()),
                is_detached: false,
                message: None,
                author: None,
                is_fix_commit: Some(false),
                is_revert_commit: Some(false),
                ticket_ids: vec![],
            };
            Snapshot::new(
                git_context,
                vec![
                    mk_report("foo", format!("{root}/src/x.rs"), 10),
                    mk_report("bar", format!("{root}/src/y.rs"), 5),
                ],
            )
        };
        let parent = mk("/tmp/worktree-aaaa");
        let current = mk("/tmp/worktree-bbbb");

        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        assert_eq!(
            delta.deltas.len(),
            2,
            "expected exactly 2 matched functions, not 4 (2 new + 2 deleted)"
        );
        for entry in &delta.deltas {
            assert_eq!(
                entry.status,
                FunctionStatus::Unchanged,
                "function_id {} should match across worktree roots",
                entry.function_id
            );
        }
    }

    fn create_test_snapshot(
        sha: &str,
        parent_sha: &str,
        cc: u32,
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
            language: Language::TypeScript,
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
            band: RiskBand::parse(band).unwrap_or(RiskBand::Low),
            suppression_reason: None,
            patterns: vec![],
            pattern_details: None,
            callees: vec![],
            explanation: None,
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
    fn test_compute_delta_missing_parent_snapshot_is_still_baseline() {
        // Regression test for hotspots#141: a commit that HAS a parent SHA, but
        // whose parent snapshot simply isn't on disk (missing history, cold
        // cache, fresh CI runner), must still produce a baseline delta — the
        // fix here is a loud eprintln! warning (not asserted here, matching
        // this codebase's convention of not unit-testing warning text
        // elsewhere), not a change to the return value. This confirms the
        // `compute_delta` code path touched by that fix still behaves
        // identically for callers.
        let repo_root_dir = tempfile::tempdir().expect("failed to create temp dir");
        let repo_root = repo_root_dir.path();

        // parent123 has no snapshot persisted anywhere under repo_root.
        let current = create_test_snapshot("current123", "parent123", 5, 4.8, "moderate");

        let delta =
            compute_delta(repo_root, &current).expect("should compute baseline delta, not error");

        assert!(delta.baseline);
        assert_eq!(delta.commit.parent, "parent123");
        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].status, FunctionStatus::New);
    }

    #[test]
    fn test_compute_delta_true_root_commit_is_baseline() {
        // A genuine root commit (parent_shas is an EMPTY vec, not a vec
        // containing an empty string — create_test_snapshot always inserts
        // one entry, so it's built manually here) should also produce a
        // baseline delta, with no warning expected (distinct from the
        // missing-snapshot case above — this is the normal, expected
        // baseline path, since `current.commit.parents.first()` is None).
        let repo_root_dir = tempfile::tempdir().expect("failed to create temp dir");
        let repo_root = repo_root_dir.path();

        // create_test_snapshot always inserts one parent_shas entry, so the
        // genuinely-empty-parents case is built by clearing it after the fact.
        let mut current = create_test_snapshot("root123", "unused", 5, 4.8, "moderate");
        current.commit.parents.clear();

        assert!(current.commit.parents.is_empty());

        let delta = compute_delta(repo_root, &current).expect("should compute baseline delta");

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

    fn make_fn_snapshot(name: &str, file: &str, line: u32) -> FunctionSnapshot {
        let git_context = GitContext {
            head_sha: "sha".to_string(),
            parent_shas: vec![],
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
            file: file.to_string(),
            function: name.to_string(),
            line,
            language: Language::TypeScript,
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 5,
            },
            risk: crate::report::RiskReport {
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
        Snapshot::new(git_context, vec![report]).functions[0].clone()
    }

    fn make_delta_entry(function_id: &str, status: FunctionStatus) -> FunctionDeltaEntry {
        FunctionDeltaEntry {
            function_id: function_id.to_string(),
            status,
            before: None,
            after: None,
            delta: None,
            band_transition: None,
            suppression_reason: None,
            rename_hint: None,
        }
    }

    #[test]
    fn test_apply_rename_hints_is_order_independent() {
        // Two deleted functions with the same name in different files both
        // qualify for a rename match against a single new function with that
        // name (heuristic #1: same name, different file). Only one deleted
        // entry can win the match (each new function is claimed at most
        // once), so the outcome must be decided by a stable tiebreak key
        // rather than by incidental input ordering.
        let del_a = make_fn_snapshot("a.ts::foo", "a.ts", 1);
        let del_b = make_fn_snapshot("b.ts::foo", "b.ts", 1);
        let new_c = make_fn_snapshot("c.ts::foo", "c.ts", 1);

        let parent_funcs: HashMap<&str, &FunctionSnapshot> =
            [("a.ts::foo", &del_a), ("b.ts::foo", &del_b)]
                .into_iter()
                .collect();
        let current_funcs: HashMap<&str, &FunctionSnapshot> =
            [("c.ts::foo", &new_c)].into_iter().collect();

        // Order 1: b before a.
        let mut deltas_order1 = vec![
            make_delta_entry("b.ts::foo", FunctionStatus::Deleted),
            make_delta_entry("a.ts::foo", FunctionStatus::Deleted),
            make_delta_entry("c.ts::foo", FunctionStatus::New),
        ];
        apply_rename_hints(&mut deltas_order1, &parent_funcs, &current_funcs);

        // Order 2: a before b (shuffled relative to order 1).
        let mut deltas_order2 = vec![
            make_delta_entry("a.ts::foo", FunctionStatus::Deleted),
            make_delta_entry("c.ts::foo", FunctionStatus::New),
            make_delta_entry("b.ts::foo", FunctionStatus::Deleted),
        ];
        apply_rename_hints(&mut deltas_order2, &parent_funcs, &current_funcs);

        let hint_for = |deltas: &[FunctionDeltaEntry], id: &str| {
            deltas
                .iter()
                .find(|e| e.function_id == id)
                .and_then(|e| e.rename_hint.clone())
        };

        // Whichever deleted entry wins, both orderings must agree.
        assert_eq!(
            hint_for(&deltas_order1, "a.ts::foo"),
            hint_for(&deltas_order2, "a.ts::foo")
        );
        assert_eq!(
            hint_for(&deltas_order1, "b.ts::foo"),
            hint_for(&deltas_order2, "b.ts::foo")
        );
        // Exactly one of the two deleted entries should have been matched.
        let matched_count = [
            hint_for(&deltas_order1, "a.ts::foo"),
            hint_for(&deltas_order1, "b.ts::foo"),
        ]
        .iter()
        .filter(|h| h.is_some())
        .count();
        assert_eq!(matched_count, 1);
    }

    fn make_state(lrs: f64, band: RiskBand) -> FunctionState {
        FunctionState {
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 5,
            },
            lrs,
            band,
        }
    }

    #[test]
    fn test_pure_rename_function_ids_nets_out_identical_rename() {
        // Regression for #223: a pure rename (rename_hint set, identical
        // before/after state) must be netted out, not double-counted as a
        // separate new+deleted pair.
        let state = make_state(5.4, RiskBand::Moderate);
        let mut deleted = make_delta_entry("old/path.ts::foo", FunctionStatus::Deleted);
        deleted.before = Some(state.clone());
        deleted.rename_hint = Some("new/path.ts::foo".to_string());

        let mut created = make_delta_entry("new/path.ts::foo", FunctionStatus::New);
        created.after = Some(state);

        let deltas = vec![deleted, created];
        let excluded = pure_rename_function_ids(&deltas);

        assert!(excluded.contains("old/path.ts::foo"));
        assert!(excluded.contains("new/path.ts::foo"));
    }

    #[test]
    fn test_pure_rename_function_ids_keeps_renames_with_content_change() {
        // A rename_hint paired with a metrics/LRS change is a real modification
        // riding along with a move — it must NOT be netted out.
        let mut deleted = make_delta_entry("old/path.ts::foo", FunctionStatus::Deleted);
        deleted.before = Some(make_state(5.4, RiskBand::Moderate));
        deleted.rename_hint = Some("new/path.ts::foo".to_string());

        let mut created = make_delta_entry("new/path.ts::foo", FunctionStatus::New);
        created.after = Some(make_state(9.0, RiskBand::High));

        let deltas = vec![deleted, created];
        let excluded = pure_rename_function_ids(&deltas);

        assert!(excluded.is_empty());
    }

    #[test]
    fn test_pr_risk_summary_nets_out_pure_rename() {
        use crate::aggregates::{compute_pr_risk_summary, ChangeSizeThresholds};

        let state = make_state(5.4, RiskBand::Moderate);
        let mut deleted = make_delta_entry("old/path.ts::foo", FunctionStatus::Deleted);
        deleted.before = Some(state.clone());
        deleted.rename_hint = Some("new/path.ts::foo".to_string());

        let mut created = make_delta_entry("new/path.ts::foo", FunctionStatus::New);
        created.after = Some(state);

        let delta = Delta {
            schema_version: DELTA_SCHEMA_VERSION,
            commit: DeltaCommitInfo {
                sha: "sha".to_string(),
                parent: "parent".to_string(),
            },
            baseline: false,
            deltas: vec![deleted, created],
            policy: None,
            aggregates: None,
            change_risk: None,
        };

        let summary = compute_pr_risk_summary(&delta, 0, &ChangeSizeThresholds::default());
        assert_eq!(summary.new_count, 0, "pure rename must not count as new");
        assert_eq!(
            summary.deleted_count, 0,
            "pure rename must not count as deleted"
        );
        assert_eq!(
            summary.pr_risk_score, 0.0,
            "pure rename must contribute zero risk score"
        );
    }

    // ── compute_change_risk (hotspots#202) ──────────────────────────────────

    fn test_scope() -> ChangeRiskScope {
        ChangeRiskScope {
            kind: "commit".to_string(),
            base: "parent123".to_string(),
            head: "current123".to_string(),
        }
    }

    #[test]
    fn change_risk_empty_diff_lines_skips_overlap_filter_and_scores_by_lrs() {
        let parent = create_test_snapshot("parent123", "grandparent", 4, 3.9, "moderate");
        let current = create_test_snapshot("current123", "parent123", 6, 6.2, "high");
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &HashMap::new(),
            Path::new("/repo"),
            test_scope(),
        );

        assert!(!cr.inputs.overlap_filter_applied);
        assert_eq!(cr.components.changed_functions, 1);
        assert!((cr.components.sum_positive_delta_lrs - 2.3).abs() < 0.01);
        assert!((cr.components.max_delta_lrs - 2.3).abs() < 0.01);
        assert_eq!(cr.score.value, cr.components.sum_positive_delta_lrs);
        assert_eq!(cr.score.kind, "sum_positive_delta_lrs");
        assert_eq!(cr.score.version, "1.0.0");
    }

    #[test]
    fn change_risk_noise_epsilon_excludes_tiny_lrs_change() {
        // Diff (5e-10) is above functions_differ's f64::EPSILON gate (so status is still
        // Modified, delta is still populated) but below CHANGE_RISK_NOISE_EPSILON (1e-9) and
        // the band is unchanged — must be excluded from change_risk's own scored set.
        let parent = create_test_snapshot("parent123", "grandparent", 4, 3.9, "moderate");
        let current = create_test_snapshot("current123", "parent123", 4, 3.9 + 5e-10, "moderate");
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");
        assert_eq!(delta.deltas[0].status, FunctionStatus::Modified);

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &HashMap::new(),
            Path::new("/repo"),
            test_scope(),
        );

        assert_eq!(cr.components.changed_functions, 0);
        assert_eq!(cr.score.value, 0.0);
    }

    #[test]
    fn change_risk_overlap_filter_excludes_non_overlapping_function() {
        // create_test_snapshot's function sits at line 42, loc 10 -> span 42..=51.
        let parent = create_test_snapshot("parent123", "grandparent", 4, 3.9, "moderate");
        let current = create_test_snapshot("current123", "parent123", 6, 6.2, "high");
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        let mut diff_lines = HashMap::new();
        diff_lines.insert(
            "src/foo.ts".to_string(),
            crate::git::DiffLineSet {
                old_lines: Default::default(),
                new_lines: [200u32].into_iter().collect(), // outside the 42..=51 span
            },
        );

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &diff_lines,
            Path::new("/repo"),
            test_scope(),
        );

        assert!(cr.inputs.overlap_filter_applied);
        assert_eq!(
            cr.components.changed_functions, 0,
            "band-transitioning function with no line overlap must still be excluded"
        );
    }

    #[test]
    fn change_risk_overlap_filter_includes_overlapping_function() {
        let parent = create_test_snapshot("parent123", "grandparent", 4, 3.9, "moderate");
        let current = create_test_snapshot("current123", "parent123", 6, 6.2, "high");
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");

        let mut diff_lines = HashMap::new();
        diff_lines.insert(
            "src/foo.ts".to_string(),
            crate::git::DiffLineSet {
                old_lines: Default::default(),
                new_lines: [45u32].into_iter().collect(), // inside the 42..=51 span
            },
        );

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &diff_lines,
            Path::new("/repo"),
            test_scope(),
        );

        assert!(cr.inputs.overlap_filter_applied);
        assert_eq!(cr.components.changed_functions, 1);
    }

    #[test]
    fn change_risk_new_function_contributes_full_after_lrs_and_counts_critical() {
        let git_context = GitContext {
            head_sha: "parent123".to_string(),
            parent_shas: vec![],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
            message: None,
            author: None,
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };
        let parent = Snapshot::new(git_context, vec![]);
        let current = create_test_snapshot("current123", "parent123", 9, 9.5, "critical");
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");
        assert_eq!(delta.deltas[0].status, FunctionStatus::New);

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &HashMap::new(),
            Path::new("/repo"),
            test_scope(),
        );

        assert_eq!(cr.components.new_critical_functions, 1);
        assert!((cr.score.value - 9.5).abs() < 0.01);
    }

    #[test]
    fn change_risk_deleted_function_contributes_zero() {
        let parent = create_test_snapshot("parent123", "grandparent", 5, 9.5, "critical");
        let git_context = GitContext {
            head_sha: "current123".to_string(),
            parent_shas: vec!["parent123".to_string()],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
            message: None,
            author: None,
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };
        let current = Snapshot::new(git_context, vec![]);
        let delta = Delta::new(&current, Some(&parent)).expect("should create delta");
        assert_eq!(delta.deltas[0].status, FunctionStatus::Deleted);

        let cr = compute_change_risk(
            &delta,
            &current,
            &parent,
            &HashMap::new(),
            Path::new("/repo"),
            test_scope(),
        );

        assert_eq!(
            cr.components.changed_functions, 1,
            "deleted entries are still scored"
        );
        assert_eq!(
            cr.score.value, 0.0,
            "removing code must not increase the change-risk score"
        );
    }
}
