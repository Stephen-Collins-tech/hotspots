//! XLSX (spreadsheet) report — a real multi-sheet workbook for triage/planning.
//!
//! Reuses `csv_report`'s row computation (`compute_file_csv_rows`,
//! `compute_coupling_csv_rows`) — only the writer differs. Exists because CSV
//! has no notion of multiple tables in one file: coupling has a real value for
//! only a minority of files on a typical repo (see `csv_report` module docs),
//! so it needs a home that isn't the main table and isn't a second loose file
//! next to it. A real workbook is the only format that gives one physical
//! file with the tables kept separate, plus room for formatting (frozen
//! header, autofilter, a color scale on risk) that a plain CSV can't carry.

use crate::csv_report::{compute_coupling_csv_rows, compute_file_csv_rows};
use crate::snapshot::FunctionSnapshot;
use rust_xlsxwriter::{Color, ConditionalFormat3ColorScale, Format, Workbook};
use std::path::Path;

const FILES_SHEET: &str = "Files";
const COUPLING_SHEET: &str = "Coupling";

/// Render `functions` as an XLSX workbook with two sheets — "Files" (the main
/// triage table) and "Coupling" (files with a real `directed_coupling` value
/// only) — returning the file's raw bytes for the caller to write to disk.
pub fn render_xlsx(functions: &[FunctionSnapshot], repo_root: &Path) -> anyhow::Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let header_format = Format::new().set_bold();

    write_files_sheet(&mut workbook, functions, repo_root, &header_format)?;
    write_coupling_sheet(&mut workbook, functions, repo_root, &header_format)?;

    Ok(workbook.save_to_buffer()?)
}

fn write_files_sheet(
    workbook: &mut Workbook,
    functions: &[FunctionSnapshot],
    repo_root: &Path,
    header_format: &Format,
) -> anyhow::Result<()> {
    let rows = compute_file_csv_rows(functions, repo_root);
    let sheet = workbook.add_worksheet();
    sheet.set_name(FILES_SHEET)?;

    let headers = [
        "file",
        "risk_score",
        "band",
        "quadrant",
        "top_function",
        "top_function_line",
        "ownership_newcomer_rate",
        "function_count",
        "critical_count",
        "loc",
        "subsystem",
    ];
    for (col, name) in headers.iter().enumerate() {
        sheet.write_with_format(0, col as u16, *name, header_format)?;
    }

    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, row.file.as_str())?;
        sheet.write(r, 1, row.risk_score)?;
        sheet.write(r, 2, row.band.as_str())?;
        sheet.write(r, 3, row.quadrant.as_str())?;
        sheet.write(r, 4, row.top_function.as_str())?;
        sheet.write(r, 5, row.top_function_line)?;
        sheet.write(r, 6, row.ownership_newcomer_rate.as_str())?;
        sheet.write(r, 7, row.function_count as u32)?;
        sheet.write(r, 8, row.critical_count as u32)?;
        sheet.write(r, 9, row.loc)?;
        sheet.write(r, 10, row.subsystem.as_str())?;
    }

    let last_row = rows.len() as u32;
    let last_col = (headers.len() - 1) as u16;
    if last_row > 0 {
        sheet.autofilter(0, 0, last_row, last_col)?;

        // Risk axis color scale: low = green (safe), high = red (bad) — the
        // library's default 3-color scale runs the other way, so min/max are
        // swapped here.
        let risk_scale = ConditionalFormat3ColorScale::new()
            .set_minimum_color(Color::RGB(0x63BE7B))
            .set_midpoint_color(Color::RGB(0xFFEB84))
            .set_maximum_color(Color::RGB(0xF8696B));
        sheet.add_conditional_format(1, 1, last_row, 1, &risk_scale)?;
    }
    sheet.set_freeze_panes(1, 0)?;

    sheet.set_column_width(0, 45)?;
    sheet.set_column_width(4, 28)?;
    sheet.set_column_width(10, 18)?;

    Ok(())
}

fn write_coupling_sheet(
    workbook: &mut Workbook,
    functions: &[FunctionSnapshot],
    repo_root: &Path,
    header_format: &Format,
) -> anyhow::Result<()> {
    let rows = compute_coupling_csv_rows(functions, repo_root);
    let sheet = workbook.add_worksheet();
    sheet.set_name(COUPLING_SHEET)?;

    sheet.write_with_format(0, 0, "file", header_format)?;
    sheet.write_with_format(0, 1, "coupling", header_format)?;

    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write(r, 0, row.file.as_str())?;
        sheet.write(r, 1, row.coupling)?;
    }

    let last_row = rows.len() as u32;
    if last_row > 0 {
        sheet.autofilter(0, 0, last_row, 1)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    sheet.set_column_width(0, 45)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;
    use crate::report::MetricsReport;
    use crate::risk::RiskBand;

    fn fixture(file: &str) -> FunctionSnapshot {
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
                loc: 10,
            },
            lrs: 0.0,
            band: RiskBand::Low,
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

    #[test]
    fn render_xlsx_produces_nonempty_valid_zip() {
        let functions = vec![fixture("a.rs")];
        let bytes = render_xlsx(&functions, Path::new("/repo")).unwrap();
        // XLSX is a zip archive; verify the local file header magic bytes.
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn render_xlsx_handles_empty_input() {
        let bytes = render_xlsx(&[], Path::new("/repo")).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn render_xlsx_includes_coupling_data() {
        let mut functions = vec![fixture("a.rs"), fixture("b.rs")];
        functions[0].directed_coupling = Some(5.5);
        // Just verify it doesn't error with coupling data present.
        let bytes = render_xlsx(&functions, Path::new("/repo")).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }
}
