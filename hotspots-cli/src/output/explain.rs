use crate::util::{driving_dimension, truncate_string};

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
pub(crate) fn format_risk_factor_lines(
    func: &hotspots_core::snapshot::FunctionSnapshot,
) -> Vec<String> {
    let factors = match func.risk_factors.as_ref() {
        Some(f) => f,
        None => return Vec::new(),
    };
    let mut lines = Vec::new();
    if factors.complexity > 0.0 {
        lines.push(format!(
            "     • Complexity:      {:>6.2}  (cyclomatic={}, nesting={}, fanout={})",
            factors.complexity, func.metrics.cc, func.metrics.nd, func.metrics.fo
        ));
    }
    if factors.churn > 0.0 {
        let churn_lines = func
            .churn
            .as_ref()
            .map(|c| c.lines_added + c.lines_deleted)
            .unwrap_or(0);
        lines.push(format!(
            "     • Churn:           {:>6.2}  ({} lines changed recently)",
            factors.churn, churn_lines
        ));
    }
    if factors.activity > 0.0 {
        let touches = func.touch_count_30d.unwrap_or(0);
        lines.push(format!(
            "     • Activity:        {:>6.2}  ({} commits in last 30 days)",
            factors.activity, touches
        ));
    }
    if factors.recency > 0.0 {
        let days = func.days_since_last_change.unwrap_or(0);
        lines.push(format!(
            "     • Recency:         {:>6.2}  (last changed {} days ago)",
            factors.recency, days
        ));
    }
    if factors.fan_in > 0.0 {
        let fi = func.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0);
        lines.push(format!(
            "     • Fan-in:          {:>6.2}  ({} functions depend on this)",
            factors.fan_in, fi
        ));
    }
    if factors.cyclic_dependency > 0.0 {
        let scc = func.callgraph.as_ref().map(|cg| cg.scc_size).unwrap_or(1);
        lines.push(format!(
            "     • Cyclic deps:     {:>6.2}  (in a {}-function cycle)",
            factors.cyclic_dependency, scc
        ));
    }
    if factors.depth > 0.0 {
        let depth = func
            .callgraph
            .as_ref()
            .and_then(|cg| cg.dependency_depth)
            .unwrap_or(0);
        lines.push(format!(
            "     • Depth:           {:>6.2}  ({} levels from entry point)",
            factors.depth, depth
        ));
    }
    if factors.neighbor_churn > 0.0 {
        let nc = func
            .callgraph
            .as_ref()
            .and_then(|cg| cg.neighbor_churn)
            .unwrap_or(0);
        lines.push(format!(
            "     • Neighbor churn:  {:>6.2}  ({} lines changed in dependencies)",
            factors.neighbor_churn, nc
        ));
    }
    lines
}

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
    let display_count = snapshot.functions.len();

    if display_count == 0 {
        println!("No functions to display.");
        return Ok(());
    }

    let title = if display_count < total_count {
        format!("Top {} Functions by Activity Risk", display_count)
    } else {
        "All Functions by Activity Risk".to_string()
    };

    println!("{}", title);
    println!("{}", "=".repeat(80));
    println!();

    for (i, func) in snapshot.functions.iter().take(display_count).enumerate() {
        let score = func.activity_risk.unwrap_or(func.lrs);
        let func_name = func
            .function_id
            .split("::")
            .last()
            .unwrap_or(&func.function_id);
        let (driver, action) = driving_dimension(func);
        println!(
            "#{} {} [{}] [{}]",
            i + 1,
            func_name,
            func.band.to_uppercase(),
            driver
        );
        println!("   File: {}:{}", func.file, func.line);
        println!(
            "   Risk Score: {:.2} (complexity base: {:.2})",
            score, func.lrs
        );
        let factor_lines = format_risk_factor_lines(func);
        if !factor_lines.is_empty() {
            println!("   Risk Breakdown:");
            for line in factor_lines {
                println!("{}", line);
            }
        }
        println!("   Action: {}", action);
        if driver == "composite" {
            if let Some(ref detail) = func.driver_detail {
                println!("   Near-threshold: {}", detail);
            }
        }
        if !func.patterns.is_empty() {
            println!("   Patterns: {}", func.patterns.join(", "));
            if let Some(ref details) = func.pattern_details {
                for pd in details {
                    let triggered = pd
                        .triggered_by
                        .iter()
                        .map(|t| format!("{}={} ({}{})", t.metric, t.value, t.op, t.threshold))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("     • {}: {}", pd.id, triggered);
                }
            }
        }
        println!();
    }

    println!("{}", "-".repeat(80));
    let critical_count = snapshot
        .functions
        .iter()
        .take(display_count)
        .filter(|f| f.band == "critical")
        .count();
    let high_count = snapshot
        .functions
        .iter()
        .take(display_count)
        .filter(|f| f.band == "high")
        .count();
    println!(
        "Showing {}/{} functions  |  Critical: {}  High: {}",
        display_count, total_count, critical_count, high_count
    );

    print_co_change_section(co_change);

    Ok(())
}
