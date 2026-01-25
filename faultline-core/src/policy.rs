//! Policy engine for CI enforcement
//!
//! Evaluates built-in policies on delta output to block or warn on regressions.
//!
//! Global invariants enforced:
//! - Policies are deterministic (same input = same output)
//! - Policies operate on deltas and snapshots (no IO, no CLI logic)
//! - Policy evaluation order is deterministic
//! - Baseline deltas skip all policy evaluation

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
}

impl PolicyId {
    /// Get policy name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyId::CriticalIntroduction => "critical-introduction",
            PolicyId::ExcessiveRiskRegression => "excessive-risk-regression",
            PolicyId::NetRepoRegression => "net-repo-regression",
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
    // Primary: by id (enum discriminant order)
    let id_order = match (a.id, b.id) {
        (PolicyId::CriticalIntroduction, PolicyId::CriticalIntroduction) => Ordering::Equal,
        (PolicyId::CriticalIntroduction, _) => Ordering::Less,
        (PolicyId::ExcessiveRiskRegression, PolicyId::CriticalIntroduction) => Ordering::Greater,
        (PolicyId::ExcessiveRiskRegression, PolicyId::ExcessiveRiskRegression) => Ordering::Equal,
        (PolicyId::ExcessiveRiskRegression, _) => Ordering::Less,
        (PolicyId::NetRepoRegression, PolicyId::NetRepoRegression) => Ordering::Equal,
        (PolicyId::NetRepoRegression, _) => Ordering::Greater,
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
///
/// # Returns
///
/// Policy results, or None if baseline delta (baseline deltas skip policy evaluation)
pub fn evaluate_policies(
    delta: &Delta,
    current_snapshot: &Snapshot,
    repo_root: &Path,
) -> Result<Option<PolicyResults>> {
    // Skip policy evaluation for baseline deltas
    if delta.baseline {
        return Ok(None);
    }

    let mut results = PolicyResults::new();

    // Evaluation order: Function-level policies first, then repo-level
    // 1. Function-level policies
    evaluate_critical_introduction(&delta.deltas, &mut results);
    evaluate_excessive_risk_regression(&delta.deltas, &mut results);

    // 2. Repo-level policies
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
            let message = format!(
                "Function {} introduced as Critical",
                entry.function_id
            );

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
        // Only check Modified functions
        if entry.status != FunctionStatus::Modified {
            continue;
        }

        // Check if delta.lrs exceeds threshold
        if let Some(delta) = &entry.delta {
            if delta.lrs >= REGRESSION_THRESHOLD {
                let message = format!(
                    "Function {} regressed by {:.2} LRS",
                    entry.function_id,
                    delta.lrs
                );

                results.failed.push(PolicyResult {
                    id: PolicyId::ExcessiveRiskRegression,
                    severity: PolicySeverity::Blocking,
                    function_id: Some(entry.function_id.clone()),
                    message,
                    metadata: Some(PolicyMetadata {
                        delta_lrs: Some(delta.lrs),
                        total_delta: None,
                    }),
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
        let message = format!(
            "Repository total LRS increased by {:.2}",
            total_delta
        );

        results.warnings.push(PolicyResult {
            id: PolicyId::NetRepoRegression,
            severity: PolicySeverity::Warning,
            function_id: None,
            message,
            metadata: Some(PolicyMetadata {
                delta_lrs: None,
                total_delta: Some(total_delta),
            }),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delta::{Delta, DeltaCommitInfo, FunctionDelta, FunctionDeltaEntry, FunctionState};
    use crate::report::MetricsReport;
    use crate::git::GitContext;
    use tempfile::TempDir;

    fn create_test_delta_entry(
        function_id: &str,
        status: FunctionStatus,
        before_band: Option<&str>,
        after_band: Option<&str>,
        delta_lrs: Option<f64>,
    ) -> FunctionDeltaEntry {
        let before = before_band.map(|band| FunctionState {
            metrics: MetricsReport { cc: 4, nd: 2, fo: 2, ns: 1 },
            lrs: 3.9,
            band: band.to_string(),
        });

        let after = after_band.map(|band| FunctionState {
            metrics: MetricsReport { cc: 6, nd: 3, fo: 3, ns: 1 },
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
        assert_eq!(results.failed[0].function_id, Some("src/foo.ts::handler".to_string()));
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
        assert!(results.failed[0].metadata.as_ref().unwrap().delta_lrs.is_some());
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
        assert_eq!(results.failed[0].function_id, Some("src/a.ts::func".to_string()));
        assert_eq!(results.failed[1].id, PolicyId::CriticalIntroduction);
        assert_eq!(results.failed[1].function_id, Some("src/b.ts::func".to_string()));
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

        let result = evaluate_policies(&delta, &snapshot, repo_root).unwrap();
        assert!(result.is_none());
    }
}
