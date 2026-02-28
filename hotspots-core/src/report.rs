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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub patterns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_details: Option<Vec<crate::patterns::PatternDetail>>,
    #[serde(skip, default)]
    pub callees: Vec<String>,
}

/// Metrics in report format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsReport {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
    pub ns: usize,
    pub loc: usize,
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

/// Grouped analysis results for constructing a FunctionRiskReport
pub struct FunctionAnalysis {
    pub metrics: RawMetrics,
    pub risk: RiskComponents,
    pub lrs: f64,
    pub band: RiskBand,
    pub patterns: Vec<String>,
}

impl FunctionRiskReport {
    /// Create a new function risk report
    pub fn new(
        function: &FunctionNode,
        file: String,
        language: String,
        analysis: FunctionAnalysis,
        source_map: &swc_common::SourceMap,
    ) -> Self {
        let line = function.start_line(source_map);
        let function_name = function
            .name
            .as_deref()
            .unwrap_or(&format!("<anonymous>@{}:{}", file, line))
            .to_string();

        FunctionRiskReport {
            file,
            function: function_name,
            line,
            language,
            metrics: MetricsReport {
                cc: analysis.metrics.cc,
                nd: analysis.metrics.nd,
                fo: analysis.metrics.fo,
                ns: analysis.metrics.ns,
                loc: analysis.metrics.loc,
            },
            risk: RiskReport {
                r_cc: analysis.risk.r_cc,
                r_nd: analysis.risk.r_nd,
                r_fo: analysis.risk.r_fo,
                r_ns: analysis.risk.r_ns,
            },
            lrs: analysis.lrs,
            band: analysis.band.as_str().to_string(),
            suppression_reason: function.suppression_reason.clone(),
            patterns: analysis.patterns,
            pattern_details: None,
            callees: analysis.metrics.callee_names,
        }
    }
}

/// Sort reports deterministically
pub fn sort_reports(mut reports: Vec<FunctionRiskReport>) -> Vec<FunctionRiskReport> {
    reports.sort_by(|a, b| {
        // 1. LRS descending
        b.lrs
            .partial_cmp(&a.lrs)
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
    let show_patterns = reports.iter().any(|r| !r.patterns.is_empty());

    // Header
    if show_patterns {
        output.push_str(&format!(
            "{:<8} {:<10} {:<20} {:<6} {:<30} {}\n",
            "LRS", "BAND", "FILE", "LINE", "FUNCTION", "PATTERNS"
        ));
    } else {
        output.push_str(&format!(
            "{:<8} {:<20} {:<6} {}\n",
            "LRS", "File", "Line", "Function"
        ));
    }

    // Reports
    for report in reports {
        let lrs_str = format!("{:.2}", report.lrs);
        if show_patterns {
            let patterns_str = if report.patterns.is_empty() {
                "-".to_string()
            } else {
                report.patterns.join(", ")
            };
            output.push_str(&format!(
                "{:<8} {:<10} {:<20} {:<6} {:<30} {}\n",
                lrs_str,
                report.band,
                truncate_or_pad(&report.file, 20),
                report.line,
                truncate_or_pad(&report.function, 30),
                patterns_str,
            ));
            if let Some(ref details) = report.pattern_details {
                for d in details {
                    let conds = d
                        .triggered_by
                        .iter()
                        .map(|t| format!("{}={} ({}{})", t.metric, t.value, t.op, t.threshold))
                        .collect::<Vec<_>>()
                        .join(", ");
                    output.push_str(&format!("           {}: {}\n", d.id, conds));
                }
            }
        } else {
            output.push_str(&format!(
                "{:<8} {:<20} {:<6} {}\n",
                lrs_str,
                truncate_or_pad(&report.file, 20),
                report.line,
                report.function
            ));
        }
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
