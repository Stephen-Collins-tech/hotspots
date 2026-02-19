//! Hotspots CLI - multi-language static analysis tool

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

use anyhow::Context;
use clap::{Parser, Subcommand};
use hotspots_core::config;
use hotspots_core::delta::Delta;
use hotspots_core::policy::{PolicyResult, PolicyResults};
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
#[command(version = env!("HOTSPOTS_VERSION"))]
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

        /// Overwrite existing snapshot if it already exists
        #[arg(short = 'f', long)]
        force: bool,

        /// Skip writing snapshot to disk (analyze without persisting; only valid with --mode snapshot or --mode delta)
        #[arg(long)]
        no_persist: bool,

        /// Output level for text format: file shows a ranked file risk table (only valid with --mode snapshot --format text)
        #[arg(long, value_name = "LEVEL")]
        level: Option<OutputLevel>,

        /// Use per-function git log -L for touch metrics (accurate but ~50× slower)
        #[arg(long)]
        per_function_touches: bool,
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

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum OutputLevel {
    File,
    Module,
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
            force,
            no_persist,
            level,
            per_function_touches,
        } => handle_analyze(AnalyzeArgs {
            path,
            format,
            mode,
            policy,
            top,
            min_lrs,
            config_path,
            output,
            explain,
            force,
            no_persist,
            level,
            per_function_touches,
        })?,
        Commands::Prune {
            unreachable,
            older_than,
            dry_run,
        } => handle_prune(unreachable, older_than, dry_run)?,
        Commands::Compact { level } => handle_compact(level)?,
        Commands::Config { action } => handle_config(action)?,
        Commands::Trends {
            path,
            format,
            window,
            top,
        } => handle_trends(path, format, window, top)?,
    }

    Ok(())
}

struct AnalyzeArgs {
    path: PathBuf,
    format: OutputFormat,
    mode: Option<OutputMode>,
    policy: bool,
    top: Option<usize>,
    min_lrs: Option<f64>,
    config_path: Option<PathBuf>,
    output: Option<PathBuf>,
    explain: bool,
    force: bool,
    no_persist: bool,
    level: Option<OutputLevel>,
    per_function_touches: bool,
}

/// Validate flag combinations that are mode/format-specific.
fn validate_analyze_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    let AnalyzeArgs {
        mode,
        format,
        policy,
        explain,
        per_function_touches,
        no_persist,
        force,
        level,
        ..
    } = args;
    if *policy && *mode != Some(OutputMode::Delta) {
        anyhow::bail!("--policy flag is only valid with --mode delta");
    }
    if *explain && *mode != Some(OutputMode::Snapshot) {
        anyhow::bail!("--explain flag is only valid with --mode snapshot");
    }
    if *per_function_touches && mode.is_none() {
        anyhow::bail!("--per-function-touches is only valid with --mode snapshot or --mode delta");
    }
    if *no_persist {
        if mode.is_none() {
            anyhow::bail!("--no-persist is only valid with --mode snapshot or --mode delta");
        }
        if *force {
            anyhow::bail!("--no-persist and --force are mutually exclusive");
        }
    }
    if level.is_some() {
        if *mode != Some(OutputMode::Snapshot) {
            anyhow::bail!("--level is only valid with --mode snapshot");
        }
        if !matches!(format, OutputFormat::Text) {
            anyhow::bail!("--level is only valid with --format text");
        }
        if *explain {
            anyhow::bail!("--level and --explain are mutually exclusive");
        }
    }
    Ok(())
}

fn handle_analyze(args: AnalyzeArgs) -> anyhow::Result<()> {
    validate_analyze_flags(&args)?;

    let AnalyzeArgs {
        path,
        format,
        mode,
        policy,
        top,
        min_lrs,
        config_path,
        output,
        explain,
        force,
        no_persist,
        level,
        per_function_touches,
    } = args;

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

    // Load configuration
    let project_root = find_repo_root(&normalized_path).unwrap_or_else(|_| normalized_path.clone());
    let resolved_config = config::load_and_resolve(&project_root, config_path.as_deref())
        .context("failed to load configuration")?;

    if let Some(ref p) = resolved_config.config_path {
        eprintln!("Using config: {}", p.display());
    }

    // CLI flags override config file values
    let effective_min_lrs = min_lrs.or(resolved_config.min_lrs);
    let effective_top = top.or(resolved_config.top_n);

    // If mode is specified, use snapshot/delta mode
    if let Some(output_mode) = mode {
        return handle_mode_output(
            &normalized_path,
            output_mode,
            &resolved_config,
            ModeOutputOptions {
                format,
                policy,
                top: effective_top,
                min_lrs: effective_min_lrs,
                output,
                explain,
                force,
                no_persist,
                level,
                per_function_touches,
            },
        );
    }

    // Default behavior: preserve existing text/JSON output
    let options = AnalysisOptions {
        min_lrs: effective_min_lrs,
        top_n: effective_top,
    };
    let reports = analyze_with_config(&normalized_path, options, Some(&resolved_config))?;

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

    Ok(())
}

fn handle_prune(unreachable: bool, older_than: Option<u64>, dry_run: bool) -> anyhow::Result<()> {
    if !unreachable {
        anyhow::bail!("--unreachable flag must be specified to prune snapshots");
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let options = prune::PruneOptions {
        ref_patterns: vec!["refs/heads/*".to_string()],
        older_than_days: older_than,
        dry_run,
    };
    let result = prune::prune_unreachable(&repo_root, options)?;

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

    Ok(())
}

fn handle_compact(level: u32) -> anyhow::Result<()> {
    if level > 2 {
        anyhow::bail!("compaction level must be 0, 1, or 2 (got {})", level);
    }
    if level > 0 {
        anyhow::bail!(
            "compaction to level {} is not yet implemented (only level 0 is supported)",
            level
        );
    }

    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let index_path = snapshot::index_path(&repo_root);
    let mut index = snapshot::Index::load_or_new(&index_path)?;
    let old_level = index.compaction_level();
    index.set_compaction_level(level);
    let index_json = index.to_json()?;
    snapshot::atomic_write(&index_path, &index_json)?;

    println!("Compaction level set to {} (was {})", level, old_level);
    Ok(())
}

fn handle_config(action: ConfigAction) -> anyhow::Result<()> {
    match action {
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
    }
    Ok(())
}

fn handle_trends(
    path: PathBuf,
    format: OutputFormat,
    window: usize,
    top: usize,
) -> anyhow::Result<()> {
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

/// Run the full enrichment pipeline: git context, churn, touch metrics, call graph, activity risk.
///
/// Both snapshot and delta modes call this, then diverge for their mode-specific output.
fn build_enriched_snapshot(
    repo_root: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    reports: Vec<hotspots_core::FunctionRiskReport>,
    per_function_touches: bool,
) -> anyhow::Result<Snapshot> {
    let git_context =
        git::extract_git_context_at(repo_root).context("failed to extract git context")?;

    // Build call graph before snapshot creation (snapshot consumes reports)
    let call_graph = hotspots_core::build_call_graph(&reports).ok();
    if let Some(ref cg) = call_graph {
        let total = cg.total_callee_names;
        let resolved = cg.resolved_callee_names;
        if total > 0 {
            let pct = (resolved as f64 / total as f64) * 100.0;
            eprintln!(
                "call graph: resolved {}/{} callee references ({:.0}% internal)",
                resolved, total, pct
            );
        }
    }

    let mut enricher = snapshot::SnapshotEnricher::new(Snapshot::new(git_context.clone(), reports));

    // Populate churn metrics if a parent commit exists
    if !git_context.parent_shas.is_empty() {
        match git::extract_commit_churn_at(repo_root, &git_context.head_sha) {
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
                enricher = enricher.with_churn(&churn_map);
            }
            Err(e) => {
                eprintln!("Warning: failed to extract churn: {}", e);
            }
        }
    }

    if per_function_touches {
        eprintln!("Warning: --per-function-touches enabled; analysis will be significantly slower");
    }
    enricher = enricher.with_touch_metrics(repo_root, per_function_touches);

    if let Some(ref graph) = call_graph {
        enricher = enricher.with_callgraph(graph);
    }

    Ok(enricher
        .enrich(Some(&resolved_config.scoring_weights))
        .build())
}

struct ModeOutputOptions {
    format: OutputFormat,
    policy: bool,
    top: Option<usize>,
    min_lrs: Option<f64>,
    output: Option<PathBuf>,
    explain: bool,
    force: bool,
    no_persist: bool,
    level: Option<OutputLevel>,
    per_function_touches: bool,
}

/// Handle snapshot or delta mode output
fn handle_mode_output(
    path: &Path,
    mode: OutputMode,
    resolved_config: &hotspots_core::ResolvedConfig,
    opts: ModeOutputOptions,
) -> anyhow::Result<()> {
    let ModeOutputOptions {
        format,
        policy,
        top,
        min_lrs,
        output,
        explain,
        force,
        no_persist,
        level,
        per_function_touches,
    } = opts;

    let repo_root = find_repo_root(path)?;
    // top_n is NOT applied here — applied post-scoring so functions are ranked by
    // activity_risk (not just LRS) before truncation
    let reports = analyze_with_config(
        path,
        AnalysisOptions {
            min_lrs,
            top_n: None,
        },
        Some(resolved_config),
    )?;
    let pr_context = git::detect_pr_context();

    match mode {
        OutputMode::Snapshot => {
            let mut snapshot =
                build_enriched_snapshot(&repo_root, resolved_config, reports, per_function_touches)
                    .context("failed to build enriched snapshot")?;

            // Persist only in mainline mode; skip when --no-persist is set
            if !pr_context.is_pr && !no_persist {
                snapshot::persist_snapshot(&repo_root, &snapshot, force)
                    .context("failed to persist snapshot")?;
                snapshot::append_to_index(&repo_root, &snapshot)
                    .context("failed to update index")?;
            }

            let total_function_count = snapshot.functions.len();
            // For file/module level views, keep all functions so aggregation is over the full set
            let is_aggregate_level =
                level == Some(OutputLevel::File) || level == Some(OutputLevel::Module);
            if (explain || top.is_some()) && !is_aggregate_level {
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

            emit_snapshot_output(
                &mut snapshot,
                SnapshotOutputOpts {
                    format,
                    explain,
                    level,
                    top,
                    total_function_count,
                    output,
                    co_change_window_days: resolved_config.co_change_window_days,
                    co_change_min_count: resolved_config.co_change_min_count,
                },
                &repo_root,
            )?;
        }
        OutputMode::Delta => {
            let snapshot =
                build_enriched_snapshot(&repo_root, resolved_config, reports, per_function_touches)
                    .context("failed to build enriched snapshot")?;

            let delta = if pr_context.is_pr {
                compute_pr_delta(&repo_root, &snapshot)?
            } else {
                delta::compute_delta(&repo_root, &snapshot)?
            };

            // Compute import edges and co-change for delta aggregates
            let mut unique_files: Vec<String> = snapshot
                .functions
                .iter()
                .map(|f| f.file.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            unique_files.sort();
            let files_as_str: Vec<&str> = unique_files.iter().map(|s| s.as_str()).collect();
            let import_edges = hotspots_core::imports::resolve_file_deps(&files_as_str, &repo_root);
            let mut current_co_change = hotspots_core::git::extract_co_change_pairs(
                &repo_root,
                resolved_config.co_change_window_days,
                resolved_config.co_change_min_count,
            )
            .unwrap_or_default();
            hotspots_core::aggregates::annotate_static_deps(
                &mut current_co_change,
                &import_edges,
                &repo_root,
            );

            // Try to get prev co-change from parent snapshot aggregates (empty if not stored)
            let parent_sha = snapshot.commit.parents.first().cloned();
            let prev_co_change: Vec<hotspots_core::git::CoChangePair> = parent_sha
                .as_deref()
                .and_then(|sha| {
                    hotspots_core::delta::load_parent_snapshot(&repo_root, sha)
                        .ok()
                        .flatten()
                })
                .and_then(|s| s.aggregates)
                .map(|a| a.co_change)
                .unwrap_or_default();

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
                    delta_with_extras.policy = Some(results);
                }
            }
            delta_with_extras.aggregates =
                Some(hotspots_core::aggregates::compute_delta_aggregates(
                    &delta,
                    &current_co_change,
                    &prev_co_change,
                ));

            if emit_delta_output(&delta_with_extras, format, policy, output)? {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

struct SnapshotOutputOpts {
    format: OutputFormat,
    explain: bool,
    level: Option<OutputLevel>,
    top: Option<usize>,
    total_function_count: usize,
    output: Option<PathBuf>,
    co_change_window_days: u64,
    co_change_min_count: usize,
}

fn emit_snapshot_output(
    snapshot: &mut Snapshot,
    opts: SnapshotOutputOpts,
    repo_root: &Path,
) -> anyhow::Result<()> {
    let SnapshotOutputOpts {
        format,
        explain,
        level,
        top,
        total_function_count,
        output,
        co_change_window_days,
        co_change_min_count,
    } = opts;
    match format {
        OutputFormat::Json => {
            let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                snapshot,
                repo_root,
                co_change_window_days,
                co_change_min_count,
            );
            snapshot.aggregates = Some(aggregates);
            println!("{}", snapshot.to_json()?);
        }
        OutputFormat::Jsonl => {
            println!("{}", snapshot.to_jsonl()?);
        }
        OutputFormat::Text => {
            if level == Some(OutputLevel::File) {
                let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                    snapshot,
                    repo_root,
                    co_change_window_days,
                    co_change_min_count,
                );
                print_file_risk_output(&aggregates.file_risk, top)?;
            } else if level == Some(OutputLevel::Module) {
                let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                    snapshot,
                    repo_root,
                    co_change_window_days,
                    co_change_min_count,
                );
                print_module_output(&aggregates.modules, top)?;
            } else if explain {
                let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                    snapshot,
                    repo_root,
                    co_change_window_days,
                    co_change_min_count,
                );
                print_explain_output(snapshot, total_function_count, &aggregates.co_change)?;
            } else {
                anyhow::bail!(
                    "text format without --explain is not supported for snapshot mode (use --format json or add --explain)"
                );
            }
        }
        OutputFormat::Html => {
            let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                snapshot,
                repo_root,
                co_change_window_days,
                co_change_min_count,
            );
            snapshot.aggregates = Some(aggregates);
            let html = hotspots_core::html::render_html_snapshot(snapshot);
            let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
            write_html_report(&output_path, &html)?;
            eprintln!("HTML report written to: {}", output_path.display());
        }
    }
    Ok(())
}

/// Returns true if there are blocking policy failures (caller should exit non-zero).
fn emit_delta_output(
    delta: &Delta,
    format: OutputFormat,
    policy: bool,
    output: Option<PathBuf>,
) -> anyhow::Result<bool> {
    let has_blocking_failures = delta
        .policy
        .as_ref()
        .map(|p| p.has_blocking_failures())
        .unwrap_or(false);

    match format {
        OutputFormat::Json => {
            println!("{}", delta.to_json()?);
        }
        OutputFormat::Jsonl => {
            anyhow::bail!("JSONL format is not supported for delta mode (use --mode snapshot)");
        }
        OutputFormat::Text => {
            if policy {
                if let Some(ref policy_results) = delta.policy {
                    print_policy_text_output(delta, policy_results)?;
                } else {
                    println!("Delta Analysis");
                    println!("{}", "=".repeat(80));
                    println!("Baseline delta (no parent snapshot) - policy evaluation skipped.");
                    println!("\nDelta contains {} function changes.", delta.deltas.len());
                }
            } else {
                anyhow::bail!(
                    "text format is not supported for delta mode without --policy (use --format json)"
                );
            }
        }
        OutputFormat::Html => {
            let html = hotspots_core::html::render_html_delta(delta);
            let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
            write_html_report(&output_path, &html)?;
            eprintln!("HTML report written to: {}", output_path.display());
        }
    }

    Ok(has_blocking_failures)
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

    // Build the set of files touched in this delta
    let touched: std::collections::HashSet<String> = delta
        .deltas
        .iter()
        .filter_map(|e| {
            e.function_id
                .rfind("::")
                .map(|pos| e.function_id[..pos].to_string())
        })
        .collect();

    // Show only pairs relevant to touched files (or all if none matched)
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

/// Print ranked file risk table
fn print_file_risk_output(
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

/// Print ranked module instability table
fn print_module_output(
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
        let module_display = truncate_string(&m.module, 40);
        println!(
            "{:<3} {:<40} {:>5} {:>5} {:>7.1} {:>9} {:>9} {:>11.3} {:>5}",
            i + 1,
            module_display,
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
fn format_risk_factor_lines(func: &hotspots_core::snapshot::FunctionSnapshot) -> Vec<String> {
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
fn print_co_change_section(co_change: &[hotspots_core::git::CoChangePair]) {
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

/// Print human-readable risk explanations for top functions
fn print_explain_output(
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
        println!("#{} {} [{}]", i + 1, func_name, func.band.to_uppercase());
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
        println!("   Action: {}", get_recommendation(func));
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
