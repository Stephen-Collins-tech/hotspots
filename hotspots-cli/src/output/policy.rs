use crate::util::truncate_string;
use hotspots_core::delta::Delta;
use hotspots_core::policy::{PolicyResult, PolicyResults};

/// Print all policy sections followed by a summary.
pub(crate) fn print_policy_text_output(
    delta: &Delta,
    policy_results: &PolicyResults,
) -> anyhow::Result<()> {
    println!("Policy Evaluation Results");
    println!("{}", "=".repeat(80));
    print_failing_functions_section(delta, policy_results);
    print_threshold_warning_section(
        "Watch Level (approaching moderate threshold)",
        "watch-threshold",
        delta,
        &policy_results.warnings,
    );
    print_threshold_warning_section(
        "Attention Level (approaching high threshold)",
        "attention-threshold",
        delta,
        &policy_results.warnings,
    );
    print_rapid_growth_section(delta, &policy_results.warnings);
    print_repo_warnings_section(&policy_results.warnings);
    print_co_change_delta_section(delta);
    print_policy_summary(policy_results);
    Ok(())
}

fn print_co_change_delta_section(delta: &Delta) {
    let co_change_delta = match delta.aggregates.as_ref().map(|a| &a.co_change_delta) {
        Some(d) if !d.is_empty() => d,
        _ => return,
    };

    let touched: std::collections::HashSet<String> = delta
        .deltas
        .iter()
        .filter_map(|e| {
            e.function_id
                .rfind("::")
                .map(|pos| e.function_id[..pos].to_string())
        })
        .collect();

    let relevant: Vec<&hotspots_core::aggregates::CoChangeDeltaEntry> = co_change_delta
        .iter()
        .filter(|e| {
            e.status != "dropped" && (touched.contains(&e.file_a) || touched.contains(&e.file_b))
        })
        .collect();

    if relevant.is_empty() {
        return;
    }

    println!("\nCo-Change Coupling (files touched in this delta):");
    println!("{}", "-".repeat(80));
    for entry in &relevant {
        let risk = entry.curr_risk.as_deref().unwrap_or("unknown");
        let dep_tag = if entry.has_static_dep {
            " [expected]"
        } else {
            ""
        };
        println!(
            "  {} ↔ {}  [{}{}]  co-changed {:.0}% of the time",
            entry.file_a,
            entry.file_b,
            risk,
            dep_tag,
            entry.coupling_ratio * 100.0,
        );
    }
}

fn print_failing_functions_section(delta: &Delta, policy_results: &PolicyResults) {
    if policy_results.failed.is_empty() {
        return;
    }
    println!("\nPolicy failures:");
    for result in &policy_results.failed {
        if let Some(ref function_id) = result.function_id {
            println!("- {}: {}", result.id.as_str(), function_id);
        } else {
            println!("- {}", result.id.as_str());
        }
    }
    println!("\nViolating functions:");
    println!(
        "{:<40} {:<12} {:<12} {:<10} {:<20}",
        "Function", "Before", "After", "ΔLRS", "Policy"
    );
    println!("{}", "-".repeat(94));
    let violating_ids: std::collections::HashSet<&str> = policy_results
        .failed
        .iter()
        .filter_map(|r| r.function_id.as_deref())
        .collect();
    for entry in &delta.deltas {
        if !violating_ids.contains(entry.function_id.as_str()) {
            continue;
        }
        let before_band = entry
            .before
            .as_ref()
            .map(|b| b.band.as_str())
            .unwrap_or("N/A");
        let after_band = entry
            .after
            .as_ref()
            .map(|a| a.band.as_str())
            .unwrap_or("N/A");
        let delta_lrs = entry
            .delta
            .as_ref()
            .map(|d| format!("{:.2}", d.lrs))
            .unwrap_or_else(|| "N/A".to_string());
        let policies: Vec<&str> = policy_results
            .failed
            .iter()
            .filter(|r| r.function_id.as_deref() == Some(entry.function_id.as_str()))
            .map(|r| r.id.as_str())
            .collect();
        println!(
            "{:<40} {:<12} {:<12} {:<10} {:<20}",
            truncate_string(&entry.function_id, 40),
            before_band,
            after_band,
            delta_lrs,
            policies.join(", ")
        );
    }
}

fn print_threshold_warning_section(
    header: &str,
    policy_id: &str,
    delta: &Delta,
    warnings: &[PolicyResult],
) {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == policy_id)
        .collect();
    if group.is_empty() {
        return;
    }
    println!("\n{}:", header);
    println!("{:<40} {:<12} {:<12}", "Function", "Current LRS", "Band");
    println!("{}", "-".repeat(64));
    for warning in group {
        if let Some(function_id) = &warning.function_id {
            if let Some(entry) = delta.deltas.iter().find(|e| &e.function_id == function_id) {
                let after_lrs = entry
                    .after
                    .as_ref()
                    .map(|a| format!("{:.2}", a.lrs))
                    .unwrap_or_else(|| "N/A".to_string());
                let after_band = entry
                    .after
                    .as_ref()
                    .map(|a| a.band.as_str())
                    .unwrap_or("N/A");
                println!(
                    "{:<40} {:<12} {:<12}",
                    truncate_string(function_id, 40),
                    after_lrs,
                    after_band
                );
            }
        }
    }
}

fn print_rapid_growth_section(delta: &Delta, warnings: &[PolicyResult]) {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == "rapid-growth")
        .collect();
    if group.is_empty() {
        return;
    }
    println!("\nRapid Growth (significant LRS increase):");
    println!(
        "{:<40} {:<12} {:<12} {:<12}",
        "Function", "Current LRS", "Delta", "Growth"
    );
    println!("{}", "-".repeat(76));
    for warning in group {
        if let Some(function_id) = &warning.function_id {
            if let Some(entry) = delta.deltas.iter().find(|e| &e.function_id == function_id) {
                let after_lrs = entry
                    .after
                    .as_ref()
                    .map(|a| format!("{:.2}", a.lrs))
                    .unwrap_or_else(|| "N/A".to_string());
                let delta_lrs = entry
                    .delta
                    .as_ref()
                    .map(|d| format!("{:+.2}", d.lrs))
                    .unwrap_or_else(|| "N/A".to_string());
                let growth_pct = warning
                    .metadata
                    .as_ref()
                    .and_then(|m| m.growth_percent)
                    .map(|g| format!("{:+.0}%", g))
                    .unwrap_or_else(|| "N/A".to_string());
                println!(
                    "{:<40} {:<12} {:<12} {:<12}",
                    truncate_string(function_id, 40),
                    after_lrs,
                    delta_lrs,
                    growth_pct
                );
            }
        }
    }
}

fn print_repo_warnings_section(warnings: &[PolicyResult]) {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == "net-repo-regression")
        .collect();
    if group.is_empty() {
        return;
    }
    println!("\nRepository-Level Warnings:");
    for warning in group {
        println!("- {}", warning.message);
    }
}

fn print_policy_summary(policy_results: &PolicyResults) {
    if policy_results.failed.is_empty() && policy_results.warnings.is_empty() {
        println!("\nNo policy violations detected.");
        return;
    }
    println!("\nSummary:");
    println!("  Blocking failures: {}", policy_results.failed.len());
    let watch_count = policy_results
        .warnings
        .iter()
        .filter(|r| r.id.as_str() == "watch-threshold")
        .count();
    let attention_count = policy_results
        .warnings
        .iter()
        .filter(|r| r.id.as_str() == "attention-threshold")
        .count();
    let rapid_growth_count = policy_results
        .warnings
        .iter()
        .filter(|r| r.id.as_str() == "rapid-growth")
        .count();
    let other_count =
        policy_results.warnings.len() - watch_count - attention_count - rapid_growth_count;
    if watch_count > 0 {
        println!("  Watch warnings: {}", watch_count);
    }
    if attention_count > 0 {
        println!("  Attention warnings: {}", attention_count);
    }
    if rapid_growth_count > 0 {
        println!("  Rapid growth warnings: {}", rapid_growth_count);
    }
    if other_count > 0 {
        println!("  Other warnings: {}", other_count);
    }
}
