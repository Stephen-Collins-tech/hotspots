//! `hotspots estimate` (#187): a cheap pre-flight projection of `analyze
//! --touch-mode per-function`'s wall-clock cost, so callers (e.g. hotspots-cloud)
//! can pick a touch mode before committing to a full run instead of guessing from
//! `size_kb` — see the #163 spike's findings for why size is a weak proxy.
//!
//! Two tiers:
//! - **Tier 0** (near-instant, no parsing): file discovery + a raw newline count
//!   per file. Always completes fast, even on repos where Tier 1 itself is slow
//!   (e.g. golang/go in the #163 spike, where even the touch-free structural pass
//!   didn't finish in 500s+).
//! - **Tier 1** (the real `--touch-mode none` structural pass — parsing + CFG +
//!   call graph, but zero git-log calls): gives the real `function_count` per
//!   language. Run on a background thread with a timeout so Tier 0's report is
//!   never blocked by it.
//!
//! Per-language ms/function calibration is seeded from the #163 spike's measured
//! values (7 repos, 6 languages) — directional, not a fitted regression; refine
//! once hotspots-cloud logs real duration/function_count/language per run.

use crate::OutputFormat;
use anyhow::Context;
use hotspots_core::language::Language;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(crate) struct EstimateArgs {
    pub path: PathBuf,
    pub format: OutputFormat,
    pub config_path: Option<PathBuf>,
    pub budget_seconds: Option<u64>,
    pub tier1_timeout_seconds: u64,
}

pub(crate) fn validate_estimate_flags(format: OutputFormat) -> anyhow::Result<()> {
    if !matches!(format, OutputFormat::Text | OutputFormat::Json) {
        anyhow::bail!("hotspots estimate only supports --format text or --format json");
    }
    Ok(())
}

/// ms/function for `--touch-mode per-function`, seeded from the #163 spike
/// (django/react/jadx/svelte/gin/redis/iced-rs, 2026-09-07). Directional, not a
/// fitted model — see docs/REFERENCE.md.
fn ms_per_function(lang: Language) -> f64 {
    match lang {
        Language::Python => 16.90,
        Language::Go => 10.35,
        Language::C | Language::CHeader => 10.01,
        Language::Rust => 6.58,
        Language::Java => 5.45,
        Language::JavaScript
        | Language::JavaScriptReact
        | Language::TypeScript
        | Language::TypeScriptReact
        | Language::Vue => 3.75,
        // No spike measurement for C#; use the mean of measured languages.
        Language::CSharp => DEFAULT_MS_PER_FUNCTION,
    }
}

/// Mean ms/function across the #163 spike's 6 measured languages, used for any
/// language the spike didn't cover.
const DEFAULT_MS_PER_FUNCTION: f64 = 8.84;

#[derive(Default, Serialize)]
struct LangCounts {
    files: usize,
    loc: u64,
    functions: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct EstimateReport {
    path: String,
    tier0_file_count: usize,
    tier0_total_loc: u64,
    tier1_completed: bool,
    tier1_elapsed_seconds: f64,
    by_language: BTreeMap<String, LangCounts>,
    projected_per_function_seconds: Option<f64>,
    budget_seconds: Option<u64>,
    recommendation: Option<String>,
}

fn count_newlines(path: &Path) -> u64 {
    match std::fs::read(path) {
        Ok(bytes) => bytes.iter().filter(|&&b| b == b'\n').count() as u64,
        Err(_) => 0,
    }
}

pub(crate) fn handle_estimate(args: EstimateArgs) -> anyhow::Result<()> {
    validate_estimate_flags(args.format)?;

    let normalized_path = if args.path.is_relative() {
        std::env::current_dir()?.join(&args.path)
    } else {
        args.path.clone()
    };
    if !normalized_path.exists() {
        anyhow::bail!("Path does not exist: {}", normalized_path.display());
    }

    let project_root =
        crate::util::find_repo_root(&normalized_path).unwrap_or_else(|_| normalized_path.clone());
    let resolved_config =
        hotspots_core::config::load_and_resolve(&project_root, args.config_path.as_deref())
            .context("failed to load configuration")?;

    // --- Tier 0: near-instant file discovery + raw LOC, no parsing ---
    let tier0_start = Instant::now();
    let files: Vec<PathBuf> = hotspots_core::collect_source_files(&normalized_path)?
        .into_iter()
        .filter(|f| resolved_config.should_include(f))
        .collect();

    let mut by_language: BTreeMap<String, LangCounts> = BTreeMap::new();
    let mut total_loc: u64 = 0;
    for f in &files {
        let lang = f
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Language::from_extension);
        let Some(lang) = lang else { continue };
        let loc = count_newlines(f);
        total_loc += loc;
        let entry = by_language.entry(format!("{lang:?}")).or_default();
        entry.files += 1;
        entry.loc += loc;
    }
    let tier0_elapsed = tier0_start.elapsed();
    eprintln!(
        "Tier 0: {} files, {} total lines ({:.2}s)",
        files.len(),
        total_loc,
        tier0_elapsed.as_secs_f64()
    );

    // --- Tier 1: the real structural pass, backgrounded with a timeout so Tier 0's
    // report is never held hostage by a repo where even this pass is slow. ---
    let tier1_start = Instant::now();
    let (tx, rx) = mpsc::channel();
    let path_for_thread = normalized_path.clone();
    std::thread::spawn(move || {
        let result = hotspots_core::analyze_with_progress(
            &path_for_thread,
            hotspots_core::AnalysisOptions {
                min_lrs: None,
                top_n: None,
            },
            None,
            None,
        );
        // Ignore send errors: the receiver may have already timed out and moved on.
        let _ = tx.send(result);
    });

    let tier1_result = rx.recv_timeout(Duration::from_secs(args.tier1_timeout_seconds));
    let tier1_elapsed = tier1_start.elapsed();

    let mut projected_per_function_seconds = None;
    let tier1_completed = match tier1_result {
        Ok(Ok(reports)) => {
            let mut per_lang_functions: BTreeMap<String, usize> = BTreeMap::new();
            for r in &reports {
                *per_lang_functions
                    .entry(format!("{:?}", r.language))
                    .or_insert(0) += 1;
            }
            let mut projected_ms = 0.0;
            for (lang_name, count) in &per_lang_functions {
                let entry = by_language.entry(lang_name.clone()).or_default();
                entry.functions = Some(*count);
                let lang = Language::from_extension(lang_extension_hint(lang_name));
                let per_fn_ms = lang.map(ms_per_function).unwrap_or(DEFAULT_MS_PER_FUNCTION);
                projected_ms += *count as f64 * per_fn_ms;
            }
            projected_per_function_seconds = Some(projected_ms / 1000.0);
            eprintln!(
                "Tier 1: {} functions across {} files ({:.2}s)",
                reports.len(),
                files.len(),
                tier1_elapsed.as_secs_f64()
            );
            true
        }
        Ok(Err(e)) => {
            eprintln!("Tier 1: structural pass failed: {e}");
            false
        }
        Err(_) => {
            eprintln!(
                "Tier 1: did not complete within {}s — projection based on Tier 0 file/LOC counts only, no reliable per-function-touches estimate available",
                args.tier1_timeout_seconds
            );
            false
        }
    };

    let recommendation = args.budget_seconds.and_then(|budget| {
        projected_per_function_seconds.map(|projected| {
            if projected <= budget as f64 {
                format!(
                    "--touch-mode per-function projected at {projected:.0}s, within the {budget}s budget — safe to use"
                )
            } else {
                format!(
                    "--touch-mode per-function projected at {projected:.0}s, over the {budget}s budget — use --touch-mode hybrid or --touch-mode file instead"
                )
            }
        })
    });

    let report = EstimateReport {
        path: normalized_path.display().to_string(),
        tier0_file_count: files.len(),
        tier0_total_loc: total_loc,
        tier1_completed,
        tier1_elapsed_seconds: tier1_elapsed.as_secs_f64(),
        by_language,
        projected_per_function_seconds,
        budget_seconds: args.budget_seconds,
        recommendation,
    };

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => print_text_report(&report),
    }

    Ok(())
}

/// `by_language` keys are `{lang:?}` Debug strings (e.g. "Python", "JavaScript");
/// map back to a representative extension so `ms_per_function` can look the
/// calibration up via `Language::from_extension`, rather than duplicating the
/// match arms against Debug-formatted names.
fn lang_extension_hint(lang_debug_name: &str) -> &'static str {
    match lang_debug_name {
        "TypeScript" => "ts",
        "TypeScriptReact" => "tsx",
        "JavaScript" => "js",
        "JavaScriptReact" => "jsx",
        "Go" => "go",
        "Java" => "java",
        "Python" => "py",
        "Rust" => "rs",
        "Vue" => "vue",
        "CSharp" => "cs",
        "C" => "c",
        "CHeader" => "h",
        _ => "",
    }
}

fn print_text_report(report: &EstimateReport) {
    println!("Runtime estimate for {}", report.path);
    println!();
    println!(
        "Tier 0 (file discovery, no parsing): {} files, {} lines",
        report.tier0_file_count, report.tier0_total_loc
    );
    println!();
    println!("By language:");
    for (lang, counts) in &report.by_language {
        match counts.functions {
            Some(n) => println!(
                "  {lang:<18} {:>6} files  {:>10} lines  {:>8} functions",
                counts.files, counts.loc, n
            ),
            None => println!(
                "  {lang:<18} {:>6} files  {:>10} lines  {:>8}",
                counts.files, counts.loc, "?"
            ),
        }
    }
    println!();
    if !report.tier1_completed {
        println!(
            "Tier 1 structural pass did not complete within the timeout ({:.0}s elapsed) — no reliable --touch-mode per-function projection available. Consider --touch-mode hybrid or --touch-mode file for a repo this size/shape.",
            report.tier1_elapsed_seconds
        );
        return;
    }
    if let Some(seconds) = report.projected_per_function_seconds {
        println!("Projected --touch-mode per-function wall time: ~{seconds:.0}s");
    }
    if let Some(rec) = &report.recommendation {
        println!();
        println!("{rec}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ms_per_function_covers_all_spike_measured_languages() {
        // The 6 languages actually measured in the #163 spike.
        assert_eq!(ms_per_function(Language::Python), 16.90);
        assert_eq!(ms_per_function(Language::Go), 10.35);
        assert_eq!(ms_per_function(Language::C), 10.01);
        assert_eq!(ms_per_function(Language::Rust), 6.58);
        assert_eq!(ms_per_function(Language::Java), 5.45);
        assert_eq!(ms_per_function(Language::JavaScript), 3.75);
    }

    #[test]
    fn ms_per_function_unmeasured_language_falls_back_to_default() {
        assert_eq!(ms_per_function(Language::CSharp), DEFAULT_MS_PER_FUNCTION);
    }

    #[test]
    fn lang_extension_hint_round_trips_through_from_extension() {
        for (debug_name, expected) in [
            ("Python", Language::Python),
            ("Go", Language::Go),
            ("Rust", Language::Rust),
            ("Java", Language::Java),
            ("JavaScript", Language::JavaScript),
            ("TypeScript", Language::TypeScript),
            ("C", Language::C),
        ] {
            let ext = lang_extension_hint(debug_name);
            assert_eq!(
                Language::from_extension(ext),
                Some(expected),
                "{debug_name}"
            );
        }
    }

    #[test]
    fn validate_estimate_flags_rejects_non_text_json() {
        assert!(validate_estimate_flags(OutputFormat::Text).is_ok());
        assert!(validate_estimate_flags(OutputFormat::Json).is_ok());
        assert!(validate_estimate_flags(OutputFormat::Html).is_err());
        assert!(validate_estimate_flags(OutputFormat::Sarif).is_err());
    }

    /// A small repo: Tier 0 and Tier 1 both complete well within a generous timeout.
    #[test]
    fn small_repo_all_tiers_succeed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("main.rs"),
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )
        .unwrap();

        let args = EstimateArgs {
            path: dir.path().to_path_buf(),
            format: OutputFormat::Json,
            config_path: None,
            budget_seconds: Some(60),
            tier1_timeout_seconds: 30,
        };
        // handle_estimate prints instead of returning the report; exercise the
        // pieces it composes directly so the test doesn't depend on stdout capture.
        let files = hotspots_core::collect_source_files(&args.path).unwrap();
        assert_eq!(files.len(), 1);
        let result = hotspots_core::analyze_with_progress(
            &args.path,
            hotspots_core::AnalysisOptions {
                min_lrs: None,
                top_n: None,
            },
            None,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    /// Tier 0 must terminate quickly even when the Tier 1 budget is effectively
    /// zero (the "Tier 1 itself would be slow" case from #187's acceptance
    /// criteria) — this exercises the recv_timeout path returning promptly rather
    /// than blocking on the spawned analysis thread.
    #[test]
    fn tier0_terminates_quickly_when_tier1_budget_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn f() {}\n").unwrap();

        let start = Instant::now();
        let files = hotspots_core::collect_source_files(dir.path()).unwrap();
        let tier0_elapsed = start.elapsed();
        assert_eq!(files.len(), 1);
        assert!(tier0_elapsed.as_secs_f64() < 1.0);

        let (tx, rx) = mpsc::channel();
        let path = dir.path().to_path_buf();
        std::thread::spawn(move || {
            let result = hotspots_core::analyze_with_progress(
                &path,
                hotspots_core::AnalysisOptions {
                    min_lrs: None,
                    top_n: None,
                },
                None,
                None,
            );
            let _ = tx.send(result);
        });
        let recv_start = Instant::now();
        let _ = rx.recv_timeout(Duration::from_secs(0));
        assert!(recv_start.elapsed().as_secs_f64() < 1.0);
    }
}
