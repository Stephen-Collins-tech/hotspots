//! Policy engine for CI enforcement
//!
//! Evaluates built-in policies on delta output to block or warn on regressions.
//!
//! Global invariants enforced:
//! - Policies are deterministic (same input = same output)
//! - Policies operate on deltas and snapshots (no IO, no CLI logic)
//! - Policy evaluation order is deterministic
//! - Baseline deltas skip all policy evaluation

use crate::config::{PolicyMode, ResolvedConfig};
use crate::delta::{Delta, FunctionDeltaEntry, FunctionState, FunctionStatus};
use crate::risk::RiskBand;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Policy identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyId {
    CriticalIntroduction,
    ExcessiveRiskRegression,
    RapidGrowth,
}

impl PolicyId {
    /// Get policy name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyId::CriticalIntroduction => "critical-introduction",
            PolicyId::ExcessiveRiskRegression => "excessive-risk-regression",
            PolicyId::RapidGrowth => "rapid-growth",
        }
    }

    /// Deterministic sort priority (lower = earlier in output)
    fn sort_order(self) -> u8 {
        match self {
            PolicyId::CriticalIntroduction => 0,
            PolicyId::ExcessiveRiskRegression => 1,
            PolicyId::RapidGrowth => 4,
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
    // Primary: by id priority
    let id_order = a.id.sort_order().cmp(&b.id.sort_order());
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

/// Iterate over delta entries that have not been suppressed
fn active_deltas(deltas: &[FunctionDeltaEntry]) -> impl Iterator<Item = &FunctionDeltaEntry> {
    deltas.iter().filter(|e| e.suppression_reason.is_none())
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
pub fn evaluate_policies(delta: &Delta, config: &ResolvedConfig) -> Result<Option<PolicyResults>> {
    // Skip policy evaluation for baseline deltas
    if delta.baseline {
        return Ok(None);
    }

    let mut results = PolicyResults::new();

    // Evaluation order: Blocking policies first, then warning policies, then repo-level
    // 1. Blocking function-level policies
    evaluate_critical_introduction(&delta.deltas, config, &mut results);
    evaluate_excessive_risk_regression(&delta.deltas, config, &mut results);

    // 2. Warning function-level policies
    evaluate_rapid_growth(&delta.deltas, &mut results);

    // Sort results deterministically
    results.sort();

    Ok(Some(results))
}

/// Evaluate Critical Introduction policy
///
/// Triggers when `after.band == Critical AND (before.band != Critical OR before is None)`
/// `before is None` means no matching `function_id` in the parent snapshot (delta status `new`),
/// UNLESS another entry's `rename_hint` points at this `function_id` — in that case the
/// renamed-from entry's `before` state is used instead, so a pure rename/move of an
/// already-Critical function isn't treated as a fresh introduction.
///
/// A brand-new function that scores Critical on its first commit and an existing function
/// that regresses into Critical are treated identically here — a Critical function needs
/// review either way, regardless of whether the code is new or old. `config.critical_introduction_mode`
/// controls the severity of that review requirement repo-wide (block/warn/off); it does not
/// distinguish new from regressed.
fn evaluate_critical_introduction(
    deltas: &[FunctionDeltaEntry],
    config: &ResolvedConfig,
    results: &mut PolicyResults,
) {
    if config.critical_introduction_mode == PolicyMode::Off {
        return;
    }

    let severity = match config.critical_introduction_mode {
        PolicyMode::Block => PolicySeverity::Blocking,
        PolicyMode::Warn => PolicySeverity::Warning,
        PolicyMode::Off => unreachable!("handled above"),
    };

    // Maps a rename target's function_id to the pre-rename `before` state, so a
    // `New` entry produced by a rename/move can be judged against what the
    // function actually was, not treated as if it had no prior history.
    let rename_before_state: HashMap<&str, &FunctionState> = deltas
        .iter()
        .filter_map(|entry| {
            let target = entry.rename_hint.as_deref()?;
            let before = entry.before.as_ref()?;
            Some((target, before))
        })
        .collect();

    for entry in active_deltas(deltas) {
        // Check if function becomes Critical
        let becomes_critical = if let Some(after) = &entry.after {
            after.band == RiskBand::Critical
        } else {
            false
        };

        if !becomes_critical {
            continue;
        }

        // Check if it was Critical before, falling back to the pre-rename state
        // for a New entry that another entry's rename_hint identifies as a rename target.
        let was_critical_before = if let Some(before) = &entry.before {
            before.band == RiskBand::Critical
        } else if let Some(before) = rename_before_state.get(entry.function_id.as_str()) {
            before.band == RiskBand::Critical
        } else {
            // before is None and no rename was detected: genuinely new function
            false
        };

        // Trigger if becomes Critical and wasn't Critical before
        if !was_critical_before {
            let message = format!("Function {} introduced as Critical", entry.function_id);

            let result = PolicyResult {
                id: PolicyId::CriticalIntroduction,
                severity,
                function_id: Some(entry.function_id.clone()),
                message,
                metadata: None,
            };

            match severity {
                PolicySeverity::Blocking => results.failed.push(result),
                PolicySeverity::Warning => results.warnings.push(result),
            }
        }
    }
}

/// Evaluate Excessive Risk Regression policy
///
/// Triggers when `status == Modified && delta.lrs >= 1.0`
/// Threshold is fixed at 1.0 LRS (absolute, not relative).
/// `config.excessive_risk_regression_mode` controls the severity (block/warn/off).
fn evaluate_excessive_risk_regression(
    deltas: &[FunctionDeltaEntry],
    config: &ResolvedConfig,
    results: &mut PolicyResults,
) {
    if config.excessive_risk_regression_mode == PolicyMode::Off {
        return;
    }

    let severity = match config.excessive_risk_regression_mode {
        PolicyMode::Block => PolicySeverity::Blocking,
        PolicyMode::Warn => PolicySeverity::Warning,
        PolicyMode::Off => unreachable!("handled above"),
    };

    // This is a flat absolute threshold, unlike the sibling RapidGrowth policy's
    // relative one (`(delta_lrs / before_lrs) * 100 >= 50`). An earlier analysis
    // derived that this asymmetry is mechanistically real — the required relative
    // growth to hit `Δlrs = 1.0` is `100 / before_lrs`%, which shrinks as functions
    // get larger, so a flat threshold is easier to trip on large functions and
    // harder on small ones. That analysis attempted to measure whether this matters in
    // practice (precision against real future fixes) but could not: borrowing
    // RapidGrowth's exact 50% value produced only 4 disagreement events across 1200
    // sampled modifications on 4 repos, far below the pre-registered 20-event floor
    // needed to test the hypothesis at all. The conclusion was that this is not a confirmed-safe threshold, nor a
    // confirmed-miscalibrated one; the question remains open pending a differently-
    // tuned relative threshold (15-25% looks plausible) that would actually generate
    // enough disagreement volume to test against.
    const REGRESSION_THRESHOLD: f64 = 1.0;

    for entry in active_deltas(deltas) {
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

                let result = PolicyResult {
                    id: PolicyId::ExcessiveRiskRegression,
                    severity,
                    function_id: Some(entry.function_id.clone()),
                    message,
                    metadata: Some(PolicyMetadata {
                        delta_lrs: Some(delta.lrs),
                        total_delta: None,
                        growth_percent: None,
                    }),
                };

                match severity {
                    PolicySeverity::Blocking => results.failed.push(result),
                    PolicySeverity::Warning => results.warnings.push(result),
                }
            }
        }
    }
}

/// Percent LRS increase at which `rapid-growth` warns (fixed, not configurable).
const RAPID_GROWTH_PERCENT: f64 = 50.0;

/// Evaluate Rapid Growth policy
///
/// Triggers when `delta.lrs / before.lrs >= RAPID_GROWTH_PERCENT / 100.0`
/// Only applies to Modified functions (not New, since no baseline)
fn evaluate_rapid_growth(deltas: &[FunctionDeltaEntry], results: &mut PolicyResults) {
    for entry in active_deltas(deltas) {
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
        if growth_percent >= RAPID_GROWTH_PERCENT {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delta::{Delta, DeltaCommitInfo, FunctionDelta, FunctionDeltaEntry, FunctionState};
    use crate::report::MetricsReport;

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
                loc: 10,
                missing_docstring: false,
            },
            lrs: 3.9,
            band: RiskBand::parse(band).unwrap_or(RiskBand::Low),
        });

        let after = after_band.map(|band| FunctionState {
            metrics: MetricsReport {
                cc: 6,
                nd: 3,
                fo: 3,
                ns: 1,
                loc: 15,
                missing_docstring: false,
            },
            lrs: if band == "critical" { 10.5 } else { 6.2 },
            band: RiskBand::parse(band).unwrap_or(RiskBand::Low),
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
            rename_hint: None,
        }
    }

    #[test]
    fn test_critical_introduction_new_function() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        )];

        evaluate_critical_introduction(&deltas, &config, &mut results);

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
        let config = ResolvedConfig::defaults().unwrap();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("high"),
            Some("critical"),
            Some(2.3),
        )];

        evaluate_critical_introduction(&deltas, &config, &mut results);

        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, PolicyId::CriticalIntroduction);
    }

    #[test]
    fn test_critical_introduction_no_violation() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();
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

        evaluate_critical_introduction(&deltas, &config, &mut results);

        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_critical_introduction_pure_rename_is_not_a_violation() {
        // Regression test for hotspots#143: a pure rename of an already-Critical
        // function produces a Deleted entry (old id, carrying rename_hint) and a
        // New entry (new id, before=None). The policy must consult rename_hint
        // and treat this as "was already Critical", not a fresh introduction.
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        let mut deleted_entry = create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Deleted,
            Some("critical"),
            None,
            None,
        );
        deleted_entry.rename_hint = Some("src/foo.ts::handleRequest".to_string());

        let new_entry = create_test_delta_entry(
            "src/foo.ts::handleRequest",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        );

        let deltas = vec![deleted_entry, new_entry];

        evaluate_critical_introduction(&deltas, &config, &mut results);

        assert_eq!(
            results.failed.len(),
            0,
            "pure rename of an already-Critical function must not trigger critical-introduction"
        );
    }

    #[test]
    fn test_critical_introduction_rename_from_non_critical_still_fires() {
        // A rename that also becomes Critical for the first time (i.e. the
        // renamed-from function was NOT Critical before) is a genuine
        // introduction and must still fire.
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();

        let mut deleted_entry = create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Deleted,
            Some("moderate"),
            None,
            None,
        );
        deleted_entry.rename_hint = Some("src/foo.ts::handleRequest".to_string());

        let new_entry = create_test_delta_entry(
            "src/foo.ts::handleRequest",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        );

        let deltas = vec![deleted_entry, new_entry];

        evaluate_critical_introduction(&deltas, &config, &mut results);

        assert_eq!(results.failed.len(), 1);
        assert_eq!(
            results.failed[0].function_id,
            Some("src/foo.ts::handleRequest".to_string())
        );
    }

    #[test]
    fn test_excessive_risk_regression_triggered() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("moderate"),
            Some("high"),
            Some(1.5), // >= 1.0 threshold
        )];

        evaluate_excessive_risk_regression(&deltas, &config, &mut results);

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
        let config = ResolvedConfig::defaults().unwrap();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("moderate"),
            Some("moderate"),
            Some(0.9), // < 1.0 threshold
        )];

        evaluate_excessive_risk_regression(&deltas, &config, &mut results);

        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_excessive_risk_regression_new_function() {
        let mut results = PolicyResults::new();
        let config = ResolvedConfig::defaults().unwrap();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("high"),
            None,
        )];

        evaluate_excessive_risk_regression(&deltas, &config, &mut results);

        // New functions don't trigger Excessive Risk Regression (only Modified)
        assert_eq!(results.failed.len(), 0);
    }

    #[test]
    fn test_critical_introduction_warn_mode_downgrades_severity() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("warn".to_string()),
                critical_introduction_reason: Some(
                    "research repo, dense one-shot scripts".to_string(),
                ),
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        let config = raw.resolve().unwrap();

        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        )];

        evaluate_critical_introduction(&deltas, &config, &mut results);

        // Same trigger condition, but downgraded to a warning, not a blocking failure.
        assert_eq!(results.failed.len(), 0);
        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::CriticalIntroduction);
        assert_eq!(results.warnings[0].severity, PolicySeverity::Warning);
    }

    #[test]
    fn test_critical_introduction_off_mode_skips_entirely() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("off".to_string()),
                critical_introduction_reason: Some(
                    "research repo, dense one-shot scripts".to_string(),
                ),
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        let config = raw.resolve().unwrap();

        let mut results = PolicyResults::new();
        let deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        )];

        evaluate_critical_introduction(&deltas, &config, &mut results);

        assert_eq!(results.failed.len(), 0);
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_critical_introduction_new_vs_regressed_same_severity() {
        // A brand-new function scored Critical and an existing function that regressed
        // to Critical must produce identical severity under the same config — this
        // policy does not distinguish "new" from "regressed", only overall strictness.
        let config = ResolvedConfig::defaults().unwrap();

        let mut new_results = PolicyResults::new();
        let new_deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some("critical"),
            None,
        )];
        evaluate_critical_introduction(&new_deltas, &config, &mut new_results);

        let mut regressed_results = PolicyResults::new();
        let regressed_deltas = vec![create_test_delta_entry(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some("high"),
            Some("critical"),
            Some(2.3),
        )];
        evaluate_critical_introduction(&regressed_deltas, &config, &mut regressed_results);

        assert_eq!(
            new_results.failed[0].severity,
            regressed_results.failed[0].severity
        );
    }

    #[test]
    fn test_policy_config_invalid_mode_rejected() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("blocc".to_string()),
                critical_introduction_reason: None,
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        assert!(raw.resolve().is_err());
    }

    #[test]
    fn test_policy_config_warn_without_reason_rejected() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("warn".to_string()),
                critical_introduction_reason: None,
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        let err = raw.resolve().unwrap_err();
        assert!(err.to_string().contains("critical_introduction_reason"));
    }

    #[test]
    fn test_policy_config_warn_with_blank_reason_rejected() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("off".to_string()),
                critical_introduction_reason: Some("   ".to_string()),
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        assert!(raw.resolve().is_err());
    }

    #[test]
    fn test_policy_config_block_mode_does_not_require_reason() {
        let raw = crate::config::HotspotsConfig {
            policy: Some(crate::config::PolicyConfig {
                critical_introduction: Some("block".to_string()),
                critical_introduction_reason: None,
                excessive_risk_regression: None,
                excessive_risk_regression_reason: None,
            }),
            ..Default::default()
        };
        assert!(raw.resolve().is_ok());
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
            commit: DeltaCommitInfo {
                sha: "abc123".to_string(),
                parent: "".to_string(),
            },
            baseline: true,
            deltas: vec![],
            policy: None,
            aggregates: None,
        };

        let config = ResolvedConfig::defaults().unwrap();
        let result = evaluate_policies(&delta, &config).unwrap();
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
                loc: 10,
                missing_docstring: false,
            },
            lrs,
            band: if lrs >= 9.0 {
                RiskBand::Critical
            } else if lrs >= 6.0 {
                RiskBand::High
            } else if lrs >= 3.0 {
                RiskBand::Moderate
            } else {
                RiskBand::Low
            },
        });

        let after = after_lrs.map(|lrs| FunctionState {
            metrics: MetricsReport {
                cc: 6,
                nd: 3,
                fo: 3,
                ns: 1,
                loc: 15,
                missing_docstring: false,
            },
            lrs,
            band: if lrs >= 9.0 {
                RiskBand::Critical
            } else if lrs >= 6.0 {
                RiskBand::High
            } else if lrs >= 3.0 {
                RiskBand::Moderate
            } else {
                RiskBand::Low
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
            rename_hint: None,
        }
    }

    #[test]
    fn test_rapid_growth_triggered() {
        let mut results = PolicyResults::new();

        // Modified function with 100% growth (2.0 -> 4.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(4.0),
        )];

        evaluate_rapid_growth(&deltas, &mut results);

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

        // Modified function with exactly 50% growth (2.0 -> 3.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.0),
            Some(3.0),
        )];

        evaluate_rapid_growth(&deltas, &mut results);

        assert_eq!(results.warnings.len(), 1);
        assert_eq!(results.warnings[0].id, PolicyId::RapidGrowth);
    }

    #[test]
    fn test_rapid_growth_below_threshold() {
        let mut results = PolicyResults::new();

        // Modified function with 40% growth (below 50% threshold)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(2.5),
            Some(3.5),
        )];

        evaluate_rapid_growth(&deltas, &mut results);

        // Should not trigger - below threshold
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_rapid_growth_negative_delta() {
        let mut results = PolicyResults::new();

        // Modified function with improvement (5.0 -> 3.0)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::Modified,
            Some(5.0),
            Some(3.0),
        )];

        evaluate_rapid_growth(&deltas, &mut results);

        // Should not trigger - negative delta (improvement)
        assert_eq!(results.warnings.len(), 0);
    }

    #[test]
    fn test_rapid_growth_new_function() {
        let mut results = PolicyResults::new();

        // New function (no baseline)
        let deltas = vec![create_test_delta_entry_with_lrs(
            "src/foo.ts::handler",
            FunctionStatus::New,
            None,
            Some(10.0),
        )];

        evaluate_rapid_growth(&deltas, &mut results);

        // Should not trigger - new functions don't have rapid growth warnings
        assert_eq!(results.warnings.len(), 0);
    }
}
