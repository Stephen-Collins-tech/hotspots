//! Policy engine for CI enforcement
//!
//! Evaluates built-in policies on delta output to block or warn on regressions.
//!
//! Global invariants enforced:
//! - Policies are deterministic (same input = same output)
//! - Policies operate on deltas and snapshots (no IO, no CLI logic)
//! - Policy evaluation order is deterministic
//! - Baseline deltas skip all policy evaluation

use crate::config::ResolvedConfig;
use crate::delta::{Delta, FunctionDeltaEntry, FunctionStatus};
use crate::snapshot::Snapshot;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

/// Policy identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyId {
    CriticalIntroduction,
    ExcessiveRiskRegression,
    NetRepoRegression,
    // Warning policies
    WatchThreshold,
    AttentionThreshold,
    RapidGrowth,
    SuppressionMissingReason,
}

impl PolicyId {
    /// Get policy name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyId::CriticalIntroduction => "critical-introduction",
            PolicyId::ExcessiveRiskRegression => "excessive-risk-regression",
            PolicyId::NetRepoRegression => "net-repo-regression",
            PolicyId::WatchThreshold => "watch-threshold",
            PolicyId::AttentionThreshold => "attention-threshold",
            PolicyId::RapidGrowth => "rapid-growth",
            PolicyId::SuppressionMissingReason => "suppression-missing-reason",
        }
    }
}

/// Policy severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicySeverity {
    Blocking,
    Warning,
}

/// Policy metadata (numeric values only)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PolicyMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_lrs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_delta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub growth_percent: Option<f64>,
}

/// Policy evaluation result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PolicyResult {
    pub id: PolicyId,
    pub severity: PolicySeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PolicyMetadata>,
}

/// Policy evaluation results container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PolicyResults {
    pub failed: Vec<PolicyResult>,
    pub warnings: Vec<PolicyResult>,
}

impl PolicyResults {
    /// Create empty policy results
    pub fn new() -> Self {
        Self {
            failed: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Check if there are any blocking failures
    pub fn has_blocking_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    /// Sort results deterministically
    ///
    /// Primary sort: by id (enum discriminant order)
    /// Secondary sort: by function_id ASCII (None last)
    pub fn sort(&mut self) {
        self.failed.sort_by(compare_policy_results);
        self.warnings.sort_by(compare_policy_results);
    }
}

impl Default for PolicyResults {
    fn default() -> Self {
        Self::new()
    }
}

/// Compare policy results for deterministic ordering
fn compare_policy_results(a: &PolicyResult, b: &PolicyResult) -> Ordering {
    use PolicyId::*;

    // Primary: by id (enum discriminant order)
    // Order: CriticalIntroduction → ExcessiveRiskRegression → WatchThreshold → AttentionThreshold → RapidGrowth → SuppressionMissingReason → NetRepoRegression
    let id_order = match (a.id, b.id) {
        // Same IDs are equal
        (CriticalIntroduction, CriticalIntroduction) => Ordering::Equal,
        (ExcessiveRiskRegression, ExcessiveRiskRegression) => Ordering::Equal,
        (WatchThreshold, WatchThreshold) => Ordering::Equal,
        (AttentionThreshold, AttentionThreshold) => Ordering::Equal,
        (RapidGrowth, RapidGrowth) => Ordering::Equal,
        (SuppressionMissingReason, SuppressionMissingReason) => Ordering::Equal,
        (NetRepoRegression, NetRepoRegression) => Ordering::Equal,

        // CriticalIntroduction is always first
        (CriticalIntroduction, _) => Ordering::Less,
        (_, CriticalIntroduction) => Ordering::Greater,

        // ExcessiveRiskRegression is second
        (ExcessiveRiskRegression, _) => Ordering::Less,
        (_, ExcessiveRiskRegression) => Ordering::Greater,

        // WatchThreshold is third
        (WatchThreshold, _) => Ordering::Less,
        (_, WatchThreshold) => Ordering::Greater,

        // AttentionThreshold is fourth
        (AttentionThreshold, _) => Ordering::Less,
        (_, AttentionThreshold) => Ordering::Greater,

        // RapidGrowth is fifth
        (RapidGrowth, _) => Ordering::Less,
        (_, RapidGrowth) => Ordering::Greater,

        // SuppressionMissingReason is sixth
        (SuppressionMissingReason, _) => Ordering::Less,
        (_, SuppressionMissingReason) => Ordering::Greater,
    };

    if id_order != Ordering::Equal {
        return id_order;
    }

    // Secondary: by function_id ASCII (None last)
    match (&a.function_id, &b.function_id) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a_id), Some(b_id)) => a_id.cmp(b_id),
    }
}

/// Evaluate all policies on a delta
///
/// # Arguments
///
/// * `delta` - Delta to evaluate
/// * `current_snapshot` - Current snapshot (for repo-level policies)
/// * `repo_root` - Repository root path (for loading parent snapshot)
/// * `config` - Resolved configuration (for warning thresholds)
///
/// # Returns
///
/// Policy results, or None if baseline delta (baseline deltas skip policy evaluation)
pub fn evaluate_policies(
    delta: &Delta,
    current_snapshot: &Snapshot,
    repo_root: &Path,
    config: &ResolvedConfig,
) -> Result<Option<PolicyResults>> {
    // Skip policy evaluation for baseline deltas
    if delta.baseline {
        return Ok(None);
    }

    let mut results = PolicyResults::new();

    // Evaluation order: Blocking policies first, then warning policies, then repo-level
    // 1. Blocking function-level policies
    evaluate_critical_introduction(&delta.deltas, &mut results);
    evaluate_excessive_risk_regression(&delta.deltas, &mut results);

    // 2. Warning function-level policies
    evaluate_watch_threshold(&delta.deltas, config, &mut results);
    evaluate_attention_threshold(&delta.deltas, config, &mut results);
    evaluate_rapid_growth(&delta.deltas, config, &mut results);
    evaluate_suppression_missing_reason(&delta.deltas, &mut results);

    // 3. Repo-level policies
    evaluate_net_repo_regression(delta, current_snapshot, repo_root, &mut results)?;

    // Sort results deterministically
    results.sort();

    Ok(Some(results))
}

/// Evaluate Critical Introduction policy
///
/// Triggers when `after.band == Critical AND (before.band != Critical OR before is None)`
/// `before is None` means no matching `function_id` in the parent snapshot (delta status `new`)
fn evaluate_critical_introduction(deltas: &[FunctionDeltaEntry], results: &mut PolicyResults) {
    const CRITICAL_BAND: &str = "critical";

    for entry in deltas {
        // Skip suppressed functions
        if entry.suppression_reason.is_some() {
            continue;
        }

        // Check if function becomes Critical
        let becomes_critical = if let Some(after) = &entry.after {
            after.band == CRITICAL_BAND
        } else {
            false
        };

        if !becomes_critical {
            continue;
        }

        // Check if it was Critical before
        let was_critical_before = if let Some(before) = &entry.before {
            before.band == CRITICAL_BAND
        } else {
            // before is None means delta status `new` (new function)
            false
        };

        // Trigger if becomes Critical and wasn't Critical before
        if !was_critical_before {
            let message = format!("Function {} introduced as Critical", entry.function_id);

            results.failed.push(PolicyResult {
                id: PolicyId::CriticalIntroduction,
                severity: PolicySeverity::Blocking,
                function_id: Some(entry.function_id.clone()),
                message,
                metadata: None,
            });
        }
    }
}

/// Evaluate Excessive Risk Regression policy
///
/// Triggers when `status == Modified && delta.lrs >= 1.0`
/// Threshold is fixed at 1.0 LRS (absolute, not relative)
fn evaluate_excessive_risk_regression(deltas: &[FunctionDeltaEntry], results: &mut PolicyResults) {
    const REGRESSION_THRESHOLD: f64 = 1.0;

    for entry in deltas {
        // Skip suppressed functions
        if entry.suppression_reason.is_some() {
            continue;
        }

        // Only check Modified functions
        if entry.status != FunctionStatus::Modified {
            continue;
        }

        // Check if delta.lrs exceeds threshold
        if let Some(delta) = &entry.delta {
            if delta.lrs >= REGRESSION_THRESHOLD {
                let message = format!(
                    "Function {} regressed by {:.2} LRS",
                    entry.function_id, delta.lrs
                );

                results.failed.push(PolicyResult {
                    id: PolicyId::ExcessiveRiskRegression,
                    severity: PolicySeverity::Blocking,
                    function_id: Some(entry.function_id.clone()),
                    message,
                    metadata: Some(PolicyMetadata {
                        delta_lrs: Some(delta.lrs),
                        total_delta: None,
                        growth_percent: None,
                    }),
                });
            }
        }
    }
}

/// Evaluate Watch Threshold policy
///
/// Triggers when `after.lrs` is in [watch_min, watch_max) AND `before.lrs` < watch_min
/// Only applies to New or Modified functions (functions entering the watch range)
fn evaluate_watch_threshold(
    deltas: &[FunctionDeltaEntry],
    config: &ResolvedConfig,
    results: &mut PolicyResults,
) {
    for entry in deltas {
        // Skip suppressed functions
        if entry.suppression_reason.is_some() {
            continue;
        }

        // Only check New or Modified functions
        if entry.status != FunctionStatus::New && entry.status != FunctionStatus::Modified {
            continue;
        }

        // Get after LRS
        let after_lrs = if let Some(after) = &entry.after {
            after.lrs
        } else {
            continue;
        };

        // Check if after LRS is in watch range
        let in_watch_range = after_lrs >= config.watch_min && after_lrs < config.watch_max;
        if !in_watch_range {
            continue;
        }

        // Check if before LRS was below watch_min (entering the range)
        let entering_watch = if let Some(before) = &entry.before {
            before.lrs < config.watch_min
        } else {
            // New functions with no before state are entering
            true
        };

        if entering_watch {
            let message = format!(
                "Function {} approaching moderate threshold (LRS: {:.2})",
                entry.function_id, after_lrs
            );

            results.warnings.push(PolicyResult {
                id: PolicyId::WatchThreshold,
                severity: PolicySeverity::Warning,
                function_id: Some(entry.function_id.clone()),
                message,
                metadata: entry.delta.as_ref().map(|d| PolicyMetadata {
                    delta_lrs: Some(d.lrs),
                    total_delta: None,
                    growth_percent: None,
                }),
            });
        }
    }
}

/// Evaluate Attention Threshold policy
///
/// Triggers when `after.lrs` is in [attention_min, attention_max) AND `before.lrs` < attention_min
/// Only applies to New or Modified functions (functions entering the attention range)
fn evaluate_attention_threshold(
    deltas: &[FunctionDeltaEntry],
    config: &ResolvedConfig,
    results: &mut PolicyResults,
) {
    for entry in deltas {
        // Skip suppressed functions
        if entry.suppression_reason.is_some() {
            continue;
        }

        // Only check New or Modified functions
        if entry.status != FunctionStatus::New && entry.status != FunctionStatus::Modified {
            continue;
        }

        // Get after LRS
        let after_lrs = if let Some(after) = &entry.after {
            after.lrs
        } else {
            continue;
        };

        // Check if after LRS is in attention range
        let in_attention_range =
            after_lrs >= config.attention_min && after_lrs < config.attention_max;
        if !in_attention_range {
            continue;
        }

        // Check if before LRS was below attention_min (entering the range)
        let entering_attention = if let Some(before) = &entry.before {
            before.lrs < config.attention_min
        } else {
            // New functions with no before state are entering
            true
        };

        if entering_attention {
            let message = format!(
                "Function {} approaching high threshold (LRS: {:.2})",
                entry.function_id, after_lrs
            );

            results.warnings.push(PolicyResult {
                id: PolicyId::AttentionThreshold,
                severity: PolicySeverity::Warning,
                function_id: Some(entry.function_id.clone()),
                message,
                metadata: entry.delta.as_ref().map(|d| PolicyMetadata {
                    delta_lrs: Some(d.lrs),
                    total_delta: None,
                    growth_percent: None,
                }),
            });
        }
    }
}

/// Evaluate Rapid Growth policy
///
/// Triggers when `delta.lrs / before.lrs >= rapid_growth_percent / 100.0`
/// Only applies to Modified functions (not New, since no baseline)
fn evaluate_rapid_growth(
    deltas: &[FunctionDeltaEntry],
    config: &ResolvedConfig,
    results: &mut PolicyResults,
) {
    for entry in deltas {
        // Skip suppressed functions
        if entry.suppression_reason.is_some() {
            continue;
        }

        // Only check Modified functions
        if entry.status != FunctionStatus::Modified {
            continue;
        }

        // Get before and after LRS
        let (before_lrs, after_lrs) = match (&entry.before, &entry.after) {
            (Some(before), Some(after)) => (before.lrs, after.lrs),
            _ => continue,
        };

        // Skip if before_lrs is zero (avoid division by zero)
        if before_lrs <= f64::EPSILON {
            continue;
        }

        // Calculate growth percentage
        let delta_lrs = after_lrs - before_lrs;
        let growth_percent = (delta_lrs / before_lrs) * 100.0;

        // Trigger if growth exceeds threshold
        if growth_percent >= config.rapid_growth_percent {
            let message = format!(
                "Function {} LRS increased by {:.1}% ({:.2} -> {:.2})",
                entry.function_id, growth_percent, before_lrs, after_lrs
            );

            results.warnings.push(PolicyResult {
                id: PolicyId::RapidGrowth,
                severity: PolicySeverity::Warning,
                function_id: Some(entry.function_id.clone()),
                message,
                metadata: Some(PolicyMetadata {
                    delta_lrs: Some(delta_lrs),
                    total_delta: None,
                    growth_percent: Some(growth_percent),
                }),
            });
        }
    }
}

/// Evaluate Suppression Missing Reason policy
///
/// Triggers when `suppression_reason == Some("")` (suppression without reason)
/// Warning only - reminds developers to document why functions are suppressed
fn evaluate_suppression_missing_reason(deltas: &[FunctionDeltaEntry], results: &mut PolicyResults) {
    for entry in deltas {
        // Check if function has suppression without reason
        if let Some(reason) = &entry.suppression_reason {
            if reason.is_empty() {
                let message = format!("Function {} suppressed without reason", entry.function_id);

                results.warnings.push(PolicyResult {
                    id: PolicyId::SuppressionMissingReason,
                    severity: PolicySeverity::Warning,
                    function_id: Some(entry.function_id.clone()),
                    message,
                    metadata: None,
                });
            }
        }
    }
}

/// Evaluate Net Repo Regression policy
///
/// Computes `Σ(all after.lrs) - Σ(all before.lrs)` by loading parent snapshot and using current snapshot
/// Triggers when result > 0 (warning only, non-blocking)
fn evaluate_net_repo_regression(
    delta: &Delta,
    current_snapshot: &Snapshot,
    repo_root: &Path,
    results: &mut PolicyResults,
) -> Result<()> {
    // Load parent snapshot (before)
    let parent_sha = &delta.commit.parent;
    let before_snapshot = if !parent_sha.is_empty() {
        crate::delta::load_parent_snapshot(repo_root, parent_sha)?
    } else {
        None
    };

    // Compute totals
    let before_total: f64 = if let Some(snapshot) = &before_snapshot {
        snapshot.functions.iter().map(|f| f.lrs).sum()
    } else {
        0.0
    };

    // Use current snapshot directly (after)
    let after_total: f64 = current_snapshot.functions.iter().map(|f| f.lrs).sum();

    let total_delta = after_total - before_total;

    // Trigger if repo total increased
    if total_delta > 0.0 {
        let message = format!("Repository total LRS increased by {:.2}", total_delta);

        results.warnings.push(PolicyResult {
            id: PolicyId::NetRepoRegression,
            severity: PolicySeverity::Warning,
            function_id: None,
            message,
            metadata: Some(PolicyMetadata {
                delta_lrs: None,
                total_delta: Some(total_delta),
                growth_percent: None,
            }),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delta::{Delta, DeltaCommitInfo, FunctionDelta, FunctionDeltaEntry, FunctionState};
    use crate::git::GitContext;
    use crate::report::MetricsReport;
    use tempfile::TempDir;

    fn create_test_delta_entry(
        function_id: &str,
        status: FunctionStatus,
        before_band: Option<&str>,
        after_band: Option<&str>,
        delta_lrs: Option<f64>,
    ) -> FunctionDeltaEntry {
        let before = before_band.map(|band| FunctionState {
            metrics: MetricsReport {
                cc: 4,
                nd: 2,
                fo: 2,
                ns: 1,
            },
            lrs: 3.9,
            band: band.to_string(),
        });

        let after = after_band.map(|band| FunctionState {
            metrics: MetricsReport {
                cc: 6,
                nd: 3,
                fo: 3,
                ns: 1,
            },
            lrs: if band == "critical" { 10.5 } else { 6.2 },
            band: band.to_string(),
        });

        let delta = delta_lrs.map(|lrs| FunctionDelta {
            cc: 2,
            nd: 1,
            fo: 1,
            ns: 0,
            lrs,
        });

        FunctionDeltaEntry {
            function_id: function_id.to_string(),
            status,
            before,
            after,
            delta,
            band_transition: None,
            suppression_reason: None,
        }
    }

    #[test]
    fn test_critical_introduction_new_function() {
        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        )];

        evaluate_critical_introduction(&deltas, &mut results);

        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
        assert_eq!(results.failed[0].severity, PolicySeverity::Blocking);
        assert_eq!(
            results.failed[0].function_id,
            Some("src/foo.ts::handler".to_string())
        );
    }

    #[test]
    fn test_critical_introduction_modified_function() {
        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("high"),
            Some("critical"),
            Some(2.3),
        )];

        evaluate_critical_introduction(&deltas, &mut results);

        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
    }

    #[test]
    fn test_critical_introduction_no_violation() {
        let mut results = PolicyResults::new();
        let deltas = vec![
            // Already Critical, stays Critical
            create_test_delta_entry(
                "src/foo.ts::handler",
                FunctionStatus::Modified,
                Some("critical"),
                Some("critical"),
                Some(0.1),
            ),
            // Becomes High, not Critical
            create_test_delta_entry(
                "src/bar.ts::process",
                FunctionStatus::Modified,
                Some("moderate"),
                Some("high"),
                Some(2.0),
            ),
        ];

        evaluate_critical_introduction(&deltas, &mut results);

        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_excessive_risk_regression_triggered() {
        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("moderate"),
            Some("high"),
            Some(1.5), // >= 1.0 threshold
        )];

        evaluate_excessive_risk_regression(&deltas, &mut results);

        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, PolicyId::ExcessiveRiskRegression);
        assert_eq!(results.failed[0].severity, PolicySeverity::Blocking);
        assert!(results.failed[0]
            .metadata
            .as_ref()
            .unwrap()
            .delta_lrs
            .is_some());
    }

    #[test]
    fn test_excessive_risk_regression_below_threshold() {
        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("moderate"),
            Some("moderate"),
            Some(0.9), // < 1.0 threshold
        )];

        evaluate_excessive_risk_regression(&deltas, &mut results);

        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_excessive_risk_regression_new_function() {
        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("high"),
            None,
        )];

        evaluate_excessive_risk_regression(&deltas, &mut results);

        // New functions don't trigger Excessive Risk Regression (only Modified)
        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_policy_results_sorting() {
        let mut results = PolicyResults::new();

        // Add results in non-deterministic order
        results.failed.push(PolicyResult {
            id: PolicyId::ExcessiveRiskRegression,
            severity: PolicySeverity::Blocking,
            function_id: Some("src/z.ts::func".to_string()),
            message: "".to_string(),
            metadata: None,
        });

        results.failed.push(PolicyResult {
            id: PolicyId::CriticalIntroduction,
            severity: PolicySeverity::Blocking,
            function_id: Some("src/a.ts::func".to_string()),
            message: "".to_string(),
            metadata: None,
        });

        results.failed.push(PolicyResult {
            id: PolicyId::CriticalIntroduction,
            severity: PolicySeverity::Blocking,
            function_id: Some("src/b.ts::func".to_string()),
            message: "".to_string(),
            metadata: None,
        });

        results.sort();

        // Should be sorted by id first (CriticalIntroduction < ExcessiveRiskRegression)
        // Then by function_id ASCII
        assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
        assert_eq!(
            results.failed[0].function_id,
            Some("src/a.ts::func".to_string())
        );
        assert_eq!(results.failed[1].id, PolicyId::CriticalIntroduction);
        assert_eq!(
            results.failed[1].function_id,
            Some("src/b.ts::func".to_string())
        );
        assert_eq!(results.failed[2].id, PolicyId::ExcessiveRiskRegression);
    }

    #[test]
    fn test_baseline_delta_skips_policies() {
        let delta = Delta {
            schema_version: 1,
            commit: DeltaCommitInfo {
                sha: "abc123".to_string(),
                parent: "".to_string(),
            },
            baseline: true,
            deltas: vec![],
            policy: None,
            aggregates: None,
        };

        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create a dummy snapshot for testing
        let git_context = GitContext {
            head_sha: "abc123".to_string(),
            parent_shas: vec![],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
        };
        let snapshot = Snapshot::new(git_context, vec![]);

        let config = ResolvedConfig::defaults().unwrap();
        let result = evaluate_policies(&delta, &snapshot, repo_root, &config).unwrap();
        assert!(result.is_none());
    }

    // Helper to create delta entry with specific LRS values
    fn create_test_delta_entry_with_lrs(
        function_id: &str,
        status: FunctionStatus,
        before_lrs: Option<f64>,
        after_lrs: Option<f64>,
    ) -> FunctionDeltaEntry {
        let before = before_lrs.map(|lrs| FunctionState {
            metrics: MetricsReport {
                cc: 4,
                nd: 2,
                fo: 2,
                ns: 1,
            },
            lrs,
            band: if lrs >= 9.0 {
                "critical".to_string()
            } else if lrs >= 6.0 {
                "high".to_string()
            } else if lrs >= 3.0 {
                "moderate".to_string()
            } else {
                "low".to_string()
            },
        });

        let after = after_lrs.map(|lrs| FunctionState {
            metrics: MetricsReport {
                cc: 6,
                nd: 3,
                fo: 3,
                ns: 1,
            },
            lrs,
            band: if lrs >= 9.0 {
                "critical".to_string()
            } else if lrs >= 6.0 {
                "high".to_string()
            } else if lrs >= 3.0 {
                "moderate".to_string()
            } else {
                "low".to_string()
            },
        });

        let delta = match (before_lrs, after_lrs) {
            (Some(before), Some(after)) => Some(FunctionDelta {
                cc: 2,
                nd: 1,
                fo: 1,
                ns: 0,
                lrs: after - before,
            }),
            _ => None,
        };

        FunctionDeltaEntry {
            function_id: function_id.to_string(),
            status,
            before,
            after,
            delta,
            band_transition: None,
            suppression_reason: None,
        }
    }

    #[test]
    fn test_watch_threshold_new_function() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // New function with LRS in watch range [2.5, 3.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some(2.7),
        )];

        evaluate_watch_threshold(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::WatchThreshold);
        assert_eq!(results.warnings[0].severity, PolicySeverity::Warning);
    }

    #[test]
    fn test_watch_threshold_modified_entering_range() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function entering watch range (2.0 -> 2.8)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(2.8),
        )];

        evaluate_watch_threshold(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::WatchThreshold);
    }

    #[test]
    fn test_watch_threshold_already_in_range() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Function already in watch range (2.6 -> 2.9)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.6),
            Some(2.9),
        )];

        evaluate_watch_threshold(&deltas, &config, &mut results);

        // Should not trigger - already in range
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_watch_threshold_above_range() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Function above watch range
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(4.0),
        )];

        evaluate_watch_threshold(&deltas, &config, &mut results);

        // Should not trigger - above watch_max
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_attention_threshold_new_function() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // New function with LRS in attention range [5.5, 6.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some(5.8),
        )];

        evaluate_attention_threshold(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::AttentionThreshold);
        assert_eq!(results.warnings[0].severity, PolicySeverity::Warning);
    }

    #[test]
    fn test_attention_threshold_modified_entering_range() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function entering attention range (5.0 -> 5.7)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(5.0),
            Some(5.7),
        )];

        evaluate_attention_threshold(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::AttentionThreshold);
    }

    #[test]
    fn test_rapid_growth_triggered() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function with 100% growth (2.0 -> 4.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(4.0),
        )];

        evaluate_rapid_growth(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::RapidGrowth);
        assert_eq!(results.warnings[0].severity, PolicySeverity::Warning);
        assert!(results.warnings[0]
            .metadata
            .as_ref()
            .unwrap()
            .growth_percent
            .is_some());
        let growth = results.warnings[0]
            .metadata
            .as_ref()
            .unwrap()
            .growth_percent
            .unwrap();
        assert!((growth - 100.0).abs() < 0.1); // ~100%
    }

    #[test]
    fn test_rapid_growth_exactly_at_threshold() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function with exactly 50% growth (2.0 -> 3.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(3.0),
        )];

        evaluate_rapid_growth(&deltas, &config, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::RapidGrowth);
    }

    #[test]
    fn test_rapid_growth_below_threshold() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function with 40% growth (below 50% threshold)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.5),
            Some(3.5),
        )];

        evaluate_rapid_growth(&deltas, &config, &mut results);

        // Should not trigger - below threshold
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_rapid_growth_negative_delta() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // Modified function with improvement (5.0 -> 3.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(5.0),
            Some(3.0),
        )];

        evaluate_rapid_growth(&deltas, &config, &mut results);

        // Should not trigger - negative delta (improvement)
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_rapid_growth_new_function() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        // New function (no baseline)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some(10.0),
        )];

        evaluate_rapid_growth(&deltas, &config, &mut results);

        // Should not trigger - new functions don't have rapid growth warnings
        assert_eq!(results.warnings.len(), 0);
    }
}
