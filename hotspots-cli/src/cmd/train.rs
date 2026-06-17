//! `hotspots train` — fit a local RandomForest ranker from git history.

use anyhow::{bail, Context, Result};
use hotspots_core::snapshot::{index_path, load_snapshot, Snapshot};
use hotspots_core::trainer::{
    collect_fix_files, precision_at_k, score, screen_repo, train, FunctionId, RankerModel,
    ScoredFunction, ScreenerVerdict, TrainConfig,
};
use std::path::{Path, PathBuf};

pub(crate) struct TrainArgs {
    pub path: PathBuf,
    pub output: PathBuf,
    pub label_window_days: u32,
    pub n_estimators: usize,
    pub max_depth: usize,
    pub blame_labels: bool,
    pub eval: bool,
    pub screen: bool,
}

pub(crate) fn handle_train(args: TrainArgs) -> Result<()> {
    let repo_root = args.path.canonicalize().context("resolve repo path")?;
    let snapshot = load_latest_snapshot(&repo_root)?;

    // Resolve relative --output against repo_root, not CWD.
    let output = if args.output.is_relative() {
        repo_root.join(&args.output)
    } else {
        args.output.clone()
    };
    let args = TrainArgs { output, ..args };

    let cfg = TrainConfig {
        label_window_days: args.label_window_days,
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
    eprintln!(
        "hotspots train: {} functions in snapshot, scanning {} days of git history ({})…",
        n_funcs, cfg.label_window_days, label_mode
    );

    match train(&snapshot, &repo_root, &cfg)? {
        None => {
            bail!(
                "Not enough training signal: need ≥50 labelled functions, ≥5 positive and ≥10 negative. \
                 Try a larger --label-window or run `hotspots analyze` first to build a snapshot."
            );
        }
        Some(model) => {
            report_model(&model, n_funcs);
            model.save(&args.output)?;
            eprintln!("Model saved → {}", args.output.display());
            if args.eval {
                run_eval(&model, &snapshot, &repo_root, args.label_window_days)?;
            }
        }
    }

    Ok(())
}

fn run_eval(
    model: &RankerModel,
    snapshot: &Snapshot,
    repo_root: &Path,
    label_window_days: u32,
) -> Result<()> {
    let fix_files = collect_fix_files(repo_root, label_window_days)?;
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

fn report_model(model: &RankerModel, n_funcs: usize) {
    let m = &model.meta;
    let base_rate = m.n_pos as f64 / m.n_samples as f64;
    eprintln!(
        "Trained: {} trees × depth {} | {} samples ({} pos, {} neg) | base rate {:.1}% | {} functions in repo",
        m.n_estimators, m.max_depth,
        m.n_samples, m.n_pos, m.n_neg,
        base_rate * 100.0,
        n_funcs,
    );
}
