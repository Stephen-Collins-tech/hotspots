//! Hotspots CLI - multi-language static analysis tool

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

mod cmd;
mod output;
mod util;

use clap::{Parser, Subcommand};
use cmd::{analyze::AnalyzeArgs, config::ConfigAction};
use std::path::PathBuf;

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

        /// Skip writing snapshot to disk (only valid with --mode snapshot or --mode delta)
        #[arg(long)]
        no_persist: bool,

        /// Output level for text format: file shows a ranked file risk table
        #[arg(long, value_name = "LEVEL")]
        level: Option<OutputLevel>,

        /// Use per-function git log -L for touch metrics
        #[arg(long)]
        per_function_touches: bool,

        /// Output all functions as a flat array (only valid with --mode snapshot --format json)
        #[arg(long)]
        all_functions: bool,

        /// Populate and emit pattern details for --explain-patterns
        #[arg(long)]
        explain_patterns: bool,

        /// URL of the written analysis post to link from the HTML report (HTML format only)
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,
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

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    Text,
    Json,
    Html,
    Jsonl,
    Sarif,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
pub(crate) enum OutputMode {
    Snapshot,
    Delta,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
pub(crate) enum OutputLevel {
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
            all_functions,
            explain_patterns,
            source_url,
        } => cmd::analyze::handle_analyze(AnalyzeArgs {
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
            all_functions,
            explain_patterns,
            source_url,
        })?,
        Commands::Prune {
            unreachable,
            older_than,
            dry_run,
        } => cmd::prune::handle_prune(unreachable, older_than, dry_run)?,
        Commands::Compact { level } => cmd::compact::handle_compact(level)?,
        Commands::Config { action } => cmd::config::handle_config(action)?,
        Commands::Trends {
            path,
            format,
            window,
            top,
        } => cmd::trends::handle_trends(path, format, window, top)?,
    }

    Ok(())
}
