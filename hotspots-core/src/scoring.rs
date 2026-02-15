//! Activity-weighted risk scoring
//!
//! Combines LRS (complexity-based risk) with activity metrics and call graph metrics
//! to produce a unified risk score that identifies functions most in need of attention.

use serde::{Deserialize, Serialize};

/// Weights for computing activity-weighted risk score
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringWeights {
    pub churn: f64,
    pub touch: f64,
    pub recency: f64,
    pub fan_in: f64,
    pub scc: f64,
    pub depth: f64,
    pub neighbor_churn: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        ScoringWeights {
            churn: 0.5,
            touch: 0.3,
            recency: 0.2,
            fan_in: 0.4,
            scc: 0.3,
            depth: 0.1,
            neighbor_churn: 0.2,
        }
    }
}

/// Breakdown of risk score components
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RiskFactors {
    pub complexity: f64,
    pub churn: f64,
    pub activity: f64,
    pub recency: f64,
    pub fan_in: f64,
    pub cyclic_dependency: f64,
    pub depth: f64,
    pub neighbor_churn: f64,
}

/// Input metrics for activity risk computation
pub struct ActivityRiskInput {
    pub lrs: f64,
    /// Lines added/deleted (optional)
    pub churn: Option<(usize, usize)>,
    pub touch_count_30d: Option<usize>,
    pub days_since_last_change: Option<u32>,
    pub fan_in: Option<usize>,
    pub scc_size: Option<usize>,
    pub dependency_depth: Option<usize>,
    pub neighbor_churn: Option<usize>,
}

/// Compute activity-weighted risk score
///
/// Combines LRS (complexity risk) with activity and graph metrics.
pub fn compute_activity_risk(
    input: &ActivityRiskInput,
    weights: &ScoringWeights,
) -> (f64, RiskFactors) {
    // Base complexity score
    let complexity_score = input.lrs;

    // Churn factor: (lines_added + lines_deleted) / 100
    let churn_score = if let Some((added, deleted)) = input.churn {
        ((added + deleted) as f64 / 100.0) * weights.churn
    } else {
        0.0
    };

    // Touch factor: min(touch_count_30d / 10, 5.0)
    let touch_score = if let Some(touches) = input.touch_count_30d {
        ((touches as f64 / 10.0).min(5.0)) * weights.touch
    } else {
        0.0
    };

    // Recency factor: max(0, 5.0 - days_since_last_change / 7)
    let recency_score = if let Some(days) = input.days_since_last_change {
        ((5.0 - (days as f64 / 7.0)).max(0.0)) * weights.recency
    } else {
        0.0
    };

    // Fan-in factor: min(fan_in / 5, 10.0)
    let fan_in_score = if let Some(fi) = input.fan_in {
        ((fi as f64 / 5.0).min(10.0)) * weights.fan_in
    } else {
        0.0
    };

    // SCC penalty: scc_size if > 1, else 0
    let scc_score = if let Some(size) = input.scc_size {
        if size > 1 {
            (size as f64) * weights.scc
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Depth penalty: min(dependency_depth / 3, 5.0)
    let depth_score = if let Some(depth) = input.dependency_depth {
        ((depth as f64 / 3.0).min(5.0)) * weights.depth
    } else {
        0.0
    };

    // Neighbor churn factor: neighbor_churn / 500
    let neighbor_churn_score = if let Some(nc) = input.neighbor_churn {
        (nc as f64 / 500.0) * weights.neighbor_churn
    } else {
        0.0
    };

    // Total activity risk
    let activity_risk = complexity_score
        + churn_score
        + touch_score
        + recency_score
        + fan_in_score
        + scc_score
        + depth_score
        + neighbor_churn_score;

    let risk_factors = RiskFactors {
        complexity: complexity_score,
        churn: churn_score,
        activity: touch_score,
        recency: recency_score,
        fan_in: fan_in_score,
        cyclic_dependency: scc_score,
        depth: depth_score,
        neighbor_churn: neighbor_churn_score,
    };

    (activity_risk, risk_factors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_activity_risk_base_lrs_only() {
        let (risk, factors) = compute_activity_risk(
            &ActivityRiskInput {
                lrs: 10.0,
                churn: None,
                touch_count_30d: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: None,
                neighbor_churn: None,
            },
            &ScoringWeights::default(),
        );

        assert_eq!(risk, 10.0);
        assert_eq!(factors.complexity, 10.0);
        assert_eq!(factors.churn, 0.0);
        assert_eq!(factors.activity, 0.0);
    }

    #[test]
    fn test_compute_activity_risk_with_churn() {
        let (risk, factors) = compute_activity_risk(
            &ActivityRiskInput {
                lrs: 10.0,
                churn: Some((50, 50)), // 100 lines changed
                touch_count_30d: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: None,
                neighbor_churn: None,
            },
            &ScoringWeights::default(),
        );

        // churn_factor = 100 / 100 = 1.0, weighted = 1.0 * 0.5 = 0.5
        assert_eq!(risk, 10.5);
        assert_eq!(factors.churn, 0.5);
    }

    #[test]
    fn test_compute_activity_risk_with_all_factors() {
        let (risk, factors) = compute_activity_risk(
            &ActivityRiskInput {
                lrs: 10.0,
                churn: Some((50, 50)), // 100 lines changed
                touch_count_30d: Some(20), // 20 commits in 30d
                days_since_last_change: Some(1), // changed 1 day ago
                fan_in: Some(25),      // 25 callers
                scc_size: Some(3),     // in a 3-node cycle
                dependency_depth: Some(9), // depth 9
                neighbor_churn: Some(1000), // 1000 neighbor churn
            },
            &ScoringWeights::default(),
        );

        // Expected contributions:
        // complexity: 10.0
        // churn: (100/100) * 0.5 = 0.5
        // touch: min(20/10, 5.0) * 0.3 = 2.0 * 0.3 = 0.6
        // recency: max(0, 5.0 - 1/7) * 0.2 ≈ 4.857 * 0.2 ≈ 0.971
        // fan_in: min(25/5, 10.0) * 0.4 = 5.0 * 0.4 = 2.0
        // scc: 3 * 0.3 = 0.9
        // depth: min(9/3, 5.0) * 0.1 = 3.0 * 0.1 = 0.3
        // neighbor_churn: 1000/500 * 0.2 = 2.0 * 0.2 = 0.4

        assert!(risk > 15.0); // Should be significantly higher than base LRS
        assert_eq!(factors.complexity, 10.0);
        assert_eq!(factors.churn, 0.5);
        assert_eq!(factors.activity, 0.6);
        assert!((factors.cyclic_dependency - 0.9).abs() < 0.001); // Approximate equality for floats
    }
}
