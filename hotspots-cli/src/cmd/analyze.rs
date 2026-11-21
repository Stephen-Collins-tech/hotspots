use crate::output::explain;
use crate::util::{find_repo_root, write_html_report};
use crate::{OutputFormat, OutputLevel};
use anyhow::Context;
use hotspots_core::gate::{check_gate_smoothed, GateConfig, GateVerdict};
use hotspots_core::git;
use hotspots_core::snapshot::{self, Snapshot};
use hotspots_core::{analyze_with_progress, AnalysisOptions};
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

pub(crate) struct AnalyzeArgs {
    pub path: PathBuf,
    pub format: OutputFormat,
    pub top: Option<usize>,
    pub min_lrs: Option<f64>,
    pub config_path: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub explain: bool,
    pub force: bool,
    pub no_persist: bool,
    pub level: Option<OutputLevel>,
    pub all_functions: bool,
    /// URL of the corresponding written analysis post, embedded as a banner in HTML output.
    pub source_url: Option<String>,
    /// Also write the HTML report from the same analysis (non-html --format).
    pub html_output: Option<PathBuf>,
    /// Rank via Gini-gated cold-start routing instead of a trained ranker.
    /// Explicit opt-in only; reads no fix-commit label data.
    pub cold_start: bool,
    /// Print three independent ranked sections (Risk, Coupling, Ownership)
    /// instead of a single merged list.
    pub axes: bool,
    /// Comma-separated file list for the absorbed `coordinate` ownership-risk report.
    pub files: Option<String>,
    /// Derive the coordinate file set from a unified diff read on stdin.
    pub diff: bool,
    /// Derive the coordinate file set from `git diff --cached --name-only`.
    pub staged: bool,
    /// Run the ownership-risk report (defaults file selection to `--staged`
    /// when none of `--files`/`--diff`/`--staged` is also given).
    pub ownership: bool,
    /// Run prune instead of analyze.
    pub prune: bool,
    pub unreachable: bool,
    pub older_than: Option<u64>,
    pub compact: Option<u32>,
    pub dry_run: bool,
    pub show_config: bool,
    pub validate_config: bool,
    pub help_agent: Option<crate::cmd::agent::AgentAction>,
}

/// Validate flag combinations that are format-specific, or that govern which
/// of the absorbed subcommands (coordinate/prune/config/agent) runs instead
/// of a plain analysis.
pub(crate) fn validate_analyze_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    validate_dispatch_flags(args)?;
    validate_no_persist_flag(args)?;
    validate_html_output_flag(args)?;
    validate_level_flag(args)?;
    validate_all_functions_flag(args)?;
    Ok(())
}

/// `--cold-start`/`--axes` bypass the trained-ranker/snapshot pipeline
/// entirely and are mutually exclusive with each other and with every other
/// absorbed-subcommand trigger (`--prune`, `--show-config`/`--validate-config`,
/// `--help-agent`, and the coordinate selectors `--files`/`--diff`/`--staged`/
/// `--ownership`). Each dispatch group runs instead of, never alongside,
/// another.
fn validate_dispatch_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.axes && args.cold_start {
        anyhow::bail!("--axes and --cold-start are mutually exclusive");
    }

    let prune_sub_flags_set =
        args.unreachable || args.older_than.is_some() || args.compact.is_some() || args.dry_run;
    if prune_sub_flags_set && !args.prune {
        anyhow::bail!("--unreachable, --older-than, --compact, and --dry-run require --prune");
    }

    let coordinate_requested = args.files.is_some() || args.diff || args.staged || args.ownership;
    let config_requested = args.show_config || args.validate_config;
    let agent_requested = args.help_agent.is_some();

    let active = [
        args.cold_start,
        args.axes,
        coordinate_requested,
        args.prune,
        config_requested,
        agent_requested,
    ]
    .iter()
    .filter(|&&v| v)
    .count();
    if active > 1 {
        anyhow::bail!(
            "--cold-start, --axes, --prune, --show-config/--validate-config, --help-agent, and \
             the coordinate flags (--files/--diff/--staged/--ownership) are mutually exclusive"
        );
    }
    Ok(())
}

fn validate_no_persist_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if !args.no_persist {
        return Ok(());
    }
    if args.force {
        anyhow::bail!("--no-persist and --force are mutually exclusive");
    }
    Ok(())
}

fn validate_html_output_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.html_output.is_none() {
        return Ok(());
    }
    if matches!(args.format, OutputFormat::Html) {
        anyhow::bail!("--html-output is redundant with --format html (use --output)");
    }
    Ok(())
}

fn validate_level_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.level.is_none() {
        return Ok(());
    }
    if !matches!(args.format, OutputFormat::Text) {
        anyhow::bail!("--level is only valid with --format text");
    }
    if args.explain {
        anyhow::bail!("--level and --explain are mutually exclusive");
    }
    Ok(())
}

fn validate_all_functions_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.all_functions && !matches!(args.format, OutputFormat::Json) {
        anyhow::bail!("--all-functions is only valid with --format json");
    }
    Ok(())
}

/// `true` when the env var is set at all (any value, including empty),
/// mirroring `NO_COLOR`'s presence-based truthiness convention elsewhere in
/// this file rather than parsing "true"/"1"/etc.
fn env_flag_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn env_jobs() -> Option<usize> {
    std::env::var("HOTSPOTS_JOBS").ok()?.parse().ok()
}

/// Checks whether `args` selects one of the subcommands absorbed into
/// `analyze` (`--help-agent`; `--show-config`/`--validate-config`; `--prune`
/// plus its subflags; or the coordinate selectors `--files`/`--diff`/
/// `--staged`/`--ownership`) and, if so, runs it and returns its result.
/// Returns `None` when none of those flags is set, so `handle_analyze` falls
/// through to the plain analysis path. `validate_dispatch_flags` has already
/// ensured at most one of these triggers is active, so trying each in turn
/// (rather than one combined condition) is just a dispatch order, not a
/// precedence rule. Split into one small function per absorbed command —
/// mirroring `validate_analyze_flags`'s own decomposition — so each stays
/// simple on its own instead of recreating a single large dispatch function.
fn dispatch_absorbed_command(args: &AnalyzeArgs) -> Option<anyhow::Result<()>> {
    dispatch_help_agent(args)
        .or_else(|| dispatch_config(args))
        .or_else(|| dispatch_prune(args))
        .or_else(|| dispatch_coordinate(args))
}

fn dispatch_help_agent(args: &AnalyzeArgs) -> Option<anyhow::Result<()>> {
    let action = args.help_agent?;
    crate::cmd::agent::handle_agent(action);
    Some(Ok(()))
}

fn dispatch_config(args: &AnalyzeArgs) -> Option<anyhow::Result<()>> {
    if !(args.show_config || args.validate_config) {
        return None;
    }
    let action = if args.validate_config {
        crate::cmd::config::ConfigAction::Validate {
            path: args.config_path.clone(),
        }
    } else {
        crate::cmd::config::ConfigAction::Show {
            path: args.config_path.clone(),
        }
    };
    Some(crate::cmd::config::handle_config(action))
}

fn dispatch_prune(args: &AnalyzeArgs) -> Option<anyhow::Result<()>> {
    if !args.prune {
        return None;
    }
    Some(crate::cmd::prune::handle_prune(
        args.unreachable,
        args.older_than,
        args.compact,
        args.dry_run,
    ))
}

fn dispatch_coordinate(args: &AnalyzeArgs) -> Option<anyhow::Result<()>> {
    if !(args.files.is_some() || args.diff || args.staged || args.ownership) {
        return None;
    }
    let diff_text = match read_coordinate_diff_stdin(args) {
        Ok(text) => text,
        Err(e) => return Some(Err(e)),
    };
    // `--ownership` on its own (no explicit file selector) defaults to the
    // most common pre-commit use case: ownership risk for staged changes.
    let staged = args.staged || (args.ownership && args.files.is_none() && diff_text.is_none());
    Some(crate::cmd::coordinate::handle_coordinate(
        crate::cmd::coordinate::CoordinateArgs {
            files: args.files.clone(),
            diff: diff_text,
            staged,
            path: args.path.clone(),
        },
    ))
}

/// Reads the unified diff from stdin for `dispatch_coordinate` when `--diff`
/// is set; `None` otherwise (no stdin read happens in that case).
fn read_coordinate_diff_stdin(args: &AnalyzeArgs) -> anyhow::Result<Option<String>> {
    if !args.diff {
        return Ok(None);
    }
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("read diff from stdin")?;
    Ok(Some(buf))
}

pub(crate) fn handle_analyze(args: AnalyzeArgs) -> anyhow::Result<()> {
    // Anchor progress timestamps to the start of the run.
    let _ = process_start();
    validate_analyze_flags(&args)?;

    if let Some(result) = dispatch_absorbed_command(&args) {
        return result;
    }

    let AnalyzeArgs {
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
        ..
    } = args;

    // Configure the global rayon thread pool before any parallel work begins.
    // Worker threads default to a 2MB stack (std::thread default), which is too
    // small for the recursive-descent CFG builders on deeply nested or generated
    // source files (e.g. rust-lang/rust's tests/ui/ fixtures) and can overflow
    // and abort the whole process. Give worker threads a larger stack unconditionally.
    // Errors are ignored: build_global() fails if rayon was already initialized
    // (e.g. in tests), which is harmless.
    {
        let mut builder = rayon::ThreadPoolBuilder::new().stack_size(32 * 1024 * 1024);
        if let Some(n) = env_jobs() {
            builder = builder.num_threads(n);
        }
        let _ = builder.build_global();
    }

    let normalized_path = if path.is_relative() {
        std::env::current_dir()?.join(&path)
    } else {
        path
    };

    if !normalized_path.exists() {
        anyhow::bail!("Path does not exist: {}", normalized_path.display());
    }

    let project_root = find_repo_root(&normalized_path).unwrap_or_else(|_| normalized_path.clone());
    let resolved_config =
        hotspots_core::config::load_and_resolve(&project_root, config_path.as_deref())
            .context("failed to load configuration")?;

    if let Some(ref p) = resolved_config.config_path {
        eprintln!("Using config: {}", p.display());
    }

    let effective_min_lrs = min_lrs.or(resolved_config.min_lrs);
    let effective_top = top.or(resolved_config.top_n);

    if cold_start {
        let result = handle_cold_start(&normalized_path, &resolved_config, effective_top);
        return result;
    }

    if axes {
        let result = handle_axes(&normalized_path, &resolved_config, effective_top, format);
        return result;
    }

    // Snapshot mode is the only analysis mode now (post-v4.0.0's `--mode
    // delta` removal and this change's removal of `--mode` itself), so every
    // plain analysis always takes this path. `--no-persist` (default false)
    // is the only way to skip writing the snapshot to disk now; there is no
    // more implicit "promote to snapshot but force no-persist" branch.
    handle_mode_output(
        &normalized_path,
        &resolved_config,
        ModeOutputOptions {
            format,
            top: effective_top,
            min_lrs: effective_min_lrs,
            output,
            explain,
            force,
            no_persist,
            level,
            all_functions,
            source_url,
            html_output,
            skip_touch_metrics: env_flag_set("HOTSPOTS_SKIP_TOUCH_METRICS"),
            skip_gate: env_flag_set("HOTSPOTS_SKIP_GATE"),
        },
    )
}

/// `hotspots analyze --cold-start`: Gini-gated cold-start routing.
///
/// Skips the trained-ranker lookup entirely. Builds a snapshot with the cold-start
/// history signals populated (but no touch/churn/call-graph work — not needed for the
/// 8-feature cold-start vector), runs `cold_start_rank()`, and prints the routing
/// decision (Formula / Anomaly / UniformPrior) before the ranked list. Reads no
/// fix-commit label data.
fn handle_cold_start(
    path: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    top: Option<usize>,
) -> anyhow::Result<()> {
    let repo_root = find_repo_root(path)?;
    let analysis_progress = make_analysis_progress();
    let reports = hotspots_core::analyze_with_progress(
        path,
        AnalysisOptions {
            min_lrs: None,
            top_n: None,
            file_allowlist: None,
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;

    let mut snapshot = build_enriched_snapshot(
        &repo_root, reports,
        true, // skip touch metrics — not part of the cold-start feature set
    )
    .context("failed to build snapshot for cold-start ranking")?;
    snapshot.populate_history_signals(&repo_root);
    snapshot.populate_authors_90d(&repo_root);

    let train_cfg = hotspots_core::trainer::TrainConfig {
        gini_dead_zone: resolved_config.cold_start_gini_dead_zone,
        ..Default::default()
    };
    let result = hotspots_core::trainer::cold_start_rank(&snapshot, &repo_root, &train_cfg);

    let route_label = match result.route {
        hotspots_core::trainer::ColdStartRoute::Formula => "formula",
        hotspots_core::trainer::ColdStartRoute::Anomaly => "anomaly",
        hotspots_core::trainer::ColdStartRoute::UniformPrior => "uniform-prior",
    };
    println!("cold-start route: {route_label}");

    if result.ranked.is_empty() {
        println!("(uniform prior — no ranking to show; all files equally likely)");
        return Ok(());
    }

    let limit = match top {
        Some(0) => result.ranked.len(),
        Some(n) => n,
        None => 20,
    };
    for scored in result.ranked.iter().take(limit) {
        println!("{:.4}  {}", scored.score, scored.function_id);
    }

    Ok(())
}

/// `--axes`: prints three independent ranked sections — Risk, Coupling,
/// Ownership — in that fixed order, instead of a single merged list. Files
/// may appear in more than one section; overlap is not deduplicated, and the
/// axes are never blended into a composite score.
fn handle_axes(
    path: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    top: Option<usize>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    use hotspots_core::ranking::{rank_by_axis, HotspotAxis};

    if !matches!(format, OutputFormat::Text | OutputFormat::Json) {
        anyhow::bail!("--axes only supports --format text or --format json");
    }

    let repo_root = find_repo_root(path)?;
    let analysis_progress = make_analysis_progress();
    let reports = hotspots_core::analyze_with_progress(
        path,
        AnalysisOptions {
            min_lrs: None,
            top_n: None,
            file_allowlist: None,
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;

    let mut snapshot = build_enriched_snapshot(
        &repo_root, reports, false, // needed for directed_coupling and newcomer_rate below
    )
    .context("failed to build snapshot for --axes ranking")?;
    snapshot.populate_history_signals(&repo_root);

    use hotspots_core::trainer::{make_rel, repo_prefixes};
    let (prefix_can, prefix_raw) = repo_prefixes(&repo_root);
    let partner_scores: std::collections::HashMap<String, f64> = snapshot
        .functions
        .iter()
        .map(|f| {
            let rel = make_rel(&f.file, &prefix_can, &prefix_raw);
            (rel, f.activity_risk.unwrap_or(f.lrs))
        })
        .collect();
    snapshot.populate_directed_coupling(&repo_root, &partner_scores);

    let top_n = match top {
        Some(0) => snapshot.functions.len(),
        Some(n) => n,
        None => 10,
    };

    let sections = [
        (HotspotAxis::Risk, "Risk Hotspots", "risk"),
        (HotspotAxis::Coupling, "Coupling Hotspots", "coupling"),
        (HotspotAxis::Ownership, "Ownership Hotspots", "ownership"),
    ];

    if matches!(format, OutputFormat::Json) {
        let axes: serde_json::Map<String, serde_json::Value> = sections
            .iter()
            .map(|(axis, _, key)| {
                let ranked = rank_by_axis(&snapshot.functions, *axis, top_n);
                (key.to_string(), serde_json::to_value(ranked).unwrap())
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": hotspots_core::snapshot::SNAPSHOT_SCHEMA_VERSION,
            "axes": axes,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    for (axis, header, _) in sections {
        println!("{header}");
        let ranked = rank_by_axis(&snapshot.functions, axis, top_n);
        if ranked.is_empty() {
            println!("  (no qualifying files)");
        }
        for r in &ranked {
            println!("  {:.4}  {}", r.score, r.file);
        }
        println!();
    }

    Ok(())
}

pub(crate) struct ModeOutputOptions {
    pub format: OutputFormat,
    pub top: Option<usize>,
    pub min_lrs: Option<f64>,
    pub output: Option<PathBuf>,
    pub explain: bool,
    pub force: bool,
    pub no_persist: bool,
    pub level: Option<OutputLevel>,
    pub all_functions: bool,
    pub source_url: Option<String>,
    pub html_output: Option<PathBuf>,
    pub skip_touch_metrics: bool,
    pub skip_gate: bool,
}

pub(crate) fn handle_mode_output(
    path: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    opts: ModeOutputOptions,
) -> anyhow::Result<()> {
    let repo_root = find_repo_root(path)?;
    let analysis_progress = make_analysis_progress();
    let reports = hotspots_core::analyze_with_progress(
        path,
        AnalysisOptions {
            min_lrs: opts.min_lrs,
            top_n: None,
            file_allowlist: None,
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;
    let pr_context = git::detect_pr_context();

    handle_snapshot_mode(&repo_root, reports, pr_context, opts)
}

fn handle_snapshot_mode(
    repo_root: &Path,
    reports: Vec<hotspots_core::FunctionRiskReport>,
    pr_context: hotspots_core::git::PrContext,
    opts: ModeOutputOptions,
) -> anyhow::Result<()> {
    let ModeOutputOptions {
        format,
        explain,
        force,
        no_persist,
        level,
        all_functions,
        source_url,
        html_output,
        skip_touch_metrics,
        skip_gate,
        top,
        output,
        ..
    } = opts;

    // Validate output-format compatibility before any persistence side effects
    // (snapshot::persist_snapshot / append_to_index below), so a command that's
    // going to bail out with an error leaves no durable state behind. See #222.
    if matches!(format, OutputFormat::Text) && !explain && !matches!(level, Some(OutputLevel::File))
    {
        anyhow::bail!(
            "text format without --explain is not supported (use --format json or add --explain)"
        );
    }

    let (mut snapshot, dc_fetch) = build_snapshot_via_db(repo_root, reports, skip_touch_metrics)
        .context("failed to build enriched snapshot")?;

    // Populate DC before persisting so the snapshot on disk includes the values,
    // and before apply_trained_ranker so the ranker reads real DC instead of 0.0.
    // Partner scores use make_rel so keys match the repo-relative paths that
    // compute_directed_coupling_for_repo produces from git log output.
    //
    // Gated behind skip_touch_metrics: like touch metrics, this runs a
    // potentially unbounded `git log` walk (see coupling.rs) — but the walk
    // itself already happened inside build_snapshot_via_db, concurrently with
    // burst score and touch metrics (`dc_fetch`); this only does the cheap,
    // in-memory scoring step now that partner_scores (from activity_risk) is
    // available. --skip-touch-metrics still must skip it, same as before.
    if let Some(dc_fetch) = dc_fetch {
        use hotspots_core::trainer::{make_rel, repo_prefixes};
        let (prefix_can, prefix_raw) = repo_prefixes(repo_root);
        let partner_scores: std::collections::HashMap<String, f64> = snapshot
            .functions
            .iter()
            .map(|f| {
                let rel = make_rel(&f.file, &prefix_can, &prefix_raw);
                (rel, f.activity_risk.unwrap_or(f.lrs))
            })
            .collect();
        let (dc_scores, _jaccard) =
            hotspots_core::coupling::finish_directed_coupling(&dc_fetch, &partner_scores);
        snapshot.apply_directed_coupling_data(repo_root, &dc_scores);
    }

    if !pr_context.is_pr && !no_persist {
        snapshot::persist_snapshot(repo_root, &snapshot, force)
            .context("failed to persist snapshot")?;
        snapshot::append_to_index(repo_root, &snapshot).context("failed to update index")?;
    }

    let applied_model_class = apply_trained_ranker(repo_root, &mut snapshot);
    let ranker_applied = applied_model_class.is_some();

    if let Some(model_class) = &applied_model_class {
        let label = match model_class {
            hotspots_core::trainer::ModelClass::Ridge => "Ridge",
            hotspots_core::trainer::ModelClass::RandomForest => "RandomForest",
        };
        eprintln!("hotspots: using trained ranker (model class: {label})");
    }

    // Populate explanation phrases for CRITICAL/HIGH functions when --explain is set
    // and a trained ranker was applied. Percentiles are computed over all functions
    // before top-N truncation so the reference distribution is repo-wide.
    if ranker_applied && explain {
        populate_explanations(&mut snapshot);
    }

    let total_function_count = snapshot.functions.len();

    // Suppression gate: check if the activity ranker is working on this repo.
    // Run on the full sorted snapshot (before top-N truncation) so calibration
    // sees a representative top-50. The verdict is smoothed across runs
    // (`.hotspots/gate_history.json`) so a single sparse/bursty 90-day window
    // can't flip the reported verdict — see `GateConfig::consecutive_suppressed`.
    // Inconclusive is silent — it fires on greenfield repos or non-conventional
    // commit histories and is not actionable.
    if !skip_gate {
        let gate_verdict =
            check_gate_smoothed(repo_root, &snapshot.functions, &GateConfig::default());
        if let GateVerdict::Suppressed {
            p_at_10, threshold, ..
        } = &gate_verdict
        {
            if ranker_applied {
                eprintln!(
                    "hotspots: activity ranker P@10={p_at_10:.2} (< {threshold:.2}); using trained ranker instead."
                );
            } else {
                eprintln!(
                    "hotspots: warning: activity ranker P@10={p_at_10:.2} < {threshold:.2} — rankings may be misleading."
                );
                eprintln!("          Run `hotspots train` to fit a local ranker from this repo's fix history.");
            }
        }
    }

    apply_top_n(&mut snapshot, format, explain, level, top);

    if let Some(html_path) = html_output {
        // Render from a copy: HTML attaches aggregates, which other formats must not carry.
        let mut html_snapshot = snapshot.clone();
        emit_html_output(
            &mut html_snapshot,
            repo_root,
            SnapshotOutputOpts {
                format: OutputFormat::Html,
                explain,
                level,
                top,
                total_function_count,
                output: Some(html_path),
                co_change_window_days: hotspots_core::git::CO_CHANGE_WINDOW_DAYS,
                co_change_min_count: hotspots_core::git::CO_CHANGE_MIN_COUNT,
                all_functions,
                source_url: source_url.clone(),
                risk_thresholds: hotspots_core::risk::RiskThresholds::default(),
            },
        )?;
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
            co_change_window_days: hotspots_core::git::CO_CHANGE_WINDOW_DAYS,
            co_change_min_count: hotspots_core::git::CO_CHANGE_MIN_COUNT,
            all_functions,
            source_url: source_url.clone(),
            risk_thresholds: hotspots_core::risk::RiskThresholds::default(),
        },
        repo_root,
    )
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
    all_functions: bool,
    source_url: Option<String>,
    risk_thresholds: hotspots_core::risk::RiskThresholds,
}

fn emit_snapshot_output(
    snapshot: &mut Snapshot,
    opts: SnapshotOutputOpts,
    repo_root: &Path,
) -> anyhow::Result<()> {
    match opts.format {
        OutputFormat::Json => emit_json_output(snapshot, repo_root, opts),
        OutputFormat::Text => emit_text_output(snapshot, repo_root, opts),
        OutputFormat::Html => emit_html_output(snapshot, repo_root, opts),
        OutputFormat::Sarif => emit_sarif_output(snapshot, repo_root, opts),
    }
}

fn emit_json_output(
    snapshot: &mut Snapshot,
    repo_root: &Path,
    opts: SnapshotOutputOpts,
) -> anyhow::Result<()> {
    let SnapshotOutputOpts {
        all_functions,
        co_change_window_days,
        co_change_min_count,
        output,
        ..
    } = opts;
    let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
        snapshot,
        repo_root,
        co_change_window_days,
        co_change_min_count,
    );
    if all_functions {
        snapshot.aggregates = Some(aggregates);
    } else {
        let (triage, architecture, co_change) =
            hotspots_core::aggregates::compute_agent_sections(snapshot, &aggregates, repo_root);
        // The slimmed default output never carried a full `functions[]` array
        // (the merged-away `AgentSnapshotOutput` had no such field) — clear it
        // here so the unified `Snapshot` continues to omit it in this mode,
        // matching behavior prior to the envelope merge.
        snapshot.functions.clear();
        snapshot.triage = Some(triage);
        snapshot.architecture = architecture;
        snapshot.co_change = Some(co_change);
    }
    write_json_snapshot(snapshot, output)
}

fn emit_text_output(
    snapshot: &mut Snapshot,
    repo_root: &Path,
    opts: SnapshotOutputOpts,
) -> anyhow::Result<()> {
    let SnapshotOutputOpts {
        level,
        explain,
        top,
        total_function_count,
        co_change_window_days,
        co_change_min_count,
        ..
    } = opts;
    let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
        snapshot,
        repo_root,
        co_change_window_days,
        co_change_min_count,
    );
    if level == Some(OutputLevel::File) {
        explain::print_file_risk_output(&aggregates.file_risk, top)?;
    } else if explain {
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        explain::print_explain_output(snapshot, total_function_count, color)?;
    } else {
        anyhow::bail!(
            "text format without --explain is not supported (use --format json or add --explain)"
        );
    }
    Ok(())
}

fn emit_html_output(
    snapshot: &mut Snapshot,
    repo_root: &Path,
    opts: SnapshotOutputOpts,
) -> anyhow::Result<()> {
    let SnapshotOutputOpts {
        co_change_window_days,
        co_change_min_count,
        source_url,
        risk_thresholds,
        output,
        ..
    } = opts;
    let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
        snapshot,
        repo_root,
        co_change_window_days,
        co_change_min_count,
    );
    snapshot.aggregates = Some(aggregates);
    let history: Vec<_> = hotspots_core::trends::load_snapshot_window(repo_root, 30)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.summary.map(|sum| (s.commit, sum)))
        .collect();
    log_phase("rendering HTML report");
    let html = hotspots_core::html::render_html_snapshot(
        snapshot,
        &history,
        source_url.as_deref(),
        &risk_thresholds,
    );
    let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
    write_html_report(&output_path, &html)?;
    eprintln!("HTML report written to: {}", output_path.display());
    Ok(())
}

fn emit_sarif_output(
    snapshot: &mut Snapshot,
    repo_root: &Path,
    opts: SnapshotOutputOpts,
) -> anyhow::Result<()> {
    let sarif = hotspots_core::sarif::render_sarif(snapshot, repo_root);
    if let Some(output_path) = opts.output {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        std::fs::write(&output_path, &sarif)
            .with_context(|| format!("failed to write SARIF to {}", output_path.display()))?;
        eprintln!("SARIF report written to: {}", output_path.display());
    } else {
        println!("{sarif}");
    }
    Ok(())
}

fn apply_top_n(
    snapshot: &mut Snapshot,
    format: OutputFormat,
    explain: bool,
    level: Option<OutputLevel>,
    top: Option<usize>,
) {
    let is_aggregate_level = level == Some(OutputLevel::File);
    let is_text = matches!(format, OutputFormat::Text);
    if !is_aggregate_level && (top.is_some() || (is_text && explain)) {
        snapshot.functions.sort_by(|a, b| {
            let a_score = a.activity_risk.unwrap_or(a.lrs);
            let b_score = b.activity_risk.unwrap_or(b.lrs);
            b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // 0 = show all; None in text+explain defaults to 20
        let limit = match top {
            Some(0) => usize::MAX,
            Some(n) => n,
            None if is_text => 20,
            None => usize::MAX,
        };
        if limit != usize::MAX {
            snapshot.functions.truncate(limit);
        }
    }
}

/// If `.hotspots/ranker.json` exists, overwrite each function's `activity_risk`
/// with the trained model's score. Returns the model class that was applied.
/// Silent no-op (returns `None`) if the model is absent or fails to load.
fn apply_trained_ranker(
    repo_root: &Path,
    snapshot: &mut Snapshot,
) -> Option<hotspots_core::trainer::ModelClass> {
    let model_path = snapshot::hotspots_dir(repo_root).join("ranker.json");
    if !model_path.exists() {
        return None;
    }
    let model = match hotspots_core::trainer::RankerModel::load(&model_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("hotspots: warning: failed to load ranker.json: {e}");
            return None;
        }
    };
    for func in &mut snapshot.functions {
        func.activity_risk = Some(hotspots_core::trainer::score(&model, func));
    }
    Some(model.model_class.clone())
}

/// Compute per-feature repo-wide percentile ranks and populate `explanation` on
/// every CRITICAL/HIGH function. Called only when `--explain` and a ranker are active.
fn populate_explanations(snapshot: &mut Snapshot) {
    use hotspots_core::phrases::{top_phrases, FeaturePercentiles};
    use hotspots_core::risk::RiskBand;
    use hotspots_core::trainer::extract_features;

    let n = snapshot.functions.len();
    if n == 0 {
        return;
    }

    // Extract feature vectors for all functions up front.
    let feature_vecs: Vec<[f64; 9]> = snapshot.functions.iter().map(extract_features).collect();

    // For each feature column, build a sorted copy for percentile lookups.
    // Feature indices: 0=lrs, 1=cc, 2=nd, 3=loc, 4=fo, 5=fan_in,
    //                  6=total_churn, 7=authors_90d, 8=directed_coupling
    let mut sorted: Vec<Vec<f64>> = (0..9usize).map(|_| Vec::with_capacity(n)).collect();
    for fv in &feature_vecs {
        for (j, &v) in fv.iter().enumerate() {
            sorted[j].push(v);
        }
    }
    for col in sorted.iter_mut() {
        col.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }

    let pct = |col: usize, val: f64| -> f32 {
        let pos = sorted[col].partition_point(|&x| x < val);
        pos as f32 / n as f32
    };

    for (i, func) in snapshot.functions.iter_mut().enumerate() {
        if !matches!(func.band, RiskBand::Critical | RiskBand::High) {
            continue;
        }
        let fv = &feature_vecs[i];
        let fp = FeaturePercentiles {
            lrs: pct(0, fv[0]),
            cc: pct(1, fv[1]),
            nd: pct(2, fv[2]),
            loc: pct(3, fv[3]),
            fo: pct(4, fv[4]),
            // Require fan_in >= 3 before "depended on by many callers" fires;
            // fan_in=1 can clear the 90th percentile on sparse repos, producing
            // a misleading phrase.
            fan_in: if fv[5] >= 3.0 { pct(5, fv[5]) } else { 0.0 },
            total_churn: pct(6, fv[6]),
            authors_90d: pct(7, fv[7]),
            directed_coupling: pct(8, fv[8]),
            // Direct, binary, per-function fact read straight off this
            // function's own metrics — not a repo-relative percentile and
            // not part of the trained ranker's feature set.
            missing_docstring: if func.metrics.missing_docstring {
                1.0
            } else {
                0.0
            },
        };
        let phrase = top_phrases(&fp, 3);
        if !phrase.is_empty() {
            func.explanation = Some(phrase);
        }
    }
}

fn write_snapshot_json_file<F>(output_path: &Path, write: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut std::io::BufWriter<std::fs::File>) -> anyhow::Result<()>,
{
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }
    let file = std::fs::File::create(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let mut out = std::io::BufWriter::new(file);
    write(&mut out)
}

fn write_json_snapshot(snapshot: &Snapshot, output: Option<PathBuf>) -> anyhow::Result<()> {
    if let Some(output_path) = output {
        write_snapshot_json_file(&output_path, |out| {
            snapshot
                .write_json_to(out)
                .context("failed to write snapshot JSON")
        })?;
        eprintln!("JSON report written to: {}", output_path.display());
    } else {
        let stdout = std::io::stdout();
        let mut out = std::io::BufWriter::new(stdout.lock());
        snapshot
            .write_json_to(&mut out)
            .context("failed to write snapshot JSON")?;
    }
    Ok(())
}

/// Fetches burst-score history and touch metrics concurrently on the rayon
/// pool (already built for CFG/parse work and otherwise idle during this
/// phase) instead of running two independent, blocking `git log` calls one
/// after another on the main thread, then applies both results.
fn with_burst_and_touch_metrics(
    enricher: snapshot::SnapshotEnricher,
    repo_root: &Path,
    window_days: u32,
) -> snapshot::SnapshotEnricher {
    let (burst_signals, touch_data) = rayon::join(
        || hotspots_core::history_signals::compute_history_signals_for_repo(repo_root),
        || enricher.fetch_touch_metrics_data(repo_root, window_days),
    );

    let enricher = if burst_signals.is_empty() {
        enricher
    } else {
        enricher.with_burst_score_data(repo_root, &burst_signals)
    };
    enricher.with_touch_metrics_data(repo_root, &touch_data.0, &touch_data.1)
}

/// Memory-efficient snapshot pipeline using SQLite as an in-process buffer.
///
/// Pipeline: insert reports → drop Vec → update churn → build call graph from DB →
/// update callgraph metrics → drop CallGraph → load FunctionSnapshot Vec → enrich → return.
///
/// Peak memory compared to `build_enriched_snapshot`:
///   Before: ~250 MB (reports + call graph + snapshot Vec all overlap)
///   After:  ~45 MB  (only call graph + SQLite overlap; reports freed before graph builds)
pub(crate) fn build_snapshot_via_db(
    repo_root: &Path,
    reports: Vec<hotspots_core::FunctionRiskReport>,
    skip_touch_metrics: bool,
) -> anyhow::Result<(
    Snapshot,
    Option<hotspots_core::coupling::DirectedCouplingFetch>,
)> {
    use hotspots_core::db::TempDb;
    use hotspots_core::snapshot::{
        AnalysisInfo, CommitInfo, FORMULA_VERSION, SNAPSHOT_SCHEMA_VERSION,
    };

    let git_context =
        git::extract_git_context_at(repo_root).context("failed to extract git context")?;
    let merge_base = hotspots_core::git::find_merge_base(repo_root);

    let commit_info = CommitInfo::from(git_context.clone());
    let sha = commit_info.sha.clone();

    // Phase 1: write reports to DB, then free the Vec (~23 MB).
    let db = TempDb::new().context("failed to create pipeline TempDb")?;
    db.insert_reports(&commit_info, &reports)
        .context("failed to insert reports into pipeline DB")?;
    drop(reports);

    // Phase 2: churn (needed before callgraph so neighbor_churn can read it).
    log_phase("reading git history (churn)");
    if !git_context.parent_shas.is_empty() {
        match git::windowed_churn_at(repo_root, &sha, git::CHURN_WINDOW_DAYS) {
            Ok(churns) => {
                let churn_map: std::collections::HashMap<String, _> = churns
                    .into_iter()
                    .map(|c| {
                        let absolute_path = repo_root.join(&c.file);
                        let normalized_path = absolute_path.to_string_lossy().to_string();
                        (normalized_path, c)
                    })
                    .collect();
                db.update_churn(&sha, &churn_map)
                    .context("failed to update churn in pipeline DB")?;
            }
            Err(e) => eprintln!("Warning: failed to extract churn: {}", e),
        }
    }

    // Phase 3: build call graph from lean DB rows, run algorithms, write back, free graph.
    log_phase("building call graph");
    let call_graph = hotspots_core::build_call_graph_from_db(&db, &sha, repo_root)
        .context("failed to build call graph from DB")?;
    db.update_callgraph_metrics(&sha, &call_graph)
        .context("failed to update callgraph metrics in pipeline DB")?;
    // call_graph dropped here, freeing its memory.

    // Phase 4: load enriched functions from DB (churn + callgraph already set).
    let functions = db
        .load_snapshot_functions(&sha)
        .context("failed to load snapshot functions from pipeline DB")?;
    // SQLite connection dropped with `db` at end of scope; no longer needed.

    let snapshot = Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        commit: commit_info,
        analysis: AnalysisInfo {
            scope: "full".to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            formula_version: FORMULA_VERSION,
            touch_window_days: 365,
        },
        functions,
        summary: None,
        aggregates: None,
        triage: None,
        architecture: None,
        co_change: None,
    };

    // Phase 5: remaining enrichment (touch, activity risk, percentiles, driver, quadrant).
    let mut enricher = snapshot::SnapshotEnricher::new(snapshot).with_subsystems(repo_root);
    let mut dc_fetch = None;
    if !skip_touch_metrics {
        // burst_score, touch metrics, and directed coupling's git log walk
        // each independently read full history. Directed coupling's scoring
        // step needs partner_scores (built from activity_risk, computed only
        // after this function returns), but its git fetch doesn't — so fetch
        // it here, in parallel with burst/touch, and let the caller finish
        // the (cheap, in-memory) scoring once partner_scores is available.
        // Gate all three alongside the other touch/churn git log calls so
        // --skip-touch-metrics actually skips them too.
        let (fetched_dc, enriched) = rayon::join(
            || hotspots_core::coupling::fetch_directed_coupling_data(repo_root),
            || {
                with_burst_and_touch_metrics(
                    enricher,
                    repo_root,
                    hotspots_core::git::TOUCH_WINDOW_DAYS as u32,
                )
            },
        );
        dc_fetch = fetched_dc;
        enricher = enriched.with_branch_recency_adjustment(repo_root, merge_base.as_ref());
    }

    log_phase(if skip_touch_metrics {
        "scoring functions"
    } else {
        "reading touch history, burst score and scoring functions"
    });
    let result = enricher
        .with_relative_paths(repo_root)
        .enrich(repo_root)
        .build();
    Ok((result, dc_fetch))
}

/// Run the full enrichment pipeline: git context, churn, touch metrics, call graph, activity risk.
///
/// `skip_touch_metrics` bypasses all git-log touch calls (file-level and per-function).
pub(crate) fn build_enriched_snapshot(
    repo_root: &Path,
    mut reports: Vec<hotspots_core::FunctionRiskReport>,
    skip_touch_metrics: bool,
) -> anyhow::Result<Snapshot> {
    let git_context =
        git::extract_git_context_at(repo_root).context("failed to extract git context")?;

    let merge_base = hotspots_core::git::find_merge_base(repo_root);

    log_phase("building call graph");
    let call_graph = { hotspots_core::build_call_graph(&reports, repo_root).ok() };

    for r in &mut reports {
        r.callees.clear();
        r.callees.shrink_to_fit();
    }

    let mut enricher = snapshot::SnapshotEnricher::new(Snapshot::new(git_context.clone(), reports))
        .with_subsystems(repo_root);

    if !git_context.parent_shas.is_empty() {
        match git::windowed_churn_at(repo_root, &git_context.head_sha, git::CHURN_WINDOW_DAYS) {
            Ok(churns) => {
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

    if !skip_touch_metrics {
        // burst_score and touch metrics each do their own full-history `git log`
        // walk, run concurrently by with_burst_and_touch_metrics — gate both
        // alongside the other touch/churn git log calls so --skip-touch-metrics
        // actually skips them too.
        enricher = with_burst_and_touch_metrics(
            enricher,
            repo_root,
            hotspots_core::git::TOUCH_WINDOW_DAYS as u32,
        );
        enricher = enricher.with_branch_recency_adjustment(repo_root, merge_base.as_ref());
    }

    if let Some(ref graph) = call_graph {
        enricher = enricher.with_callgraph(graph);
    }

    log_phase(if skip_touch_metrics {
        "scoring functions"
    } else {
        "reading touch history, burst score and scoring functions"
    });
    Ok(enricher
        .with_relative_paths(repo_root)
        .enrich(repo_root)
        .build())
}

/// Analyzes and persists a snapshot at `sha` in a temporary worktree. When
/// `file_allowlist` is `Some`, restricts analysis to those paths (relative to
/// the repo root) instead of always doing a full-repo walk — used by
/// `diff --auto-analyze --scope-to-diff` to skip parsing files outside the
/// PR's changed-file set.
///
/// Used by `hotspots diff --auto-analyze` to generate missing snapshots on
/// demand. A temporary git worktree is created at `sha` and torn down
/// automatically when this function returns (or on error).
///
/// Per-function touch metrics are skipped (the touch cache lives in
/// `repo_root/.hotspots/` and cannot be pre-warmed for historical refs; file-
/// level batched metrics are still computed as normal).
pub(crate) fn analyze_and_persist_at_ref_scoped(
    repo_root: &Path,
    sha: &str,
    resolved_config: &hotspots_core::ResolvedConfig,
    file_allowlist: Option<&std::collections::HashSet<PathBuf>>,
) -> anyhow::Result<hotspots_core::snapshot::Snapshot> {
    let worktree = hotspots_core::git::create_worktree(repo_root, sha)
        .with_context(|| format!("failed to create worktree for {sha}"))?;

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
        file_allowlist: file_allowlist.cloned(),
    };
    let progress = make_analysis_progress();
    let reports = analyze_with_progress(
        &worktree.path,
        options,
        Some(resolved_config),
        Some(progress.as_ref()),
    )
    .with_context(|| format!("analysis failed for ref {sha}"))?;

    // Build enriched snapshot using the worktree as the repo root so that
    // git context (HEAD, parents, timestamp) is read from the checked-out ref.
    let snapshot = build_enriched_snapshot(&worktree.path, reports, false)
        .with_context(|| format!("enrichment failed for ref {sha}"))?;

    // Persist into the *real* repo's .hotspots/ directory, not the worktree.
    hotspots_core::snapshot::persist_snapshot(repo_root, &snapshot, false)
        .with_context(|| format!("failed to persist snapshot for {sha}"))?;
    hotspots_core::snapshot::append_to_index(repo_root, &snapshot)
        .with_context(|| format!("failed to update index for {sha}"))?;

    // Drop of `worktree` runs `git worktree remove` here.
    Ok(snapshot)
}

/// Compact duration for progress lines: `45s`, `3m07s`, `1h02m`.
fn fmt_duration(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// Estimated seconds left from files done so far; `None` until something has finished.
fn eta_secs(elapsed_secs: u64, done: usize, total: usize) -> Option<u64> {
    if done == 0 || done >= total {
        return None;
    }
    Some(elapsed_secs * (total - done) as u64 / done as u64)
}

/// The periodic non-TTY progress line; the ETA is omitted until a file has finished.
fn heartbeat_line(done: usize, total: usize, elapsed_secs: u64) -> String {
    let pct = done * 100 / total.max(1);
    match eta_secs(elapsed_secs, done, total) {
        Some(eta) => format!(
            "Analyzing: {done}/{total} files ({pct}%) · {} elapsed · ~{} remaining",
            fmt_duration(elapsed_secs),
            fmt_duration(eta)
        ),
        None => format!(
            "Analyzing: {done}/{total} files ({pct}%) · {} elapsed",
            fmt_duration(elapsed_secs)
        ),
    }
}

fn process_start() -> std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

/// Print a labelled phase line on stderr when it is not a terminal (the interactive
/// bar already shows live progress there), so CI logs show which phase a long run is in.
pub(crate) fn log_phase(label: &str) {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        eprintln!(
            "hotspots: {label} ({} elapsed)",
            fmt_duration(process_start().elapsed().as_secs())
        );
    }
}

pub(crate) fn make_analysis_progress() -> Box<dyn Fn(usize, usize) + Send + Sync> {
    use std::io::IsTerminal;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    let started = process_start();
    if !std::io::stderr().is_terminal() {
        // Callbacks only record progress; a heartbeat thread prints a line every 30s so
        // long runs stay visible even when no file finishes in that interval.
        let done_count = Arc::new(AtomicUsize::new(0));
        let total_count = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        {
            let (done_count, total_count, finished) =
                (done_count.clone(), total_count.clone(), finished.clone());
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                if finished.load(Ordering::Relaxed) {
                    return;
                }
                let (done, total) = (
                    done_count.load(Ordering::Relaxed),
                    total_count.load(Ordering::Relaxed),
                );
                if total == 0 {
                    continue;
                }
                eprintln!(
                    "{}",
                    heartbeat_line(done, total, started.elapsed().as_secs())
                );
            });
        }
        return Box::new(move |done: usize, total: usize| {
            total_count.store(total, Ordering::Relaxed);
            done_count.store(done, Ordering::Relaxed);
            if done == 0 {
                eprintln!("Analyzing: 0/{total} files");
            } else if done >= total {
                finished.store(true, Ordering::Relaxed);
                eprintln!(
                    "Analyzing: {done}/{total} files [done in {}]",
                    fmt_duration(started.elapsed().as_secs())
                );
            }
        });
    }
    use indicatif::{ProgressBar, ProgressStyle};
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("Analyzing [{bar:40}] {pos}/{len} files  {elapsed} · ~{eta} remaining")
            .unwrap()
            .progress_chars("##-"),
    );
    Box::new(move |done: usize, total: usize| {
        if done == 0 {
            if total == 0 {
                pb.finish_and_clear();
                return;
            }
            pb.set_length(total as u64);
            pb.set_position(0);
        } else {
            pb.set_position(done as u64);
            if done >= total {
                pb.finish_and_clear();
            }
        }
    })
}

#[cfg(test)]
mod progress_format_tests {
    use super::{eta_secs, fmt_duration, heartbeat_line};

    #[test]
    fn durations_are_compact() {
        assert_eq!(fmt_duration(45), "45s");
        assert_eq!(fmt_duration(187), "3m07s");
        assert_eq!(fmt_duration(3720), "1h02m");
    }

    #[test]
    fn heartbeat_shows_eta_once_files_have_finished() {
        assert_eq!(
            heartbeat_line(10, 40, 60),
            "Analyzing: 10/40 files (25%) · 1m00s elapsed · ~3m00s remaining"
        );
    }

    #[test]
    fn heartbeat_omits_eta_before_any_file_finishes() {
        assert_eq!(
            heartbeat_line(0, 40, 30),
            "Analyzing: 0/40 files (0%) · 30s elapsed"
        );
    }

    #[test]
    fn eta_extrapolates_from_files_done() {
        assert_eq!(eta_secs(60, 10, 40), Some(180));
        assert_eq!(eta_secs(60, 0, 40), None);
        assert_eq!(eta_secs(60, 40, 40), None);
    }
}

#[cfg(test)]
mod validate_analyze_flags_tests {
    use super::*;

    /// All-permissive baseline: text format, every bool false, every
    /// optional field unset. Individual tests override just the field(s) under test.
    fn base_args() -> AnalyzeArgs {
        AnalyzeArgs {
            path: PathBuf::new(),
            format: OutputFormat::Text,
            top: None,
            min_lrs: None,
            config_path: None,
            output: None,
            explain: false,
            force: false,
            no_persist: false,
            level: None,
            all_functions: false,
            source_url: None,
            html_output: None,
            cold_start: false,
            axes: false,
            files: None,
            diff: false,
            staged: false,
            ownership: false,
            prune: false,
            unreachable: false,
            older_than: None,
            compact: None,
            dry_run: false,
            show_config: false,
            validate_config: false,
            help_agent: None,
        }
    }

    #[test]
    fn html_output_rejects_html_format() {
        let mut a = base_args();
        a.format = OutputFormat::Html;
        a.html_output = Some(PathBuf::from("r.html"));
        assert!(validate_analyze_flags(&a).is_err());
    }

    #[test]
    fn html_output_is_fine_with_non_html_format() {
        let mut a = base_args();
        a.format = OutputFormat::Json;
        a.html_output = Some(PathBuf::from("r.html"));
        assert!(validate_analyze_flags(&a).is_ok());
    }

    #[test]
    fn base_args_pass_validation() {
        assert!(validate_analyze_flags(&base_args()).is_ok());
    }

    #[test]
    fn cold_start_and_axes_are_mutually_exclusive() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            cold_start: true,
            axes: true,
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn explain_is_permitted_by_default() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            explain: true,
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn no_persist_rejects_force() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            no_persist: true,
            force: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            no_persist: true,
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn level_requires_text_format_and_rejects_explain() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            format: OutputFormat::Json,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            explain: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn all_functions_requires_json_format() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            all_functions: true,
            format: OutputFormat::Json,
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            all_functions: true,
            format: OutputFormat::Text,
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn sarif_format_is_unconditionally_valid_now() {
        // Previously `--format sarif` required `--mode snapshot`; snapshot is the
        // only analysis mode now, so sarif works with no extra flag.
        assert!(validate_analyze_flags(&AnalyzeArgs {
            format: OutputFormat::Sarif,
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn prune_sub_flags_require_prune() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            unreachable: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            older_than: Some(30),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            compact: Some(1),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            dry_run: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            prune: true,
            unreachable: true,
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn dispatch_triggers_are_mutually_exclusive() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            prune: true,
            show_config: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            cold_start: true,
            files: Some("a.rs".to_string()),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            axes: true,
            help_agent: Some(crate::cmd::agent::AgentAction::All),
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn single_dispatch_trigger_is_fine() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            show_config: true,
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            staged: true,
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            help_agent: Some(crate::cmd::agent::AgentAction::Diff),
            ..base_args()
        })
        .is_ok());
    }
}
