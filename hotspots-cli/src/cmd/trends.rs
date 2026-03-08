use crate::util::truncate_string;
use crate::OutputFormat;
use anyhow::Context;
use hotspots_core::trends::TrendsAnalysis;
use std::path::PathBuf;

pub(crate) fn handle_trends(
    path: PathBuf,
    format: OutputFormat,
    window: usize,
    top: usize,
) -> anyhow::Result<()> {
    use crate::util::find_repo_root;

    let normalized_path = if path.is_relative() {
        std::env::current_dir()?.join(&path)
    } else {
        path
    };

    if !normalized_path.exists() {
        anyhow::bail!("Path does not exist: {}", normalized_path.display());
    }

    let repo_root = find_repo_root(&normalized_path)?;
    let trends = hotspots_core::trends::analyze_trends(&repo_root, window, top)
        .context("failed to analyze trends")?;

    match format {
        OutputFormat::Json => {
            let json = trends
                .to_json()
                .context("failed to serialize trends to JSON")?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            print_trends_text_output(&trends)?;
        }
        OutputFormat::Html | OutputFormat::Jsonl => {
            anyhow::bail!("HTML/JSONL format is not supported for trends analysis");
        }
    }

    Ok(())
}

fn print_trends_text_output(trends: &TrendsAnalysis) -> anyhow::Result<()> {
    println!("Trends Analysis");
    println!("{}", "=".repeat(80));

    if !trends.velocities.is_empty() {
        println!("\nRisk Velocities:");
        println!(
            "{:<40} {:<12} {:<12} {:<12} {:<12}",
            "Function", "Velocity", "Direction", "First LRS", "Last LRS"
        );
        println!("{}", "-".repeat(100));

        for velocity in &trends.velocities {
            let direction_str = match velocity.direction {
                hotspots_core::trends::VelocityDirection::Positive => "positive",
                hotspots_core::trends::VelocityDirection::Negative => "negative",
                hotspots_core::trends::VelocityDirection::Flat => "flat",
            };
            println!(
                "{:<40} {:<12.2} {:<12} {:<12.2} {:<12.2}",
                truncate_string(&velocity.function_id, 40),
                velocity.velocity,
                direction_str,
                velocity.first_lrs,
                velocity.last_lrs
            );
        }
    }

    if !trends.hotspots.is_empty() {
        println!("\nHotspot Stability:");
        println!(
            "{:<40} {:<12} {:<12} {:<12}",
            "Function", "Stability", "Overlap", "Appearances"
        );
        println!("{}", "-".repeat(88));

        for hotspot in &trends.hotspots {
            let stability_str = match hotspot.stability {
                hotspots_core::trends::HotspotStability::Stable => "stable",
                hotspots_core::trends::HotspotStability::Emerging => "emerging",
                hotspots_core::trends::HotspotStability::Volatile => "volatile",
            };
            println!(
                "{:<40} {:<12} {:<12.2} {:<12}/{}",
                truncate_string(&hotspot.function_id, 40),
                stability_str,
                hotspot.overlap_ratio,
                hotspot.appearances_in_top_k,
                hotspot.total_snapshots
            );
        }
    }

    if !trends.refactors.is_empty() {
        println!("\nRefactor Effectiveness:");
        println!(
            "{:<40} {:<12} {:<12} {:<12}",
            "Function", "Outcome", "Improvement", "Sustained"
        );
        println!("{}", "-".repeat(88));

        for refactor in &trends.refactors {
            let outcome_str = match refactor.outcome {
                hotspots_core::trends::RefactorOutcome::Successful => "successful",
                hotspots_core::trends::RefactorOutcome::Partial => "partial",
                hotspots_core::trends::RefactorOutcome::Cosmetic => "cosmetic",
            };
            println!(
                "{:<40} {:<12} {:<12.2} {:<12}",
                truncate_string(&refactor.function_id, 40),
                outcome_str,
                refactor.improvement_delta,
                refactor.sustained_commits
            );
        }
    }

    println!("\nSummary:");
    println!("  Risk velocities: {}", trends.velocities.len());
    println!("  Hotspots analyzed: {}", trends.hotspots.len());
    println!("  Refactors detected: {}", trends.refactors.len());

    Ok(())
}
