use crate::util::{find_repo_root, write_html_report};
use crate::OutputFormat;
use anyhow::Context;
use hotspots_core::delta::Delta;
use hotspots_core::git;
use hotspots_core::snapshot;
use std::path::PathBuf;

pub(crate) struct DiffArgs {
    pub base: String,
    pub head: String,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub policy: bool,
    pub top: Option<usize>,
    pub config_path: Option<PathBuf>,
    pub auto_analyze: bool,
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
    } = args;

    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    // Resolve both refs to full SHAs
    let base_sha = git::resolve_ref_to_sha(&repo_root, &base)
        .with_context(|| format!("failed to resolve base ref '{base}'"))?;
    let head_sha = git::resolve_ref_to_sha(&repo_root, &head)
        .with_context(|| format!("failed to resolve head ref '{head}'"))?;

    // Check both snapshots exist before bailing, so the user sees all missing refs at once
    let base_snapshot = load_snapshot_or_report(&repo_root, &base, &base_sha, auto_analyze);
    let head_snapshot = load_snapshot_or_report(&repo_root, &head, &head_sha, auto_analyze);

    let (base_snapshot, head_snapshot) = match (base_snapshot, head_snapshot) {
        (Ok(b), Ok(h)) => (b, h),
        (base_result, head_result) => {
            // Print errors for any missing snapshots, then exit 3
            if let Err(e) = base_result {
                eprintln!("{e}");
            }
            if let Err(e) = head_result {
                eprintln!("{e}");
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
    delta_val.aggregates = Some(hotspots_core::aggregates::compute_delta_aggregates(
        &delta_val,
        current_co_change,
        prev_co_change,
    ));

    // Filter out Unchanged, then optionally keep top N by risk magnitude
    {
        use hotspots_core::delta::FunctionStatus;
        delta_val
            .deltas
            .retain(|e| e.status != FunctionStatus::Unchanged);
    }
    if let Some(n) = top {
        use hotspots_core::delta::FunctionStatus;
        delta_val.deltas.sort_by(|a, b| {
            // New: rank by after.lrs; Deleted: rank by before.lrs; Modified: rank by |Δlrs|
            let score = |e: &hotspots_core::delta::FunctionDeltaEntry| match e.status {
                FunctionStatus::New => e.after.as_ref().map(|s| s.lrs).unwrap_or(0.0),
                FunctionStatus::Deleted => e.before.as_ref().map(|s| s.lrs).unwrap_or(0.0),
                FunctionStatus::Modified => e.delta.as_ref().map(|d| d.lrs.abs()).unwrap_or(0.0),
                FunctionStatus::Unchanged => 0.0,
            };
            score(b)
                .partial_cmp(&score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        delta_val.deltas.truncate(n);
    }

    // Evaluate policy if requested
    if policy {
        let resolved_config =
            hotspots_core::config::load_and_resolve(&repo_root, config_path.as_deref())
                .context("failed to load configuration")?;
        let policy_results = hotspots_core::policy::evaluate_policies(
            &delta_val,
            &head_snapshot,
            &repo_root,
            &resolved_config,
        )
        .context("failed to evaluate policies")?;
        if let Some(results) = policy_results {
            delta_val.policy = Some(results);
        }
    }

    // Render output
    let has_blocking_failures = emit_diff_output(&delta_val, format, policy, output)?;
    if has_blocking_failures {
        std::process::exit(1);
    }

    Ok(())
}

/// Try to load a snapshot, returning a descriptive Err string if missing.
/// Calls process::exit on auto_analyze (not yet implemented).
fn load_snapshot_or_report(
    repo_root: &std::path::Path,
    git_ref: &str,
    sha: &str,
    auto_analyze: bool,
) -> Result<hotspots_core::snapshot::Snapshot, String> {
    match snapshot::load_snapshot(repo_root, sha) {
        Ok(Some(s)) => Ok(s),
        Ok(None) => {
            if auto_analyze {
                eprintln!(
                    "[hotspots] --auto-analyze: no snapshot for '{git_ref}' ({}) — auto-analysis not yet implemented",
                    &sha[..8]
                );
                eprintln!("  → run: git checkout {git_ref} && hotspots analyze --mode snapshot");
                std::process::exit(2);
            }
            Err(format!(
                "error: no snapshot found for ref '{git_ref}' ({})\n  → run: git checkout {git_ref} && hotspots analyze --mode snapshot",
                &sha[..8]
            ))
        }
        Err(e) => Err(format!(
            "error: failed to load snapshot for '{git_ref}' ({}): {e}",
            &sha[..8]
        )),
    }
}

/// Render diff output. Returns true if there are blocking policy failures.
fn emit_diff_output(
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
        OutputFormat::Text => {
            let text = render_diff_text(delta_val, with_policy)?;
            write_or_print(output, &text)?;
        }
        OutputFormat::Json => {
            let json = delta_val.to_json()?;
            write_or_print(output, &json)?;
        }
        OutputFormat::Jsonl => {
            let jsonl = delta_val.to_jsonl()?;
            write_or_print(output, &jsonl)?;
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

fn render_diff_text(delta_val: &Delta, with_policy: bool) -> anyhow::Result<String> {
    use hotspots_core::delta::FunctionStatus;
    use std::fmt::Write;

    let mut out = String::new();

    let new_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::New)
        .count();
    let modified_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Modified)
        .count();
    let deleted_count = delta_val
        .deltas
        .iter()
        .filter(|e| e.status == FunctionStatus::Deleted)
        .count();

    writeln!(
        out,
        "{modified_count} modified, {new_count} new, {deleted_count} deleted"
    )?;
    writeln!(out, "{}", "=".repeat(100))?;

    if delta_val.deltas.is_empty() {
        writeln!(out, "No changes.")?;
        return Ok(out);
    }

    writeln!(
        out,
        "{:<12} {:<40} {:<30} {:<14}  {:<14}  BAND",
        "STATUS", "FUNCTION", "FILE", "LRS", "CC"
    )?;
    writeln!(out, "{}", "-".repeat(100))?;

    for entry in &delta_val.deltas {
        let status_label = match entry.status {
            FunctionStatus::New => "new",
            FunctionStatus::Deleted => "deleted",
            FunctionStatus::Modified => "modified",
            FunctionStatus::Unchanged => continue,
        };

        let lrs_str = match (&entry.before, &entry.after) {
            (Some(b), Some(a)) => format!("{:.2} → {:.2}", b.lrs, a.lrs),
            (None, Some(a)) => format!("— → {:.2}", a.lrs),
            (Some(b), None) => format!("{:.2} → —", b.lrs),
            (None, None) => "—".to_string(),
        };

        let cc_str = match (&entry.before, &entry.after) {
            (Some(b), Some(a)) => format!("{} → {}", b.metrics.cc, a.metrics.cc),
            (None, Some(a)) => format!("— → {}", a.metrics.cc),
            (Some(b), None) => format!("{} → —", b.metrics.cc),
            (None, None) => "—".to_string(),
        };

        let band_str = match entry.band_transition.as_ref() {
            Some(t) => format!("{} → {}", t.from, t.to),
            None => entry
                .after
                .as_ref()
                .or(entry.before.as_ref())
                .map(|s| s.band.clone())
                .unwrap_or_default(),
        };

        // function_id is "file::function"; split for display
        let (file_display, fn_display) = entry
            .function_id
            .split_once("::")
            .unwrap_or(("", &entry.function_id));

        writeln!(
            out,
            "{:<12} {:<40} {:<30} {:<14}  {:<14}  {}",
            status_label,
            crate::util::truncate_string(fn_display, 40),
            crate::util::truncate_string(file_display, 30),
            lrs_str,
            cc_str,
            band_str,
        )?;
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
