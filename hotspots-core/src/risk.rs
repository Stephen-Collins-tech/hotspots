//! Local Risk Score (LRS) calculation
//!
//! Global invariants enforced:
//! - Deterministic risk calculations
//! - Monotonic risk transforms

use crate::metrics::RawMetrics;

/// Risk components after transformation
#[derive(Debug, Clone)]
pub struct RiskComponents {
    pub r_cc: f64,
    pub r_nd: f64,
    pub r_fo: f64,
    pub r_ns: f64,
}

/// Risk band classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskBand {
    Low,      // < 3
    Moderate, // 3-6
    High,     // 6-9
    Critical, // >= 9
}

impl RiskBand {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskBand::Low => "low",
            RiskBand::Moderate => "moderate",
            RiskBand::High => "high",
            RiskBand::Critical => "critical",
        }
    }
}

/// Calculate risk transforms from raw metrics
///
/// Transforms:
/// - R_cc = min(log2(CC + 1), 6)
/// - R_nd = min(ND, 8)
/// - R_fo = min(log2(FO + 1), 6)
/// - R_ns = min(NS, 6)
pub fn calculate_risk_components(metrics: &RawMetrics) -> RiskComponents {
    RiskComponents {
        r_cc: (metrics.cc as f64 + 1.0).log2().min(6.0),
        r_nd: (metrics.nd as f64).min(8.0),
        r_fo: (metrics.fo as f64 + 1.0).log2().min(6.0),
        r_ns: (metrics.ns as f64).min(6.0),
    }
}

/// Configurable weights for LRS calculation
#[derive(Debug, Clone, Copy)]
pub struct LrsWeights {
    pub cc: f64,
    pub nd: f64,
    pub fo: f64,
    pub ns: f64,
}

impl Default for LrsWeights {
    fn default() -> Self {
        LrsWeights {
            cc: 1.0,
            nd: 0.8,
            fo: 0.6,
            ns: 0.7,
        }
    }
}

/// Configurable risk band thresholds
#[derive(Debug, Clone, Copy)]
pub struct RiskThresholds {
    pub moderate: f64,
    pub high: f64,
    pub critical: f64,
}

impl Default for RiskThresholds {
    fn default() -> Self {
        RiskThresholds {
            moderate: 3.0,
            high: 6.0,
            critical: 9.0,
        }
    }
}

/// Calculate Local Risk Score (LRS) with default weights
///
/// Formula:
/// LRS = 1.0 * R_cc + 0.8 * R_nd + 0.6 * R_fo + 0.7 * R_ns
pub fn calculate_lrs(risk: &RiskComponents) -> f64 {
    calculate_lrs_with_weights(risk, &LrsWeights::default())
}

/// Calculate LRS with custom weights
pub fn calculate_lrs_with_weights(risk: &RiskComponents, weights: &LrsWeights) -> f64 {
    weights.cc * risk.r_cc
        + weights.nd * risk.r_nd
        + weights.fo * risk.r_fo
        + weights.ns * risk.r_ns
}

/// Assign risk band based on LRS with default thresholds
pub fn assign_risk_band(lrs: f64) -> RiskBand {
    assign_risk_band_with_thresholds(lrs, &RiskThresholds::default())
}

/// Assign risk band with custom thresholds
pub fn assign_risk_band_with_thresholds(lrs: f64, thresholds: &RiskThresholds) -> RiskBand {
    if lrs < thresholds.moderate {
        RiskBand::Low
    } else if lrs < thresholds.high {
        RiskBand::Moderate
    } else if lrs < thresholds.critical {
        RiskBand::High
    } else {
        RiskBand::Critical
    }
}

/// Calculate complete risk analysis from raw metrics (default weights/thresholds)
pub fn analyze_risk(metrics: &RawMetrics) -> (RiskComponents, f64, RiskBand) {
    let risk = calculate_risk_components(metrics);
    let lrs = calculate_lrs(&risk);
    let band = assign_risk_band(lrs);
    (risk, lrs, band)
}

/// Calculate complete risk analysis with custom weights and thresholds
pub fn analyze_risk_with_config(
    metrics: &RawMetrics,
    weights: &LrsWeights,
    thresholds: &RiskThresholds,
) -> (RiskComponents, f64, RiskBand) {
    let risk = calculate_risk_components(metrics);
    let lrs = calculate_lrs_with_weights(&risk, weights);
    let band = assign_risk_band_with_thresholds(lrs, thresholds);
    (risk, lrs, band)
}
