use crate::output::{explain, policy};
use crate::util::{find_repo_root, write_html_report};
use crate::{OutputFormat, OutputLevel, OutputMode};
use anyhow::Context;
use hotspots_core::delta::Delta;
use hotspots_core::gate::{check_gate_smoothed, GateConfig, GateVerdict};
use hotspots_core::snapshot::{self, Snapshot};
use hotspots_core::{analyze_with_progress, AnalysisOptions};
use hotspots_core::{delta, git};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub(crate) struct AnalyzeArgs {
    pub path: PathBuf,
    pub format: OutputFormat,
    pub mode: Option<OutputMode>,
    pub policy: bool,
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
    /// Also write the HTML report from the same analysis (snapshot mode, non-html --format).
    pub html_output: Option<PathBuf>,
    /// Number of rayon worker threads; None = use all logical CPUs.
    pub jobs: Option<usize>,
    /// When true, skip all touch metrics (no git log calls at all).
    pub skip_touch_metrics: bool,
    /// Skip the suppression gate check entirely.
    pub skip_gate: bool,
    /// Rank via Gini-gated cold-start routing instead of a trained ranker.
    /// Explicit opt-in only; reads no fix-commit label data.
    pub cold_start: bool,
    /// Print three independent ranked sections (Risk, Coupling, Ownership)
    /// instead of a single merged list.
    pub axes: bool,
}

/// Validate flag combinations that are mode/format-specific.
pub(crate) fn validate_analyze_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    validate_mode_bypass_flags(args)?;
    validate_policy_flag(args)?;
    validate_explain_flag(args)?;
    validate_no_persist_flag(args)?;
    validate_html_output_flag(args)?;
    validate_level_flag(args)?;
    validate_all_functions_flag(args)?;
    validate_format_mode_requirements(args)?;
    Ok(())
}

/// `--cold-start`/`--axes` both bypass the trained-ranker/snapshot pipeline
/// entirely, so neither is compatible with `--mode`, and they're mutually
/// exclusive with each other.
fn validate_mode_bypass_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.cold_start && args.mode.is_some() {
        anyhow::bail!("--cold-start is not compatible with --mode (it bypasses the trained-ranker/snapshot pipeline entirely)");
    }
    if args.axes && args.mode.is_some() {
        anyhow::bail!("--axes is not compatible with --mode (it bypasses the trained-ranker/snapshot pipeline entirely)");
    }
    if args.axes && args.cold_start {
        anyhow::bail!("--axes and --cold-start are mutually exclusive");
    }
    Ok(())
}

fn validate_policy_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.policy && args.mode != Some(OutputMode::Delta) {
        anyhow::bail!("--policy flag is only valid with --mode delta");
    }
    Ok(())
}

fn validate_explain_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if args.explain && args.mode.is_some() && args.mode != Some(OutputMode::Snapshot) {
        anyhow::bail!("--explain is not compatible with --mode delta or --mode models");
    }
    Ok(())
}

fn validate_no_persist_flag(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if !args.no_persist {
        return Ok(());
    }
    if args.mode.is_none() {
        anyhow::bail!("--no-persist is only valid with --mode snapshot or --mode delta");
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
    if args.mode != Some(OutputMode::Snapshot) {
        anyhow::bail!("--html-output is only valid with --mode snapshot");
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
    if args.mode != Some(OutputMode::Snapshot) {
        anyhow::bail!("--level is only valid with --mode snapshot");
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
    if args.all_functions
        && (args.mode != Some(OutputMode::Snapshot) || !matches!(args.format, OutputFormat::Json))
    {
        anyhow::bail!("--all-functions is only valid with --mode snapshot --format json");
    }
    Ok(())
}

/// Output formats that require `--mode snapshot` specifically.
fn validate_format_mode_requirements(args: &AnalyzeArgs) -> anyhow::Result<()> {
    if matches!(args.format, OutputFormat::Sarif) && args.mode != Some(OutputMode::Snapshot) {
        anyhow::bail!("--format sarif requires --mode snapshot");
    }
    Ok(())
}

pub(crate) fn handle_analyze(args: AnalyzeArgs) -> anyhow::Result<()> {
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
        skip_touch_metrics,
        all_functions,
        source_url,
        html_output,
        jobs,
        skip_gate,
        cold_start,
        axes,
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
        if let Some(n) = jobs {
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

    if let Some(output_mode) = mode {
        let result = handle_mode_output(
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
                all_functions,
                source_url,
                html_output,
                skip_touch_metrics,
                skip_gate,
            },
        );
        return result;
    }

    // If a trained ranker exists, promote to snapshot mode so activity_risk
    // fields are populated and the ranker can be applied. The ranker has no
    // effect in the default LRS-only path.
    let repo_root_for_ranker =
        find_repo_root(&normalized_path).unwrap_or_else(|_| normalized_path.clone());
    let ranker_path = snapshot::hotspots_dir(&repo_root_for_ranker).join("ranker.json");
    if ranker_path.exists() {
        let result = handle_mode_output(
            &normalized_path,
            OutputMode::Snapshot,
            &resolved_config,
            ModeOutputOptions {
                format,
                policy: false,
                top: effective_top,
                min_lrs: effective_min_lrs,
                output,
                explain: matches!(format, OutputFormat::Text),
                force,
                no_persist: true, // default analyze doesn't persist snapshots
                level,
                all_functions: false,
                source_url,
                html_output,
                skip_touch_metrics,
                skip_gate,
            },
        );
        return result;
    }

    // Default behavior (no --mode): simple text/JSON output
    handle_default_output(
        &normalized_path,
        format,
        effective_min_lrs,
        effective_top,
        &resolved_config,
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

fn handle_default_output(
    path: &Path,
    format: OutputFormat,
    min_lrs: Option<f64>,
    top: Option<usize>,
    resolved_config: &hotspots_core::ResolvedConfig,
) -> anyhow::Result<()> {
    let analysis_progress = make_analysis_progress();
    let explicit_top = top.or(resolved_config.top_n);
    // 0 is the sentinel for "show all"; otherwise default to 20 for text output
    let limit = match explicit_top {
        Some(0) => usize::MAX,
        Some(n) => n,
        None => 20,
    };
    let reports = hotspots_core::analyze_with_progress(
        path,
        AnalysisOptions {
            min_lrs,
            top_n: if matches!(format, OutputFormat::Text) {
                Some(limit).filter(|&n| n != usize::MAX)
            } else {
                explicit_top.filter(|&n| n != 0)
            },
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;

    let result: anyhow::Result<()> = match format {
        OutputFormat::Text => {
            let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
            print!(
                "{}",
                hotspots_core::render_text_grouped(&reports, limit, color)
            );
            Ok(())
        }
        OutputFormat::Json => {
            println!("{}", hotspots_core::render_json_envelope(&reports));
            Ok(())
        }
        OutputFormat::Html => {
            anyhow::bail!("HTML format requires --mode snapshot or --mode delta");
        }
        OutputFormat::Sarif => {
            anyhow::bail!("SARIF format requires --mode snapshot");
        }
    };
    result
}

pub(crate) struct ModeOutputOptions {
    pub format: OutputFormat,
    pub policy: bool,
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
    mode: OutputMode,
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
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;
    let pr_context = git::detect_pr_context();

    match mode {
        OutputMode::Snapshot => handle_snapshot_mode(&repo_root, reports, pr_context, opts),
        OutputMode::Delta => {
            handle_delta_mode(&repo_root, resolved_config, reports, pr_context, opts)
        }
    }
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
            "text format without --explain is not supported for snapshot mode (use --format json or add --explain)"
        );
    }

    let mut snapshot = build_snapshot_via_db(repo_root, reports, skip_touch_metrics)
        .context("failed to build enriched snapshot")?;

    // Populate DC before persisting so the snapshot on disk includes the values,
    // and before apply_trained_ranker so the ranker reads real DC instead of 0.0.
    // Partner scores use make_rel so keys match the repo-relative paths that
    // compute_directed_coupling_for_repo produces from git log output.
    //
    // Gated behind skip_touch_metrics: like touch metrics, this runs a
    // potentially unbounded `git log` walk (see coupling.rs), so
    // --skip-touch-metrics must skip it too rather than leaving it as a
    // hidden cost the flag doesn't cover.
    if !skip_touch_metrics {
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
        snapshot.populate_directed_coupling(repo_root, &partner_scores);
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

fn handle_delta_mode(
    repo_root: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    reports: Vec<hotspots_core::FunctionRiskReport>,
    pr_context: hotspots_core::git::PrContext,
    opts: ModeOutputOptions,
) -> anyhow::Result<()> {
    let ModeOutputOptions {
        format,
        policy,
        output,
        source_url,
        skip_touch_metrics,
        ..
    } = opts;
    let snapshot = build_enriched_snapshot(repo_root, reports, skip_touch_metrics)
        .context("failed to build enriched snapshot")?;

    let delta_val = if pr_context.is_pr {
        compute_pr_delta(repo_root, &snapshot)?
    } else {
        delta::compute_delta(repo_root, &snapshot)?
    };

    let delta_with_extras = enrich_delta(repo_root, resolved_config, &snapshot, delta_val, policy)?;

    let blocking = emit_delta_output(
        &delta_with_extras,
        format,
        policy,
        output,
        source_url.as_deref(),
    )?;
    if blocking {
        std::process::exit(1);
    }
    Ok(())
}

fn enrich_delta(
    repo_root: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    snapshot: &Snapshot,
    delta_val: delta::Delta,
    policy: bool,
) -> anyhow::Result<delta::Delta> {
    let mut unique_files: Vec<String> = snapshot
        .functions
        .iter()
        .map(|f| f.file.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    unique_files.sort();
    let files_as_str: Vec<&str> = unique_files.iter().map(|s| s.as_str()).collect();
    let import_edges = hotspots_core::imports::resolve_file_deps(&files_as_str, repo_root);
    let mut current_co_change = hotspots_core::git::extract_co_change_pairs(
        repo_root,
        hotspots_core::git::CO_CHANGE_WINDOW_DAYS,
        hotspots_core::git::CO_CHANGE_MIN_COUNT,
    )
    .unwrap_or_default();
    hotspots_core::aggregates::annotate_static_deps(
        &mut current_co_change,
        &import_edges,
        repo_root,
    );

    let parent_sha = snapshot.commit.parents.first().cloned();
    let parent_snapshot: Option<Snapshot> = parent_sha.as_deref().and_then(|sha| {
        hotspots_core::delta::load_parent_snapshot(repo_root, sha)
            .ok()
            .flatten()
    });
    let prev_co_change: Vec<hotspots_core::git::CoChangePair> = parent_snapshot
        .as_ref()
        .and_then(|s| s.aggregates.as_ref())
        .map(|a| a.co_change.clone())
        .unwrap_or_default();

    let mut enriched = delta_val.clone();
    if policy {
        let policy_results = hotspots_core::policy::evaluate_policies(&delta_val, resolved_config)
            .context("failed to evaluate policies")?;
        if let Some(results) = policy_results {
            enriched.policy = Some(results);
        }
    }
    // fn_changed_lines is the strongest tested predictor of PR defect risk in our evaluation. Best-effort — no parent snapshot (first commit, or an untracked parent)
    // or a failed git diff both degrade to an empty line-set map, giving 0, not an error.
    let fn_changed_lines = match (&parent_snapshot, &parent_sha) {
        (Some(parent), Some(parent_sha)) => {
            let diff_lines =
                hotspots_core::git::diff_line_sets_at(repo_root, parent_sha, &snapshot.commit.sha)
                    .unwrap_or_default();
            hotspots_core::git::compute_fn_changed_lines(
                repo_root,
                &delta_val.deltas,
                snapshot,
                parent,
                &diff_lines,
            )
        }
        _ => 0,
    };
    enriched.aggregates = Some(hotspots_core::aggregates::compute_delta_aggregates(
        &delta_val,
        &current_co_change,
        &prev_co_change,
        fn_changed_lines,
    ));
    Ok(enriched)
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
            "text format without --explain is not supported for snapshot mode (use --format json or add --explain)"
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

/// Returns true if there are blocking policy failures (caller should exit non-zero).
fn emit_delta_output(
    delta_val: &Delta,
    format: OutputFormat,
    with_policy: bool,
    output: Option<PathBuf>,
    source_url: Option<&str>,
) -> anyhow::Result<bool> {
    let has_blocking_failures = delta_val
        .policy
        .as_ref()
        .map(|p| p.has_blocking_failures())
        .unwrap_or(false);

    match format {
        OutputFormat::Json => {
            println!("{}", delta_val.to_json()?);
        }
        OutputFormat::Text => {
            emit_delta_text(delta_val, with_policy)?;
        }
        OutputFormat::Html => {
            emit_delta_html(delta_val, source_url, output)?;
        }
        OutputFormat::Sarif => {
            anyhow::bail!("SARIF format is not supported for delta mode (use --mode snapshot)");
        }
    }

    Ok(has_blocking_failures)
}

fn emit_delta_text(delta_val: &Delta, with_policy: bool) -> anyhow::Result<()> {
    if !with_policy {
        anyhow::bail!(
            "text format is not supported for delta mode without --policy (use --format json)"
        );
    }
    if let Some(ref policy_results) = delta_val.policy {
        policy::print_policy_text_output(delta_val, policy_results)?;
    } else {
        println!("Delta Analysis");
        println!("{}", "=".repeat(80));
        println!("Baseline delta (no parent snapshot) - policy evaluation skipped.");
        println!(
            "\nDelta contains {} function changes.",
            delta_val.deltas.len()
        );
    }
    Ok(())
}

fn emit_delta_html(
    delta_val: &Delta,
    source_url: Option<&str>,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let html = hotspots_core::html::render_html_delta(delta_val, source_url);
    let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
    write_html_report(&output_path, &html)?;
    eprintln!("HTML report written to: {}", output_path.display());
    Ok(())
}

/// Compute delta for PR mode (compares vs merge-base).
fn compute_pr_delta(repo_root: &Path, snapshot: &Snapshot) -> anyhow::Result<delta::Delta> {
    let merge_base_sha = git::resolve_merge_base_auto();
    let fallback_sha = snapshot.commit.parents.first().map(|s| s.as_str());
    let parent = load_merge_base_or_fallback(repo_root, merge_base_sha.as_deref(), fallback_sha)?;
    delta::Delta::new(snapshot, parent.as_ref())
}

fn load_merge_base_or_fallback(
    repo_root: &Path,
    merge_base_sha: Option<&str>,
    fallback_sha: Option<&str>,
) -> anyhow::Result<Option<Snapshot>> {
    if let Some(sha) = merge_base_sha {
        match delta::load_parent_snapshot(repo_root, sha)? {
            Some(snap) => return Ok(Some(snap)),
            None => {
                eprintln!("Warning: merge-base snapshot not found, falling back to direct parent")
            }
        }
    } else {
        eprintln!("Warning: failed to resolve merge-base, falling back to direct parent");
    }
    if let Some(sha) = fallback_sha {
        delta::load_parent_snapshot(repo_root, sha)
    } else {
        Ok(None)
    }
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
) -> anyhow::Result<Snapshot> {
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
    if !skip_touch_metrics {
        // burst_score does its own full-history `git log` walk (history_signals::
        // load_commits_with_author) — gate it alongside the other touch/churn git
        // log calls so --skip-touch-metrics actually skips it too.
        enricher = enricher.with_burst_score(repo_root);
        enricher =
            enricher.with_touch_metrics(repo_root, hotspots_core::git::TOUCH_WINDOW_DAYS as u32);
        enricher = enricher.with_branch_recency_adjustment(repo_root, merge_base.as_ref());
    }

    let result = enricher.with_relative_paths(repo_root).enrich().build();
    Ok(result)
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
        // burst_score does its own full-history `git log` walk (history_signals::
        // load_commits_with_author) — gate it alongside the other touch/churn git
        // log calls so --skip-touch-metrics actually skips it too.
        enricher = enricher.with_burst_score(repo_root);
        enricher =
            enricher.with_touch_metrics(repo_root, hotspots_core::git::TOUCH_WINDOW_DAYS as u32);
        enricher = enricher.with_branch_recency_adjustment(repo_root, merge_base.as_ref());
    }

    if let Some(ref graph) = call_graph {
        enricher = enricher.with_callgraph(graph);
    }

    Ok(enricher.with_relative_paths(repo_root).enrich().build())
}

/// Analyze the source tree at `sha`, enrich it, persist it to `repo_root`, and
/// return the resulting snapshot.
///
/// Used by `hotspots diff --auto-analyze` to generate missing snapshots on
/// demand. A temporary git worktree is created at `sha` and torn down
/// automatically when this function returns (or on error).
///
/// Per-function touch metrics are skipped (the touch cache lives in
/// `repo_root/.hotspots/` and cannot be pre-warmed for historical refs; file-
/// level batched metrics are still computed as normal).
pub(crate) fn analyze_and_persist_at_ref(
    repo_root: &Path,
    sha: &str,
    resolved_config: &hotspots_core::ResolvedConfig,
) -> anyhow::Result<hotspots_core::snapshot::Snapshot> {
    let worktree = hotspots_core::git::create_worktree(repo_root, sha)
        .with_context(|| format!("failed to create worktree for {sha}"))?;

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
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

pub(crate) fn make_analysis_progress() -> Box<dyn Fn(usize, usize) + Send + Sync> {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        let last_print = std::sync::Mutex::new(std::time::Instant::now());
        return Box::new(move |done: usize, total: usize| {
            if done == 0 {
                eprintln!("Analyzing: 0/{total} files");
                return;
            }
            if done >= total {
                eprintln!("Analyzing: {done}/{total} files [done]");
                return;
            }
            if let Ok(mut last) = last_print.try_lock() {
                if last.elapsed().as_secs() >= 30 {
                    let pct = (done as f64 / total as f64 * 100.0) as usize;
                    eprintln!("Analyzing: {done}/{total} files ({pct}%)");
                    *last = std::time::Instant::now();
                }
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
mod validate_analyze_flags_tests {
    use super::*;

    /// All-permissive baseline: no mode, text format, every bool false, every
    /// optional field unset. Individual tests override just the field(s) under test.
    fn base_args() -> AnalyzeArgs {
        AnalyzeArgs {
            path: PathBuf::new(),
            format: OutputFormat::Text,
            mode: None,
            policy: false,
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
            jobs: None,
            skip_touch_metrics: false,
            skip_gate: false,
            cold_start: false,
            axes: false,
        }
    }

    #[test]
    fn html_output_requires_snapshot_mode() {
        let mut a = base_args();
        a.html_output = Some(PathBuf::from("r.html"));
        assert!(validate_analyze_flags(&a).is_err());
        a.mode = Some(OutputMode::Snapshot);
        a.format = OutputFormat::Json;
        assert!(validate_analyze_flags(&a).is_ok());
    }

    #[test]
    fn html_output_rejects_html_format() {
        let mut a = base_args();
        a.mode = Some(OutputMode::Snapshot);
        a.format = OutputFormat::Html;
        a.html_output = Some(PathBuf::from("r.html"));
        assert!(validate_analyze_flags(&a).is_err());
    }

    #[test]
    fn base_args_pass_validation() {
        assert!(validate_analyze_flags(&base_args()).is_ok());
    }

    #[test]
    fn cold_start_and_axes_reject_mode() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            cold_start: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            axes: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_err());
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
    fn policy_requires_delta_mode() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            policy: true,
            mode: Some(OutputMode::Delta),
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            policy: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn explain_rejects_delta_and_models_mode() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            explain: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            explain: true,
            mode: Some(OutputMode::Delta),
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn no_persist_requires_snapshot_or_delta_mode_and_rejects_force() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            no_persist: true,
            mode: None,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            no_persist: true,
            force: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            no_persist: true,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn level_requires_snapshot_mode_text_format_and_rejects_explain() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            mode: Some(OutputMode::Delta),
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            mode: Some(OutputMode::Snapshot),
            format: OutputFormat::Json,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            mode: Some(OutputMode::Snapshot),
            explain: true,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            level: Some(OutputLevel::File),
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_ok());
    }

    #[test]
    fn all_functions_requires_snapshot_mode_and_json_format() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            all_functions: true,
            mode: Some(OutputMode::Snapshot),
            format: OutputFormat::Json,
            ..base_args()
        })
        .is_ok());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            all_functions: true,
            mode: Some(OutputMode::Snapshot),
            format: OutputFormat::Text,
            ..base_args()
        })
        .is_err());
    }

    #[test]
    fn sarif_format_requires_snapshot_mode() {
        assert!(validate_analyze_flags(&AnalyzeArgs {
            format: OutputFormat::Sarif,
            mode: None,
            ..base_args()
        })
        .is_err());
        assert!(validate_analyze_flags(&AnalyzeArgs {
            format: OutputFormat::Sarif,
            mode: Some(OutputMode::Snapshot),
            ..base_args()
        })
        .is_ok());
    }
}
