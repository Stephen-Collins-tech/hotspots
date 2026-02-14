//! Trend semantics - extract high-signal trends from snapshot history
//!
//! Analyzes historical snapshots to identify:
//! - Risk velocity (rate of LRS change)
//! - Hotspot stability (consistency of high-risk functions)
//! - Refactor effectiveness (sustained improvements)
//!
//! Global invariants enforced:
//! - Deterministic ordering (by commit timestamp, then SHA)
//! - No snapshot mutation
//! - Trends are derived, not stored

use crate::snapshot::{Index, Snapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Flat velocity tolerance (epsilon)
const FLAT_VELOCITY_EPSILON: f64 = 1e-9;

/// Significant improvement threshold for refactor detection
const REFACTOR_IMPROVEMENT_THRESHOLD: f64 = -1.0;

/// Rebound threshold after improvement
const REFACTOR_REBOUND_THRESHOLD: f64 = 0.5;

/// Velocity direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VelocityDirection {
    Positive,
    Negative,
    Flat,
}

/// Risk velocity for a function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RiskVelocity {
    pub function_id: String,
    pub velocity: f64,
    pub direction: VelocityDirection,
    pub first_lrs: f64,
    pub last_lrs: f64,
    pub commit_count: usize,
}

/// Hotspot stability classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotspotStability {
    Stable,
    Emerging,
    Volatile,
}

/// Hotspot analysis for a function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HotspotAnalysis {
    pub function_id: String,
    pub stability: HotspotStability,
    pub overlap_ratio: f64,
    pub appearances_in_top_k: usize,
    pub total_snapshots: usize,
}

/// Refactor effectiveness classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefactorOutcome {
    Successful,
    Partial,
    Cosmetic,
}

/// Refactor effectiveness analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RefactorAnalysis {
    pub function_id: String,
    pub outcome: RefactorOutcome,
    pub improvement_delta: f64,
    pub sustained_commits: usize,
    pub rebound_detected: bool,
}

/// Complete trends analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TrendsAnalysis {
    pub velocities: Vec<RiskVelocity>,
    pub hotspots: Vec<HotspotAnalysis>,
    pub refactors: Vec<RefactorAnalysis>,
}

impl TrendsAnalysis {
    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize trends analysis to JSON")
    }
}

/// Load snapshots from history using a sliding window
///
/// # Arguments
///
/// * `repo_root` - Repository root path
/// * `window_size` - Maximum number of snapshots to include (default: 10)
///
/// # Returns
///
/// Vector of snapshots ordered by commit timestamp (ascending), then SHA
pub fn load_snapshot_window(repo_root: &Path, window_size: usize) -> Result<Vec<Snapshot>> {
    // Load index
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

    // Load snapshots
    let mut snapshots = Vec::new();
    for entry in commits_to_load {
        let snapshot_path = crate::snapshot::snapshot_path(repo_root, &entry.sha);
        if snapshot_path.exists() {
            let json = std::fs::read_to_string(&snapshot_path)
                .with_context(|| format!("failed to read snapshot: {}", snapshot_path.display()))?;
            let snapshot = Snapshot::from_json(&json).with_context(|| {
                format!("failed to parse snapshot: {}", snapshot_path.display())
            })?;
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

/// Compute risk velocity for all functions in a window
///
/// Formula: `(LRS_last - LRS_first) / (commit_count - 1)`
/// Requires at least 2 data points.
pub fn compute_risk_velocities(snapshots: &[Snapshot]) -> Vec<RiskVelocity> {
    if snapshots.len() < 2 {
        return Vec::new();
    }

    // Collect function LRS values across snapshots
    let mut function_lrs: HashMap<String, Vec<(usize, f64)>> = HashMap::new();

    for (snapshot_idx, snapshot) in snapshots.iter().enumerate() {
        for func in &snapshot.functions {
            function_lrs
                .entry(func.function_id.clone())
                .or_default()
                .push((snapshot_idx, func.lrs));
        }
    }

    let mut velocities = Vec::new();

    for (function_id, lrs_points) in function_lrs {
        // Require at least 2 data points
        if lrs_points.len() < 2 {
            continue;
        }

        // Skip baseline-only functions (only appear in first snapshot)
        if lrs_points.len() == 1 && lrs_points[0].0 == 0 {
            continue;
        }

        // Sort by snapshot index
        let mut sorted_points = lrs_points;
        sorted_points.sort_by_key(|(idx, _)| *idx);

        let first_lrs = sorted_points[0].1;
        let last_lrs = sorted_points.last().unwrap().1;
        let commit_count = sorted_points.len();

        // Compute velocity: (LRS_last - LRS_first) / (commit_count - 1)
        let velocity = if commit_count > 1 {
            (last_lrs - first_lrs) / (commit_count - 1) as f64
        } else {
            0.0
        };

        // Determine direction
        let direction = if velocity.abs() < FLAT_VELOCITY_EPSILON {
            VelocityDirection::Flat
        } else if velocity > 0.0 {
            VelocityDirection::Positive
        } else {
            VelocityDirection::Negative
        };

        velocities.push(RiskVelocity {
            function_id,
            velocity,
            direction,
            first_lrs,
            last_lrs,
            commit_count,
        });
    }

    // Sort deterministically by function_id
    velocities.sort_by(|a, b| a.function_id.cmp(&b.function_id));

    velocities
}

/// Identify top K functions by LRS in a snapshot
fn top_k_functions(snapshot: &Snapshot, k: usize) -> Vec<String> {
    let mut functions: Vec<(&crate::snapshot::FunctionSnapshot, f64)> =
        snapshot.functions.iter().map(|f| (f, f.lrs)).collect();

    // Sort by LRS descending
    functions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top K
    functions
        .into_iter()
        .take(k)
        .map(|(f, _)| f.function_id.clone())
        .collect()
}

/// Compute hotspot stability for functions
///
/// Identifies top K functions per snapshot and computes overlap ratio.
pub fn compute_hotspot_stability(snapshots: &[Snapshot], top_k: usize) -> Vec<HotspotAnalysis> {
    if snapshots.is_empty() {
        return Vec::new();
    }

    // Collect top K functions per snapshot
    let mut top_k_per_snapshot: Vec<Vec<String>> = Vec::new();
    for snapshot in snapshots {
        top_k_per_snapshot.push(top_k_functions(snapshot, top_k));
    }

    // Collect all unique function IDs that appear in top K
    let mut all_top_k_functions: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for top_k_list in &top_k_per_snapshot {
        for function_id in top_k_list {
            all_top_k_functions.insert(function_id.clone());
        }
    }

    let mut hotspot_analyses = Vec::new();

    for function_id in all_top_k_functions {
        // Count appearances in top K
        let appearances = top_k_per_snapshot
            .iter()
            .filter(|top_k_list| top_k_list.contains(&function_id))
            .count();

        let total_snapshots = snapshots.len();
        let overlap_ratio = appearances as f64 / total_snapshots as f64;

        // Classify stability
        let stability = if overlap_ratio >= 0.8 {
            HotspotStability::Stable
        } else if overlap_ratio >= 0.5 {
            HotspotStability::Emerging
        } else {
            HotspotStability::Volatile
        };

        hotspot_analyses.push(HotspotAnalysis {
            function_id,
            stability,
            overlap_ratio,
            appearances_in_top_k: appearances,
            total_snapshots,
        });
    }

    // Sort deterministically by function_id
    hotspot_analyses.sort_by(|a, b| a.function_id.cmp(&b.function_id));

    hotspot_analyses
}

/// Compute refactor effectiveness for functions
///
/// Detects significant negative LRS deltas and classifies outcomes.
pub fn compute_refactor_effectiveness(snapshots: &[Snapshot]) -> Vec<RefactorAnalysis> {
    if snapshots.len() < 2 {
        return Vec::new();
    }

    // Track function LRS changes across snapshots
    let mut function_deltas: HashMap<String, Vec<(usize, f64)>> = HashMap::new();

    for i in 1..snapshots.len() {
        let prev_snapshot = &snapshots[i - 1];
        let curr_snapshot = &snapshots[i];

        // Build maps for efficient lookup
        let prev_funcs: HashMap<&str, &crate::snapshot::FunctionSnapshot> = prev_snapshot
            .functions
            .iter()
            .map(|f| (f.function_id.as_str(), f))
            .collect();

        let curr_funcs: HashMap<&str, &crate::snapshot::FunctionSnapshot> = curr_snapshot
            .functions
            .iter()
            .map(|f| (f.function_id.as_str(), f))
            .collect();

        // Compute deltas for functions that exist in both snapshots
        for (function_id, curr_func) in &curr_funcs {
            if let Some(prev_func) = prev_funcs.get(function_id) {
                let delta = curr_func.lrs - prev_func.lrs;
                function_deltas
                    .entry(function_id.to_string())
                    .or_default()
                    .push((i, delta));
            }
        }
    }

    let mut refactor_analyses = Vec::new();

    for (function_id, deltas) in function_deltas {
        // Find significant improvements (delta <= -1.0)
        let improvements: Vec<(usize, f64)> = deltas
            .iter()
            .filter(|(_, delta)| *delta <= REFACTOR_IMPROVEMENT_THRESHOLD)
            .copied()
            .collect();

        if improvements.is_empty() {
            continue;
        }

        // Find the first improvement
        let first_improvement_idx = improvements[0].0;
        let improvement_delta = improvements[0].1;

        // Check if improvement is sustained across ≥ 2 commits
        let mut sustained_commits = 1;
        let mut rebound_detected = false;

        // Look ahead to check sustainment and rebound
        for i in first_improvement_idx..snapshots.len().min(first_improvement_idx + 3) {
            if i >= snapshots.len() {
                break;
            }

            // Find delta for this commit
            if let Some((_, delta)) = deltas.iter().find(|(idx, _)| *idx == i) {
                if *delta <= REFACTOR_IMPROVEMENT_THRESHOLD {
                    sustained_commits += 1;
                } else if *delta >= REFACTOR_REBOUND_THRESHOLD {
                    rebound_detected = true;
                    break;
                }
            }
        }

        // Classify outcome
        let outcome = if sustained_commits >= 2 && !rebound_detected {
            RefactorOutcome::Successful
        } else if sustained_commits >= 2 && rebound_detected {
            RefactorOutcome::Partial
        } else {
            RefactorOutcome::Cosmetic
        };

        refactor_analyses.push(RefactorAnalysis {
            function_id,
            outcome,
            improvement_delta,
            sustained_commits,
            rebound_detected,
        });
    }

    // Sort deterministically by function_id
    refactor_analyses.sort_by(|a, b| a.function_id.cmp(&b.function_id));

    refactor_analyses
}

/// Compute complete trends analysis
pub fn analyze_trends(
    repo_root: &Path,
    window_size: usize,
    top_k: usize,
) -> Result<TrendsAnalysis> {
    let snapshots = load_snapshot_window(repo_root, window_size)?;

    let velocities = compute_risk_velocities(&snapshots);
    let hotspots = compute_hotspot_stability(&snapshots, top_k);
    let refactors = compute_refactor_effectiveness(&snapshots);

    Ok(TrendsAnalysis {
        velocities,
        hotspots,
        refactors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitContext;
    use crate::report::{FunctionRiskReport, MetricsReport, RiskReport};
    use crate::snapshot::FunctionSnapshot;

    fn create_test_snapshot(
        sha: &str,
        parent_sha: &str,
        functions: Vec<FunctionSnapshot>,
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

        let reports: Vec<FunctionRiskReport> = functions
            .iter()
            .map(|f| FunctionRiskReport {
                file: f.file.clone(),
                function: f.function_id.split("::").last().unwrap_or("").to_string(),
                line: f.line,
                language: f.language.clone(),
                metrics: f.metrics.clone(),
                risk: RiskReport {
                    r_cc: 0.0,
                    r_nd: 0.0,
                    r_fo: 0.0,
                    r_ns: 0.0,
                },
                lrs: f.lrs,
                band: f.band.clone(),
                suppression_reason: None,
                callees: vec![],
            })
            .collect();

        Snapshot::new(git_context, reports)
    }

    #[test]
    fn test_risk_velocity_positive() {
        let snapshots = vec![
            create_test_snapshot(
                "sha1",
                "sha0",
                vec![FunctionSnapshot {
                    function_id: "src/foo.ts::func".to_string(),
                    file: "src/foo.ts".to_string(),
                    line: 1,
                    language: "TypeScript".to_string(),
                    metrics: MetricsReport {
                        cc: 1,
                        nd: 0,
                        fo: 0,
                        ns: 0,
                        loc: 10,
                    },
                    lrs: 1.0,
                    band: "low".to_string(),
                    suppression_reason: None,
                    churn: None,
                    touch_count_30d: None,
                    days_since_last_change: None,
                    callgraph: None,
                    activity_risk: None,
                    risk_factors: None,
                    percentile: None,
                }],
            ),
            create_test_snapshot(
                "sha2",
                "sha1",
                vec![FunctionSnapshot {
                    function_id: "src/foo.ts::func".to_string(),
                    file: "src/foo.ts".to_string(),
                    line: 1,
                    language: "TypeScript".to_string(),
                    metrics: MetricsReport {
                        cc: 2,
                        nd: 1,
                        fo: 0,
                        ns: 0,
                        loc: 10,
                    },
                    lrs: 3.0,
                    band: "moderate".to_string(),
                    suppression_reason: None,
                    churn: None,
                    touch_count_30d: None,
                    days_since_last_change: None,
                    callgraph: None,
                    activity_risk: None,
                    risk_factors: None,
                    percentile: None,
                }],
            ),
        ];

        let velocities = compute_risk_velocities(&snapshots);
        assert_eq!(velocities.len(), 1);
        assert_eq!(velocities[0].function_id, "src/foo.ts::func");
        assert_eq!(velocities[0].velocity, 2.0); // (3.0 - 1.0) / (2 - 1) = 2.0
        assert_eq!(velocities[0].direction, VelocityDirection::Positive);
    }

    #[test]
    fn test_risk_velocity_flat() {
        let snapshots = vec![
            create_test_snapshot(
                "sha1",
                "sha0",
                vec![FunctionSnapshot {
                    function_id: "src/foo.ts::func".to_string(),
                    file: "src/foo.ts".to_string(),
                    line: 1,
                    language: "TypeScript".to_string(),
                    metrics: MetricsReport {
                        cc: 1,
                        nd: 0,
                        fo: 0,
                        ns: 0,
                        loc: 10,
                    },
                    lrs: 1.0,
                    band: "low".to_string(),
                    suppression_reason: None,
                    churn: None,
                    touch_count_30d: None,
                    days_since_last_change: None,
                    callgraph: None,
                    activity_risk: None,
                    risk_factors: None,
                    percentile: None,
                }],
            ),
            create_test_snapshot(
                "sha2",
                "sha1",
                vec![FunctionSnapshot {
                    function_id: "src/foo.ts::func".to_string(),
                    file: "src/foo.ts".to_string(),
                    line: 1,
                    language: "TypeScript".to_string(),
                    metrics: MetricsReport {
                        cc: 1,
                        nd: 0,
                        fo: 0,
                        ns: 0,
                        loc: 10,
                    },
                    lrs: 1.0,
                    band: "low".to_string(),
                    suppression_reason: None,
                    churn: None,
                    touch_count_30d: None,
                    days_since_last_change: None,
                    callgraph: None,
                    activity_risk: None,
                    risk_factors: None,
                    percentile: None,
                }],
            ),
        ];

        let velocities = compute_risk_velocities(&snapshots);
        assert_eq!(velocities.len(), 1);
        assert_eq!(velocities[0].direction, VelocityDirection::Flat);
    }

    #[test]
    fn test_hotspot_stability() {
        let snapshots = vec![
            create_test_snapshot(
                "sha1",
                "sha0",
                vec![
                    FunctionSnapshot {
                        function_id: "src/foo.ts::func1".to_string(),
                        file: "src/foo.ts".to_string(),
                        line: 1,
                        language: "TypeScript".to_string(),
                        metrics: MetricsReport {
                            cc: 10,
                            nd: 5,
                            fo: 3,
                            ns: 2,
                            loc: 20,
                        },
                        lrs: 15.0,
                        band: "high".to_string(),
                        suppression_reason: None,
                        churn: None,
                        touch_count_30d: None,
                        days_since_last_change: None,
                        callgraph: None,
                        activity_risk: None,
                        risk_factors: None,
                        percentile: None,
                    },
                    FunctionSnapshot {
                        function_id: "src/bar.ts::func2".to_string(),
                        file: "src/bar.ts".to_string(),
                        line: 1,
                        language: "TypeScript".to_string(),
                        metrics: MetricsReport {
                            cc: 5,
                            nd: 2,
                            fo: 1,
                            ns: 0,
                            loc: 10,
                        },
                        lrs: 5.0,
                        band: "moderate".to_string(),
                        suppression_reason: None,
                        churn: None,
                        touch_count_30d: None,
                        days_since_last_change: None,
                        callgraph: None,
                        activity_risk: None,
                        risk_factors: None,
                        percentile: None,
                    },
                ],
            ),
            create_test_snapshot(
                "sha2",
                "sha1",
                vec![
                    FunctionSnapshot {
                        function_id: "src/foo.ts::func1".to_string(),
                        file: "src/foo.ts".to_string(),
                        line: 1,
                        language: "TypeScript".to_string(),
                        metrics: MetricsReport {
                            cc: 12,
                            nd: 6,
                            fo: 4,
                            ns: 2,
                            loc: 25,
                        },
                        lrs: 18.0,
                        band: "high".to_string(),
                        suppression_reason: None,
                        churn: None,
                        touch_count_30d: None,
                        days_since_last_change: None,
                        callgraph: None,
                        activity_risk: None,
                        risk_factors: None,
                        percentile: None,
                    },
                    FunctionSnapshot {
                        function_id: "src/bar.ts::func2".to_string(),
                        file: "src/bar.ts".to_string(),
                        line: 1,
                        language: "TypeScript".to_string(),
                        metrics: MetricsReport {
                            cc: 5,
                            nd: 2,
                            fo: 1,
                            ns: 0,
                            loc: 10,
                        },
                        lrs: 5.0,
                        band: "moderate".to_string(),
                        suppression_reason: None,
                        churn: None,
                        touch_count_30d: None,
                        days_since_last_change: None,
                        callgraph: None,
                        activity_risk: None,
                        risk_factors: None,
                        percentile: None,
                    },
                ],
            ),
        ];

        let hotspots = compute_hotspot_stability(&snapshots, 1);
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].function_id, "src/foo.ts::func1");
        assert_eq!(hotspots[0].stability, HotspotStability::Stable);
        assert_eq!(hotspots[0].overlap_ratio, 1.0);
    }
}
