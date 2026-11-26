//! Hotspots CLI - multi-language static analysis tool

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

mod cmd;
mod output;
mod util;

use clap::{Parser, Subcommand};
use cmd::{agent::AgentAction, analyze::AnalyzeArgs, diff::DiffArgs};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hotspots")]
#[command(
    about = "Multi-language static analysis tool (TypeScript, JavaScript, Go, Java, Python, Rust)"
)]
#[command(version = env!("HOTSPOTS_VERSION"))]
#[command(after_help = "Docs: https://docs.hotspots.dev/quickstart")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze source files (TypeScript, JavaScript, Go, Java, Python, Rust)
    Analyze {
        /// Path to source file or directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,

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

        /// Show human-readable risk explanations
        #[arg(long)]
        explain: bool,

        /// Overwrite existing snapshot if it already exists
        #[arg(short = 'f', long)]
        force: bool,

        /// Skip writing the snapshot to disk (persisted by default)
        #[arg(long)]
        no_persist: bool,

        /// Output level for text format: file shows a ranked file risk table
        #[arg(long, value_name = "LEVEL")]
        level: Option<OutputLevel>,

        /// Output all functions as a flat array (only valid with --format json).
        /// Note: `tier` is population-relative and can change run-to-run even when a
        /// function's own `lrs`/`activity_risk`/`band` don't; see docs/REFERENCE.md
        /// "Field stability" before keying CI gates on it.
        #[arg(long)]
        all_functions: bool,

        /// URL of the written analysis post to link from the HTML report (HTML format only)
        #[arg(long, value_name = "URL")]
        source_url: Option<String>,

        /// Also write the HTML report from this same analysis to PATH (only valid with
        /// a non-html --format; avoids a second full analysis)
        #[arg(long, value_name = "PATH")]
        html_output: Option<PathBuf>,

        /// Rank using a Gini-gated cold-start strategy (formula / IsolationForest
        /// anomaly / uniform prior) instead of a trained ranker. For repos with no
        /// fix-commit label history yet. Reads no label data. Explicit opt-in only —
        /// not an automatic fallback when `hotspots train` fails its label threshold.
        #[arg(long)]
        cold_start: bool,

        /// Print three independent ranked sections — Risk, Coupling, Ownership —
        /// instead of a single merged list. Files may appear in more than one
        /// section; overlap is not deduplicated. Use --top to control section size.
        #[arg(long)]
        axes: bool,

        /// Report ownership risk instead of analyzing: comma-separated list of files
        /// (was `hotspots coordinate --files`)
        #[arg(long, value_name = "FILES")]
        files: Option<String>,

        /// Report ownership risk instead of analyzing: derive the file set from a
        /// unified diff read on stdin (was `hotspots coordinate --diff`)
        #[arg(long)]
        diff: bool,

        /// Report ownership risk instead of analyzing: derive the file set from
        /// `git diff --cached --name-only` (was `hotspots coordinate --staged`)
        #[arg(long)]
        staged: bool,

        /// Report ownership risk for files before starting work; defaults to
        /// `--staged` when none of --files/--diff/--staged is also given
        /// (was `hotspots coordinate`)
        #[arg(long)]
        ownership: bool,

        /// Run prune instead of analyze: reduce snapshot storage (was `hotspots prune`)
        #[arg(long)]
        prune: bool,

        /// With --prune: prune unreachable snapshots (must be explicitly specified)
        #[arg(long)]
        unreachable: bool,

        /// With --prune --unreachable: only prune commits older than this many days
        #[arg(long)]
        older_than: Option<u64>,

        /// With --prune: compact history to the given level (0 = full snapshots,
        /// 1 = deltas only, 2 = band transitions only)
        #[arg(long, value_name = "LEVEL")]
        compact: Option<u32>,

        /// With --prune: dry-run mode (report what would change without modifying
        /// any files)
        #[arg(long)]
        dry_run: bool,

        /// Show the resolved configuration instead of analyzing (was `hotspots config show`)
        #[arg(long)]
        show_config: bool,

        /// Validate the config file instead of analyzing (was `hotspots config validate`)
        #[arg(long)]
        validate_config: bool,

        /// Print a dense, machine-readable contract for coding agents instead of
        /// analyzing (was `hotspots agent`)
        #[arg(long, value_name = "SUBCOMMAND", num_args = 0..=1, default_missing_value = "all")]
        help_agent: Option<AgentAction>,
    },
    /// Compare analysis snapshots between two git refs
    Diff {
        /// Base git ref (branch, tag, SHA, or HEAD~N). Omit to auto-detect the
        /// merge-base against the repo's default branch.
        base: Option<String>,

        /// Head git ref (branch, tag, SHA, or HEAD~N)
        #[arg(default_value = "HEAD")]
        head: String,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,

        /// Write output to file instead of stdout (HTML default: .hotspots/delta-report.html)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Evaluate policy rules; exit 1 on blocking failures (exit 4 on a
        /// tool error, so CI can tell a real policy violation apart from
        /// diff itself failing to run)
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

        /// With --auto-analyze, restrict analysis to files the diff actually
        /// touches instead of walking the whole repo (much faster/lighter on
        /// large repos with small PRs; see docs/REFERENCE.md for caveats)
        #[arg(long)]
        scope_to_diff: bool,

        /// Show a one-line factor breakdown (which metric drove the score
        /// change) under each changed function
        #[arg(long)]
        explain: bool,
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

        /// Upper bound for the label window (ISO date, e.g. "2025-01-01").
        /// When set, only fix commits before this date are used as training labels.
        /// Useful for matching a specific benchmark label window exactly.
        #[arg(long)]
        label_before: Option<String>,

        /// Use blame-based function-level labelling instead of file-level labelling.
        /// More precise labels (only the function that owned the changed lines is
        /// marked positive) but slower on repos with many fix commits.
        #[arg(long, default_value = "false")]
        blame: bool,

        /// Skip the confirmation prompt before training (non-interactive / CI use).
        /// Only has an effect when the repo has more than 1000 functions —
        /// that's the only case a confirmation prompt is shown at all.
        #[arg(long, short = 'y', default_value = "false")]
        yes: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    Text,
    Json,
    Html,
    Sarif,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
pub(crate) enum OutputLevel {
    File,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    cmd::telemetry::maybe_print_first_run_notice();

    let command_name = command_name(&cli.command);

    match cli.command {
        Commands::Analyze {
            path,
            format,
            top,
            min_lrs,
            config: config_path,
            output,
            explain,
            force,
            no_persist,
            level,
            all_functions,
            source_url,
            html_output,
            cold_start,
            axes,
            files,
            diff,
            staged,
            ownership,
            prune,
            unreachable,
            older_than,
            compact,
            dry_run,
            show_config,
            validate_config,
            help_agent,
        } => cmd::analyze::handle_analyze(AnalyzeArgs {
            path,
            format,
            top,
            min_lrs,
            config_path,
            output,
            explain,
            force,
            no_persist,
            level,
            all_functions,
            source_url,
            html_output,
            cold_start,
            axes,
            files,
            diff,
            staged,
            ownership,
            prune,
            unreachable,
            older_than,
            compact,
            dry_run,
            show_config,
            validate_config,
            help_agent,
        })?,
        Commands::Diff {
            base,
            head,
            format,
            output,
            policy,
            top,
            config,
            auto_analyze,
            scope_to_diff,
            explain,
        } => {
            if let Err(e) = cmd::diff::handle_diff(DiffArgs {
                base,
                head,
                format,
                output,
                policy,
                top,
                config_path: config,
                auto_analyze,
                scope_to_diff,
                explain,
            }) {
                // Exit 1 is reserved for a policy *violation* (see --policy's
                // --help text and handle_diff's own std::process::exit(1)).
                // Any other failure reaching here (config load, ref
                // resolution, delta computation, etc.) is a tool error, not
                // a policy finding, so CI must be able to tell them apart.
                eprintln!("Error: {e:#}");
                std::process::exit(4);
            }
        }
        Commands::Train {
            path,
            output,
            label_window,
            label_before,
            blame,
            yes,
        } => cmd::train::handle_train(cmd::train::TrainArgs {
            path,
            output,
            label_window_days: label_window,
            label_before,
            blame_labels: blame,
            yes,
        })?,
    }

    cmd::telemetry::after_command(command_name);

    Ok(())
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Analyze { .. } => "analyze",
        Commands::Diff { .. } => "diff",
        Commands::Train { .. } => "train",
    }
}

#[cfg(test)]
mod help_text_tests {
    use super::Cli;
    use clap::CommandFactory;

    /// Regression test for #229: `train -y/--yes` only has an effect above
    /// 1000 functions; that threshold must be disclosed in --help, not just
    /// discoverable by reading source.
    #[test]
    fn train_yes_help_discloses_threshold() {
        let cmd = Cli::command();
        let train = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "train")
            .expect("train subcommand must exist");
        let yes_help = train
            .get_arguments()
            .find(|a| a.get_id() == "yes")
            .and_then(|a| a.get_help())
            .map(|h| h.to_string())
            .unwrap_or_default();
        assert!(
            yes_help.contains("1000"),
            "train --yes help should disclose the 1000-function threshold, got: {yes_help:?}"
        );
    }

    /// Regression test for #226, moved to `analyze --older-than` now that
    /// `prune` is a flag on `analyze` rather than its own subcommand:
    /// `--older-than` requires `--unreachable` (and `--prune`) to also be
    /// set; that dependency must be disclosed in --help.
    #[test]
    fn analyze_older_than_help_discloses_unreachable_dependency() {
        let cmd = Cli::command();
        let analyze = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "analyze")
            .expect("analyze subcommand must exist");
        let older_than_help = analyze
            .get_arguments()
            .find(|a| a.get_id() == "older_than")
            .and_then(|a| a.get_help())
            .map(|h| h.to_string())
            .unwrap_or_default();
        assert!(
            older_than_help.contains("--unreachable"),
            "analyze --older-than help should disclose its dependency on --unreachable, got: {older_than_help:?}"
        );
    }
}
