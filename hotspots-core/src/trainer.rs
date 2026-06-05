//! `hotspots train` — fit a local RandomForest ranker from git history.
//!
//! # Feature set (9 features, index-stable — model_version = 4)
//!
//! 0  lrs                composite complexity score
//! 1  cc                 cyclomatic complexity
//! 2  nd                 nesting depth
//! 3  loc                lines of code
//! 4  fo                 fan-out
//! 5  fan_in             call-graph fan-in
//! 6  total_churn        lifetime lines added + deleted (non-windowed structural signal)
//! 7  authors_90d        distinct commit authors in last 90 days (ownership diversity)
//! 8  directed_coupling  co-change weighted by partner defect score (F37/F38/F39)
//!
//! Deliberately excluded:
//! - `touch_count_30d`, `days_since_last_change` — windowed activity signals that
//!   correlate tautologically with labels when the training window overlaps the label
//!   scan window (temporal leakage; see research Finding 15 and Finding 31).
//! - `bug_commits`, `convention_bug_fix_count` — derived from the same fix-keyword
//!   scan used to construct labels (direct data leakage).
//! - `activity_risk` — a composite of `touch_count_30d` and `days_since_last_change`;
//!   including it is indirect temporal leakage of the same windowed signals. It also
//!   causes the trained ranker to reproduce the heuristic score rather than learning
//!   from structural features, making `hotspots train` a no-op in practice.

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

// ── Path helpers ─────────────────────────────────────────────────────────────

/// Strip `repo_root` prefix from an absolute snapshot path to produce a
/// repo-relative key matching `git diff-tree` / `git log --name-only` output.
///
/// Handles three sources of mismatch:
/// - macOS `/tmp` → `/private/tmp` symlink: tries both canonical and raw prefix forms
/// - Snapshot path itself may be a symlink: canonicalises as a last resort
/// - `hotspots analyze .` stores paths with a leading `./` component: strips it
pub(crate) fn make_rel(path: &str, prefix_can: &str, prefix_raw: &str) -> String {
    let p = path.replace('\\', "/");
    let rel = if let Some(r) = p.strip_prefix(prefix_can) {
        r.to_string()
    } else if let Some(r) = p.strip_prefix(prefix_raw) {
        r.to_string()
    } else if let Some(r) = std::path::Path::new(&p).canonicalize().ok().and_then(|cp| {
        cp.to_str()
            .and_then(|s| s.strip_prefix(prefix_can))
            .map(str::to_string)
    }) {
        r
    } else {
        return p;
    };
    rel.strip_prefix("./").unwrap_or(&rel).to_string()
}

/// Build the canonical and raw prefix strings for a repo root.
pub(crate) fn repo_prefixes(repo_root: &Path) -> (String, String) {
    let canonical = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let prefix_can = format!(
        "{}/",
        canonical.to_str().unwrap_or("").trim_end_matches('/')
    );
    let prefix_raw = repo_root
        .to_str()
        .map(|s| format!("{}/", s.trim_end_matches('/')))
        .unwrap_or_default();
    (prefix_can, prefix_raw)
}

// ── Feature extraction ────────────────────────────────────────────────────────

pub const FEATURE_NAMES: [&str; 9] = [
    "lrs",
    "cc",
    "nd",
    "loc",
    "fo",
    "fan_in",
    "total_churn",
    "authors_90d",
    "directed_coupling",
];

pub fn extract_features(func: &FunctionSnapshot) -> [f64; 9] {
    let cg = func.callgraph.as_ref();
    let total_churn = func
        .churn
        .as_ref()
        .map(|c| (c.lines_added + c.lines_deleted) as f64)
        .unwrap_or(0.0);
    [
        func.lrs,
        f64::from(func.metrics.cc),
        f64::from(func.metrics.nd),
        f64::from(func.metrics.loc),
        f64::from(func.metrics.fo),
        cg.map(|c| c.fan_in as f64).unwrap_or(0.0),
        total_churn,
        func.authors_90d.unwrap_or(0) as f64,
        func.directed_coupling.unwrap_or(0.0),
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

/// Blame-based label extraction.
///
/// For each fix commit in the window, parse `git diff-tree` hunk headers to get
/// the old (pre-fix) line ranges that changed.  Map each changed line to the
/// snapshot function whose `start_line` is closest above that line.  Only that
/// function is labelled positive — not every function in the file.
///
/// Returns `(file_path, start_line)` pairs.  Falls back silently to an empty set
/// on any git subprocess error (caller should fall back to file-level labels).
pub fn collect_fix_functions(
    snapshot: &Snapshot,
    repo_root: &Path,
    window_days: u32,
) -> Result<HashSet<(String, u32)>> {
    use std::process::Command;

    // Build a per-file sorted index of function start lines from the snapshot.
    // Snapshot stores absolute paths; git diff-tree emits repo-relative paths.
    // Strip the repo_root prefix so lookup keys match diff-tree output.
    // On macOS /tmp is a symlink to /private/tmp — canonicalize both sides so the
    // prefix strip works regardless of which form the snapshot recorded.
    let (repo_prefix_canonical, repo_prefix_raw) = repo_prefixes(repo_root);

    let mut file_index: std::collections::HashMap<String, Vec<(u32, String)>> =
        std::collections::HashMap::new();
    for func in &snapshot.functions {
        let rel = make_rel(&func.file, &repo_prefix_canonical, &repo_prefix_raw);
        file_index
            .entry(rel)
            .or_default()
            .push((func.line, func.function_id.clone()));
    }
    for entries in file_index.values_mut() {
        entries.sort_by_key(|(line, _)| *line);
    }

    // Collect fix commit SHAs and their touched files in one git log pass.
    // Format: "<sha>|<subject>" followed by filenames, separated by blank lines.
    let after = format!("{}.days.ago", window_days);
    let out = Command::new("git")
        .args([
            "log",
            "--after",
            &after,
            "--name-only",
            "--pretty=format:%H|%s",
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
    let mut fix_shas: Vec<String> = Vec::new();
    let mut current_sha: Option<String> = None;

    for line in text.lines() {
        if line.is_empty() {
            current_sha = None;
            continue;
        }
        if let Some((sha, subject)) = line.split_once('|') {
            if is_fix_message(subject) {
                current_sha = Some(sha.to_string());
                fix_shas.push(sha.to_string());
            }
            continue;
        }
        // If we have an active fix commit, this is a touched filename — we only
        // need the SHA list; skip the per-file tracking here.
        let _ = current_sha.as_ref();
    }

    fix_shas.dedup();

    let mut labelled: HashSet<(String, u32)> = HashSet::new();

    for sha in &fix_shas {
        // Get the unified diff for this commit with zero context lines.
        let diff_out = Command::new("git")
            .args(["diff-tree", "--no-commit-id", "-r", "--unified=0", sha])
            .current_dir(repo_root)
            .output();

        let diff_out = match diff_out {
            Ok(o) if o.status.success() => o,
            _ => continue,
        };

        let diff_text = String::from_utf8_lossy(&diff_out.stdout);
        let mut current_file: Option<String> = None;
        for dline in diff_text.lines() {
            // "--- a/path" or "+++ b/path" lines
            if let Some(rest) = dline.strip_prefix("+++ b/") {
                current_file = Some(rest.replace('\\', "/"));
                continue;
            }
            // Hunk header: @@ -<old_start>[,<old_count>] +<new_start>[,<new_count>] @@
            if let Some(rest) = dline.strip_prefix("@@ ") {
                if let Some(file) = &current_file {
                    if let Some(old_start) = parse_hunk_old_start(rest) {
                        if let Some(func_line) = nearest_function_above(
                            file_index.get(file).map(Vec::as_slice).unwrap_or(&[]),
                            old_start,
                        ) {
                            labelled.insert((file.clone(), func_line));
                        }
                    }
                }
            }
        }
    }

    Ok(labelled)
}

/// Parse the old-file start line from a hunk header suffix like "-42,5 +50,3 @@".
pub(crate) fn parse_hunk_old_start(rest: &str) -> Option<u32> {
    // rest begins with "-<start>[,<count>] ..."
    let after_minus = rest.strip_prefix('-')?;
    let num_str = after_minus.split([',', ' ']).next()?;
    num_str.parse::<u32>().ok()
}

/// Given a sorted list of (start_line, function_id) for a file, return the
/// start_line of the function whose start_line is the largest value ≤ `line`.
pub(crate) fn nearest_function_above(entries: &[(u32, String)], line: u32) -> Option<u32> {
    if entries.is_empty() {
        return None;
    }
    // Binary search for the last entry with start_line <= line
    let pos = entries.partition_point(|(start, _)| *start <= line);
    if pos == 0 {
        return None;
    }
    Some(entries[pos - 1].0)
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
    /// Use blame-based function-level labelling instead of file-level labelling.
    /// More precise labels but slower (one git diff-tree subprocess per fix commit).
    pub blame_labels: bool,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            label_window_days: 365,
            n_estimators: 200,
            max_depth: 6,
            seed: 42,
            blame_labels: false,
        }
    }
}

/// Train a ranker from a snapshot and the repo's git history.
///
/// Returns `None` when the snapshot has fewer than 50 functions or the fix
/// scan yields fewer than 5 positive / 10 negative labels.
pub fn train(
    snapshot: &Snapshot,
    repo_root: &Path,
    cfg: &TrainConfig,
) -> Result<Option<RankerModel>> {
    // Populate directed coupling on a mutable clone so the caller's snapshot
    // is unchanged.  Partner scores are the hotspots_score proxy: use
    // activity_risk when available, falling back to lrs.
    let mut snapshot = snapshot.clone();
    {
        let partner_scores: std::collections::HashMap<String, f64> = {
            let (prefix_can, prefix_raw) = repo_prefixes(repo_root);
            snapshot
                .functions
                .iter()
                .map(|f| {
                    let rel = make_rel(&f.file, &prefix_can, &prefix_raw);
                    let score = f.activity_risk.unwrap_or(f.lrs);
                    (rel, score)
                })
                .collect()
        };
        snapshot.populate_directed_coupling(repo_root, &partner_scores);
    }

    // Build (features, label) pairs from snapshot functions
    let mut rows: Vec<([f64; 9], bool)> = Vec::new();

    if cfg.blame_labels {
        let fix_funcs = collect_fix_functions(&snapshot, repo_root, cfg.label_window_days)?;
        let (prefix_can, prefix_raw) = repo_prefixes(repo_root);
        for func in &snapshot.functions {
            let rel = make_rel(&func.file, &prefix_can, &prefix_raw);
            let label = fix_funcs.contains(&(rel, func.line));
            rows.push((extract_features(func), label));
        }
    } else {
        let fix_files = collect_fix_files(repo_root, cfg.label_window_days)?;
        for func in &snapshot.functions {
            let file_norm = func.file.replace('\\', "/");
            let label = fix_files.contains(&file_norm)
                || fix_files.iter().any(|f| file_norm.ends_with(f.as_str()));
            rows.push((extract_features(func), label));
        }
    }

    let n_pos = rows.iter().filter(|(_, l)| *l).count();
    let n_neg = rows.len() - n_pos;

    if rows.len() < 50 || n_pos < 5 || n_neg < 10 {
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

    Ok(Some(RankerModel {
        model_version: 4,
        trees,
        meta,
    }))
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
        let val = if global_idx < 8 {
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

fn default_model_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankerModel {
    #[serde(default = "default_model_version")]
    pub model_version: u32,
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
        let model: Self = serde_json::from_str(&json).context("deserialize model")?;
        if model.model_version < 3 {
            bail!(
                "{} was trained with an older feature set (model_version={}). \
                 Run `hotspots train` to retrain with the current feature set.",
                path.display(),
                model.model_version
            );
        }
        Ok(model)
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Feature names ─────────────────────────────────────────────────────────

    #[test]
    fn feature_names_count_matches_array() {
        assert_eq!(FEATURE_NAMES.len(), 9);
    }

    #[test]
    fn no_leaky_windowed_features() {
        for name in FEATURE_NAMES {
            assert_ne!(name, "touch_count_30d", "leaky feature still present");
            assert_ne!(
                name, "days_since_last_change",
                "leaky feature still present"
            );
            assert_ne!(name, "activity_risk", "leaky feature still present");
        }
        assert!(FEATURE_NAMES.contains(&"total_churn"));
    }

    // ── parse_hunk_old_start ──────────────────────────────────────────────────

    #[test]
    fn parse_hunk_simple() {
        assert_eq!(parse_hunk_old_start("-42,5 +50,3 @@"), Some(42));
    }

    #[test]
    fn parse_hunk_no_count() {
        assert_eq!(parse_hunk_old_start("-10 +10 @@"), Some(10));
    }

    #[test]
    fn parse_hunk_wrong_prefix() {
        assert_eq!(parse_hunk_old_start("+42,5 -50,3 @@"), None);
        assert_eq!(parse_hunk_old_start(""), None);
    }

    // ── nearest_function_above ────────────────────────────────────────────────

    #[test]
    fn nearest_exact_match() {
        let entries = vec![
            (1u32, "a".to_string()),
            (10u32, "b".to_string()),
            (20u32, "c".to_string()),
        ];
        assert_eq!(nearest_function_above(&entries, 10), Some(10));
    }

    #[test]
    fn nearest_between_funcs() {
        let entries = vec![
            (1u32, "a".to_string()),
            (10u32, "b".to_string()),
            (20u32, "c".to_string()),
        ];
        assert_eq!(nearest_function_above(&entries, 15), Some(10));
    }

    #[test]
    fn nearest_before_first_func() {
        let entries = vec![(10u32, "a".to_string())];
        assert_eq!(nearest_function_above(&entries, 5), None);
    }

    #[test]
    fn nearest_empty_entries() {
        assert_eq!(nearest_function_above(&[], 42), None);
    }

    #[test]
    fn nearest_after_last_func() {
        let entries = vec![
            (1u32, "a".to_string()),
            (10u32, "b".to_string()),
            (50u32, "c".to_string()),
        ];
        assert_eq!(nearest_function_above(&entries, 999), Some(50));
    }

    // ── Model versioning ──────────────────────────────────────────────────────

    #[test]
    fn load_v1_model_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        // v1 = no model_version field (defaults to 1 via serde)
        std::fs::write(
            &path,
            r#"{"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let err = RankerModel::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("retrain") || err.contains("model_version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_v2_model_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        std::fs::write(
            &path,
            r#"{"model_version":2,"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let err = RankerModel::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("retrain") || err.contains("model_version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_v3_model_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        std::fs::write(
            &path,
            r#"{"model_version":3,"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let model = RankerModel::load(&path).expect("should load");
        assert_eq!(model.model_version, 3);
    }
}
