use crate::util::{find_repo_root, write_html_report};
use crate::OutputFormat;
use anyhow::Context;
use hotspots_core::delta::Delta;
use hotspots_core::git;
use hotspots_core::snapshot;
use std::path::PathBuf;

/// Structured error from [`load_snapshot_or_report`] so that the caller can
/// map each variant to a distinct exit code.
enum LoadError {
    /// Snapshot not yet generated — user needs to run `hotspots analyze`.
    Missing(String),
    /// Auto-analysis was attempted but failed (real execution error).
    Failed(String),
}

pub(crate) struct DiffArgs {
    pub base: Option<String>,
    pub head: String,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub policy: bool,
    pub top: Option<usize>,
    pub config_path: Option<PathBuf>,
    pub auto_analyze: bool,
    pub scope_to_diff: bool,
    pub explain: bool,
}

pub(crate) fn handle_diff(args: DiffArgs) -> anyhow::Result<()> {
    let DiffArgs {
        base,
        head,
        format,
        output,
        policy,
        top,
        config_path,
        auto_analyze,
        scope_to_diff,
        explain,
    } = args;

    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    let resolved_config =
        hotspots_core::config::load_and_resolve(&repo_root, config_path.as_deref())
            .context("failed to load configuration")?;

    // When --base is omitted, auto-detect the merge-base against the repo's
    // default branch (same resolution order analyze's PR-delta mode uses).
    // Unlike that path, failure here is a hard error rather than a silent
    // fallback — `diff` has no implicit parent to fall back to, so silently
    // picking something would be more surprising than failing loudly.
    let base = match base {
        Some(base) => base,
        None => {
            let auto = git::resolve_merge_base_auto().ok_or_else(|| {
                anyhow::anyhow!(
                    "could not auto-detect a base ref (no merge-base found against \
                     main/master/develop/trunk). Pass --base explicitly, e.g. \
                     `hotspots diff <base> {head}`."
                )
            })?;
            eprintln!("Comparing {head} against auto-detected merge-base {auto}");
            auto
        }
    };

    // Resolve both refs to full SHAs
    let base_sha = git::resolve_ref_to_sha(&repo_root, &base)
        .with_context(|| format!("failed to resolve base ref '{base}'"))?;
    let head_sha = git::resolve_ref_to_sha(&repo_root, &head)
        .with_context(|| format!("failed to resolve head ref '{head}'"))?;

    // --scope-to-diff restricts auto-analysis to files the base..head diff
    // actually touches (benchmarked at ~50-70x less memory/time than a
    // full-repo walk on a 49-file PR). This only narrows which files get
    // parsed for the snapshot that's missing; it does not change delta
    // computation itself, and it is opt-in rather than the --auto-analyze
    // default because scoping a snapshot to a file subset is not yet
    // verified to always produce identical function-level results to a
    // full-repo snapshot (e.g. cross-file call-graph effects outside the
    // scoped set are not accounted for).
    let file_allowlist: Option<std::collections::HashSet<PathBuf>> = if scope_to_diff {
        let changed = git::diff_line_sets_at(&repo_root, &base_sha, &head_sha).unwrap_or_default();
        Some(changed.keys().map(PathBuf::from).collect())
    } else {
        None
    };

    // Load (or auto-analyze) both snapshots before bailing, so the user sees
    // all problems at once. Auto-analysis failures exit immediately with code 2
    // so CI can distinguish them from retriable "snapshot missing" conditions
    // (exit 3).
    let base_snapshot = load_snapshot_or_report(
        &repo_root,
        &base,
        &base_sha,
        auto_analyze,
        &resolved_config,
        file_allowlist.as_ref(),
    );
    let head_snapshot = load_snapshot_or_report(
        &repo_root,
        &head,
        &head_sha,
        auto_analyze,
        &resolved_config,
        file_allowlist.as_ref(),
    );

    let (base_snapshot, head_snapshot) = match (base_snapshot, head_snapshot) {
        (Ok(b), Ok(h)) => (b, h),
        (base_result, head_result) => {
            let mut any_failed = false;
            for result in [base_result, head_result] {
                match result {
                    Ok(_) => {}
                    Err(LoadError::Failed(msg)) => {
                        eprintln!("{msg}");
                        any_failed = true;
                    }
                    Err(LoadError::Missing(msg)) => {
                        eprintln!("{msg}");
                    }
                }
            }
            if any_failed {
                std::process::exit(2);
            }
            eprintln!("\nOnce both snapshots exist, re-run: hotspots diff {base} {head}");
            std::process::exit(3);
        }
    };

    // Compute delta
    let mut delta_val = Delta::new(&head_snapshot, Some(&base_snapshot))
        .context("failed to compute delta between snapshots")?;

    // Attach delta aggregates (file-level summaries used by HTML renderer)
    let current_co_change = head_snapshot
        .aggregates
        .as_ref()
        .map(|a| a.co_change.as_slice())
        .unwrap_or(&[]);
    let prev_co_change = base_snapshot
        .aggregates
        .as_ref()
        .map(|a| a.co_change.as_slice())
        .unwrap_or(&[]);
    // fn_changed_lines (diff-changed lines inside touched functions) is the strongest
    // tested predictor of PR defect risk in our evaluation, beating every
    // LRS-weighted composite tried, including pr_risk_score itself. Best-effort — an
    // empty/failed git diff (e.g. shallow clone) degrades to an empty line-set map, giving 0,
    // not an error, matching extract_commit_churn_at's convention.
    let diff_lines = git::diff_line_sets_at(&repo_root, &base_sha, &head_sha).unwrap_or_default();
    let fn_changed_lines = git::compute_fn_changed_lines(
        &repo_root,
        &delta_val.deltas,
        &head_snapshot,
        &base_snapshot,
        &diff_lines,
    );
    let current_file_risk = head_snapshot
        .aggregates
        .as_ref()
        .map(|a| a.file_risk.as_slice())
        .unwrap_or(&[]);
    delta_val.aggregates = Some(hotspots_core::aggregates::compute_delta_aggregates(
        &delta_val,
        current_co_change,
        prev_co_change,
        fn_changed_lines,
        current_file_risk,
    ));

    filter_and_truncate_to_top_n(&mut delta_val, top);

    // Evaluate policy if requested
    if policy {
        let policy_results = hotspots_core::policy::evaluate_policies(&delta_val, &resolved_config)
            .context("failed to evaluate policies")?;
        if let Some(results) = policy_results {
            delta_val.policy = Some(results);
        }
    }

    // Render output
    let has_blocking_failures = emit_diff_output(&delta_val, format, policy, output, explain)?;
    if has_blocking_failures {
        std::process::exit(1);
    }

    Ok(())
}

/// Filters out `Unchanged` entries, then — if `top` is set — excludes `Deleted`
/// entries entirely (they never rank, never count toward `n`) and truncates the
/// remaining `New`/`Modified` entries to the top `n` by risk magnitude.
///
/// Deleted entries are excluded from `--top N` rather than ranked alongside New
/// (by absolute LRS) because research (F293) found that ranking Deleted on the
/// same absolute-LRS scale as New dilutes recall of real regressions in the
/// truncated view.
fn filter_and_truncate_to_top_n(delta_val: &mut Delta, top: Option<usize>) {
    use hotspots_core::delta::FunctionStatus;
    delta_val
        .deltas
        .retain(|e| e.status != FunctionStatus::Unchanged);
    if let Some(n) = top {
        delta_val
            .deltas
            .retain(|e| e.status != FunctionStatus::Deleted);
        delta_val.deltas.sort_by(|a, b| {
            // New: rank by after.lrs; Modified: rank by |Δlrs|
            let score = |e: &hotspots_core::delta::FunctionDeltaEntry| match e.status {
                FunctionStatus::New => e.after.as_ref().map(|s| s.lrs).unwrap_or(0.0),
                FunctionStatus::Modified => e.delta.as_ref().map(|d| d.lrs.abs()).unwrap_or(0.0),
                FunctionStatus::Deleted | FunctionStatus::Unchanged => 0.0,
            };
            score(b)
                .partial_cmp(&score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        delta_val.deltas.truncate(n);
    }
}

/// Try to load a snapshot. When `auto_analyze` is true and the snapshot is
/// missing, runs a full analysis at `sha` and persists the result.
///
/// Returns [`LoadError::Missing`] when the snapshot does not exist and
/// auto-analyze is off, or [`LoadError::Failed`] when auto-analysis was
/// attempted but encountered a real execution error.
fn load_snapshot_or_report(
    repo_root: &std::path::Path,
    git_ref: &str,
    sha: &str,
    auto_analyze: bool,
    resolved_config: &hotspots_core::config::ResolvedConfig,
    file_allowlist: Option<&std::collections::HashSet<PathBuf>>,
) -> Result<hotspots_core::snapshot::Snapshot, LoadError> {
    match snapshot::load_snapshot(repo_root, sha) {
        Ok(Some(s)) => Ok(s),
        Ok(None) => {
            if auto_analyze {
                eprintln!("[hotspots] auto-analyzing '{}' ({})...", git_ref, &sha[..8]);
                crate::cmd::analyze::analyze_and_persist_at_ref_scoped(
                    repo_root,
                    sha,
                    resolved_config,
                    file_allowlist,
                )
                .map_err(|e| {
                    LoadError::Failed(format!(
                        "error: auto-analysis failed for '{git_ref}' ({}): {e}",
                        &sha[..8]
                    ))
                })
            } else {
                Err(LoadError::Missing(format!(
                    "error: no snapshot found for ref '{git_ref}' ({})\n  → run: git checkout {git_ref} && hotspots analyze --mode snapshot",
                    &sha[..8]
                )))
            }
        }
        Err(e) => Err(LoadError::Missing(format!(
            "error: failed to load snapshot for '{git_ref}' ({}): {e}",
            &sha[..8]
        ))),
    }
}

/// Render diff output. Returns true if there are blocking policy failures.
fn emit_diff_output(
    delta_val: &Delta,
    format: OutputFormat,
    with_policy: bool,
    output: Option<PathBuf>,
    explain: bool,
) -> anyhow::Result<bool> {
    let has_blocking_failures = delta_val
        .policy
        .as_ref()
        .map(|p| p.has_blocking_failures())
        .unwrap_or(false);

    match format {
        OutputFormat::Text => {
            let text = render_diff_text(delta_val, with_policy, explain)?;
            write_or_print(output, &text)?;
        }
        OutputFormat::Json => {
            let json = delta_val.to_json()?;
            write_or_print(output, &json)?;
        }
        OutputFormat::Html => {
            let html = hotspots_core::html::render_html_delta(delta_val, None);
            let output_path =
                output.unwrap_or_else(|| PathBuf::from(".hotspots/delta-report.html"));
            write_html_report(&output_path, &html)?;
            eprintln!("HTML report written to: {}", output_path.display());
        }
        OutputFormat::Sarif => {
            anyhow::bail!(
                "--format sarif is not supported for diff (use --format json or --format html)"
            );
        }
    }

    Ok(has_blocking_failures)
}

/// Write content to a file if `output` is Some, otherwise print to stdout.
fn write_or_print(output: Option<PathBuf>, content: &str) -> anyhow::Result<()> {
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory: {}", parent.display()))?;
            }
            std::fs::write(&path, content)
                .with_context(|| format!("failed to write output to {}", path.display()))?;
            eprintln!("Output written to: {}", path.display());
        }
        None => print!("{content}"),
    }
    Ok(())
}

fn render_diff_text(delta_val: &Delta, with_policy: bool, explain: bool) -> anyhow::Result<String> {
    use hotspots_core::delta::FunctionStatus;
    use std::fmt::Write;

    let mut out = String::new();

    // Exclude pure renames (identical before/after, linked via `rename_hint`) from
    // new/deleted counts, matching `aggregates.rs`'s `compute_pr_risk_summary` — see
    // `pure_rename_function_ids`'s doc comment. Without this, this text summary
    // disagreed with the aggregate/risk-score surface, which already nets these out.
    let pure_renames = hotspots_core::delta::pure_rename_function_ids(&delta_val.deltas);
    let new_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::New && !pure_renames.contains(&e.function_id))
        .count();
    let modified_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Modified)
        .count();
    let deleted_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Deleted && !pure_renames.contains(&e.function_id))
        .count();

    writeln!(
        out,
        "{modified_count} modified, {new_count} new, {deleted_count} deleted"
    )?;
    if let Some(summary) = delta_val.aggregates.as_ref().map(|a| &a.pr_summary) {
        writeln!(
            out,
            "Changed lines in touched functions: {} (strongest tested predictor of defect \
             risk in our evaluation) · highest band: {}",
            summary.fn_changed_lines,
            summary.band.as_str()
        )?;
    }
    writeln!(out, "{}", "=".repeat(100))?;

    if delta_val.deltas.is_empty() {
        writeln!(out, "No changes.")?;
        return Ok(out);
    }

    writeln!(
        out,
        "{:<12} {:<14}  {:<14}  {:<10} FILE:LINE FUNCTION",
        "STATUS", "LRS", "CC", "BAND"
    )?;
    writeln!(out, "{}", "-".repeat(100))?;

    // Files at or above the 90th percentile of file_risk_score among the files
    // changed in this diff (not repo-wide) — see `compute_diff_file_risk_ranks`.
    let top_decile_files: std::collections::HashSet<&str> = delta_val
        .aggregates
        .as_ref()
        .map(|a| {
            a.files
                .iter()
                .filter(|f| f.file_risk_top_decile)
                .map(|f| f.file.as_str())
                .collect()
        })
        .unwrap_or_default();

    for entry in &delta_val.deltas {
        if entry.status == FunctionStatus::Unchanged {
            continue;
        }
        write_diff_row(&mut out, entry, &top_decile_files, explain)?;
    }

    if with_policy {
        if let Some(ref policy_results) = delta_val.policy {
            writeln!(out)?;
            write!(
                out,
                "{}",
                crate::output::policy::render_policy_text_output(delta_val, policy_results)?
            )?;
        }
    }

    Ok(out)
}

/// Render one row of `render_diff_text`'s table, plus its optional
/// `--explain` line. Extracted so `render_diff_text` itself stays focused on
/// headline counts/header/policy, not per-row formatting.
/// Formats a `before → after` arrow string for any pair of displayable
/// values, collapsing the four before/after presence combinations that
/// `write_diff_row`'s LRS/CC columns both need into one place.
fn arrow<T: std::fmt::Display>(before: Option<T>, after: Option<T>) -> String {
    match (before, after) {
        (Some(b), Some(a)) => format!("{b} → {a}"),
        (None, Some(a)) => format!("— → {a}"),
        (Some(b), None) => format!("{b} → —"),
        (None, None) => "—".to_string(),
    }
}

fn band_column(entry: &hotspots_core::delta::FunctionDeltaEntry) -> String {
    match entry.band_transition.as_ref() {
        Some(t) => format!("{} → {}", t.from, t.to),
        None => entry
            .after
            .as_ref()
            .or(entry.before.as_ref())
            .map(|s| s.band.as_str().to_string())
            .unwrap_or_else(|| "-".to_string()),
    }
}

/// `file:line` plus function name, with `<anonymous>` disambiguated by line
/// (every closure in a file shares that literal name otherwise) and a
/// `[top 10%]` marker when the file is in this diff's top decile by risk.
fn file_and_function_columns(
    entry: &hotspots_core::delta::FunctionDeltaEntry,
    top_decile_files: &std::collections::HashSet<&str>,
) -> (String, String) {
    let (file_display, fn_display) = entry
        .function_id
        .split_once("::")
        .unwrap_or(("", &entry.function_id));
    let line = entry
        .after
        .as_ref()
        .or(entry.before.as_ref())
        .map(|s| s.line);

    let fn_display = match line {
        Some(line) if fn_display == "<anonymous>" => format!("<anonymous>@{line}"),
        _ => fn_display.to_string(),
    };

    let file_col = match line {
        Some(line) => format!("{file_display}:{line}"),
        None => file_display.to_string(),
    };
    let file_col = if top_decile_files.contains(file_display) {
        format!("{file_col} [top 10%]")
    } else {
        file_col
    };

    (file_col, fn_display)
}

fn write_diff_row(
    out: &mut String,
    entry: &hotspots_core::delta::FunctionDeltaEntry,
    top_decile_files: &std::collections::HashSet<&str>,
    explain: bool,
) -> anyhow::Result<()> {
    use hotspots_core::delta::FunctionStatus;
    use std::fmt::Write;

    let status_label = match entry.status {
        FunctionStatus::New => "new",
        FunctionStatus::Deleted => "deleted",
        FunctionStatus::Modified => "modified",
        FunctionStatus::Unchanged => return Ok(()),
    };

    let lrs_str = arrow(
        entry.before.as_ref().map(|b| format!("{:.2}", b.lrs)),
        entry.after.as_ref().map(|a| format!("{:.2}", a.lrs)),
    );
    let cc_str = arrow(
        entry.before.as_ref().map(|b| b.metrics.cc),
        entry.after.as_ref().map(|a| a.metrics.cc),
    );
    let band_str = band_column(entry);
    let (file_col, fn_display) = file_and_function_columns(entry, top_decile_files);

    // Full file:line and function name — never truncated, so a single row
    // always has enough information to jump straight to the code.
    writeln!(
        out,
        "{:<12} {:<14}  {:<14}  {:<10} {} {}",
        status_label, lrs_str, cc_str, band_str, file_col, fn_display,
    )?;

    if explain {
        if let Some(d) = &entry.delta {
            writeln!(
                out,
                "             \u{2726} {}",
                hotspots_core::delta::explain_delta(d)
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hotspots_core::delta::{
        DeltaCommitInfo, FunctionDeltaEntry, FunctionState, FunctionStatus,
    };
    use hotspots_core::report::MetricsReport;
    use hotspots_core::risk::RiskBand;

    fn state() -> FunctionState {
        FunctionState {
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 5,
                missing_docstring: false,
            },
            lrs: 5.4,
            band: RiskBand::Moderate,
            line: 1,
        }
    }

    #[test]
    fn test_render_diff_text_nets_out_pure_rename_from_headline_counts() {
        // Regression for #223: the headline "N modified, N new, N deleted" line
        // is computed independently from `aggregates.rs`'s PR-risk-summary path
        // (which already nets out pure renames) — without applying the same
        // `pure_rename_function_ids` filter here, this line disagreed with the
        // aggregate/risk-score surface for the exact same delta.
        let mut deleted = FunctionDeltaEntry {
            function_id: "old/path.go::Foo".to_string(),
            status: FunctionStatus::Deleted,
            before: Some(state()),
            after: None,
            delta: None,
            band_transition: None,
            suppression_reason: None,
            rename_hint: Some("new/path.go::Foo".to_string()),
        };
        deleted.before = Some(state());

        let created = FunctionDeltaEntry {
            function_id: "new/path.go::Foo".to_string(),
            status: FunctionStatus::New,
            before: None,
            after: Some(state()),
            delta: None,
            band_transition: None,
            suppression_reason: None,
            rename_hint: None,
        };

        let delta_val = Delta {
            commit: DeltaCommitInfo {
                sha: "head".to_string(),
                parent: "base".to_string(),
            },
            baseline: false,
            deltas: vec![deleted, created],
            policy: None,
            aggregates: None,
        };

        let text = render_diff_text(&delta_val, false, false).expect("render");
        assert!(
            text.starts_with("0 modified, 0 new, 0 deleted"),
            "pure rename should net to zero, got: {text}"
        );
    }

    #[test]
    fn test_top_n_excludes_deleted_in_favor_of_modified_regression() {
        // F293: a Deleted entry with a high absolute pre-deletion LRS used to
        // rank alongside New/Modified by that same absolute scale, potentially
        // crowding out a real (lower-magnitude) Modified regression out of a
        // small --top N. Deleted should never win a slot now.
        let mut high_lrs_deleted = state();
        high_lrs_deleted.lrs = 50.0;
        let deleted = FunctionDeltaEntry {
            function_id: "old/path.go::BigDeletedFn".to_string(),
            status: FunctionStatus::Deleted,
            before: Some(high_lrs_deleted),
            after: None,
            delta: None,
            band_transition: None,
            suppression_reason: None,
            rename_hint: None,
        };

        let small_delta = hotspots_core::delta::FunctionDelta {
            cc: 1,
            nd: 0,
            fo: 0,
            ns: 1,
            lrs: 0.5,
        };
        let modified = FunctionDeltaEntry {
            function_id: "src/real.go::RealRegression".to_string(),
            status: FunctionStatus::Modified,
            before: Some(state()),
            after: Some(state()),
            delta: Some(small_delta),
            band_transition: None,
            suppression_reason: None,
            rename_hint: None,
        };

        let mut delta_val = Delta {
            commit: DeltaCommitInfo {
                sha: "head".to_string(),
                parent: "base".to_string(),
            },
            baseline: false,
            deltas: vec![deleted, modified],
            policy: None,
            aggregates: None,
        };

        filter_and_truncate_to_top_n(&mut delta_val, Some(1));

        assert_eq!(delta_val.deltas.len(), 1);
        assert_eq!(delta_val.deltas[0].status, FunctionStatus::Modified);
        assert_eq!(
            delta_val.deltas[0].function_id,
            "src/real.go::RealRegression"
        );
    }
}
