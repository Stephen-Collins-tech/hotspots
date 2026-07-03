//! Reporting and output generation
//!
//! Global invariants enforced:
//! - Deterministic output ordering
//! - Byte-for-byte identical output across runs

use crate::ast::FunctionNode;
use crate::language::Language;
use crate::metrics::RawMetrics;
use crate::risk::{RiskBand, RiskComponents};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

/// Complete risk report for a function
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionRiskReport {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub language: Language,
    pub metrics: MetricsReport,
    pub risk: RiskReport,
    pub lrs: f64,
    pub band: RiskBand,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub patterns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_details: Option<Vec<crate::patterns::PatternDetail>>,
    #[serde(skip, default)]
    pub callees: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// Metrics in report format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsReport {
    pub cc: u32,
    pub nd: u32,
    pub fo: u32,
    pub ns: u32,
    pub loc: u32,
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
        language: Language,
        analysis: FunctionAnalysis,
        source_map: &swc_common::SourceMap,
    ) -> Self {
        let line = function.start_line(source_map);
        let display_file = std::env::current_dir()
            .ok()
            .and_then(|cwd| {
                std::path::Path::new(&file)
                    .strip_prefix(&cwd)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| file.clone());
        let function_name = function
            .name
            .as_deref()
            .unwrap_or(&format!("<anonymous>@{}:{}", display_file, line))
            .to_string();

        FunctionRiskReport {
            file,
            function: function_name,
            line,
            language,
            metrics: MetricsReport {
                cc: analysis.metrics.cc as u32,
                nd: analysis.metrics.nd as u32,
                fo: analysis.metrics.fo as u32,
                ns: analysis.metrics.ns as u32,
                loc: analysis.metrics.loc as u32,
            },
            risk: RiskReport {
                r_cc: analysis.risk.r_cc,
                r_nd: analysis.risk.r_nd,
                r_fo: analysis.risk.r_fo,
                r_ns: analysis.risk.r_ns,
            },
            lrs: analysis.lrs,
            band: analysis.band,
            suppression_reason: function.suppression_reason.clone(),
            patterns: analysis.patterns,
            pattern_details: None,
            callees: analysis.metrics.callee_names,
            explanation: None,
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
                report.band.as_str(),
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

/// Render reports grouped by risk band (CRITICAL → HIGH → MODERATE/LOW).
///
/// MODERATE and LOW are omitted unless `limit` is `usize::MAX` (i.e. `--top 0`).
/// `color` enables ANSI codes — pass `false` when stdout is not a TTY.
pub fn render_text_grouped(reports: &[FunctionRiskReport], limit: usize, color: bool) -> String {
    let show_all = limit == usize::MAX;
    let mut output = String::new();
    let cwd = std::env::current_dir().ok();

    let rel_path = |p: &str| -> String {
        cwd.as_ref()
            .and_then(|cwd| {
                std::path::Path::new(p)
                    .strip_prefix(cwd)
                    .ok()
                    .map(|r| r.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| p.to_string())
    };

    let critical: Vec<&FunctionRiskReport> = reports
        .iter()
        .filter(|r| r.band == RiskBand::Critical)
        .collect();
    let high: Vec<&FunctionRiskReport> = reports
        .iter()
        .filter(|r| r.band == RiskBand::High)
        .collect();
    let lower: Vec<&FunctionRiskReport> = reports
        .iter()
        .filter(|r| matches!(r.band, RiskBand::Moderate | RiskBand::Low))
        .collect();

    // Compute file:line column width across all visible rows for alignment.
    let visible: Vec<&&FunctionRiskReport> = critical
        .iter()
        .chain(high.iter())
        .chain(if show_all { lower.iter() } else { [].iter() })
        .collect();
    let col_width = visible
        .iter()
        .map(|r| format!("{}:{}", rel_path(&r.file), r.line).len())
        .max()
        .unwrap_or(30)
        .min(55);

    let render_section = |header: &str,
                          rows: &[&FunctionRiskReport],
                          col_w: usize,
                          rel: &dyn Fn(&str) -> String,
                          paint: &dyn Fn(&str) -> String|
     -> String {
        let mut s = String::new();
        if rows.is_empty() {
            return s;
        }
        s.push_str(&format!("{} ({})\n", paint(header), rows.len()));
        for r in rows {
            let loc = format!("{}:{}", rel(&r.file), r.line);
            let patterns_str = if r.patterns.is_empty() {
                String::new()
            } else {
                format!("  [{}]", r.patterns.join(", "))
            };
            s.push_str(&format!(
                "  {:.2}  {:<col_w$}  {}{}",
                r.lrs,
                loc,
                r.function,
                patterns_str,
                col_w = col_w
            ));
            s.push('\n');
            if let Some(exp) = &r.explanation {
                let suffix = if r.band == RiskBand::Critical {
                    "Multiple independent signals agree."
                } else {
                    "Worth prioritising before next release."
                };
                s.push_str(&format!(
                    "         \u{2726} {}\n           {}\n",
                    exp, suffix
                ));
            }
        }
        s.push('\n');
        s
    };

    let red = |s: &str| -> String {
        if color {
            s.red().bold().to_string()
        } else {
            s.to_string()
        }
    };
    let yellow = |s: &str| -> String {
        if color {
            s.yellow().bold().to_string()
        } else {
            s.to_string()
        }
    };
    let green = |s: &str| -> String {
        if color {
            s.green().to_string()
        } else {
            s.to_string()
        }
    };

    output.push_str(&render_section(
        "CRITICAL", &critical, col_width, &rel_path, &red,
    ));
    output.push_str(&render_section(
        "HIGH", &high, col_width, &rel_path, &yellow,
    ));
    if show_all {
        output.push_str(&render_section(
            "MEDIUM / LOW",
            &lower,
            col_width,
            &rel_path,
            &green,
        ));
    }

    let sep = "─".repeat(60);
    output.push_str(&sep);
    output.push('\n');
    if show_all {
        output.push_str(&format!("{} functions total\n", reports.len()));
    } else {
        let shown = critical.len() + high.len();
        let hidden = lower.len();
        output.push_str(&format!(
            "{} functions shown ({} medium/low omitted)\n",
            shown, hidden
        ));
        output.push_str("Use --top 0 to show all  ·  --top N for a different limit  ·  --format json for full output\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;
    use crate::risk::RiskBand;

    fn make_report(file: &str, function: &str, line: u32, lrs: f64) -> FunctionRiskReport {
        FunctionRiskReport {
            file: file.to_string(),
            function: function.to_string(),
            line,
            language: Language::TypeScript,
            metrics: MetricsReport {
                cc: 5,
                nd: 1,
                fo: 2,
                ns: 0,
                loc: 20,
            },
            risk: RiskReport {
                r_cc: 1.0,
                r_nd: 0.5,
                r_fo: 0.5,
                r_ns: 0.0,
            },
            lrs,
            band: RiskBand::High,
            suppression_reason: None,
            patterns: vec![],
            pattern_details: None,
            callees: vec![],
            explanation: None,
        }
    }

    #[test]
    fn test_render_text_grouped_groups_by_band() {
        let mut critical = make_report("/repo/src/a.ts", "foo", 10, 12.0);
        critical.band = RiskBand::Critical;
        let mut high = make_report("/repo/src/b.ts", "bar", 20, 7.0);
        high.band = RiskBand::High;
        let reports = vec![critical, high];
        let out = render_text_grouped(&reports, 20, false);
        let crit_pos = out.find("CRITICAL").unwrap_or(usize::MAX);
        let high_pos = out.find("HIGH").unwrap_or(usize::MAX);
        assert!(
            crit_pos < high_pos,
            "CRITICAL section should appear before HIGH"
        );
        assert!(out.contains("foo"), "should contain foo");
        assert!(out.contains("bar"), "should contain bar");
    }

    #[test]
    fn test_render_text_grouped_footer_counts() {
        let mut r1 = make_report("/repo/src/a.ts", "foo", 10, 12.0);
        r1.band = RiskBand::Critical;
        let mut r2 = make_report("/repo/src/b.ts", "bar", 20, 7.0);
        r2.band = RiskBand::High;
        let out = render_text_grouped(&[r1, r2], 20, false);
        assert!(
            out.contains("2 functions shown"),
            "footer should show shown count"
        );
    }

    #[test]
    fn test_render_text_grouped_lower_omitted_by_default() {
        let mut r = make_report("/repo/src/a.ts", "foo", 10, 2.0);
        r.band = RiskBand::Low;
        let out = render_text_grouped(&[r], 20, false);
        assert!(
            !out.contains("foo"),
            "low band should be omitted by default"
        );
        assert!(
            out.contains("medium/low omitted"),
            "footer should note omission"
        );
    }

    #[test]
    fn test_render_text_grouped_lower_shown_when_show_all() {
        let mut r = make_report("/repo/src/a.ts", "foo", 10, 2.0);
        r.band = RiskBand::Low;
        let out = render_text_grouped(&[r], usize::MAX, false);
        assert!(
            out.contains("foo"),
            "low band should be shown with show-all"
        );
        assert!(out.contains("MEDIUM / LOW"));
    }

    #[test]
    fn test_render_text_grouped_patterns_shown() {
        let mut r = make_report("/repo/src/a.ts", "foo", 10, 12.0);
        r.band = RiskBand::Critical;
        r.patterns = vec!["god_function".to_string(), "exit_heavy".to_string()];
        let out = render_text_grouped(&[r], 20, false);
        assert!(out.contains("[god_function, exit_heavy]"));
    }

    #[test]
    fn test_render_text_grouped_empty() {
        let out = render_text_grouped(&[], 20, false);
        assert!(out.contains("0 functions shown"));
    }

    #[test]
    fn test_render_text_grouped_color_emits_ansi() {
        let mut r = make_report("/repo/src/a.ts", "critical_fn", 1, 12.0);
        r.band = RiskBand::Critical;
        let out = render_text_grouped(&[r], 10, true);
        assert!(
            out.contains("\x1b["),
            "color=true should emit ANSI escape codes"
        );
    }

    #[test]
    fn test_render_text_grouped_no_color_plain() {
        let mut r = make_report("/repo/src/a.ts", "critical_fn", 1, 12.0);
        r.band = RiskBand::Critical;
        let out = render_text_grouped(&[r], 10, false);
        assert!(
            !out.contains("\x1b["),
            "color=false must not emit ANSI escape codes"
        );
    }
}
