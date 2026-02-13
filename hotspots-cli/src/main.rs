//! Hotspots CLI - multi-language static analysis tool

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

use anyhow::Context;
use clap::{Parser, Subcommand};
use hotspots_core::config;
use hotspots_core::delta::Delta;
use hotspots_core::policy::PolicyResults;
use hotspots_core::snapshot::{self, Snapshot};
use hotspots_core::trends::TrendsAnalysis;
use hotspots_core::{analyze_with_config, render_json, render_text, AnalysisOptions};
use hotspots_core::{delta, git, prune};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "hotspots")]
#[command(
    about = "Multi-language static analysis tool (TypeScript, JavaScript, Go, Java, Python, Rust)"
)]
#[command(version = env!("FAULTLINE_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze source files (TypeScript, JavaScript, Go, Java, Python, Rust)
    Analyze {
        /// Path to source file or directory
        path: PathBuf,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,

        /// Output mode (snapshot or delta)
        /// When not specified, preserves existing text/JSON output behavior
        #[arg(long)]
        mode: Option<OutputMode>,

        /// Evaluate policies (only valid with --mode delta)
        #[arg(long)]
        policy: bool,

        /// Show only top N results (overrides config file)
        #[arg(long)]
        top: Option<usize>,

        /// Minimum LRS threshold (overrides config file)
        #[arg(long)]
        min_lrs: Option<f64>,

        /// Path to config file (default: auto-discover)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output file path (for HTML format, default: .hotspots/report.html)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Show human-readable risk explanations (only valid with --mode snapshot)
        #[arg(long)]
        explain: bool,
    },
    /// Prune unreachable snapshots
    Prune {
        /// Prune unreachable snapshots (must be explicitly specified)
        #[arg(long)]
        unreachable: bool,

        /// Only prune commits older than this many days
        #[arg(long)]
        older_than: Option<u64>,

        /// Dry-run mode (report what would be pruned without actually deleting)
        #[arg(long)]
        dry_run: bool,
    },
    /// Compact history to reduce storage
    Compact {
        /// Compaction level (0 = full snapshots, 1 = deltas only, 2 = band transitions only)
        #[arg(long)]
        level: u32,
    },
    /// Analyze trends from snapshot history
    Trends {
        /// Path to repository root
        path: PathBuf,

        /// Output format
        #[arg(long, default_value = "json")]
        format: OutputFormat,

        /// Window size (number of snapshots to analyze)
        #[arg(long, default_value = "10")]
        window: usize,

        /// Top K functions for hotspot analysis
        #[arg(long, default_value = "5")]
        top: usize,
    },
    /// Validate a configuration file
    #[command(name = "config")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Validate a config file without running analysis
    Validate {
        /// Path to config file (default: auto-discover from current directory)
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Show the resolved configuration (merged defaults + config file)
    Show {
        /// Path to config file (default: auto-discover from current directory)
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Html,
    Jsonl,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum OutputMode {
    Snapshot,
    Delta,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze {
            path,
            format,
            mode,
            policy,
            top,
            min_lrs,
            config: config_path,
            output,
            explain,
        } => {
            // Normalize path to absolute
            let normalized_path = if path.is_relative() {
                std::env::current_dir()?.join(&path)
            } else {
                path
            };

            // Validate path exists
            if !normalized_path.exists() {
                anyhow::bail!("Path does not exist: {}", normalized_path.display());
            }

            // Validate --policy flag (only valid with --mode delta)
            if policy {
                if let Some(m) = mode {
                    if m != OutputMode::Delta {
                        anyhow::bail!("--policy flag is only valid with --mode delta");
                    }
                } else {
                    anyhow::bail!("--policy flag is only valid with --mode delta");
                }
            }

            // Load configuration
            let project_root =
                find_repo_root(&normalized_path).unwrap_or_else(|_| normalized_path.clone());
            let resolved_config = config::load_and_resolve(&project_root, config_path.as_deref())
                .context("failed to load configuration")?;

            if let Some(config_path) = &resolved_config.config_path {
                eprintln!("Using config: {}", config_path.display());
            }

            // CLI flags override config file values
            let effective_min_lrs = min_lrs.or(resolved_config.min_lrs);
            let effective_top = top.or(resolved_config.top_n);

            // Validate --explain flag (only valid with --mode snapshot)
            if explain {
                if let Some(m) = mode {
                    if m != OutputMode::Snapshot {
                        anyhow::bail!("--explain flag is only valid with --mode snapshot");
                    }
                } else {
                    anyhow::bail!("--explain flag is only valid with --mode snapshot");
                }
            }

            // If mode is specified, use snapshot/delta mode
            if let Some(output_mode) = mode {
                return handle_mode_output(
                    &normalized_path,
                    output_mode,
                    format,
                    policy,
                    effective_top,
                    effective_min_lrs,
                    &resolved_config,
                    output,
                    explain,
                );
            }

            // Default behavior: preserve existing text/JSON output
            let options = AnalysisOptions {
                min_lrs: effective_min_lrs,
                top_n: effective_top,
            };

            // Analyze with config
            let reports = analyze_with_config(&normalized_path, options, Some(&resolved_config))?;

            // Render output
            match format {
                OutputFormat::Text => {
                    print!("{}", render_text(&reports));
                }
                OutputFormat::Json => {
                    println!("{}", render_json(&reports));
                }
                OutputFormat::Html | OutputFormat::Jsonl => {
                    anyhow::bail!("HTML/JSONL format requires --mode snapshot or --mode delta");
                }
            }
        }
        Commands::Prune {
            unreachable,
            older_than,
            dry_run,
        } => {
            if !unreachable {
                anyhow::bail!("--unreachable flag must be specified to prune snapshots");
            }

            // Find repository root (search up from current directory)
            let repo_root = find_repo_root(&std::env::current_dir()?)?;

            // Build prune options
            let options = prune::PruneOptions {
                ref_patterns: vec!["refs/heads/*".to_string()], // Default: local branches only
                older_than_days: older_than,
                dry_run,
            };

            // Execute pruning
            let result = prune::prune_unreachable(&repo_root, options)?;

            // Print results
            if dry_run {
                println!("Dry-run: Would prune {} snapshots", result.pruned_count);
            } else {
                println!("Pruned {} snapshots", result.pruned_count);
            }

            if !result.pruned_shas.is_empty() {
                println!("\nPruned commit SHAs:");
                for sha in &result.pruned_shas {
                    println!("  {}", sha);
                }
            }

            println!("\nReachable snapshots: {}", result.reachable_count);
            if result.unreachable_kept_count > 0 {
                println!(
                    "Unreachable snapshots kept (due to age filter): {}",
                    result.unreachable_kept_count
                );
            }
        }
        Commands::Compact { level } => {
            // Validate compaction level
            if level > 2 {
                anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
            }

            // Find repository root (search up from current directory)
            let repo_root = find_repo_root(&std::env::current_dir()?)?;

            // Load index
            let index_path = snapshot::index_path(&repo_root);
            let mut index = snapshot::Index::load_or_new(&index_path)?;

            // Set compaction level
            let old_level = index.compaction_level();
            index.set_compaction_level(level);

            // Write updated index atomically
            let index_json = index.to_json()?;
            snapshot::atomic_write(&index_path, &index_json)?;

            println!("Compaction level set to {} (was {})", level, old_level);

            // Note: Actual compaction to levels 1 or 2 is not yet implemented
            // This only sets the metadata. Level 0 (full snapshots) is the current implementation.
            if level > 0 {
                println!("Note: Compaction to level {} is not yet implemented. Only metadata was updated.", level);
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Validate { path } => {
                let project_root = std::env::current_dir()?;
                let resolved = config::load_and_resolve(&project_root, path.as_deref());

                match resolved {
                    Ok(config) => {
                        if let Some(ref p) = config.config_path {
                            println!("Config valid: {}", p.display());
                        } else {
                            println!("No config file found. Using defaults.");
                        }
                    }
                    Err(e) => {
                        eprintln!("Config validation failed: {:#}", e);
                        std::process::exit(1);
                    }
                }
            }
            ConfigAction::Show { path } => {
                let project_root = std::env::current_dir()?;
                let resolved = config::load_and_resolve(&project_root, path.as_deref())
                    .context("failed to load configuration")?;

                println!("Configuration:");
                if let Some(ref p) = resolved.config_path {
                    println!("  Source: {}", p.display());
                } else {
                    println!("  Source: defaults (no config file found)");
                }
                println!();
                println!("Weights:");
                println!("  cc: {}", resolved.weight_cc);
                println!("  nd: {}", resolved.weight_nd);
                println!("  fo: {}", resolved.weight_fo);
                println!("  ns: {}", resolved.weight_ns);
                println!();
                println!("Thresholds:");
                println!("  moderate: {}", resolved.moderate_threshold);
                println!("  high: {}", resolved.high_threshold);
                println!("  critical: {}", resolved.critical_threshold);
                println!();
                println!("Filters:");
                println!(
                    "  min_lrs: {}",
                    resolved
                        .min_lrs
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
                println!(
                    "  top: {}",
                    resolved
                        .top_n
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
                println!(
                    "  include: {}",
                    if resolved.include.is_some() {
                        "custom patterns"
                    } else {
                        "all files"
                    }
                );
                println!(
                    "  exclude: active ({} patterns)",
                    if resolved.config_path.is_some() {
                        "custom"
                    } else {
                        "default"
                    }
                );
            }
        },
        Commands::Trends {
            path,
            format,
            window,
            top,
        } => {
            // Normalize path to absolute
            let normalized_path = if path.is_relative() {
                std::env::current_dir()?.join(&path)
            } else {
                path.clone()
            };

            // Validate path exists
            if !normalized_path.exists() {
                anyhow::bail!("Path does not exist: {}", normalized_path.display());
            }

            // Find repository root
            let repo_root = find_repo_root(&normalized_path)?;

            // Analyze trends
            let trends = hotspots_core::trends::analyze_trends(&repo_root, window, top)
                .context("failed to analyze trends")?;

            // Output results
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
        }
    }

    Ok(())
}

/// Handle snapshot or delta mode output
#[allow(clippy::too_many_arguments)]
fn handle_mode_output(
    path: &Path,
    mode: OutputMode,
    format: OutputFormat,
    policy: bool,
    top: Option<usize>,
    min_lrs: Option<f64>,
    resolved_config: &hotspots_core::ResolvedConfig,
    output: Option<PathBuf>,
    explain: bool,
) -> anyhow::Result<()> {
    // Find repository root (search up from current path)
    let repo_root = find_repo_root(path)?;

    // Analyze codebase
    // Note: top_n is NOT applied here for snapshot/delta modes - it's applied post-scoring
    // so that functions are ranked by activity_risk (not just LRS) before truncation
    let options = AnalysisOptions {
        min_lrs,
        top_n: None,
    };
    let reports = analyze_with_config(path, options, Some(resolved_config))?;

    // Detect PR context (best-effort, CI env vars only)
    let pr_context = git::detect_pr_context();
    let is_mainline = !pr_context.is_pr;

    match mode {
        OutputMode::Snapshot => {
            // Extract git context
            let git_context = git::extract_git_context()
                .context("failed to extract git context (required for snapshot mode)")?;

            // Build call graph before snapshot creation (since snapshot consumes reports)
            let call_graph =
                hotspots_core::build_call_graph(path, &reports, Some(resolved_config)).ok();

            // Create snapshot
            let mut snapshot = Snapshot::new(git_context.clone(), reports);

            // Extract and populate churn metrics if parent exists
            if !git_context.parent_shas.is_empty() {
                match git::extract_commit_churn(&git_context.head_sha) {
                    Ok(churns) => {
                        // Build map from file path to churn
                        // Convert relative paths from git to absolute paths to match snapshot
                        let churn_map: std::collections::HashMap<String, _> = churns
                            .into_iter()
                            .map(|c| {
                                let absolute_path = repo_root.join(&c.file);
                                let normalized_path = absolute_path.to_string_lossy().to_string();
                                (normalized_path, c)
                            })
                            .collect();
                        snapshot.populate_churn(&churn_map);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to extract churn: {}", e);
                    }
                }
            }

            // Populate touch count and recency metrics
            if let Err(e) = snapshot.populate_touch_metrics(&repo_root) {
                eprintln!("Warning: failed to populate touch metrics: {}", e);
            }

            // Populate call graph if it was built successfully
            if let Some(ref graph) = call_graph {
                snapshot.populate_callgraph(graph);
            }

            // Compute activity risk scores (combines all metrics)
            snapshot.compute_activity_risk(None);

            // Compute percentile flags and summary (must be after activity risk)
            snapshot.compute_percentiles();
            snapshot.compute_summary();

            // Persist snapshot only in mainline mode (not in PR mode)
            // Note: Aggregates are NOT persisted (they're derived, computed on output)
            if is_mainline {
                snapshot::persist_snapshot(&repo_root, &snapshot)
                    .context("failed to persist snapshot")?;
                snapshot::append_to_index(&repo_root, &snapshot)
                    .context("failed to update index")?;
            }

            // Sort by activity_risk descending when using --explain or --top N
            let total_function_count = snapshot.functions.len();
            if explain || top.is_some() {
                snapshot.functions.sort_by(|a, b| {
                    let a_score = a.activity_risk.unwrap_or(a.lrs);
                    let b_score = b.activity_risk.unwrap_or(b.lrs);
                    b_score
                        .partial_cmp(&a_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if let Some(n) = top {
                    snapshot.functions.truncate(n);
                }
            }

            // Emit snapshot output
            match format {
                OutputFormat::Json => {
                    // Compute aggregates for output (not persisted)
                    let mut snapshot_with_aggregates = snapshot.clone();
                    snapshot_with_aggregates.aggregates =
                        Some(hotspots_core::aggregates::compute_snapshot_aggregates(
                            &snapshot, &repo_root,
                        ));
                    let json = snapshot_with_aggregates.to_json()?;
                    println!("{}", json);
                }
                OutputFormat::Jsonl => {
                    let jsonl = snapshot.to_jsonl()?;
                    println!("{}", jsonl);
                }
                OutputFormat::Text => {
                    if explain {
                        print_explain_output(&snapshot, total_function_count)?;
                    } else {
                        anyhow::bail!(
                            "text format without --explain is not supported for snapshot mode (use --format json or add --explain)"
                        );
                    }
                }
                OutputFormat::Html => {
                    // Compute aggregates for output
                    let mut snapshot_with_aggregates = snapshot.clone();
                    snapshot_with_aggregates.aggregates =
                        Some(hotspots_core::aggregates::compute_snapshot_aggregates(
                            &snapshot, &repo_root,
                        ));

                    // Render HTML
                    let html = hotspots_core::html::render_html_snapshot(&snapshot_with_aggregates);

                    // Write to file
                    let output_path =
                        output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
                    write_html_report(&output_path, &html)?;
                    eprintln!("HTML report written to: {}", output_path.display());
                }
            }
        }
        OutputMode::Delta => {
            // Extract git context
            let git_context = git::extract_git_context()
                .context("failed to extract git context (required for delta mode)")?;

            // Build call graph before snapshot creation (since snapshot consumes reports)
            let call_graph =
                hotspots_core::build_call_graph(path, &reports, Some(resolved_config)).ok();

            // Create snapshot
            let mut snapshot = Snapshot::new(git_context.clone(), reports);

            // Extract and populate churn metrics if parent exists
            if !git_context.parent_shas.is_empty() {
                match git::extract_commit_churn(&git_context.head_sha) {
                    Ok(churns) => {
                        // Convert relative paths from git to absolute paths to match snapshot
                        let churn_map: std::collections::HashMap<String, _> = churns
                            .into_iter()
                            .map(|c| {
                                let absolute_path = repo_root.join(&c.file);
                                let normalized_path = absolute_path.to_string_lossy().to_string();
                                (normalized_path, c)
                            })
                            .collect();
                        snapshot.populate_churn(&churn_map);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to extract churn: {}", e);
                    }
                }
            }

            // Populate touch count and recency metrics
            if let Err(e) = snapshot.populate_touch_metrics(&repo_root) {
                eprintln!("Warning: failed to populate touch metrics: {}", e);
            }

            // Populate call graph if it was built successfully
            if let Some(ref graph) = call_graph {
                snapshot.populate_callgraph(graph);
            }

            // Compute activity risk scores (combines all metrics)
            snapshot.compute_activity_risk(None);

            // Compute delta
            let delta = if pr_context.is_pr {
                // PR mode: compare vs merge-base
                compute_pr_delta(&repo_root, &snapshot)?
            } else {
                // Mainline mode: compare vs direct parent (parents[0])
                delta::compute_delta(&repo_root, &snapshot)?
            };

            // Evaluate policies if requested
            let mut delta_with_extras = delta.clone();
            if policy {
                let policy_results = hotspots_core::policy::evaluate_policies(
                    &delta,
                    &snapshot,
                    &repo_root,
                    resolved_config,
                )
                .context("failed to evaluate policies")?;

                if let Some(results) = policy_results {
                    delta_with_extras.policy = Some(results.clone());
                }
            }

            // Compute aggregates for output (not stored in delta computation)
            delta_with_extras.aggregates =
                Some(hotspots_core::aggregates::compute_delta_aggregates(&delta));

            // Emit delta output
            let has_blocking_failures = delta_with_extras
                .policy
                .as_ref()
                .map(|p| p.has_blocking_failures())
                .unwrap_or(false);

            match format {
                OutputFormat::Json => {
                    let json = delta_with_extras.to_json()?;
                    println!("{}", json);
                }
                OutputFormat::Jsonl => {
                    anyhow::bail!(
                        "JSONL format is not supported for delta mode (use --mode snapshot)"
                    );
                }
                OutputFormat::Text => {
                    if policy {
                        // Text output for delta mode is only supported with --policy
                        if let Some(ref policy_results) = delta_with_extras.policy {
                            print_policy_text_output(&delta_with_extras, policy_results)?;
                        } else {
                            // Baseline delta - no policies evaluated, but still show delta info
                            println!("Delta Analysis");
                            println!("{}", "=".repeat(80));
                            println!(
                                "Baseline delta (no parent snapshot) - policy evaluation skipped."
                            );
                            println!(
                                "\nDelta contains {} function changes.",
                                delta_with_extras.deltas.len()
                            );
                        }
                    } else {
                        anyhow::bail!("text format is not supported for delta mode without --policy (use --format json)");
                    }
                }
                OutputFormat::Html => {
                    // Render HTML
                    let html = hotspots_core::html::render_html_delta(&delta_with_extras);

                    // Write to file
                    let output_path =
                        output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
                    write_html_report(&output_path, &html)?;
                    eprintln!("HTML report written to: {}", output_path.display());
                }
            }

            // Exit with error code if there are blocking failures
            if has_blocking_failures {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Compute delta for PR mode (compares vs merge-base)
///
/// Resolves merge-base and compares current snapshot against it.
/// Falls back to direct parent if merge-base cannot be resolved (with warning).
fn compute_pr_delta(
    repo_root: &std::path::Path,
    snapshot: &Snapshot,
) -> anyhow::Result<delta::Delta> {
    // Try to resolve merge-base
    let merge_base_sha = git::resolve_merge_base_auto();

    let parent = if let Some(sha) = &merge_base_sha {
        // Load merge-base snapshot
        match delta::load_parent_snapshot(repo_root, sha)? {
            Some(parent_snapshot) => Some(parent_snapshot),
            None => {
                // Merge-base snapshot not found - fall back to direct parent with warning
                eprintln!("Warning: merge-base snapshot not found, falling back to direct parent");
                let parent_sha = snapshot.commit.parents.first();
                if let Some(sha) = parent_sha {
                    delta::load_parent_snapshot(repo_root, sha)?
                } else {
                    None
                }
            }
        }
    } else {
        // Merge-base resolution failed - fall back to direct parent with warning
        eprintln!("Warning: failed to resolve merge-base, falling back to direct parent");
        let parent_sha = snapshot.commit.parents.first();
        if let Some(sha) = parent_sha {
            delta::load_parent_snapshot(repo_root, sha)?
        } else {
            None
        }
    };

    delta::Delta::new(snapshot, parent.as_ref())
}

/// Print policy results as text output
fn print_policy_text_output(delta: &Delta, policy_results: &PolicyResults) -> anyhow::Result<()> {
    // Print policy summary header
    println!("Policy Evaluation Results");
    println!("{}", "=".repeat(80));

    // Print blocking failures first
    if !policy_results.failed.is_empty() {
        println!("\nPolicy failures:");
        for result in &policy_results.failed {
            if let Some(ref function_id) = result.function_id {
                println!("- {}: {}", result.id.as_str(), function_id);
            } else {
                println!("- {}", result.id.as_str());
            }
        }

        // Print function table for blocking failures
        println!("\nViolating functions:");
        println!(
            "{:<40} {:<12} {:<12} {:<10} {:<20}",
            "Function", "Before", "After", "ΔLRS", "Policy"
        );
        println!("{}", "-".repeat(94));

        // Collect function IDs that triggered failures
        let violating_function_ids: std::collections::HashSet<&str> = policy_results
            .failed
            .iter()
            .filter_map(|r| r.function_id.as_deref())
            .collect();

        // Find corresponding delta entries
        for entry in &delta.deltas {
            if violating_function_ids.contains(entry.function_id.as_str()) {
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

                // Find which policies this function violated
                let policies: Vec<&str> = policy_results
                    .failed
                    .iter()
                    .filter(|r| r.function_id.as_deref() == Some(entry.function_id.as_str()))
                    .map(|r| r.id.as_str())
                    .collect();
                let policy_str = policies.join(", ");

                println!(
                    "{:<40} {:<12} {:<12} {:<10} {:<20}",
                    truncate_string(&entry.function_id, 40),
                    before_band,
                    after_band,
                    delta_lrs,
                    policy_str
                );
            }
        }
    }

    // Print warnings second, grouped by level
    if !policy_results.warnings.is_empty() {
        // Group warnings by policy ID
        let watch_warnings: Vec<_> = policy_results
            .warnings
            .iter()
            .filter(|r| r.id.as_str() == "watch-threshold")
            .collect();
        let attention_warnings: Vec<_> = policy_results
            .warnings
            .iter()
            .filter(|r| r.id.as_str() == "attention-threshold")
            .collect();
        let rapid_growth_warnings: Vec<_> = policy_results
            .warnings
            .iter()
            .filter(|r| r.id.as_str() == "rapid-growth")
            .collect();
        let repo_warnings: Vec<_> = policy_results
            .warnings
            .iter()
            .filter(|r| r.id.as_str() == "net-repo-regression")
            .collect();

        // Print Watch level warnings
        if !watch_warnings.is_empty() {
            println!("\nWatch Level (approaching moderate threshold):");
            println!("{:<40} {:<12} {:<12}", "Function", "Current LRS", "Band");
            println!("{}", "-".repeat(64));

            for warning in watch_warnings {
                if let Some(function_id) = &warning.function_id {
                    // Find the function in delta to get details
                    if let Some(entry) = delta.deltas.iter().find(|e| &e.function_id == function_id)
                    {
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

        // Print Attention level warnings
        if !attention_warnings.is_empty() {
            println!("\nAttention Level (approaching high threshold):");
            println!("{:<40} {:<12} {:<12}", "Function", "Current LRS", "Band");
            println!("{}", "-".repeat(64));

            for warning in attention_warnings {
                if let Some(function_id) = &warning.function_id {
                    if let Some(entry) = delta.deltas.iter().find(|e| &e.function_id == function_id)
                    {
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

        // Print Rapid Growth warnings
        if !rapid_growth_warnings.is_empty() {
            println!("\nRapid Growth (significant LRS increase):");
            println!(
                "{:<40} {:<12} {:<12} {:<12}",
                "Function", "Current LRS", "Delta", "Growth"
            );
            println!("{}", "-".repeat(76));

            for warning in rapid_growth_warnings {
                if let Some(function_id) = &warning.function_id {
                    if let Some(entry) = delta.deltas.iter().find(|e| &e.function_id == function_id)
                    {
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

        // Print repo-level warnings
        if !repo_warnings.is_empty() {
            println!("\nRepository-Level Warnings:");
            for warning in repo_warnings {
                println!("- {}", warning.message);
            }
        }
    }

    // Print summary
    if policy_results.failed.is_empty() && policy_results.warnings.is_empty() {
        println!("\nNo policy violations detected.");
    } else {
        println!("\nSummary:");
        println!("  Blocking failures: {}", policy_results.failed.len());

        // Break down warnings by type
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
        let other_warnings_count =
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
        if other_warnings_count > 0 {
            println!("  Other warnings: {}", other_warnings_count);
        }
    }

    Ok(())
}

/// Print human-readable risk explanations for top functions
fn print_explain_output(
    snapshot: &hotspots_core::snapshot::Snapshot,
    total_count: usize,
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

    // Functions are already sorted by activity_risk before this is called
    for (i, func) in snapshot.functions.iter().take(display_count).enumerate() {
        let score = func.activity_risk.unwrap_or(func.lrs);
        let func_name = func
            .function_id
            .split("::")
            .last()
            .unwrap_or(&func.function_id);
        let file_line = format!("{}:{}", func.file, func.line);

        println!("#{} {} [{}]", i + 1, func_name, func.band.to_uppercase());
        println!("   File: {}", file_line);
        println!(
            "   Risk Score: {:.2} (complexity base: {:.2})",
            score, func.lrs
        );

        // Print risk factor breakdown if available
        if let Some(ref factors) = func.risk_factors {
            println!("   Risk Breakdown:");

            // Collect non-zero factors with their explanations
            let mut factor_lines = Vec::new();

            if factors.complexity > 0.0 {
                factor_lines.push(format!(
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
                factor_lines.push(format!(
                    "     • Churn:           {:>6.2}  ({} lines changed recently)",
                    factors.churn, churn_lines
                ));
            }
            if factors.activity > 0.0 {
                let touches = func.touch_count_30d.unwrap_or(0);
                factor_lines.push(format!(
                    "     • Activity:        {:>6.2}  ({} commits in last 30 days)",
                    factors.activity, touches
                ));
            }
            if factors.recency > 0.0 {
                let days = func.days_since_last_change.unwrap_or(0);
                factor_lines.push(format!(
                    "     • Recency:         {:>6.2}  (last changed {} days ago)",
                    factors.recency, days
                ));
            }
            if factors.fan_in > 0.0 {
                let fi = func.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0);
                factor_lines.push(format!(
                    "     • Fan-in:          {:>6.2}  ({} functions depend on this)",
                    factors.fan_in, fi
                ));
            }
            if factors.cyclic_dependency > 0.0 {
                let scc = func.callgraph.as_ref().map(|cg| cg.scc_size).unwrap_or(1);
                factor_lines.push(format!(
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
                factor_lines.push(format!(
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
                factor_lines.push(format!(
                    "     • Neighbor churn:  {:>6.2}  ({} lines changed in dependencies)",
                    factors.neighbor_churn, nc
                ));
            }

            for line in factor_lines {
                println!("{}", line);
            }
        }

        // Print a recommendation
        println!("   Action: {}", get_recommendation(func));
        println!();
    }

    // Summary
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

    Ok(())
}

/// Generate a human-readable action recommendation based on risk factors
fn get_recommendation(func: &hotspots_core::snapshot::FunctionSnapshot) -> &'static str {
    let score = func.activity_risk.unwrap_or(func.lrs);
    let in_cycle = func
        .callgraph
        .as_ref()
        .map(|cg| cg.scc_size > 1)
        .unwrap_or(false);
    let high_fan_in = func
        .callgraph
        .as_ref()
        .map(|cg| cg.fan_in > 10)
        .unwrap_or(false);

    if func.band == "critical" || score > 20.0 {
        if in_cycle {
            "URGENT: Break cyclic dependency and refactor this function"
        } else if high_fan_in {
            "URGENT: Stabilize or split this high-dependency function"
        } else {
            "URGENT: Reduce complexity - extract sub-functions"
        }
    } else if func.band == "high" || score > 10.0 {
        if in_cycle {
            "Refactor: Break cyclic dependency in this function cluster"
        } else {
            "Refactor: Reduce complexity and improve test coverage"
        }
    } else if func.band == "moderate" {
        "Watch: Monitor for complexity growth on next change"
    } else {
        "OK: Low risk - consider refactoring only if modifying"
    }
}

/// Truncate string to max length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Print trends analysis as text output
fn print_trends_text_output(trends: &TrendsAnalysis) -> anyhow::Result<()> {
    println!("Trends Analysis");
    println!("{}", "=".repeat(80));

    // Risk Velocities
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

    // Hotspot Stability
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

    // Refactor Effectiveness
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

    // Summary
    println!("\nSummary:");
    println!("  Risk velocities: {}", trends.velocities.len());
    println!("  Hotspots analyzed: {}", trends.hotspots.len());
    println!("  Refactors detected: {}", trends.refactors.len());

    Ok(())
}

/// Write HTML report to file with atomic write pattern
fn write_html_report(path: &Path, html: &str) -> anyhow::Result<()> {
    use std::fs;

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        let parent_str = parent.display().to_string();
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent_str))?;
    }

    // Atomic write (temp + rename pattern)
    let temp_path = path.with_extension("html.tmp");
    std::fs::write(&temp_path, html)
        .with_context(|| format!("Failed to write temporary file: {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename temporary file to: {}", path.display()))?;

    Ok(())
}

/// Find git repository root by searching up the directory tree
fn find_repo_root(start_path: &Path) -> anyhow::Result<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid file path"))?
            .to_path_buf()
    } else {
        start_path.to_path_buf()
    };

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Ok(current);
        }

        // Move up one directory
        match current.parent() {
            Some(parent) => {
                current = parent.to_path_buf();
            }
            None => {
                anyhow::bail!("not in a git repository (no .git directory found)");
            }
        }
    }
}
