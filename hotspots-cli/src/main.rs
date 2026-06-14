//! Hotspots CLI - multi-language static analysis tool

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

mod cmd;
mod output;
mod util;

use clap::{Parser, Subcommand};
use cmd::{analyze::AnalyzeArgs, config::ConfigAction, diff::DiffArgs};
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

        /// Output mode (snapshot, delta, or models)
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

        /// Use per-function git log -L for touch metrics (more accurate than file-level
        /// batching). Results are cached in .hotspots/touch-cache.json.zst — the first
        /// run on a new commit is slow (~9 ms per uncached function); subsequent runs
        /// are fast. A warning is printed when 50+ functions need to be fetched.
        #[arg(long)]
        per_function_touches: bool,

        /// Disable per-function touch metrics, use file-level batching instead.
        /// Overrides config and --per-function-touches. Useful for large repos
        /// where the cold-start per-function git log -L calls dominate CPU time.
        #[arg(long, conflicts_with = "per_function_touches")]
        no_per_function_touches: bool,

        /// Skip all touch metrics entirely (no git log calls for churn/recency).
        /// Overrides --per-function-touches and --no-per-function-touches.
        /// Use for benchmarking pure analysis + call graph performance.
        #[arg(long, conflicts_with = "per_function_touches")]
        skip_touch_metrics: bool,

        /// Output all functions as a flat array (only valid with --mode snapshot --format json)
        #[arg(long)]
        all_functions: bool,

        /// Include model risk map data in snapshot JSON/HTML reports.
        #[arg(long)]
        include_models: bool,

        /// Populate and emit pattern details for --explain-patterns
        #[arg(long)]
        explain_patterns: bool,

        /// URL of the written analysis post to link from the HTML report (HTML format only)
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,

        /// Number of parallel worker threads (default: number of logical CPUs)
        #[arg(long, short = 'j', value_name = "N")]
        jobs: Option<usize>,

        /// Skip all call graph algorithms when the repo exceeds N functions.
        /// Omits PageRank, betweenness, fan-in/fan-out, SCC, and dependency depth.
        /// Useful for very large repos where graph computation dominates CPU time.
        #[arg(long, value_name = "N")]
        callgraph_skip_above: Option<usize>,

        /// Skip the suppression gate check (the P@10 calibration that warns when
        /// the activity ranker may be producing misleading rankings).
        #[arg(long)]
        skip_gate: bool,

        /// Hybrid touch mode: run file-level touch first, then per-function only for
        /// files with touch_count_30d >= N. Balances accuracy and performance for
        /// large repos. Conflicts with --per-function-touches and --no-per-function-touches.
        #[arg(long, value_name = "N", conflicts_with_all = ["per_function_touches", "no_per_function_touches"])]
        hybrid_touches: Option<usize>,
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

        /// Report what would be done without modifying any files
        #[arg(long)]
        dry_run: bool,
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
    /// Print hook templates for CI/CD integration
    Init {
        /// Print pre-commit framework and raw shell hook templates to stdout
        #[arg(long)]
        hooks: bool,
    },
    /// Compare analysis snapshots between two git refs
    Diff {
        /// Base git ref (branch, tag, SHA, or HEAD~N)
        base: String,

        /// Head git ref (branch, tag, SHA, or HEAD~N)
        head: String,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,

        /// Write output to file instead of stdout (HTML default: .hotspots/delta-report.html)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Evaluate policy rules; exit 1 on blocking failures
        #[arg(long)]
        policy: bool,

        /// Limit output to top N changed functions (by |ΔLRS|)
        #[arg(long)]
        top: Option<usize>,

        /// Path to config file (default: auto-discover)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Analyze missing refs automatically using git worktrees
        #[arg(long)]
        auto_analyze: bool,
    },
    /// Assess coordination risk for a set of files before multi-agent or multi-developer work
    Coordinate {
        /// Comma-separated list of files to assess (relative to repo root)
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        files: Vec<String>,

        /// Path to repository root (default: current directory)
        #[arg(long, default_value = ".")]
        path: std::path::PathBuf,

        /// Force JSON output (default when stdout is not a TTY)
        #[arg(long, conflicts_with = "text")]
        json: bool,

        /// Force human-readable text output (default when stdout is a TTY)
        #[arg(long, conflicts_with = "json")]
        text: bool,
    },
    /// Train a local RandomForest ranker from fix-commit history
    Train {
        /// Path to repository root
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output path for the trained model (JSON)
        #[arg(long, default_value = ".hotspots/ranker.json")]
        output: PathBuf,

        /// Days of git history to scan for fix-commit labels
        #[arg(long, default_value = "365")]
        label_window: u32,

        /// Number of trees in the RandomForest
        #[arg(long, default_value = "200")]
        n_estimators: usize,

        /// Maximum tree depth
        #[arg(long, default_value = "6")]
        max_depth: usize,

        /// Use blame-based function-level labelling instead of file-level labelling.
        /// More precise labels (only the function that owned the changed lines is
        /// marked positive) but slower on repos with many fix commits.
        #[arg(long, default_value = "false")]
        blame: bool,

        /// After training, evaluate the model with P@K on the same label window.
        /// Prints a table of Precision@K for K in [10, 20, 50, 100, 200] alongside
        /// the fix-label base rate so you can judge whether the ranker beats random.
        #[arg(long, default_value = "false")]
        eval: bool,
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
    Models,
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
            no_per_function_touches,
            skip_touch_metrics,
            all_functions,
            include_models,
            explain_patterns,
            source_url,
            jobs,
            callgraph_skip_above,
            hybrid_touches,
            skip_gate,
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
            no_per_function_touches,
            skip_touch_metrics,
            all_functions,
            include_models,
            explain_patterns,
            source_url,
            jobs,
            callgraph_skip_above,
            hybrid_touches,
            skip_gate,
        })?,
        Commands::Prune {
            unreachable,
            older_than,
            dry_run,
        } => cmd::prune::handle_prune(unreachable, older_than, dry_run)?,
        Commands::Compact { level, dry_run } => cmd::compact::handle_compact(level, dry_run)?,
        Commands::Config { action } => cmd::config::handle_config(action)?,
        Commands::Trends {
            path,
            format,
            window,
            top,
        } => cmd::trends::handle_trends(path, format, window, top)?,
        Commands::Init { hooks } => cmd::init::handle_init(hooks)?,
        Commands::Diff {
            base,
            head,
            format,
            output,
            policy,
            top,
            config,
            auto_analyze,
        } => cmd::diff::handle_diff(DiffArgs {
            base,
            head,
            format,
            output,
            policy,
            top,
            config_path: config,
            auto_analyze,
        })?,
        Commands::Coordinate {
            files,
            path,
            json,
            text,
        } => {
            let use_json = json || (!text && !std::io::IsTerminal::is_terminal(&std::io::stdout()));
            cmd::coordinate::handle_coordinate(cmd::coordinate::CoordinateArgs {
                path,
                files,
                json: use_json,
            })?
        }
        Commands::Train {
            path,
            output,
            label_window,
            n_estimators,
            max_depth,
            blame,
            eval,
        } => cmd::train::handle_train(cmd::train::TrainArgs {
            path,
            output,
            label_window_days: label_window,
            n_estimators,
            max_depth,
            blame_labels: blame,
            eval,
        })?,
    }

    Ok(())
}
