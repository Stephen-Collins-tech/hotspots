//! Export visualization data from Hotspots snapshots
//!
//! Reads snapshots from a repository's .hotspots/ directory and generates
//! data.json for use with the Vega-Lite visualizations in visualizations/

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Data structure for visualizations/data.json
#[derive(Debug, Serialize, Deserialize)]
struct ReportData {
    meta: Meta,
    lrs_series: Vec<LrsSeriesEntry>,
    deltas_long: Vec<DeltaLongEntry>,
    repo_distribution: Vec<RepoDistributionEntry>,
    risk_concentration: Vec<RiskConcentrationEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    repo: String,
    target_function_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LrsSeriesEntry {
    version: String,
    sha: String,
    commit_index: usize,
    lrs: f64,
    cc: usize,
    nd: usize,
    fo: usize,
    ns: usize,
    band: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeltaLongEntry {
    version: String,
    sha: String,
    commit_index: usize,
    metric: String,
    delta: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RepoDistributionEntry {
    version: String,
    function_id: String,
    lrs: f64,
    band: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RiskConcentrationEntry {
    version: String,
    sha: String,
    commit_index: usize,
    function_id: String,
    lrs: f64,
}

/// Snapshot structure (from hotspots-core)
#[derive(Debug, Deserialize)]
struct Snapshot {
    #[serde(rename = "schema_version")]
    #[allow(dead_code)]
    schema_version: u32,
    commit: CommitInfo,
    #[allow(dead_code)]
    analysis: AnalysisInfo,
    functions: Vec<FunctionSnapshot>,
}

#[derive(Debug, Deserialize)]
struct CommitInfo {
    sha: String,
    #[allow(dead_code)]
    parents: Vec<String>,
    #[allow(dead_code)]
    timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalysisInfo {
    #[allow(dead_code)]
    scope: String,
    #[serde(rename = "tool_version")]
    #[allow(dead_code)]
    tool_version: String,
}

#[derive(Debug, Deserialize)]
struct FunctionSnapshot {
    function_id: String,
    file: String,
    line: u32,
    metrics: MetricsReport,
    lrs: f64,
    band: String,
}

#[derive(Debug, Deserialize)]
struct MetricsReport {
    cc: usize,
    nd: usize,
    fo: usize,
    ns: usize,
}

/// Index structure (from hotspots-core)
#[derive(Debug, Deserialize)]
struct Index {
    #[serde(rename = "schema_version")]
    schema_version: u32,
    commits: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    sha: String,
    parents: Vec<String>,
    timestamp: i64,
}

#[derive(Parser)]
#[command(name = "export-visualization")]
#[command(about = "Export visualization data from Hotspots snapshots")]
struct Args {
    /// Path to repository with .hotspots snapshots
    #[arg(long, default_value = ".")]
    repo: String,

    /// Target function ID (auto-detected if not specified)
    #[arg(long)]
    target_function: Option<String>,

    /// Output directory
    #[arg(long, default_value = "./visualizations")]
    output_dir: String,
    
    /// Top K functions for risk concentration chart (default: 5)
    #[arg(long, default_value = "5")]
    top_k: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo_path = Path::new(&args.repo);
    let analysis_dir = Path::new(&args.output_dir);
    let output_path = analysis_dir.join("data.json");

    println!("Updating data.json in {}", output_path.display());
    println!("Reading snapshots from {}", repo_path.display());

    // Try to load existing data.json to preserve meta, but don't require it
    let existing_meta: Option<Meta> = if output_path.exists() {
        let json = fs::read_to_string(&output_path)
            .context("failed to read existing data.json")?;
        let existing_data: ReportData = serde_json::from_str(&json)
            .context("failed to parse existing data.json")?;
        Some(existing_data.meta)
    } else {
        None
    };

    // Load index to get commit order
    let index_path = repo_path.join(".hotspots").join("index.json");
    let index: Index = if index_path.exists() {
        let json = fs::read_to_string(&index_path)
            .context("failed to read index.json")?;
        serde_json::from_str(&json)
            .context("failed to parse index.json")?
    } else {
        anyhow::bail!("index.json not found at {}", index_path.display());
    };

    // Load all snapshots
    let snapshots_dir = repo_path.join(".hotspots").join("snapshots");
    let mut snapshots: Vec<(usize, Snapshot)> = Vec::new();

    for (idx, entry) in index.commits.iter().enumerate() {
        let snapshot_path = snapshots_dir.join(format!("{}.json", entry.sha));
        if snapshot_path.exists() {
            let json = fs::read_to_string(&snapshot_path)
                .with_context(|| format!("failed to read snapshot: {}", snapshot_path.display()))?;
            let snapshot: Snapshot = serde_json::from_str(&json)
                .with_context(|| format!("failed to parse snapshot: {}", snapshot_path.display()))?;
            snapshots.push((idx, snapshot));
        } else {
            eprintln!("Warning: snapshot not found for commit {}", entry.sha);
        }
    }

    // Sort deterministically: by commit timestamp ascending, tie-break by SHA ASCII
    // This ensures consistent ordering for visualization
    snapshots.sort_by(|(idx_a, snap_a), (idx_b, snap_b)| {
        snap_a.commit.timestamp
            .cmp(&snap_b.commit.timestamp)
            .then_with(|| snap_a.commit.sha.cmp(&snap_b.commit.sha))
            .then_with(|| idx_a.cmp(idx_b))
    });

    if snapshots.is_empty() {
        anyhow::bail!("no snapshots found in {}", snapshots_dir.display());
    }

    // Determine target function ID
    let target_function_id = if let Some(ref target) = args.target_function {
        // Use provided target
        target.clone()
    } else if let Some(ref existing) = existing_meta {
        // Use existing meta if available
        existing.target_function_id.clone()
    } else {
        // Auto-detect: find function with highest LRS in latest snapshot
        let latest_snapshot = &snapshots.last().unwrap().1;
        let highest_lrs_func = latest_snapshot.functions.iter()
            .max_by(|a, b| a.lrs.partial_cmp(&b.lrs).unwrap_or(std::cmp::Ordering::Equal));
        
        if let Some(func) = highest_lrs_func {
            // Normalize the function ID to relative path
            let normalized = normalize_function_id(&func.function_id);
            println!("Auto-detected target function: {} (LRS: {:.2})", normalized, func.lrs);
            normalized
        } else {
            anyhow::bail!("no functions found in snapshots");
        }
    };

    println!("Target function: {}", target_function_id);

    // Determine repo name
    let repo_name = if let Some(ref existing) = existing_meta {
        existing.repo.clone()
    } else {
        // Extract repo name from path
        repo_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown-repo")
            .to_string()
    };

    // Helper to normalize function ID (extract relative path from absolute)
    fn normalize_function_id(func_id: &str) -> String {
        // If it's an absolute path, extract the relative part
        // Format: /path/to/repo/./src/file.ts::function
        // We want: src/file.ts::function
        if let Some(rel_start) = func_id.find("./src/") {
            func_id[rel_start + 2..].to_string() // Skip "./"
        } else if let Some(rel_start) = func_id.find("src/") {
            func_id[rel_start..].to_string()
        } else {
            func_id.to_string()
        }
    }

    // Extract LRS series for target function
    let mut lrs_series: Vec<LrsSeriesEntry> = Vec::new();

    for (commit_index, snapshot) in &snapshots {
        // Find target function (normalize both for comparison)
        let normalized_target = normalize_function_id(&target_function_id);
        let target_func = snapshot.functions.iter()
            .find(|f| normalize_function_id(&f.function_id) == normalized_target);

        if let Some(func) = target_func {
            let version = format!("v{}", commit_index);
            let short_sha = snapshot.commit.sha.chars().take(7).collect::<String>();

            lrs_series.push(LrsSeriesEntry {
                version,
                sha: short_sha,
                commit_index: *commit_index,
                lrs: func.lrs,
                cc: func.metrics.cc,
                nd: func.metrics.nd,
                fo: func.metrics.fo,
                ns: func.metrics.ns,
                band: func.band.clone(),
            });
        } else {
            eprintln!("Warning: target function {} not found in commit {}", target_function_id, snapshot.commit.sha);
        }
    }

    // Extract deltas (compare consecutive snapshots)
    // Format: one entry per metric per commit (long format)
    let mut deltas_long: Vec<DeltaLongEntry> = Vec::new();

    let normalized_target = normalize_function_id(&target_function_id);

    for i in 1..snapshots.len() {
        let (_prev_idx, prev_snapshot) = &snapshots[i - 1];
        let (curr_idx, curr_snapshot) = &snapshots[i];

        let prev_func = prev_snapshot.functions.iter()
            .find(|f| normalize_function_id(&f.function_id) == normalized_target);
        let curr_func = curr_snapshot.functions.iter()
            .find(|f| normalize_function_id(&f.function_id) == normalized_target);

        if let (Some(prev), Some(curr)) = (prev_func, curr_func) {
            let version = format!("v{}", curr_idx);
            let short_sha = curr_snapshot.commit.sha.chars().take(7).collect::<String>();

            // Create one entry per metric
            let metrics = vec![
                ("cc", (curr.metrics.cc as i64 - prev.metrics.cc as i64) as f64),
                ("nd", (curr.metrics.nd as i64 - prev.metrics.nd as i64) as f64),
                ("fo", (curr.metrics.fo as i64 - prev.metrics.fo as i64) as f64),
                ("ns", (curr.metrics.ns as i64 - prev.metrics.ns as i64) as f64),
                ("lrs", curr.lrs - prev.lrs),
            ];

            for (metric_name, delta_value) in metrics {
                deltas_long.push(DeltaLongEntry {
                    version: version.clone(),
                    sha: short_sha.clone(),
                    commit_index: *curr_idx,
                    metric: metric_name.to_string(),
                    delta: delta_value,
                });
            }
        }
    }

    // Extract repo distribution from latest snapshot
    let latest_snapshot = snapshots.last()
        .context("no snapshots found")?;
    let latest_version = format!("v{}", latest_snapshot.0);

    let repo_distribution: Vec<RepoDistributionEntry> = latest_snapshot.1.functions.iter()
        .map(|f| RepoDistributionEntry {
            version: latest_version.clone(),
            function_id: f.function_id.clone(),
            lrs: f.lrs,
            band: f.band.clone(),
        })
        .collect();

    // Compute risk concentration (Top K functions per snapshot + Other bucket)
    let mut risk_concentration: Vec<RiskConcentrationEntry> = Vec::new();
    
    // For each snapshot, compute Top K and Other
    for (commit_index, snapshot) in &snapshots {
        // Sort functions by LRS descending
        let mut sorted_functions: Vec<(&FunctionSnapshot, f64)> = snapshot.functions
            .iter()
            .map(|f| (f, f.lrs))
            .collect();
        sorted_functions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // Take Top K
        let top_k_functions: Vec<&FunctionSnapshot> = sorted_functions
            .iter()
            .take(args.top_k)
            .map(|(f, _)| *f)
            .collect();
        
        // Compute Other bucket (sum of remaining functions)
        let other_lrs: f64 = sorted_functions
            .iter()
            .skip(args.top_k)
            .map(|(_, lrs)| *lrs)
            .sum();
        
        let version = format!("v{}", commit_index);
        let short_sha = snapshot.commit.sha.chars().take(7).collect::<String>();
        
        // Add Top K functions
        for func in &top_k_functions {
            risk_concentration.push(RiskConcentrationEntry {
                version: version.clone(),
                sha: short_sha.clone(),
                commit_index: *commit_index,
                function_id: normalize_function_id(&func.function_id),
                lrs: func.lrs,
            });
        }
        
        // Add Other bucket if non-zero
        if other_lrs > 0.0 {
            risk_concentration.push(RiskConcentrationEntry {
                version: version.clone(),
                sha: short_sha.clone(),
                commit_index: *commit_index,
                function_id: "Other".to_string(),
                lrs: other_lrs,
            });
        }
    }
    
    // Build new report data
    let report_data = ReportData {
        meta: Meta {
            repo: repo_name,
            target_function_id: target_function_id.clone(),
            note: existing_meta.and_then(|m| m.note),
        },
        lrs_series,
        deltas_long,
        repo_distribution,
        risk_concentration,
    };

    // Write updated data.json
    let json = serde_json::to_string_pretty(&report_data)
        .context("failed to serialize report data")?;
    fs::write(&output_path, json)
        .context("failed to write data.json")?;

    println!("✓ Updated data.json");
    println!("  - LRS series: {} entries", report_data.lrs_series.len());
    println!("  - Deltas: {} entries", report_data.deltas_long.len());
    println!("  - Repo distribution: {} functions", report_data.repo_distribution.len());
    println!("  - Risk concentration: {} entries", report_data.risk_concentration.len());

    Ok(())
}
