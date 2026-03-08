use crate::output::{explain, policy};
use crate::util::{find_repo_root, write_html_report};
use crate::{OutputFormat, OutputLevel, OutputMode};
use anyhow::Context;
use hotspots_core::delta::Delta;
use hotspots_core::snapshot::{self, Snapshot};
use hotspots_core::{analyze_with_progress, AnalysisOptions};
use hotspots_core::{delta, git};
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
    pub per_function_touches: bool,
    pub all_functions: bool,
    pub explain_patterns: bool,
}

/// Validate flag combinations that are mode/format-specific.
pub(crate) fn validate_analyze_flags(args: &AnalyzeArgs) -> anyhow::Result<()> {
    let AnalyzeArgs {
        mode,
        format,
        policy,
        explain,
        per_function_touches,
        no_persist,
        force,
        level,
        all_functions,
        explain_patterns,
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
    if *all_functions
        && (*mode != Some(OutputMode::Snapshot) || !matches!(format, OutputFormat::Json))
    {
        anyhow::bail!("--all-functions is only valid with --mode snapshot --format json");
    }
    if *explain_patterns && *mode != Some(OutputMode::Snapshot) && mode.is_some() {
        anyhow::bail!("--explain-patterns is only valid with --mode snapshot or without --mode");
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
        per_function_touches,
        all_functions,
        explain_patterns,
    } = args;

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
    let effective_per_function_touches =
        per_function_touches || resolved_config.per_function_touches;

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
                per_function_touches: effective_per_function_touches,
                all_functions,
                explain_patterns,
            },
        );
    }

    // Default behavior (no --mode): simple text/JSON output
    let options = AnalysisOptions {
        min_lrs: effective_min_lrs,
        top_n: effective_top,
    };
    let analysis_progress = make_analysis_progress();
    let mut reports = analyze_with_progress(
        &normalized_path,
        options,
        Some(&resolved_config),
        Some(analysis_progress.as_ref()),
    )?;

    if explain_patterns {
        for report in &mut reports {
            let t1 = hotspots_core::patterns::Tier1Input {
                cc: report.metrics.cc,
                nd: report.metrics.nd,
                fo: report.metrics.fo,
                ns: report.metrics.ns,
                loc: report.metrics.loc,
            };
            let t2 = hotspots_core::patterns::Tier2Input {
                fan_in: None,
                scc_size: None,
                churn_lines: None,
                days_since_last_change: None,
                neighbor_churn: None,
                is_entrypoint: false,
            };
            report.pattern_details = Some(hotspots_core::patterns::classify_detailed(
                &t1,
                &t2,
                &resolved_config.pattern_thresholds,
            ));
        }
    }

    match format {
        OutputFormat::Text => {
            print!("{}", hotspots_core::render_text(&reports));
        }
        OutputFormat::Json => {
            println!("{}", hotspots_core::render_json(&reports));
        }
        OutputFormat::Html | OutputFormat::Jsonl => {
            anyhow::bail!("HTML/JSONL format requires --mode snapshot or --mode delta");
        }
    }

    Ok(())
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
    pub per_function_touches: bool,
    pub all_functions: bool,
    pub explain_patterns: bool,
}

pub(crate) fn handle_mode_output(
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
        all_functions,
        explain_patterns,
    } = opts;

    let repo_root = find_repo_root(path)?;
    let analysis_progress = make_analysis_progress();
    let reports = analyze_with_progress(
        path,
        AnalysisOptions {
            min_lrs,
            top_n: None,
        },
        Some(resolved_config),
        Some(analysis_progress.as_ref()),
    )?;
    let pr_context = git::detect_pr_context();

    match mode {
        OutputMode::Snapshot => {
            let mut snapshot =
                build_enriched_snapshot(&repo_root, resolved_config, reports, per_function_touches)
                    .context("failed to build enriched snapshot")?;

            snapshot.populate_patterns(&resolved_config.pattern_thresholds);
            if explain_patterns {
                snapshot.populate_pattern_details(&resolved_config.pattern_thresholds);
            }

            if !pr_context.is_pr && !no_persist {
                snapshot::persist_snapshot(&repo_root, &snapshot, force)
                    .context("failed to persist snapshot")?;
                snapshot::append_to_index(&repo_root, &snapshot)
                    .context("failed to update index")?;
            }

            let total_function_count = snapshot.functions.len();
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
                    all_functions,
                },
                &repo_root,
            )?;
        }
        OutputMode::Delta => {
            let snapshot =
                build_enriched_snapshot(&repo_root, resolved_config, reports, per_function_touches)
                    .context("failed to build enriched snapshot")?;

            let delta_val = if pr_context.is_pr {
                compute_pr_delta(&repo_root, &snapshot)?
            } else {
                delta::compute_delta(&repo_root, &snapshot)?
            };

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

            let mut delta_with_extras = delta_val.clone();
            if policy {
                let policy_results = hotspots_core::policy::evaluate_policies(
                    &delta_val,
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
                    &delta_val,
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
    all_functions: bool,
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
        all_functions,
    } = opts;
    match format {
        OutputFormat::Json => {
            let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                snapshot,
                repo_root,
                co_change_window_days,
                co_change_min_count,
            );
            if all_functions {
                snapshot.aggregates = Some(aggregates);
                println!("{}", snapshot.to_json()?);
            } else {
                let agent_output = hotspots_core::aggregates::compute_agent_snapshot_output(
                    snapshot,
                    &aggregates,
                    repo_root,
                );
                println!(
                    "{}",
                    agent_output
                        .to_json()
                        .context("failed to serialize agent snapshot output")?
                );
            }
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
                explain::print_file_risk_output(&aggregates.file_risk, top)?;
            } else if level == Some(OutputLevel::Module) {
                let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                    snapshot,
                    repo_root,
                    co_change_window_days,
                    co_change_min_count,
                );
                explain::print_module_output(&aggregates.modules, top)?;
            } else if explain {
                let aggregates = hotspots_core::aggregates::compute_snapshot_aggregates(
                    snapshot,
                    repo_root,
                    co_change_window_days,
                    co_change_min_count,
                );
                explain::print_explain_output(
                    snapshot,
                    total_function_count,
                    &aggregates.co_change,
                )?;
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
            let history: Vec<_> = hotspots_core::trends::load_snapshot_window(repo_root, 30)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| s.summary.map(|sum| (s.commit, sum)))
                .collect();
            let html = hotspots_core::html::render_html_snapshot(snapshot, &history);
            let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
            write_html_report(&output_path, &html)?;
            eprintln!("HTML report written to: {}", output_path.display());
        }
    }
    Ok(())
}

/// Returns true if there are blocking policy failures (caller should exit non-zero).
fn emit_delta_output(
    delta_val: &Delta,
    format: OutputFormat,
    with_policy: bool,
    output: Option<PathBuf>,
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
        OutputFormat::Jsonl => {
            anyhow::bail!("JSONL format is not supported for delta mode (use --mode snapshot)");
        }
        OutputFormat::Text => {
            if with_policy {
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
            } else {
                anyhow::bail!(
                    "text format is not supported for delta mode without --policy (use --format json)"
                );
            }
        }
        OutputFormat::Html => {
            let html = hotspots_core::html::render_html_delta(delta_val);
            let output_path = output.unwrap_or_else(|| PathBuf::from(".hotspots/report.html"));
            write_html_report(&output_path, &html)?;
            eprintln!("HTML report written to: {}", output_path.display());
        }
    }

    Ok(has_blocking_failures)
}

/// Compute delta for PR mode (compares vs merge-base).
fn compute_pr_delta(repo_root: &Path, snapshot: &Snapshot) -> anyhow::Result<delta::Delta> {
    let merge_base_sha = git::resolve_merge_base_auto();

    let parent = if let Some(sha) = &merge_base_sha {
        match delta::load_parent_snapshot(repo_root, sha)? {
            Some(parent_snapshot) => Some(parent_snapshot),
            None => {
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

/// Run the full enrichment pipeline: git context, churn, touch metrics, call graph, activity risk.
pub(crate) fn build_enriched_snapshot(
    repo_root: &Path,
    resolved_config: &hotspots_core::ResolvedConfig,
    reports: Vec<hotspots_core::FunctionRiskReport>,
    per_function_touches: bool,
) -> anyhow::Result<Snapshot> {
    let git_context =
        git::extract_git_context_at(repo_root).context("failed to extract git context")?;

    let merge_base = hotspots_core::git::find_merge_base(repo_root);

    let call_graph = hotspots_core::build_call_graph(&reports, repo_root).ok();
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

    let total_functions = reports.len();
    let mut enricher = snapshot::SnapshotEnricher::new(Snapshot::new(git_context.clone(), reports));

    if !git_context.parent_shas.is_empty() {
        match git::extract_commit_churn_at(repo_root, &git_context.head_sha) {
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

    if per_function_touches
        && !hotspots_core::snapshot::hotspots_dir(repo_root)
            .join("touch-cache.json.zst")
            .exists()
    {
        eprintln!("Warning: touch cache cold start — first run will be slower (building cache)");
    }
    let progress = if per_function_touches {
        Some(make_progress_reporter(total_functions))
    } else {
        None
    };
    enricher = enricher.with_touch_metrics(repo_root, per_function_touches, progress);
    enricher = enricher.with_branch_recency_adjustment(repo_root, merge_base.as_ref());

    if let Some(ref graph) = call_graph {
        enricher = enricher.with_callgraph(graph);
    }

    Ok(enricher
        .enrich(
            Some(&resolved_config.scoring_weights),
            resolved_config.driver_threshold_percentile,
        )
        .build())
}

fn make_progress_reporter(total: usize) -> Box<dyn Fn(usize, usize)> {
    use std::io::IsTerminal;
    if total == 0 {
        return Box::new(|_i: usize, _total: usize| {});
    }
    if std::io::stderr().is_terminal() {
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("Building touch cache [{bar:40}] {pos}/{len}")
                .unwrap()
                .progress_chars("##-"),
        );
        Box::new(move |i: usize, t: usize| {
            pb.set_position((i + 1) as u64);
            if i + 1 >= t {
                pb.finish_and_clear();
            }
        })
    } else {
        eprintln!(
            "Building touch cache: 0/{} functions [next update in ~30s]",
            total
        );
        let last_print = std::sync::Mutex::new(std::time::Instant::now());
        Box::new(move |i: usize, t: usize| {
            if i + 1 >= t {
                eprintln!("Building touch cache: {}/{} functions [done]", i + 1, t);
                return;
            }
            if let Ok(mut last) = last_print.try_lock() {
                if last.elapsed().as_secs() >= 30 {
                    eprintln!("Building touch cache: {}/{} functions", i + 1, t);
                    *last = std::time::Instant::now();
                }
            }
        })
    }
}

pub(crate) fn make_analysis_progress() -> Box<dyn Fn(usize, usize) + Send + Sync> {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        return Box::new(|_: usize, _: usize| {});
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
