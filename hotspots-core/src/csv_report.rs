//! CSV (spreadsheet) report — file-level triage/planning view.
//!
//! Audience: a tech lead or EM doing triage, ownership handoff, or sprint
//! planning in a spreadsheet — not a raw per-function data dump (that's
//! `--format json --all-functions`) and not an in-IDE fix workflow (that's
//! the text/HTML outputs). Rows are one per file, with the three F05 axes
//! (Risk/Coupling/Ownership) as sortable/filterable columns side by side,
//! plus enough context (band, quadrant, function/critical counts, subsystem)
//! to triage without opening the tool again. Always the full file list,
//! ignoring `--top` — a spreadsheet is for sorting/filtering everything,
//! not a truncated view.

use crate::risk::RiskBand;
use crate::snapshot::FunctionSnapshot;
use serde::Serialize;
use std::path::Path;

/// Sentinel written for `coupling`/`ownership_newcomer_rate` when a file has no
/// defined value (excluded per `ranking::HotspotAxis`, not a measured zero) —
/// an explicit label reads as "no data for this file" in a spreadsheet, where a
/// truly blank cell reads as "something's broken."
const NO_AXIS_DATA: &str = "n/a";

/// One row per file. Column order here is the CSV column order.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileCsvRow {
    /// Repo-relative path (e.g. "hotspots-core/src/snapshot.rs"), not absolute —
    /// an absolute path is machine-specific noise in a shared spreadsheet.
    pub file: String,
    /// Risk axis: max `activity_risk.unwrap_or(lrs)` across the file's functions.
    pub risk_score: f64,
    /// Band of the file's highest-risk function.
    pub band: String,
    /// Quadrant of the file's highest-risk function (fire/debt/watch/ok), empty if unset.
    pub quadrant: String,
    /// Coupling axis: `directed_coupling` rounded to 2dp, or `"n/a"` if absent/zero
    /// (see `ranking::HotspotAxis`) — never a blank cell.
    pub coupling: String,
    /// Ownership axis: `newcomer_rate` rounded to 2dp, or `"n/a"` if the file has no
    /// commits in the 90-day window — never a blank cell, and never confused with a
    /// real `0.00` (a file with window commits and zero newcomers).
    pub ownership_newcomer_rate: String,
    pub function_count: usize,
    pub critical_count: usize,
    pub loc: u32,
    /// Nearest manifest-root ancestor (e.g. "packages/next"), empty for repo root or unset.
    pub subsystem: String,
}

fn fmt_axis_value(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.2}"),
        None => NO_AXIS_DATA.to_string(),
    }
}

/// Repo-relative path with `/` separators, falling back to the original string
/// (also `/`-normalized) if it isn't under `repo_root`.
fn relativize(file: &str, repo_root: &Path) -> String {
    Path::new(file)
        .strip_prefix(repo_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.replace('\\', "/"))
}

/// Aggregate `functions` into one row per file, sorted by `risk_score` descending
/// (ties broken by file path for determinism). Mirrors the file-level dedup logic
/// in `ranking::rank_by_axis` (max risk score per file; `directed_coupling` and
/// `newcomer_rate` are already file-level so any function's value represents the file).
pub fn compute_file_csv_rows(functions: &[FunctionSnapshot], repo_root: &Path) -> Vec<FileCsvRow> {
    use std::collections::HashMap;

    struct Acc<'a> {
        best_risk: f64,
        best_band: &'a RiskBand,
        best_quadrant: &'a Option<String>,
        coupling: Option<f64>,
        ownership_newcomer_rate: Option<f64>,
        function_count: usize,
        critical_count: usize,
        loc: u32,
        subsystem: &'a Option<String>,
    }

    let mut by_file: HashMap<&str, Acc> = HashMap::new();
    for f in functions {
        let risk = f.activity_risk.unwrap_or(f.lrs);
        let entry = by_file.entry(f.file.as_str()).or_insert(Acc {
            best_risk: f64::MIN,
            best_band: &f.band,
            best_quadrant: &f.quadrant,
            coupling: f.directed_coupling.filter(|&dc| dc != 0.0),
            ownership_newcomer_rate: f.newcomer_rate,
            function_count: 0,
            critical_count: 0,
            loc: 0,
            subsystem: &f.subsystem,
        });
        if risk > entry.best_risk {
            entry.best_risk = risk;
            entry.best_band = &f.band;
            entry.best_quadrant = &f.quadrant;
        }
        entry.function_count += 1;
        if matches!(f.band, RiskBand::Critical) {
            entry.critical_count += 1;
        }
        entry.loc += f.metrics.loc;
    }

    let mut rows: Vec<FileCsvRow> = by_file
        .into_iter()
        .map(|(file, acc)| FileCsvRow {
            file: relativize(file, repo_root),
            risk_score: acc.best_risk,
            band: acc.best_band.as_str().to_string(),
            quadrant: acc.best_quadrant.clone().unwrap_or_default(),
            coupling: fmt_axis_value(acc.coupling),
            ownership_newcomer_rate: fmt_axis_value(acc.ownership_newcomer_rate),
            function_count: acc.function_count,
            critical_count: acc.critical_count,
            loc: acc.loc,
            subsystem: acc.subsystem.clone().unwrap_or_default(),
        })
        .collect();

    rows.sort_by(|a, b| {
        b.risk_score
            .total_cmp(&a.risk_score)
            .then_with(|| a.file.cmp(&b.file))
    });
    rows
}

/// Render `functions` as a CSV string, one row per file (see `compute_file_csv_rows`).
pub fn render_csv(functions: &[FunctionSnapshot], repo_root: &Path) -> anyhow::Result<String> {
    let rows = compute_file_csv_rows(functions, repo_root);
    let mut writer = csv::Writer::from_writer(vec![]);
    for row in &rows {
        writer.serialize(row)?;
    }
    let bytes = writer.into_inner()?;
    Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;
    use crate::report::MetricsReport;

    fn fixture(file: &str, loc: u32, band: RiskBand) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: format!("{file}::f"),
            file: file.to_string(),
            line: 1,
            language: Language::Rust,
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc,
            },
            lrs: 0.0,
            band,
            suppression_reason: None,
            churn: None,
            touch_count_30d: None,
            days_since_last_change: None,
            callgraph: None,
            activity_risk: None,
            risk_factors: None,
            percentile: None,
            driver: None,
            driver_detail: None,
            quadrant: None,
            patterns: vec![],
            pattern_details: None,
            subsystem: None,
            authors_90d: None,
            directed_coupling: None,
            jaccard_label_stability: None,
            convention_bug_fix_count: None,
            burst_score: None,
            commit_count: None,
            author_count: None,
            author_entropy: None,
            isolation_rate: None,
            age_days: None,
            last_touch_days: None,
            newcomer_rate: None,
            explanation: None,
        }
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from("/repo")
    }

    #[test]
    fn one_row_per_file_not_per_function() {
        let functions = vec![
            fixture("a.rs", 10, RiskBand::Low),
            fixture("a.rs", 20, RiskBand::Low),
            fixture("b.rs", 5, RiskBand::Low),
        ];
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn risk_score_is_max_across_file_functions() {
        let mut functions = vec![
            fixture("a.rs", 10, RiskBand::Low),
            fixture("a.rs", 10, RiskBand::Low),
        ];
        functions[0].activity_risk = Some(3.0);
        functions[1].activity_risk = Some(7.0);
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].risk_score, 7.0);
    }

    #[test]
    fn loc_and_function_count_sum_across_file() {
        let functions = vec![
            fixture("a.rs", 10, RiskBand::Low),
            fixture("a.rs", 20, RiskBand::Low),
        ];
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].function_count, 2);
        assert_eq!(rows[0].loc, 30);
    }

    #[test]
    fn critical_count_matches_band() {
        let functions = vec![
            fixture("a.rs", 10, RiskBand::Critical),
            fixture("a.rs", 10, RiskBand::Low),
            fixture("a.rs", 10, RiskBand::Critical),
        ];
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].critical_count, 2);
    }

    #[test]
    fn coupling_excludes_zero() {
        let mut functions = vec![fixture("a.rs", 10, RiskBand::Low)];
        functions[0].directed_coupling = Some(0.0);
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].coupling, "n/a");
    }

    #[test]
    fn coupling_present_is_rounded_not_blank() {
        let mut functions = vec![fixture("a.rs", 10, RiskBand::Low)];
        functions[0].directed_coupling = Some(12.3456);
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].coupling, "12.35");
    }

    #[test]
    fn ownership_none_is_labeled_not_blank_and_not_zero() {
        let mut functions = vec![fixture("a.rs", 10, RiskBand::Low)];
        functions[0].newcomer_rate = None;
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].ownership_newcomer_rate, "n/a");
    }

    #[test]
    fn ownership_real_zero_is_distinct_from_no_data() {
        let mut functions = vec![fixture("a.rs", 10, RiskBand::Low)];
        functions[0].newcomer_rate = Some(0.0);
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].ownership_newcomer_rate, "0.00");
    }

    #[test]
    fn file_path_is_relativized_to_repo_root() {
        let functions = vec![fixture("/repo/src/a.rs", 10, RiskBand::Low)];
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].file, "src/a.rs");
    }

    #[test]
    fn file_path_outside_repo_root_falls_back_unchanged() {
        let functions = vec![fixture("/other/a.rs", 10, RiskBand::Low)];
        let rows = compute_file_csv_rows(&functions, &root());
        assert_eq!(rows[0].file, "/other/a.rs");
    }

    #[test]
    fn rows_sorted_by_risk_descending() {
        let mut functions = vec![
            fixture("a.rs", 10, RiskBand::Low),
            fixture("b.rs", 10, RiskBand::Low),
            fixture("c.rs", 10, RiskBand::Low),
        ];
        functions[0].activity_risk = Some(1.0);
        functions[1].activity_risk = Some(5.0);
        functions[2].activity_risk = Some(3.0);
        let rows = compute_file_csv_rows(&functions, &root());
        let files: Vec<&str> = rows.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(files, vec!["b.rs", "c.rs", "a.rs"]);
    }

    #[test]
    fn render_csv_produces_header_and_rows() {
        let functions = vec![fixture("a.rs", 10, RiskBand::Low)];
        let csv_str = render_csv(&functions, &root()).unwrap();
        let mut lines = csv_str.lines();
        assert_eq!(
            lines.next().unwrap(),
            "file,risk_score,band,quadrant,coupling,ownership_newcomer_rate,function_count,critical_count,loc,subsystem"
        );
        assert!(lines.next().unwrap().starts_with("a.rs,"));
    }
}
