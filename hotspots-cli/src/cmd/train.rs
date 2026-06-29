//! `hotspots train` — fit a local RandomForest ranker from git history.

use anyhow::{bail, Context, Result};
use hotspots_core::snapshot::{index_path, load_snapshot, Snapshot};
use hotspots_core::trainer::{
    collect_fix_files, precision_at_k, score, screen_repo, train, FunctionId, RankerModel,
    ScoredFunction, ScreenerVerdict, TrainConfig,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) struct TrainArgs {
    pub path: PathBuf,
    pub output: PathBuf,
    pub label_window_days: u32,
    pub label_before: Option<String>,
    pub n_estimators: usize,
    pub max_depth: usize,
    pub blame_labels: bool,
    pub eval: bool,
    pub screen: bool,
    pub yes: bool,
    pub quiet: bool,
}

pub(crate) fn handle_train(args: TrainArgs) -> Result<()> {
    let repo_root = args.path.canonicalize().context("resolve repo path")?;
    let snapshot = load_latest_snapshot(&repo_root)?;

    let output = if args.output.is_relative() {
        repo_root.join(&args.output)
    } else {
        args.output.clone()
    };
    let args = TrainArgs { output, ..args };

    let cfg = TrainConfig {
        label_window_days: args.label_window_days,
        label_before: args.label_before.clone(),
        n_estimators: args.n_estimators,
        max_depth: args.max_depth,
        blame_labels: args.blame_labels,
        ..Default::default()
    };

    if args.screen {
        let (verdict, mean_hs) = screen_repo(&snapshot);
        match verdict {
            ScreenerVerdict::SkipFt => {
                bail!(
                    "Screener: SKIP_FT (mean_hotspots_score={mean_hs:.4}). Use tabular ranking instead."
                );
            }
            ScreenerVerdict::Ambiguous => {
                eprintln!(
                    "Screener: AMBIGUOUS (mean_hotspots_score={mean_hs:.4}). Training may not beat tabular baseline."
                );
            }
            ScreenerVerdict::RunFt => {}
        }
    }

    let n_funcs = snapshot.functions.len();
    let label_mode = if cfg.blame_labels {
        "blame-based function labels"
    } else {
        "file-level labels"
    };

    let est = estimate_duration(n_funcs, cfg.n_estimators);
    eprintln!(
        "hotspots train: {} functions · {} trees · {} days of git history ({}) · estimated {}",
        n_funcs, cfg.n_estimators, cfg.label_window_days, label_mode, est,
    );

    if n_funcs > 1_000 && !args.yes {
        eprint!("Proceed? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => {}
            _ => bail!("Aborted. Pass --yes to skip this prompt."),
        }
    }

    let quiet = args.quiet;
    let start = Instant::now();

    // Sliding-window ETA: track (done, elapsed_secs) at each report point,
    // compute per-tree rate from the last WINDOW_SIZE intervals only.
    const WINDOW_SIZE: usize = 5; // intervals of 10 trees each = last 50 trees
                                  // Seed with (0, 0.0) so the first interval has a prior point to diff against.
    let report_points: Arc<Mutex<Vec<(u32, f64)>>> = Arc::new(Mutex::new(vec![(0, 0.0)]));
    let last_report = Arc::new(Mutex::new(0u32));
    let cb_start = start;
    let cb_points = Arc::clone(&report_points);
    let cb_last = Arc::clone(&last_report);

    let on_tree = move |done: u32, total: u32| {
        if quiet {
            return;
        }
        // Report every 10 trees, and on the last one
        let mut last = cb_last.lock().unwrap();
        if done == total || done - *last >= 10 {
            let elapsed = cb_start.elapsed().as_secs_f64();
            let mut points = cb_points.lock().unwrap();
            points.push((done, elapsed));

            let eta_str = if done == total {
                String::new()
            } else {
                // Use the last WINDOW_SIZE intervals to estimate per-tree time
                let n = points.len();
                let window_start = n.saturating_sub(WINDOW_SIZE);
                let (d0, t0) = points[window_start];
                let (d1, t1) = points[n - 1];
                let trees_in_window = (d1 - d0) as f64;
                let secs_in_window = t1 - t0;
                let per_tree = if trees_in_window > 0.0 {
                    secs_in_window / trees_in_window
                } else {
                    elapsed / done as f64
                };
                let remaining_secs = per_tree * (total - done) as f64;
                format!("  ~{} remaining", fmt_duration(remaining_secs as u64))
            };

            *last = done;
            eprintln!("  [{}/{}]{} ", done, total, eta_str);
        }
    };

    match train(&snapshot, &repo_root, &cfg, Some(&on_tree))? {
        None => {
            bail!(
                "Not enough training signal: need ≥50 labelled functions, ≥5 positive and ≥10 negative. \
                 Try a larger --label-window or run `hotspots analyze` first to build a snapshot."
            );
        }
        Some(model) => {
            let elapsed = start.elapsed();
            report_model(&model, n_funcs, elapsed.as_secs());
            model.save(&args.output)?;
            eprintln!("Model saved → {}", args.output.display());
            if args.eval {
                run_eval(
                    &model,
                    &snapshot,
                    &repo_root,
                    args.label_window_days,
                    args.label_before.as_deref(),
                )?;
            }
        }
    }

    Ok(())
}

fn estimate_duration(n_funcs: usize, n_estimators: usize) -> String {
    // Empirical: ~4.5 min for 12,914 funcs × 200 trees. Scale linearly.
    let base_secs = 270.0_f64;
    let base_funcs = 12_914.0_f64;
    let base_trees = 200.0_f64;
    let est_secs = base_secs * (n_funcs as f64 / base_funcs) * (n_estimators as f64 / base_trees);
    if est_secs < 30.0 {
        "< 30s".to_string()
    } else {
        format!("~{}", fmt_duration(est_secs as u64))
    }
}

fn fmt_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    }
}

fn run_eval(
    model: &RankerModel,
    snapshot: &Snapshot,
    repo_root: &Path,
    label_window_days: u32,
    label_before: Option<&str>,
) -> Result<()> {
    let fix_files = collect_fix_files(repo_root, label_window_days, label_before)?;
    let base_rate = snapshot.functions.len() as f64;

    let labels: std::collections::HashMap<FunctionId, bool> = snapshot
        .functions
        .iter()
        .map(|f| {
            let file_norm = f.file.replace('\\', "/");
            let is_pos = fix_files.contains(&file_norm)
                || fix_files.iter().any(|ff| file_norm.ends_with(ff.as_str()));
            (f.function_id.clone(), is_pos)
        })
        .collect();

    let n_pos = labels.values().filter(|&&v| v).count();
    let base_rate = n_pos as f64 / base_rate;

    let mut ranked: Vec<ScoredFunction> = snapshot
        .functions
        .iter()
        .map(|f| ScoredFunction {
            function_id: f.function_id.clone(),
            score: score(model, f),
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("\nP@K evaluation ({label_window_days}-day fix-label window):");
    println!("  {:<6} {:<8} base_rate", "K", "P@K");
    for k in [10, 20, 50, 100, 200] {
        let pak = precision_at_k(&ranked, &labels, k);
        println!("  {k:<6} {pak:<8.3} {base_rate:.3}");
    }

    Ok(())
}

fn load_latest_snapshot(repo_root: &Path) -> Result<Snapshot> {
    let idx_path = index_path(repo_root);
    if !idx_path.exists() {
        bail!(
            "No snapshot index found at {}. Run `hotspots analyze .` first.",
            idx_path.display()
        );
    }

    let json = std::fs::read_to_string(&idx_path)
        .with_context(|| format!("read {}", idx_path.display()))?;
    let index = hotspots_core::snapshot::Index::from_json(&json).context("parse index")?;

    let entry = index
        .commits
        .last()
        .context("snapshot index is empty — run `hotspots analyze .` first")?;

    let sha = entry.sha.clone();
    let snapshot = load_snapshot(repo_root, &sha)
        .context("load snapshot")?
        .with_context(|| format!("snapshot {} not found on disk", sha))?;

    eprintln!("Loaded snapshot {}", &sha[..8.min(sha.len())]);
    Ok(snapshot)
}

fn report_model(model: &RankerModel, n_funcs: usize, elapsed_secs: u64) {
    let m = &model.meta;
    let base_rate = m.n_pos as f64 / m.n_samples as f64;
    eprintln!(
        "Trained: {} trees × depth {} | {} samples ({} pos, {} neg) | base rate {:.1}% | {} functions in repo | elapsed {}",
        m.n_estimators, m.max_depth,
        m.n_samples, m.n_pos, m.n_neg,
        base_rate * 100.0,
        n_funcs,
        fmt_duration(elapsed_secs),
    );
}
