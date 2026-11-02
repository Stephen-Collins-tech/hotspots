//! Activity risk scoring
//!
//! Activity risk combines LRS (structural risk) with git activity signals into one
//! absolute, per-function score. Each term is scaled into [0, 1] on a fixed scale so
//! no term swamps the others and the score does not depend on the rest of the
//! population:
//!
//! ```text
//! activity_risk = lrs / LRS_SCALE
//!               + min(log2(1 + churn_lines) / CHURN_LOG2_SCALE, 1)
//!               + min(touch_count / TOUCH_SCALE, 1)
//!               + max(0, 1 - days_since_last_change / RECENCY_DAYS)
//! ```
//!
//! Churn, touch and recency are file-level values applied to every function in the
//! file. A missing signal contributes 0.

use serde::{Deserialize, Serialize};

/// LRS value that maps to 1.0 (the observed maximum of the LRS scale).
pub const LRS_SCALE: f64 = 20.2;
/// `log2(1 + churn_lines)` value that maps to 1.0 (about 16k changed lines).
pub const CHURN_LOG2_SCALE: f64 = 14.0;
/// Touch count (commits in the touch window) that maps to 1.0.
pub const TOUCH_SCALE: f64 = 50.0;
/// Days since the last change after which the recency term reaches 0.
pub const RECENCY_DAYS: f64 = 35.0;

/// Breakdown of the four terms of `activity_risk`. Each field is in [0, 1]
/// (`complexity` can exceed 1.0 only if LRS exceeds `LRS_SCALE`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RiskFactors {
    pub complexity: f64,
    pub churn: f64,
    pub touch: f64,
    pub recency: f64,
}

/// Inputs to `compute_activity_risk`.
pub struct ActivityRiskInput {
    pub lrs: f64,
    /// Lines added and deleted over the churn window.
    pub churn: Option<(usize, usize)>,
    pub touch_count: Option<usize>,
    pub days_since_last_change: Option<u32>,
}

/// Compute activity risk and its per-term breakdown.
pub fn compute_activity_risk(input: &ActivityRiskInput) -> (f64, RiskFactors) {
    let complexity = input.lrs / LRS_SCALE;

    let churn = input.churn.map_or(0.0, |(added, deleted)| {
        (((1 + added + deleted) as f64).log2() / CHURN_LOG2_SCALE).min(1.0)
    });

    let touch = input
        .touch_count
        .map_or(0.0, |t| (t as f64 / TOUCH_SCALE).min(1.0));

    let recency = input
        .days_since_last_change
        .map_or(0.0, |d| (1.0 - d as f64 / RECENCY_DAYS).max(0.0));

    let risk_factors = RiskFactors {
        complexity,
        churn,
        touch,
        recency,
    };
    (complexity + churn + touch + recency, risk_factors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        lrs: f64,
        churn: Option<(usize, usize)>,
        touch_count: Option<usize>,
        days: Option<u32>,
    ) -> ActivityRiskInput {
        ActivityRiskInput {
            lrs,
            churn,
            touch_count,
            days_since_last_change: days,
        }
    }

    #[test]
    fn no_git_signals_is_scaled_lrs_only() {
        let (score, f) = compute_activity_risk(&input(10.1, None, None, None));
        assert!((score - 0.5).abs() < 1e-12);
        assert_eq!((f.churn, f.touch, f.recency), (0.0, 0.0, 0.0));
    }

    #[test]
    fn terms_are_capped_at_one() {
        let (_, f) = compute_activity_risk(&input(0.0, Some((10_000_000, 0)), Some(500), Some(0)));
        assert_eq!(f.churn, 1.0);
        assert_eq!(f.touch, 1.0);
        assert_eq!(f.recency, 1.0);
    }

    #[test]
    fn recency_decays_linearly_to_zero_at_35_days() {
        let at = |d| {
            compute_activity_risk(&input(0.0, None, None, Some(d)))
                .1
                .recency
        };
        assert_eq!(at(0), 1.0);
        assert!((at(7) - 0.8).abs() < 1e-12);
        assert_eq!(at(35), 0.0);
        assert_eq!(at(400), 0.0);
    }

    #[test]
    fn churn_is_log_scaled_added_plus_deleted() {
        let f = compute_activity_risk(&input(0.0, Some((100, 27)), None, None)).1;
        assert!((f.churn - (128.0_f64.log2() / 14.0)).abs() < 1e-12);
    }

    /// Parity with the research reconstruction of the same formula
    /// (`L + C + T + R`, with L = LRS/20.2, C = min(log2(1+c)/14, 1),
    /// T = min(t/50, 1), R = max(0, 1 - d/35)). Expected values were computed
    /// independently in Python.
    #[test]
    fn matches_research_reconstruction() {
        let cases: [(f64, usize, usize, u32, f64); 3] = [
            (12.0, 340, 12, 9, 2.1778899722994636),
            (3.5, 0, 2, 200, 0.2132673267326733),
            (20.2, 90_000, 80, 0, 4.0),
        ];
        for (lrs, churn, touch, days, expected) in cases {
            let (score, _) =
                compute_activity_risk(&input(lrs, Some((churn, 0)), Some(touch), Some(days)));
            assert!(
                (score - expected).abs() < 1e-9,
                "lrs={lrs} churn={churn} touch={touch} days={days}: got {score}, want {expected}"
            );
        }
    }
}
