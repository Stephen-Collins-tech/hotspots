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
    /// Weight for commit-timing burstiness (F93: OSV/CVE ground truth showed a
    /// burst/ownership term the formula previously lacked outperforms the
    /// unweighted baseline by mean ΔAUC +0.116 across 10 validated repos).
    pub burst: f64,
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
            burst: 0.3,
        }
    }
}

/// Breakdown of risk score components.
///
/// `fan_in`/`cyclic_dependency` (F160), `depth` (F167), and `burst` were removed
/// in hotspots 2.0 — each was already hardcoded to always report `0.0` (dropped
/// from the live composite score per their respective validated non-inferiority
/// findings), kept only for schema compatibility until this cut. `churn` is
/// deliberately NOT included in that list: it is still a live, weighted
/// contributor to `activity_risk` (the heaviest weight in `ScoringWeights`), with
/// no equivalent non-inferiority finding for removing it — carved out of this
/// removal pass rather than bundled in by analogy to the others.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RiskFactors {
    pub complexity: f64,
    pub churn: f64,
    pub activity: f64,
    pub recency: f64,
    pub neighbor_churn: f64,
}

/// Input metrics for activity risk computation
pub struct ActivityRiskInput {
    pub lrs: f64,
    /// Lines added/deleted (optional)
    pub churn: Option<(usize, usize)>,
    pub touch_count: Option<usize>,
    pub days_since_last_change: Option<u32>,
    pub fan_in: Option<usize>,
    pub scc_size: Option<usize>,
    pub dependency_depth: Option<usize>,
    pub neighbor_churn: Option<usize>,
    /// Sliding 30-day-window max/mean commit ratio (F93). Higher values indicate
    /// a burst of frantic commit activity rather than steady, spread-out changes.
    pub burst_score: Option<f64>,
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

    // Touch factor: min(touch_count / 10, 5.0). The window is
    // `crate::git::TOUCH_WINDOW_DAYS` (365 days, per F165) — the field was
    // renamed from `touch_count_30d` in hotspots 2.0 to stop implying 30 days.
    let touch_score = if let Some(touches) = input.touch_count {
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

    // Fan-in/SCC (F160) and depth (F167) terms, and the separate burst term,
    // were already dropped from the live composite score (each individually
    // non-inferior per its own pre-registered finding) and reported as hardcoded
    // 0.0 in `RiskFactors` for schema compatibility only. hotspots 2.0 removes
    // those now-always-0.0 fields (`fan_in`, `cyclic_dependency`, `depth`,
    // `burst`) from `RiskFactors` entirely, so there is nothing left to compute
    // or report here — the underlying raw values (`fan_in`, `scc_size`,
    // `dependency_depth`) are still computed/populated/stored on the raw
    // `FunctionSnapshot`/`CallGraphMetrics` for other consumers (e.g.
    // `trainer::extract_features`'s `fan_in` column, pattern detection's
    // `scc_size`/`is_entrypoint`), this struct just no longer echoes them.

    // Neighbor churn factor: neighbor_churn / 500
    let neighbor_churn_score = if let Some(nc) = input.neighbor_churn {
        (nc as f64 / 500.0) * weights.neighbor_churn
    } else {
        0.0
    };

    // Burst factor: removed from the live composite score. burst_score is
    // computed over a file's entire commit history and is monotonic
    // non-decreasing, so including it here made activity_risk a one-way
    // ratchet — a single historical burst could permanently keep a file at
    // CRITICAL regardless of current state. See
    // hotspots-research/docs/burst-score-non-decaying-issue.md and
    // docs/promotion-briefs/burst-score-remove-from-live-score.md. The
    // `burst_score` value itself is still computed and stored (see
    // `history_signals.rs`/`snapshot.rs`) for other consumers such as
    // `trainer::cold_start_features`.

    // Total activity risk
    let activity_risk =
        complexity_score + churn_score + touch_score + recency_score + neighbor_churn_score;

    let risk_factors = RiskFactors {
        complexity: complexity_score,
        churn: churn_score,
        activity: touch_score,
        recency: recency_score,
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
                touch_count: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: None,
                neighbor_churn: None,
                burst_score: None,
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
                touch_count: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: None,
                neighbor_churn: None,
                burst_score: None,
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
                churn: Some((50, 50)),           // 100 lines changed
                touch_count: Some(20),           // 20 commits in 30d
                days_since_last_change: Some(1), // changed 1 day ago
                fan_in: Some(25),                // 25 callers
                scc_size: Some(3),               // in a 3-node cycle
                dependency_depth: Some(9),       // depth 9
                neighbor_churn: Some(1000),      // 1000 neighbor churn
                burst_score: None,
            },
            &ScoringWeights::default(),
        );

        // Expected contributions:
        // complexity: 10.0
        // churn: (100/100) * 0.5 = 0.5
        // touch: min(20/10, 5.0) * 0.3 = 2.0 * 0.3 = 0.6
        // recency: max(0, 5.0 - 1/7) * 0.2 ≈ 4.857 * 0.2 ≈ 0.971
        // fan_in, scc (F160), and depth (F167) were already dropped from the live
        // composite before this struct's 2.0 field removal; `fan_in`/`scc_size`/
        // `dependency_depth` inputs here are accepted (other callers still pass
        // them for other consumers) but contribute nothing to `risk` or `factors`.
        // neighbor_churn: 1000/500 * 0.2 = 2.0 * 0.2 = 0.4
        // total ≈ 10.0 + 0.5 + 0.6 + 0.971 + 0.4 ≈ 12.47

        assert!(risk > 12.0); // Should be higher than base LRS from the un-removed terms
        assert!(risk < 13.5); // ...but not as high as if fan_in/scc/depth still contributed
        assert_eq!(factors.complexity, 10.0);
        assert_eq!(factors.churn, 0.5);
        assert_eq!(factors.activity, 0.6);
    }

    #[test]
    fn test_compute_activity_risk_depth_does_not_affect_score() {
        let (risk_none, factors_none) = compute_activity_risk(
            &ActivityRiskInput {
                lrs: 5.0,
                churn: None,
                touch_count: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: None,
                neighbor_churn: None,
                burst_score: None,
            },
            &ScoringWeights::default(),
        );
        let (risk_deep, factors_deep) = compute_activity_risk(
            &ActivityRiskInput {
                lrs: 5.0,
                churn: None,
                touch_count: None,
                days_since_last_change: None,
                fan_in: None,
                scc_size: None,
                dependency_depth: Some(9),
                neighbor_churn: None,
                burst_score: None,
            },
            &ScoringWeights::default(),
        );

        assert_eq!(risk_none, risk_deep);
        assert_eq!(factors_none, factors_deep);
    }

    #[test]
    fn test_compute_activity_risk_with_burst_score() {
        let base_input = ActivityRiskInput {
            lrs: 10.0,
            churn: None,
            touch_count: None,
            days_since_last_change: None,
            fan_in: None,
            scc_size: None,
            dependency_depth: None,
            neighbor_churn: None,
            burst_score: None,
        };

        let (risk_without_burst, factors_without_burst) =
            compute_activity_risk(&base_input, &ScoringWeights::default());

        let (risk_with_burst, factors_with_burst) = compute_activity_risk(
            &ActivityRiskInput {
                burst_score: Some(4.0), // 4x max/mean burst ratio
                ..base_input
            },
            &ScoringWeights::default(),
        );

        // burst_score no longer contributes to activity_risk: a file with a
        // historical burst (burst_score: Some(4.0)) scores identically to
        // one without.
        assert_eq!(risk_with_burst, risk_without_burst);
        assert_eq!(factors_without_burst, factors_with_burst);
    }
}
