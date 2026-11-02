/// Print ranked file risk table.
pub(crate) fn print_file_risk_output(
    file_risk: &[hotspots_core::aggregates::FileRiskView],
    top: Option<usize>,
) -> anyhow::Result<()> {
    if file_risk.is_empty() {
        println!("No files to display.");
        return Ok(());
    }

    let total = file_risk.len();
    let display_count = top.map(|n| n.min(total)).unwrap_or(total);
    let title = if display_count < total {
        format!("Top {} Files by Risk Score", display_count)
    } else {
        "All Files by Risk Score".to_string()
    };

    println!("{}", title);
    println!("{}", "=".repeat(80));
    println!();

    for (i, view) in file_risk.iter().take(display_count).enumerate() {
        println!("#{} {}", i + 1, view.file);
        println!(
            "   Functions: {} | LOC: {} | Max CC: {} | Avg CC: {:.1}",
            view.function_count, view.loc, view.max_cc, view.avg_cc
        );
        println!("   Risk Score: {:.2}", view.file_risk_score);
        if let (Some(blended), Some(churn_pct)) =
            (view.file_risk_score_blended, view.churn_percentile)
        {
            // blended = (static_pct + churn_pct) / 2 -- recover static_pct rather than
            // carrying a third field on FileRiskView just for this display line.
            let static_pct = 2.0 * blended - churn_pct;
            println!(
                "   Blended Score: {:.2} (static percentile {:.2}, churn percentile {:.2})",
                blended, static_pct, churn_pct
            );
        }
        if view.critical_count > 0 {
            println!("   Critical functions: {}", view.critical_count);
        }
        println!();
    }

    println!("{}", "-".repeat(80));
    println!("Showing {}/{} files", display_count, total);

    Ok(())
}

/// Print human-readable risk explanations for top functions.
pub(crate) fn print_explain_output(
    snapshot: &hotspots_core::snapshot::Snapshot,
    total_count: usize,
    color: bool,
) -> anyhow::Result<()> {
    use hotspots_core::risk::RiskBand;
    use owo_colors::OwoColorize;

    let funcs = &snapshot.functions;
    if funcs.is_empty() {
        println!("No functions to display.");
        return Ok(());
    }

    let show_all = funcs.len() >= total_count;

    let critical: Vec<_> = funcs
        .iter()
        .filter(|f| f.band == RiskBand::Critical)
        .collect();
    let high: Vec<_> = funcs.iter().filter(|f| f.band == RiskBand::High).collect();
    let lower: Vec<_> = funcs
        .iter()
        .filter(|f| matches!(f.band, RiskBand::Moderate | RiskBand::Low))
        .collect();

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

    // Compute file:line column width across all visible rows.
    let visible_iter: Box<dyn Iterator<Item = &&hotspots_core::snapshot::FunctionSnapshot>> =
        if show_all {
            Box::new(critical.iter().chain(high.iter()).chain(lower.iter()))
        } else {
            Box::new(critical.iter().chain(high.iter()))
        };
    let col_width = visible_iter
        .map(|f| format!("{}:{}", rel_path(&f.file), f.line).len())
        .max()
        .unwrap_or(30)
        .min(55);

    let paint = |s: &str, paint_fn: &dyn Fn(&str) -> String| -> String {
        if color {
            paint_fn(s)
        } else {
            s.to_string()
        }
    };
    let red = |s: &str| s.red().bold().to_string();
    let yellow = |s: &str| s.yellow().bold().to_string();
    let green = |s: &str| s.green().to_string();

    let print_section = |header: &str,
                         rows: &[&hotspots_core::snapshot::FunctionSnapshot],
                         col_w: usize,
                         painter: &dyn Fn(&str) -> String| {
        if rows.is_empty() {
            return;
        }
        println!("{} ({})", paint(header, painter), rows.len());
        for f in rows {
            let score = f.activity_risk.unwrap_or(f.lrs);
            let name = f.function_id.split("::").last().unwrap_or(&f.function_id);
            let loc = format!("{}:{}", rel_path(&f.file), f.line);
            println!("  {:.2}  {:<col_w$}  {}", score, loc, name, col_w = col_w);
            if let Some(exp) = &f.explanation {
                println!("         \u{2726} {}", exp);
            }
        }
        println!();
    };

    print_section("CRITICAL", &critical, col_width, &red);
    print_section("HIGH", &high, col_width, &yellow);
    if show_all {
        print_section("MEDIUM / LOW", &lower, col_width, &green);
    }

    println!("{}", "─".repeat(60));
    if show_all {
        println!("{} functions total", funcs.len());
    } else {
        let shown = critical.len() + high.len();
        let hidden = lower.len();
        println!("{} functions shown ({} medium/low omitted)", shown, hidden);
        println!("Use --top 0 to show all  ·  --top N for a different limit  ·  --format json for full output");
    }

    Ok(())
}
