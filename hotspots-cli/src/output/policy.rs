use crate::util::truncate_string;
use hotspots_core::delta::Delta;
use hotspots_core::policy::{PolicyResult, PolicyResults};
use std::fmt::Write;

/// Render all policy sections to a String.
pub(crate) fn render_policy_text_output(
    delta: &Delta,
    policy_results: &PolicyResults,
) -> anyhow::Result<String> {
    let mut out = String::new();
    writeln!(out, "Policy Evaluation Results")?;
    writeln!(out, "{}", "=".repeat(80))?;
    write_failing_functions_section(&mut out, delta, policy_results)?;
    write_threshold_warning_section(
        &mut out,
        "Watch Level (approaching moderate threshold)",
        "watch-threshold",
        delta,
        &policy_results.warnings,
    )?;
    write_threshold_warning_section(
        &mut out,
        "Attention Level (approaching high threshold)",
        "attention-threshold",
        delta,
        &policy_results.warnings,
    )?;
    write_rapid_growth_section(&mut out, delta, &policy_results.warnings)?;
    write_repo_warnings_section(&mut out, &policy_results.warnings)?;
    write_co_change_delta_section(&mut out, delta)?;
    write_policy_summary(&mut out, policy_results)?;
    Ok(out)
}

/// Print all policy sections followed by a summary.
pub(crate) fn print_policy_text_output(
    delta: &Delta,
    policy_results: &PolicyResults,
) -> anyhow::Result<()> {
    print!("{}", render_policy_text_output(delta, policy_results)?);
    Ok(())
}

fn write_co_change_delta_section(out: &mut String, delta: &Delta) -> anyhow::Result<()> {
    let co_change_delta = match delta.aggregates.as_ref().map(|a| &a.co_change_delta) {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(()),
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
        return Ok(());
    }

    writeln!(out, "\nCo-Change Coupling (files touched in this delta):")?;
    writeln!(out, "{}", "-".repeat(80))?;
    for entry in &relevant {
        let risk = entry.curr_risk.as_deref().unwrap_or("unknown");
        let dep_tag = if entry.has_static_dep {
            " [expected]"
        } else {
            ""
        };
        writeln!(
            out,
            "  {} ↔ {}  [{}{}]  co-changed {:.0}% of the time",
            entry.file_a,
            entry.file_b,
            risk,
            dep_tag,
            entry.coupling_ratio * 100.0,
        )?;
    }
    Ok(())
}

fn write_failing_functions_section(
    out: &mut String,
    delta: &Delta,
    policy_results: &PolicyResults,
) -> anyhow::Result<()> {
    if policy_results.failed.is_empty() {
        return Ok(());
    }
    writeln!(out, "\nPolicy failures:")?;
    for result in &policy_results.failed {
        if let Some(ref function_id) = result.function_id {
            writeln!(out, "- {}: {}", result.id.as_str(), function_id)?;
        } else {
            writeln!(out, "- {}", result.id.as_str())?;
        }
    }
    writeln!(out, "\nViolating functions:")?;
    writeln!(
        out,
        "{:<40} {:<12} {:<12} {:<10} {:<20}",
        "Function", "Before", "After", "ΔLRS", "Policy"
    )?;
    writeln!(out, "{}", "-".repeat(94))?;
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
        writeln!(
            out,
            "{:<40} {:<12} {:<12} {:<10} {:<20}",
            truncate_string(&entry.function_id, 40),
            before_band,
            after_band,
            delta_lrs,
            policies.join(", ")
        )?;
    }
    Ok(())
}

fn write_threshold_warning_section(
    out: &mut String,
    header: &str,
    policy_id: &str,
    delta: &Delta,
    warnings: &[PolicyResult],
) -> anyhow::Result<()> {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == policy_id)
        .collect();
    if group.is_empty() {
        return Ok(());
    }
    writeln!(out, "\n{}:", header)?;
    writeln!(
        out,
        "{:<40} {:<12} {:<12}",
        "Function", "Current LRS", "Band"
    )?;
    writeln!(out, "{}", "-".repeat(64))?;
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
                writeln!(
                    out,
                    "{:<40} {:<12} {:<12}",
                    truncate_string(function_id, 40),
                    after_lrs,
                    after_band
                )?;
            }
        }
    }
    Ok(())
}

fn write_rapid_growth_section(
    out: &mut String,
    delta: &Delta,
    warnings: &[PolicyResult],
) -> anyhow::Result<()> {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == "rapid-growth")
        .collect();
    if group.is_empty() {
        return Ok(());
    }
    writeln!(out, "\nRapid Growth (significant LRS increase):")?;
    writeln!(
        out,
        "{:<40} {:<12} {:<12} {:<12}",
        "Function", "Current LRS", "Delta", "Growth"
    )?;
    writeln!(out, "{}", "-".repeat(76))?;
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
                writeln!(
                    out,
                    "{:<40} {:<12} {:<12} {:<12}",
                    truncate_string(function_id, 40),
                    after_lrs,
                    delta_lrs,
                    growth_pct
                )?;
            }
        }
    }
    Ok(())
}

fn write_repo_warnings_section(out: &mut String, warnings: &[PolicyResult]) -> anyhow::Result<()> {
    let group: Vec<_> = warnings
        .iter()
        .filter(|r| r.id.as_str() == "net-repo-regression")
        .collect();
    if group.is_empty() {
        return Ok(());
    }
    writeln!(out, "\nRepository-Level Warnings:")?;
    for warning in group {
        writeln!(out, "- {}", warning.message)?;
    }
    Ok(())
}

fn write_policy_summary(out: &mut String, policy_results: &PolicyResults) -> anyhow::Result<()> {
    if policy_results.failed.is_empty() && policy_results.warnings.is_empty() {
        writeln!(out, "\nNo policy violations detected.")?;
        return Ok(());
    }
    writeln!(out, "\nSummary:")?;
    writeln!(out, "  Blocking failures: {}", policy_results.failed.len())?;
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
        writeln!(out, "  Watch warnings: {watch_count}")?;
    }
    if attention_count > 0 {
        writeln!(out, "  Attention warnings: {attention_count}")?;
    }
    if rapid_growth_count > 0 {
        writeln!(out, "  Rapid growth warnings: {rapid_growth_count}")?;
    }
    if other_count > 0 {
        writeln!(out, "  Other warnings: {other_count}")?;
    }
    Ok(())
}
