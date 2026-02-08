//! Reporting and output generation
//!
//! Global invariants enforced:
//! - Deterministic output ordering
//! - Byte-for-byte identical output across runs

use crate::ast::FunctionNode;
use crate::metrics::RawMetrics;
use crate::risk::{RiskBand, RiskComponents};
use serde::{Deserialize, Serialize};

/// Complete risk report for a function
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionRiskReport {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub language: String,
    pub metrics: MetricsReport,
    pub risk: RiskReport,
    pub lrs: f64,
    pub band: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
}

/// Metrics in report format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsReport {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
    pub ns: usize,
}

/// Risk components in report format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReport {
    #[serde(rename = "r_cc")]
    pub r_cc: f64,
    #[serde(rename = "r_nd")]
    pub r_nd: f64,
    #[serde(rename = "r_fo")]
    pub r_fo: f64,
    #[serde(rename = "r_ns")]
    pub r_ns: f64,
}

impl FunctionRiskReport {
    /// Create a new function risk report
    pub fn new(
        function: &FunctionNode,
        file: String,
        language: String,
        metrics: RawMetrics,
        risk: RiskComponents,
        lrs: f64,
        band: RiskBand,
        source_map: &swc_common::SourceMap,
    ) -> Self {
        let function_name = function.name.as_deref()
            .unwrap_or(&format!("<anonymous>@{}:{}", file, function.start_line(source_map)))
            .to_string();

        FunctionRiskReport {
            file,
            function: function_name,
            line: function.start_line(source_map),
            language,
            metrics: MetricsReport {
                cc: metrics.cc,
                nd: metrics.nd,
                fo: metrics.fo,
                ns: metrics.ns,
            },
            risk: RiskReport {
                r_cc: risk.r_cc,
                r_nd: risk.r_nd,
                r_fo: risk.r_fo,
                r_ns: risk.r_ns,
            },
            lrs,
            band: band.as_str().to_string(),
            suppression_reason: function.suppression_reason.clone(),
        }
    }
}

/// Sort reports deterministically
pub fn sort_reports(mut reports: Vec<FunctionRiskReport>) -> Vec<FunctionRiskReport> {
    reports.sort_by(|a, b| {
        // 1. LRS descending
        b.lrs.partial_cmp(&a.lrs)
            .unwrap_or(std::cmp::Ordering::Equal)
            // 2. File path ascending
            .then_with(|| a.file.cmp(&b.file))
            // 3. Line number ascending
            .then_with(|| a.line.cmp(&b.line))
            // 4. Function name ascending
            .then_with(|| a.function.cmp(&b.function))
    });
    reports
}

/// Render reports as text output
pub fn render_text(reports: &[FunctionRiskReport]) -> String {
    let mut output = String::new();
    
    // Header
    output.push_str(&format!("{:<8} {:<20} {:<6} {}\n", "LRS", "File", "Line", "Function"));
    
    // Reports
    for report in reports {
        let lrs_str = format!("{:.2}", report.lrs);
        output.push_str(&format!(
            "{:<8} {:<20} {:<6} {}\n",
            lrs_str,
            truncate_or_pad(&report.file, 20),
            report.line,
            report.function
        ));
    }
    
    output
}

/// Render reports as JSON output
pub fn render_json(reports: &[FunctionRiskReport]) -> String {
    // Use serde_json with sorted keys for deterministic output
    serde_json::to_string_pretty(reports).unwrap_or_else(|_| "[]".to_string())
}

/// Truncate or pad string to fixed width
fn truncate_or_pad(s: &str, width: usize) -> String {
    if s.len() > width {
        format!("{}...", &s[..width.saturating_sub(3)])
    } else {
        format!("{:<width$}", s, width = width)
    }
}

