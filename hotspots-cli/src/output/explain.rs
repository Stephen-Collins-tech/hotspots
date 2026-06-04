use crate::util::truncate_string;

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
        if view.file_churn > 0 {
            println!("   Churn: {} lines changed (30 days)", view.file_churn);
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

/// Print ranked module instability table.
pub(crate) fn print_module_output(
    modules: &[hotspots_core::aggregates::ModuleInstability],
    top: Option<usize>,
) -> anyhow::Result<()> {
    if modules.is_empty() {
        println!("No modules to display (import resolution produced no in-project edges).");
        return Ok(());
    }

    let total = modules.len();
    let display_count = top.map(|n| n.min(total)).unwrap_or(total);
    let title = if display_count < total {
        format!("Top {} Modules by Instability Risk", display_count)
    } else {
        "All Modules by Instability".to_string()
    };

    println!("{}", title);
    println!("{}", "=".repeat(80));
    println!();
    println!(
        "{:<3} {:<40} {:>5} {:>5} {:>7} {:>9} {:>9} {:>11} {:>5}",
        "#", "module", "files", "fns", "avg_cc", "afferent", "efferent", "instability", "risk"
    );
    println!("{}", "-".repeat(98));

    for (i, m) in modules.iter().take(display_count).enumerate() {
        println!(
            "{:<3} {:<40} {:>5} {:>5} {:>7.1} {:>9} {:>9} {:>11.3} {:>5}",
            i + 1,
            truncate_string(&m.module, 40),
            m.file_count,
            m.function_count,
            m.avg_complexity,
            m.afferent,
            m.efferent,
            m.instability,
            m.module_risk,
        );
    }

    println!("{}", "-".repeat(98));
    println!("Showing {}/{} modules", display_count, total);

    let high_risk_count = modules
        .iter()
        .take(display_count)
        .filter(|m| m.module_risk == "high")
        .count();
    if high_risk_count > 0 {
        println!(
            "High-risk modules (low instability + high complexity): {}",
            high_risk_count
        );
    }

    Ok(())
}

/// Format non-zero risk factor lines for a single function.
/// Print co-change coupling section (source files only).
pub(crate) fn print_co_change_section(co_change: &[hotspots_core::git::CoChangePair]) {
    const SRC_EXTS: &[&str] = &[
        ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".go", ".java", ".c", ".cpp", ".h",
    ];
    let is_src = |f: &str| SRC_EXTS.iter().any(|ext| f.ends_with(ext));
    let is_notable = |p: &&hotspots_core::git::CoChangePair| {
        is_src(&p.file_a) && is_src(&p.file_b) && p.risk != "low"
    };
    let notable: Vec<_> = co_change.iter().filter(is_notable).take(10).collect();
    if notable.is_empty() {
        return;
    }
    println!();
    println!("Co-Change Coupling (90-day window)");
    println!("{}", "=".repeat(80));
    for (i, pair) in notable.iter().enumerate() {
        let label = if pair.has_static_dep {
            "expected".to_string()
        } else {
            pair.risk.to_uppercase()
        };
        println!(
            "#{:<2} [{:8}] {:.2} ({:2}x)  {}  ↔  {}",
            i + 1,
            label,
            pair.coupling_ratio,
            pair.co_change_count,
            pair.file_a,
            pair.file_b,
        );
    }
    println!("{}", "-".repeat(80));
    let hidden_count = co_change
        .iter()
        .filter(|p| {
            (p.risk == "high" || p.risk == "moderate")
                && !p.has_static_dep
                && is_src(&p.file_a)
                && is_src(&p.file_b)
        })
        .count();
    let total_notable = co_change.iter().filter(is_notable).count();
    println!(
        "{} notable pairs ({} hidden coupling)  |  Run with --format json for full list",
        total_notable, hidden_count
    );
}

/// Print human-readable risk explanations for top functions.
pub(crate) fn print_explain_output(
    snapshot: &hotspots_core::snapshot::Snapshot,
    total_count: usize,
    co_change: &[hotspots_core::git::CoChangePair],
) -> anyhow::Result<()> {
    use hotspots_core::risk::RiskBand;

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

    let print_section =
        |header: &str, rows: &[&hotspots_core::snapshot::FunctionSnapshot], col_w: usize| {
            if rows.is_empty() {
                return;
            }
            println!("{} ({})", header, rows.len());
            for f in rows {
                let score = f.activity_risk.unwrap_or(f.lrs);
                let name = f.function_id.split("::").last().unwrap_or(&f.function_id);
                let loc = format!("{}:{}", rel_path(&f.file), f.line);
                let patterns_str = if f.patterns.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", f.patterns.join(", "))
                };
                println!(
                    "  {:.2}  {:<col_w$}  {}{}",
                    score,
                    loc,
                    name,
                    patterns_str,
                    col_w = col_w
                );
            }
            println!();
        };

    print_section("CRITICAL", &critical, col_width);
    print_section("HIGH", &high, col_width);
    if show_all {
        print_section("MEDIUM / LOW", &lower, col_width);
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

    print_co_change_section(co_change);

    Ok(())
}
