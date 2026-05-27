//! `hotspots train` — fit a local RandomForest ranker from git history.
//!
//! # Feature set (9 features, index-stable)
//!
//! 0  lrs                    composite complexity score
//! 1  cc                     cyclomatic complexity
//! 2  nd                     nesting depth
//! 3  loc                    lines of code
//! 4  fo                     fan-out
//! 5  fan_in                 call-graph fan-in
//! 6  touch_count_30d        commit frequency (30-day window)
//! 7  days_since_last_change recency (0 = changed today)
//! 8  activity_risk          hotspots composite score (formula)
//!
//! Deliberately excluded: `bug_commits`, `convention_bug_fix_count` — both are derived
//! from the same fix-keyword scan used to construct labels (data leakage).

use crate::snapshot::{FunctionSnapshot, Snapshot};
use anyhow::{bail, Context, Result};
use linfa::prelude::Fit;
use linfa::Dataset;
use linfa_ensemble::RandomForestParams;
use linfa_trees::DecisionTreeParams;
use ndarray::{Array1, Array2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

// ── Feature extraction ────────────────────────────────────────────────────────

pub const FEATURE_NAMES: [&str; 9] = [
    "lrs",
    "cc",
    "nd",
    "loc",
    "fo",
    "fan_in",
    "touch_count_30d",
    "days_since_last_change",
    "activity_risk",
];

pub fn extract_features(func: &FunctionSnapshot) -> [f64; 9] {
    let cg = func.callgraph.as_ref();
    [
        func.lrs,
        f64::from(func.metrics.cc),
        f64::from(func.metrics.nd),
        f64::from(func.metrics.loc),
        f64::from(func.metrics.fo),
        cg.map(|c| c.fan_in as f64).unwrap_or(0.0),
        func.touch_count_30d.unwrap_or(0) as f64,
        func.days_since_last_change.unwrap_or(365) as f64,
        func.activity_risk.unwrap_or(func.lrs),
    ]
}

// ── Label extraction from git ─────────────────────────────────────────────────

/// Returns the set of file paths touched by fix-keyword commits in the last
/// `window_days` days.  Uses `git log` via subprocess; tolerates missing git.
pub fn collect_fix_files(repo_root: &Path, window_days: u32) -> Result<HashSet<String>> {
    use std::process::Command;

    let after = format!("{}.days.ago", window_days);
    let out = Command::new("git")
        .args([
            "log",
            "--after",
            &after,
            "--name-only",
            "--pretty=format:%s",
            "--diff-filter=M",
        ])
        .current_dir(repo_root)
        .output()
        .context("git log failed")?;

    if !out.status.success() {
        bail!(
            "git log exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut fix_files = HashSet::new();
    let mut in_fix_commit = false;

    for line in text.lines() {
        if line.is_empty() {
            in_fix_commit = false;
            continue;
        }
        // First non-empty line after blank is the commit subject
        if !in_fix_commit && is_fix_message(line) {
            in_fix_commit = true;
            continue;
        }
        if in_fix_commit && !line.is_empty() {
            // Normalize path separator
            fix_files.insert(line.replace('\\', "/"));
        }
    }

    Ok(fix_files)
}

fn is_fix_message(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("fix")
        || lower.contains("bug")
        || lower.contains("patch")
        || lower.contains("regression")
        || lower.contains("defect")
        || lower.contains("hotfix")
}

// ── Training ──────────────────────────────────────────────────────────────────

/// Configuration for training a local ranker.
#[derive(Debug, Clone)]
pub struct TrainConfig {
    /// Days of git history to scan for fix-commit labels.
    pub label_window_days: u32,
    /// Number of trees in the RandomForest.
    pub n_estimators: usize,
    /// Maximum tree depth.
    pub max_depth: usize,
    /// RNG seed for reproducibility.
    pub seed: u64,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            label_window_days: 365,
            n_estimators: 200,
            max_depth: 6,
            seed: 42,
        }
    }
}

/// Train a ranker from a snapshot and the repo's git history.
///
/// Returns `None` when the snapshot has fewer than 20 functions or the fix
/// scan yields fewer than 2 positive labels — not enough signal to train.
pub fn train(
    snapshot: &Snapshot,
    repo_root: &Path,
    cfg: &TrainConfig,
) -> Result<Option<RankerModel>> {
    let fix_files = collect_fix_files(repo_root, cfg.label_window_days)?;

    // Build (features, label) pairs from snapshot functions
    let mut rows: Vec<([f64; 9], bool)> = Vec::new();
    for func in &snapshot.functions {
        let file_norm = func.file.replace('\\', "/");
        let label = fix_files.contains(&file_norm)
            || fix_files.iter().any(|f| file_norm.ends_with(f.as_str()));
        rows.push((extract_features(func), label));
    }

    let n_pos = rows.iter().filter(|(_, l)| *l).count();
    let n_neg = rows.len() - n_pos;

    if rows.len() < 20 || n_pos < 2 || n_neg < 2 {
        return Ok(None);
    }

    let n = rows.len();
    let mut x_data = Vec::with_capacity(n * 9);
    let mut y_data: Vec<bool> = Vec::with_capacity(n);

    for (feats, label) in &rows {
        x_data.extend_from_slice(feats);
        y_data.push(*label);
    }

    let x: Array2<f64> =
        Array2::from_shape_vec((n, 9), x_data).context("feature matrix shape error")?;
    let y: Array1<bool> = Array1::from_vec(y_data);

    let dataset = Dataset::new(x, y).with_feature_names(
        FEATURE_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );

    let rng = SmallRng::seed_from_u64(cfg.seed);
    let tree_params = DecisionTreeParams::new().max_depth(Some(cfg.max_depth));

    let rf = RandomForestParams::new_fixed_rng(tree_params, rng.clone())
        .ensemble_size(cfg.n_estimators)
        .bootstrap_proportion(0.8)
        .fit(&dataset)
        .context("RandomForest fit failed")?;

    // Serialize each tree as a compact node list
    let mut trees: Vec<SerializedTree> = Vec::with_capacity(rf.models.len());
    for (tree, feat_indices) in rf.models.iter().zip(rf.model_features.iter()) {
        let nodes = serialize_tree(tree);
        trees.push(SerializedTree {
            nodes,
            feature_indices: feat_indices.clone(),
        });
    }

    let meta = TrainMeta {
        n_samples: n,
        n_pos,
        n_neg,
        label_window_days: cfg.label_window_days,
        n_estimators: cfg.n_estimators,
        max_depth: cfg.max_depth,
    };

    Ok(Some(RankerModel { trees, meta }))
}

// ── Tree serialization ────────────────────────────────────────────────────────

/// Flat node representation.  Leaves have `feature_idx = usize::MAX`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNodeRecord {
    pub feature_idx: usize,
    pub threshold: f64,
    /// Index into `nodes` for the left child (feature ≤ threshold).
    pub left: usize,
    /// Index into `nodes` for the right child (feature > threshold).
    pub right: usize,
    /// For leaf nodes: true = predict positive.
    pub leaf_value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedTree {
    pub nodes: Vec<TreeNodeRecord>,
    /// Which global feature indices this tree was trained on.
    pub feature_indices: Vec<usize>,
}

fn serialize_tree<F, L>(tree: &linfa_trees::DecisionTree<F, L>) -> Vec<TreeNodeRecord>
where
    F: linfa::Float,
    L: linfa::Label + Into<bool> + Copy,
{
    let mut nodes: Vec<TreeNodeRecord> = Vec::new();
    serialize_node(tree.root_node(), &mut nodes);
    nodes
}

fn serialize_node<F, L>(
    node: &linfa_trees::TreeNode<F, L>,
    nodes: &mut Vec<TreeNodeRecord>,
) -> usize
where
    F: linfa::Float,
    L: linfa::Label + Into<bool> + Copy,
{
    let idx = nodes.len();
    // Push placeholder so children can reference correct indices
    nodes.push(TreeNodeRecord {
        feature_idx: usize::MAX,
        threshold: 0.0,
        left: 0,
        right: 0,
        leaf_value: false,
    });

    if node.is_leaf() {
        let val: bool = node.prediction().map(|p| p.into()).unwrap_or(false);
        nodes[idx].leaf_value = val;
    } else {
        let (feat, thresh, _) = node.split();
        nodes[idx].feature_idx = feat;
        nodes[idx].threshold = thresh.to_f64().unwrap_or(0.0);

        let children = node.children();
        let left_child = children[0].as_deref();
        let right_child = children[1].as_deref();

        let left_idx = if let Some(lc) = left_child {
            serialize_node(lc, nodes)
        } else {
            idx // degenerate: point to self (won't happen in valid trees)
        };
        let right_idx = if let Some(rc) = right_child {
            serialize_node(rc, nodes)
        } else {
            idx
        };
        nodes[idx].left = left_idx;
        nodes[idx].right = right_idx;
    }

    idx
}

// ── Inference ─────────────────────────────────────────────────────────────────

/// Score a function using the trained ranker.
/// Returns a value in [0, 1]: fraction of trees voting positive.
pub fn score(model: &RankerModel, func: &FunctionSnapshot) -> f64 {
    let feats = extract_features(func);
    let n = model.trees.len();
    if n == 0 {
        return 0.0;
    }
    let votes: usize = model
        .trees
        .iter()
        .map(|tree| vote(tree, &feats) as usize)
        .sum();
    votes as f64 / n as f64
}

fn vote(tree: &SerializedTree, feats: &[f64; 9]) -> bool {
    let nodes = &tree.nodes;
    if nodes.is_empty() {
        return false;
    }
    let mut cur = 0usize;
    loop {
        let node = &nodes[cur];
        if node.feature_idx == usize::MAX {
            return node.leaf_value;
        }
        // Map tree-local feature index (which may be a sub-sampled global index)
        let global_idx = tree
            .feature_indices
            .get(node.feature_idx)
            .copied()
            .unwrap_or(node.feature_idx);
        let val = if global_idx < 9 {
            feats[global_idx]
        } else {
            0.0
        };
        if val <= node.threshold {
            cur = node.left;
        } else {
            cur = node.right;
        }
        // Guard against cycles (shouldn't happen with valid serialization)
        if cur >= nodes.len() {
            return false;
        }
    }
}

// ── Persisted model ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainMeta {
    pub n_samples: usize,
    pub n_pos: usize,
    pub n_neg: usize,
    pub label_window_days: u32,
    pub n_estimators: usize,
    pub max_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankerModel {
    pub trees: Vec<SerializedTree>,
    pub meta: TrainMeta,
}

impl RankerModel {
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serialize model")?;
        std::fs::write(path, json).context("write model file")?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path).context("read model file")?;
        serde_json::from_str(&json).context("deserialize model")
    }
}
