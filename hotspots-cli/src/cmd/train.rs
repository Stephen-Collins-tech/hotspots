//! `hotspots train` — fit a local RandomForest ranker from git history.

use anyhow::{bail, Context, Result};
use hotspots_core::snapshot::{index_path, load_snapshot, Snapshot};
use hotspots_core::trainer::{train, RankerModel, TrainConfig};
use std::path::{Path, PathBuf};

pub(crate) struct TrainArgs {
    pub path: PathBuf,
    pub output: PathBuf,
    pub label_window_days: u32,
    pub n_estimators: usize,
    pub max_depth: usize,
}

pub(crate) fn handle_train(args: TrainArgs) -> Result<()> {
    let repo_root = args.path.canonicalize().context("resolve repo path")?;
    let snapshot = load_latest_snapshot(&repo_root)?;

    let cfg = TrainConfig {
        label_window_days: args.label_window_days,
        n_estimators: args.n_estimators,
        max_depth: args.max_depth,
        ..Default::default()
    };

    let n_funcs = snapshot.functions.len();
    eprintln!(
        "hotspots train: {} functions in snapshot, scanning {} days of git history for fix commits…",
        n_funcs, cfg.label_window_days
    );

    match train(&snapshot, &repo_root, &cfg)? {
        None => {
            bail!(
                "Not enough training signal: need ≥20 functions and ≥2 positive/negative labels. \
                 Try a larger --label-window or run `hotspots analyze` first to build a snapshot."
            );
        }
        Some(model) => {
            report_model(&model, n_funcs);
            model.save(&args.output)?;
            eprintln!("Model saved → {}", args.output.display());
        }
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
