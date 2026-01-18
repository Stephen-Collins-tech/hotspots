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

/// Calculate Local Risk Score (LRS)
///
/// Formula:
/// LRS = 1.0 * R_cc + 0.8 * R_nd + 0.6 * R_fo + 0.7 * R_ns
pub fn calculate_lrs(risk: &RiskComponents) -> f64 {
    1.0 * risk.r_cc + 0.8 * risk.r_nd + 0.6 * risk.r_fo + 0.7 * risk.r_ns
}

/// Assign risk band based on LRS
pub fn assign_risk_band(lrs: f64) -> RiskBand {
    if lrs < 3.0 {
        RiskBand::Low
    } else if lrs < 6.0 {
        RiskBand::Moderate
    } else if lrs < 9.0 {
        RiskBand::High
    } else {
        RiskBand::Critical
    }
}

/// Calculate complete risk analysis from raw metrics
pub fn analyze_risk(metrics: &RawMetrics) -> (RiskComponents, f64, RiskBand) {
    let risk = calculate_risk_components(metrics);
    let lrs = calculate_lrs(&risk);
    let band = assign_risk_band(lrs);
    (risk, lrs, band)
}
