//! `hotspots train` — fit a local RandomForest ranker from git history.
//!
//! # Feature set (10 features, index-stable — model_version = 5)
//!
//! 0  lrs                        composite complexity score
//! 1  cc                         cyclomatic complexity
//! 2  nd                         nesting depth
//! 3  loc                        lines of code
//! 4  fo                         fan-out
//! 5  fan_in                     call-graph fan-in
//! 6  total_churn                lifetime lines added + deleted (non-windowed structural signal)
//! 7  authors_90d                distinct commit authors in last 90 days (ownership diversity)
//! 8  directed_coupling          co-change weighted by partner defect score (F37/F38/F39)
//! 9  convention_bug_fix_count   full-history count of fix-keyword commits per file (F54)
//!
//! Deliberately excluded:
//! - `touch_count_30d`, `days_since_last_change` — windowed activity signals that
//!   correlate tautologically with labels when the training window overlaps the label
//!   scan window (temporal leakage; see research Finding 15 and Finding 31).
//! - `convention_bug_fix_rate` — fix count divided by total commits; circular with the
//!   label scan (F48). Only the raw count survives temporal holdout (F54).
//! - `activity_risk` — a composite of `touch_count_30d` and `days_since_last_change`;
//!   including it is indirect temporal leakage of the same windowed signals. It also
//!   causes the trained ranker to reproduce the heuristic score rather than learning
//!   from structural features, making `hotspots train` a no-op in practice.

use crate::isolation_forest::IsolationForest;
use crate::snapshot::{FunctionSnapshot, Snapshot};
use anyhow::{bail, Context, Result};
use linfa::prelude::Fit;
use linfa::Dataset;
use linfa_linear::TweedieRegressor;
use linfa_trees::DecisionTreeParams;
use ndarray::{Array1, Array2};
use rand::rngs::SmallRng;
use rand::seq::index::sample as index_sample;
use rand::Rng;
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
pub fn make_rel(path: &str, prefix_can: &str, prefix_raw: &str) -> String {
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
pub fn repo_prefixes(repo_root: &Path) -> (String, String) {
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

pub const FEATURE_NAMES: [&str; 10] = [
    "lrs",
    "cc",
    "nd",
    "loc",
    "fo",
    "fan_in",
    "total_churn",
    "authors_90d",
    "directed_coupling",
    "convention_bug_fix_count",
];

pub fn extract_features(func: &FunctionSnapshot) -> [f64; 10] {
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
        func.convention_bug_fix_count.unwrap_or(0) as f64,
    ]
}

/// Cold-start feature vector (F62/F63) — distinct from `extract_features()`'s 10
/// structural/activity features. Order: commit_count, author_count, author_entropy,
/// burst_score, isolation_rate, age_days, last_touch_days, authors_90d. All fields
/// default to `0.0` via `.unwrap_or(0.0)` — never panics on `None`.
pub fn cold_start_features(func: &FunctionSnapshot) -> [f64; 8] {
    [
        func.commit_count.unwrap_or(0) as f64,
        func.author_count.unwrap_or(0) as f64,
        func.author_entropy.unwrap_or(0.0),
        func.burst_score.unwrap_or(0.0),
        func.isolation_rate.unwrap_or(0.0),
        func.age_days.unwrap_or(0.0),
        func.last_touch_days.unwrap_or(0.0),
        func.authors_90d.unwrap_or(0) as f64,
    ]
}

// ── Label extraction from git ─────────────────────────────────────────────────

/// Returns the set of file paths touched by fix-keyword commits in the last
/// `window_days` days.  Uses `git log` via subprocess; tolerates missing git.
pub fn collect_fix_files(
    repo_root: &Path,
    window_days: u32,
    before: Option<&str>,
) -> Result<HashSet<String>> {
    use std::process::Command;

    let after = format!("{}.days.ago", window_days);
    let mut git_args = vec![
        "log",
        "--after",
        &after,
        "--name-only",
        "--pretty=format:%s",
        "--diff-filter=M",
    ];
    if let Some(b) = before {
        git_args.push("--before");
        git_args.push(b);
    }
    let out = Command::new("git")
        .args(&git_args)
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
    before: Option<&str>,
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
    let mut git_args = vec![
        "log",
        "--after",
        &after,
        "--name-only",
        "--pretty=format:%H|%s",
        "--diff-filter=M",
    ];
    if let Some(b) = before {
        git_args.push("--before");
        git_args.push(b);
    }
    let out = Command::new("git")
        .args(&git_args)
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
    /// Optional upper bound for the label window (ISO date string, e.g. "2025-01-01").
    /// When set, only commits before this date are used as training labels.
    /// Useful for matching a specific benchmark label window.
    pub label_before: Option<String>,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            label_window_days: 365,
            n_estimators: 200,
            max_depth: 6,
            seed: 42,
            blame_labels: false,
            label_before: None,
        }
    }
}

/// Train a ranker from a snapshot and the repo's git history.
///
/// Returns `None` when the snapshot has fewer than 50 functions or the fix
/// scan yields fewer than 5 positive / 10 negative labels.
///
/// `on_tree` is called after each tree is fitted with `(completed, total)`.
/// Pass `None` to suppress progress callbacks.
pub fn train(
    snapshot: &Snapshot,
    repo_root: &Path,
    cfg: &TrainConfig,
    on_tree: Option<&dyn Fn(u32, u32)>,
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
    snapshot.populate_convention_bug_fix_count(repo_root);
    // Cold-start signals (F62/F63 prerequisite) — not part of FEATURE_NAMES/extract_features;
    // consumed via cold_start_features() by the downstream Gini-gated routing (F62/F63).
    snapshot.populate_history_signals(repo_root);

    // Build (features, label) pairs from snapshot functions
    let mut rows: Vec<([f64; 10], bool)> = Vec::new();

    if cfg.blame_labels {
        let fix_funcs = collect_fix_functions(
            &snapshot,
            repo_root,
            cfg.label_window_days,
            cfg.label_before.as_deref(),
        )?;
        let (prefix_can, prefix_raw) = repo_prefixes(repo_root);
        for func in &snapshot.functions {
            let rel = make_rel(&func.file, &prefix_can, &prefix_raw);
            let label = fix_funcs.contains(&(rel, func.line));
            rows.push((extract_features(func), label));
        }
    } else {
        let fix_files = collect_fix_files(
            repo_root,
            cfg.label_window_days,
            cfg.label_before.as_deref(),
        )?;
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
    let mut x_data = Vec::with_capacity(n * FEATURE_NAMES.len());
    let mut y_data: Vec<bool> = Vec::with_capacity(n);

    for (feats, label) in &rows {
        x_data.extend_from_slice(feats);
        y_data.push(*label);
    }

    let x: Array2<f64> = Array2::from_shape_vec((n, FEATURE_NAMES.len()), x_data)
        .context("feature matrix shape error")?;
    let y: Array1<bool> = Array1::from_vec(y_data);

    let (regime_verdict, regime_delta) = regime_screen(&x, &y);

    // Linear regime: Ridge does as well as RandomForest — use it instead.
    if regime_verdict == RegimeVerdict::Linear {
        let ridge = train_ridge(&x, &y)?;
        let meta = TrainMeta {
            n_samples: n,
            n_pos,
            n_neg,
            label_window_days: cfg.label_window_days,
            n_estimators: 0,
            max_depth: 0,
            regime_verdict: Some(regime_verdict),
            regime_delta,
        };
        return Ok(Some(RankerModel {
            model_version: 5,
            trees: vec![],
            meta,
            model_class: ModelClass::Ridge,
            ridge: Some(ridge),
        }));
    }

    let mut rng = SmallRng::seed_from_u64(cfg.seed);
    let tree_params = DecisionTreeParams::new().max_depth(Some(cfg.max_depth));

    // Number of features per tree: sqrt(total), matching sklearn/linfa_ensemble default.
    let n_all_feats = FEATURE_NAMES.len();
    let n_tree_feats = ((n_all_feats as f64).sqrt().ceil() as usize).max(1);
    let bootstrap_n = (n as f64 * 0.8).round() as usize;

    let mut trees: Vec<SerializedTree> = Vec::with_capacity(cfg.n_estimators);
    for i in 0..cfg.n_estimators {
        // Random feature subset for this tree
        let feat_indices: Vec<usize> = index_sample(&mut rng, n_all_feats, n_tree_feats).into_vec();

        // Bootstrap sample (with replacement)
        let boot_rows: Vec<usize> = (0..bootstrap_n).map(|_| rng.gen_range(0..n)).collect();

        // Build (bootstrap × tree-features) matrix and label vector
        let x_boot = Array2::from_shape_fn((bootstrap_n, n_tree_feats), |(r, c)| {
            x[[boot_rows[r], feat_indices[c]]]
        });
        let y_boot: Array1<bool> = Array1::from_shape_fn(bootstrap_n, |r| y[boot_rows[r]]);

        let boot_dataset = Dataset::new(x_boot, y_boot);
        let tree = tree_params
            .fit(&boot_dataset)
            .context("decision tree fit failed")?;

        let nodes = serialize_tree(&tree);
        trees.push(SerializedTree {
            nodes,
            feature_indices: feat_indices,
        });

        if let Some(cb) = on_tree {
            cb((i + 1) as u32, cfg.n_estimators as u32);
        }
    }

    let meta = TrainMeta {
        n_samples: n,
        n_pos,
        n_neg,
        label_window_days: cfg.label_window_days,
        n_estimators: cfg.n_estimators,
        max_depth: cfg.max_depth,
        regime_verdict: Some(regime_verdict),
        regime_delta,
    };

    Ok(Some(RankerModel {
        model_version: 5,
        trees,
        meta,
        model_class: ModelClass::RandomForest,
        ridge: None,
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

// ── P@K evaluation ───────────────────────────────────────────────────────────

pub type FunctionId = String;

/// A function scored by the ranker, for P@K evaluation.
pub struct ScoredFunction {
    pub function_id: FunctionId,
    pub score: f64,
}

/// Precision at K: fraction of top-K ranked functions that are true positives.
/// Returns 0.0 when K exceeds list length or no positives in top-K.
pub fn precision_at_k(
    ranked: &[ScoredFunction],
    labels: &std::collections::HashMap<FunctionId, bool>,
    k: usize,
) -> f64 {
    let top_k = ranked.iter().take(k);
    let count = top_k.clone().count();
    if count == 0 {
        return 0.0;
    }
    let hits = top_k
        .filter(|f| labels.get(&f.function_id).copied().unwrap_or(false))
        .count();
    hits as f64 / count as f64
}

// ── Repo screener ─────────────────────────────────────────────────────────────

/// Repos with mean score below this are too flat for fine-tuning to be useful.
/// Derived from F08: midpoint between facebook/react (0.0332) and golang/go (0.0421)
/// across a 32-repo validation corpus.
pub const SCREENER_SKIP_THRESHOLD: f64 = 0.03;

/// Repos between SKIP and RUN thresholds are ambiguous — training is allowed but
/// a warning is emitted. The gap covers the overlap zone observed in F08.
pub const SCREENER_RUN_THRESHOLD: f64 = 0.06;

/// Pre-flight verdict before committing to a full training run.
///
/// Derived from F08 (32-repo corpus validation). Clear cases are separated by
/// the SKIP/RUN zone; repos in the middle are ambiguous and emit a warning
/// but are not blocked.
#[derive(Debug, PartialEq)]
pub enum ScreenerVerdict {
    /// mean_hotspots_score >= 0.06 — training is likely to beat the tabular baseline.
    RunFt,
    /// 0.03 <= mean_hotspots_score < 0.06 — outcome is uncertain; training proceeds with a warning.
    Ambiguous,
    /// mean_hotspots_score < 0.03 — signal is too flat; skip fine-tuning.
    SkipFt,
}

/// Compute a pre-flight screener verdict from snapshot functions.
///
/// Uses `activity_risk` when available, falling back to `lrs` — the same
/// signal used as the partner-score proxy in `train()`.
pub fn screen_repo(snapshot: &Snapshot) -> (ScreenerVerdict, f64) {
    let scores: Vec<f64> = snapshot
        .functions
        .iter()
        .map(|f| f.activity_risk.unwrap_or(f.lrs))
        .collect();

    if scores.is_empty() {
        return (ScreenerVerdict::SkipFt, 0.0);
    }

    let mean_hs = scores.iter().sum::<f64>() / scores.len() as f64;

    let verdict = if mean_hs >= SCREENER_RUN_THRESHOLD {
        ScreenerVerdict::RunFt
    } else if mean_hs >= SCREENER_SKIP_THRESHOLD {
        ScreenerVerdict::Ambiguous
    } else {
        ScreenerVerdict::SkipFt
    };

    (verdict, mean_hs)
}

// ── Cold-start routing (F62/F63) ─────────────────────────────────────────────

/// Gini ≥ this on `commit_count` → the existing formula score is a sufficient
/// day-one ranking. Matches F62's validated threshold exactly.
pub const HIGH_GINI: f64 = 0.60;

/// Gini < this on `commit_count` → route to the label-free IsolationForest anomaly
/// score (F63). Between `LOW_GINI` and `HIGH_GINI` is an ungated middle zone that
/// defaults to the formula path — F62 does not define a third bucket.
pub const LOW_GINI: f64 = 0.55;

/// Uniform-prior guard: if the top 10% of files by `commit_count` account for less
/// than this share of total commits, no file stands out even by raw count — return
/// a uniform prior instead of a manufactured ranking. Adapted from F63's `pos_rate`
/// guard, which does not apply at true cold-start (no labels exist yet).
const UNIFORM_PRIOR_TOP_DECILE_SHARE: f64 = 0.20;

const COLD_START_N_TREES: usize = 100;
const COLD_START_SUBSAMPLE_SIZE: usize = 256;
const COLD_START_SEED: u64 = 42;

/// Which cold-start ranking strategy was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColdStartRoute {
    /// Gini ≥ `HIGH_GINI` (or the ungated middle zone) — existing formula score.
    Formula,
    /// Gini < `LOW_GINI` — label-free IsolationForest anomaly score.
    Anomaly,
    /// No file stands out by commit-count concentration — uniform prior.
    UniformPrior,
}

/// Result of `cold_start_rank()`: the routing decision plus the ranked functions.
pub struct ColdStartResult {
    pub route: ColdStartRoute,
    pub ranked: Vec<ScoredFunction>,
}

/// Standard Gini coefficient: `sum((2*i - n - 1) * v_i) / (n * sum(v))` over
/// ascending-sorted `values`, `i` 1-indexed. Matches F62's formula exactly. Returns
/// `0.0` for empty input or when all values are zero (no concentration to measure).
pub fn gini_coefficient(values: &[f64]) -> f64 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let total: f64 = sorted.iter().sum();
    if total == 0.0 {
        return 0.0;
    }
    let numerator: f64 = sorted
        .iter()
        .enumerate()
        .map(|(idx, &v)| {
            let i = (idx + 1) as f64;
            (2.0 * i - n as f64 - 1.0) * v
        })
        .sum();
    numerator / (n as f64 * total)
}

/// Rank a snapshot for a repo with zero (or unreliable) label history.
///
/// Routes via the Gini coefficient of `commit_count` across all functions (F62):
/// - `Gini >= HIGH_GINI` (or the 0.55-0.60 middle zone) → `Formula`: rank by the
///   existing `activity_risk`/`lrs` score, no new model needed.
/// - `Gini < LOW_GINI` → `Anomaly`: fit a label-free `IsolationForest` on the 8-feature
///   cold-start vector (`cold_start_features`) and rank by anomaly score.
/// - Uniform-prior guard (checked first): if the top 10% of files by `commit_count`
///   account for less than 20% of total commits, no file stands out even by raw count
///   — return a uniform prior rather than a manufactured ranking.
///
/// Two streaming passes over `snapshot.functions` on the `Anomaly` route (fit, then
/// score) — no intermediate full-matrix structure is built at any point.
pub fn cold_start_rank(snapshot: &Snapshot) -> ColdStartResult {
    let commit_counts: Vec<f64> = snapshot
        .functions
        .iter()
        .map(|f| f.commit_count.unwrap_or(0) as f64)
        .collect();

    if commit_counts.is_empty() {
        return ColdStartResult {
            route: ColdStartRoute::UniformPrior,
            ranked: vec![],
        };
    }

    let total_commits: f64 = commit_counts.iter().sum();
    if total_commits > 0.0 {
        let mut sorted_desc = commit_counts.clone();
        sorted_desc.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let top_decile_n = ((sorted_desc.len() as f64 * 0.10).ceil() as usize).max(1);
        let top_decile_share: f64 = sorted_desc[..top_decile_n].iter().sum::<f64>() / total_commits;
        if top_decile_share < UNIFORM_PRIOR_TOP_DECILE_SHARE {
            return ColdStartResult {
                route: ColdStartRoute::UniformPrior,
                ranked: vec![],
            };
        }
    }

    let gini = gini_coefficient(&commit_counts);

    if gini < LOW_GINI {
        let forest = IsolationForest::fit(
            snapshot.functions.iter().map(cold_start_features),
            COLD_START_N_TREES,
            COLD_START_SUBSAMPLE_SIZE,
            COLD_START_SEED,
        );
        let mut ranked: Vec<ScoredFunction> = snapshot
            .functions
            .iter()
            .map(|f| ScoredFunction {
                function_id: f.function_id.clone(),
                score: forest.anomaly_score(&cold_start_features(f)),
            })
            .collect();
        ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        return ColdStartResult {
            route: ColdStartRoute::Anomaly,
            ranked,
        };
    }

    let mut ranked: Vec<ScoredFunction> = snapshot
        .functions
        .iter()
        .map(|f| ScoredFunction {
            function_id: f.function_id.clone(),
            score: f.activity_risk.unwrap_or(f.lrs),
        })
        .collect();
    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    ColdStartResult {
        route: ColdStartRoute::Formula,
        ranked,
    }
}

// ── Regime screener ────────────────────────────────────────────────────────

/// Verdict from the pre-training regime screener (F61): compares a depth-2
/// RandomForest against Ridge regression to decide whether the fix-history
/// signal is linear (Ridge suffices) or has interaction effects (use RandomForest).
/// Thresholds mirror `scripts/eval/stats_pass.py`'s Q3 check exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegimeVerdict {
    /// Δρ < 0.03 — linear model matches RandomForest; use Ridge.
    Linear,
    /// 0.03 ≤ Δρ ≤ 0.10 — modest tree advantage; use RandomForest.
    Weak,
    /// Δρ > 0.10 — strong tree advantage; use RandomForest.
    Strong,
    /// pos_rate < 0.05 — too few positives to trust the comparison; use RandomForest.
    Unreliable,
}

/// Which model class was selected for this `RankerModel`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelClass {
    Ridge,
    RandomForest,
}

/// Serializable Ridge regression ranker (stores standardization params).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RidgeRanker {
    pub coefficients: Vec<f64>,
    pub intercept: f64,
    pub mean: Vec<f64>,
    pub std: Vec<f64>,
}

fn spearman_rho(pred: &[f64], truth: &[f64]) -> f64 {
    let n = pred.len();
    if n < 2 {
        return 0.0;
    }
    let rank_p = compute_ranks(pred);
    let rank_t = compute_ranks(truth);
    pearson_corr(&rank_p, &rank_t)
}

fn compute_ranks(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && v[idx[j]] == v[idx[i]] {
            j += 1;
        }
        let avg_rank = (i + j - 1) as f64 / 2.0 + 1.0;
        for k in i..j {
            r[idx[k]] = avg_rank;
        }
        i = j;
    }
    r
}

fn pearson_corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let num: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let da: f64 = a.iter().map(|x| (x - ma).powi(2)).sum::<f64>().sqrt();
    let db: f64 = b.iter().map(|y| (y - mb).powi(2)).sum::<f64>().sqrt();
    if da == 0.0 || db == 0.0 {
        return 0.0;
    }
    num / (da * db)
}

/// 3-fold CV comparison of Ridge vs. depth-2 RandomForest.
/// Returns the regime verdict and Δρ = tree_ρ − ridge_ρ.
pub fn regime_screen(x: &Array2<f64>, y: &Array1<bool>) -> (RegimeVerdict, f64) {
    let n = x.nrows();
    let n_feats = x.ncols();
    let n_pos = y.iter().filter(|&&b| b).count();
    let pos_rate = n_pos as f64 / n as f64;

    if pos_rate < 0.05 {
        return (RegimeVerdict::Unreliable, 0.0);
    }

    const CV_FOLDS: usize = 3;
    let fold_size = n / CV_FOLDS;

    let mut ridge_rhos: Vec<f64> = Vec::with_capacity(CV_FOLDS);
    let mut tree_rhos: Vec<f64> = Vec::with_capacity(CV_FOLDS);

    for fold in 0..CV_FOLDS {
        let test_start = fold * fold_size;
        let test_end = if fold == CV_FOLDS - 1 {
            n
        } else {
            test_start + fold_size
        };

        let train_idx: Vec<usize> = (0..n)
            .filter(|&i| i < test_start || i >= test_end)
            .collect();
        let test_idx: Vec<usize> = (test_start..test_end).collect();

        let n_train = train_idx.len();
        let n_test = test_idx.len();
        if n_test == 0 || n_train < 5 {
            continue;
        }

        let x_train = Array2::from_shape_fn((n_train, n_feats), |(r, c)| x[[train_idx[r], c]]);
        let y_train_bool: Array1<bool> = Array1::from_shape_fn(n_train, |r| y[train_idx[r]]);
        let y_train_f: Array1<f64> = y_train_bool.mapv(|b| if b { 1.0 } else { 0.0 });
        let x_test = Array2::from_shape_fn((n_test, n_feats), |(r, c)| x[[test_idx[r], c]]);
        let y_test_f: Vec<f64> = test_idx
            .iter()
            .map(|&i| if y[i] { 1.0 } else { 0.0 })
            .collect();

        // Standardize on train statistics
        let col_mean: Vec<f64> = (0..n_feats)
            .map(|c| x_train.column(c).mean().unwrap_or(0.0))
            .collect();
        let col_std: Vec<f64> = (0..n_feats)
            .map(|c| {
                let col = x_train.column(c);
                let m = col_mean[c];
                let var = col.iter().map(|&v| (v - m).powi(2)).sum::<f64>() / n_train as f64;
                var.sqrt().max(1e-8)
            })
            .collect();
        let x_train_std = Array2::from_shape_fn((n_train, n_feats), |(r, c)| {
            (x_train[[r, c]] - col_mean[c]) / col_std[c]
        });
        let x_test_std = Array2::from_shape_fn((n_test, n_feats), |(r, c)| {
            (x_test[[r, c]] - col_mean[c]) / col_std[c]
        });

        // Ridge
        let ridge_dataset = Dataset::new(x_train_std, y_train_f);
        if let Ok(ridge_model) = TweedieRegressor::params()
            .alpha(1.0_f64)
            .power(0.0_f64)
            .fit(&ridge_dataset)
        {
            let coeffs: &Array1<f64> = &ridge_model.coef;
            let intercept: f64 = ridge_model.intercept;
            let ridge_preds: Vec<f64> = (0..n_test)
                .map(|r| {
                    let row = x_test_std.row(r);
                    coeffs
                        .iter()
                        .zip(row.iter())
                        .map(|(c, x)| c * x)
                        .sum::<f64>()
                        + intercept
                })
                .collect();
            ridge_rhos.push(spearman_rho(&ridge_preds, &y_test_f));
        }

        // Depth-2 RandomForest (50 estimators)
        let mut rng = SmallRng::seed_from_u64(42 + fold as u64);
        let tree_params = DecisionTreeParams::new().max_depth(Some(2));
        let n_tree_feats = ((n_feats as f64).sqrt().ceil() as usize).max(1);
        let bootstrap_n = ((n_train as f64) * 0.8).round() as usize;

        let mut cv_trees: Vec<SerializedTree> = Vec::with_capacity(50);
        for _ in 0..50usize {
            let feat_indices: Vec<usize> = index_sample(&mut rng, n_feats, n_tree_feats).into_vec();
            let boot_rows: Vec<usize> = (0..bootstrap_n)
                .map(|_| rng.gen_range(0..n_train))
                .collect();
            let x_boot = Array2::from_shape_fn((bootstrap_n, n_tree_feats), |(r, c)| {
                x_train[[boot_rows[r], feat_indices[c]]]
            });
            let y_boot: Array1<bool> =
                Array1::from_shape_fn(bootstrap_n, |r| y_train_bool[boot_rows[r]]);
            let boot_ds = Dataset::new(x_boot, y_boot);
            if let Ok(tree) = tree_params.fit(&boot_ds) {
                cv_trees.push(SerializedTree {
                    nodes: serialize_tree(&tree),
                    feature_indices: feat_indices,
                });
            }
        }

        if !cv_trees.is_empty() {
            let tree_preds: Vec<f64> = (0..n_test)
                .map(|r| {
                    let mut feats_arr = [0.0f64; 10];
                    for c in 0..n_feats.min(10) {
                        feats_arr[c] = x_test[[r, c]];
                    }
                    let votes: usize = cv_trees.iter().map(|t| vote(t, &feats_arr) as usize).sum();
                    votes as f64 / cv_trees.len() as f64
                })
                .collect();
            tree_rhos.push(spearman_rho(&tree_preds, &y_test_f));
        }
    }

    if ridge_rhos.is_empty() || tree_rhos.is_empty() {
        return (RegimeVerdict::Unreliable, 0.0);
    }

    let avg_ridge = ridge_rhos.iter().sum::<f64>() / ridge_rhos.len() as f64;
    let avg_tree = tree_rhos.iter().sum::<f64>() / tree_rhos.len() as f64;
    let delta = avg_tree - avg_ridge;

    let verdict = if delta < 0.03 {
        RegimeVerdict::Linear
    } else if delta <= 0.10 {
        RegimeVerdict::Weak
    } else {
        RegimeVerdict::Strong
    };

    (verdict, delta)
}

/// Fit a Ridge regression ranker on standardized features.
/// Returns a `RidgeRanker` storing coefficients and standardization params.
pub fn train_ridge(x: &Array2<f64>, y: &Array1<bool>) -> Result<RidgeRanker> {
    let n = x.nrows();
    let n_feats = x.ncols();

    let mean: Vec<f64> = (0..n_feats)
        .map(|c| x.column(c).mean().unwrap_or(0.0))
        .collect();
    let std_dev: Vec<f64> = (0..n_feats)
        .map(|c| {
            let col = x.column(c);
            let m = mean[c];
            let var = col.iter().map(|&v| (v - m).powi(2)).sum::<f64>() / n as f64;
            var.sqrt().max(1e-8)
        })
        .collect();

    let x_std = Array2::from_shape_fn((n, n_feats), |(r, c)| (x[[r, c]] - mean[c]) / std_dev[c]);
    let y_f: Array1<f64> = y.mapv(|b| if b { 1.0 } else { 0.0 });

    let dataset = Dataset::new(x_std, y_f);
    let model = TweedieRegressor::params()
        .alpha(1.0_f64)
        .power(0.0_f64)
        .fit(&dataset)
        .context("ridge fit failed")?;

    Ok(RidgeRanker {
        coefficients: model.coef.to_vec(),
        intercept: model.intercept,
        mean,
        std: std_dev,
    })
}

// ── Inference ─────────────────────────────────────────────────────────────────

/// Score a function using the trained ranker.
/// Dispatches to Ridge or RandomForest based on `model.model_class`.
pub fn score(model: &RankerModel, func: &FunctionSnapshot) -> f64 {
    let feats = extract_features(func);
    match model.model_class {
        ModelClass::Ridge => {
            if let Some(ridge) = &model.ridge {
                let n_feats = ridge.coefficients.len().min(feats.len());
                let raw: f64 = (0..n_feats)
                    .map(|i| {
                        let std = ridge.std.get(i).copied().unwrap_or(1.0).max(1e-8);
                        let mean = ridge.mean.get(i).copied().unwrap_or(0.0);
                        let x_std = (feats[i] - mean) / std;
                        ridge.coefficients[i] * x_std
                    })
                    .sum::<f64>()
                    + ridge.intercept;
                raw.clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        ModelClass::RandomForest => {
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
    }
}

fn vote(tree: &SerializedTree, feats: &[f64; 10]) -> bool {
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
        // Map tree-local feature index through the per-tree feature subset
        let global_idx = tree
            .feature_indices
            .get(node.feature_idx)
            .copied()
            .unwrap_or(node.feature_idx);
        let val = feats.get(global_idx).copied().unwrap_or(0.0);
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
    #[serde(default)]
    pub regime_verdict: Option<RegimeVerdict>,
    #[serde(default)]
    pub regime_delta: f64,
}

fn default_model_version() -> u32 {
    1
}

fn default_model_class() -> ModelClass {
    ModelClass::RandomForest
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankerModel {
    #[serde(default = "default_model_version")]
    pub model_version: u32,
    pub trees: Vec<SerializedTree>,
    pub meta: TrainMeta,
    #[serde(default = "default_model_class")]
    pub model_class: ModelClass,
    #[serde(default)]
    pub ridge: Option<RidgeRanker>,
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
        if model.model_version < 5 {
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

    // ── precision_at_k ────────────────────────────────────────────────────────

    fn make_scored(ids: &[&str], scores: &[f64]) -> Vec<ScoredFunction> {
        ids.iter()
            .zip(scores.iter())
            .map(|(id, &s)| ScoredFunction {
                function_id: id.to_string(),
                score: s,
            })
            .collect()
    }

    fn make_labels(pairs: &[(&str, bool)]) -> std::collections::HashMap<FunctionId, bool> {
        pairs.iter().map(|(id, v)| (id.to_string(), *v)).collect()
    }

    #[test]
    fn pak_all_positives() {
        let ranked = make_scored(
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"],
            &[1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
        );
        let labels = make_labels(&[
            ("a", true),
            ("b", true),
            ("c", true),
            ("d", true),
            ("e", true),
            ("f", true),
            ("g", true),
            ("h", true),
            ("i", true),
            ("j", true),
        ]);
        assert_eq!(precision_at_k(&ranked, &labels, 10), 1.0);
    }

    #[test]
    fn pak_no_positives() {
        let ranked = make_scored(
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"],
            &[1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
        );
        let labels = make_labels(&[
            ("a", false),
            ("b", false),
            ("c", false),
            ("d", false),
            ("e", false),
            ("f", false),
            ("g", false),
            ("h", false),
            ("i", false),
            ("j", false),
        ]);
        assert_eq!(precision_at_k(&ranked, &labels, 10), 0.0);
    }

    #[test]
    fn pak_k_larger_than_list() {
        let ranked = make_scored(&["a", "b", "c", "d", "e"], &[1.0, 0.9, 0.8, 0.7, 0.6]);
        let labels = make_labels(&[
            ("a", true),
            ("b", false),
            ("c", true),
            ("d", false),
            ("e", false),
        ]);
        let result = precision_at_k(&ranked, &labels, 20);
        assert!(result <= 1.0);
        assert!((result - 0.4).abs() < 1e-10);
    }

    // ── Feature names ─────────────────────────────────────────────────────────

    #[test]
    fn feature_names_count_matches_array() {
        assert_eq!(FEATURE_NAMES.len(), 10);
        assert!(FEATURE_NAMES.contains(&"convention_bug_fix_count"));
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

    // ── screen_repo ───────────────────────────────────────────────────────────

    fn make_snapshot_with_activity(scores: &[f64]) -> Snapshot {
        use crate::language::Language;
        use crate::report::MetricsReport;
        use crate::risk::RiskBand;
        use crate::snapshot::{AnalysisInfo, CommitInfo, FunctionSnapshot};

        let functions = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| FunctionSnapshot {
                function_id: format!("f{i}"),
                file: "src/lib.rs".into(),
                line: i as u32 + 1,
                language: Language::Rust,
                metrics: MetricsReport {
                    cc: 1,
                    nd: 0,
                    fo: 0,
                    ns: 0,
                    loc: 10,
                },
                lrs: 0.0,
                band: RiskBand::Low,
                suppression_reason: None,
                churn: None,
                touch_count_30d: None,
                days_since_last_change: None,
                callgraph: None,
                activity_risk: Some(s),
                risk_factors: None,
                percentile: None,
                driver: None,
                driver_detail: None,
                quadrant: None,
                patterns: vec![],
                pattern_details: None,
                subsystem: None,
                authors_90d: None,
                directed_coupling: None,
                jaccard_label_stability: None,
                convention_bug_fix_count: None,
                burst_score: None,
                commit_count: None,
                author_count: None,
                author_entropy: None,
                isolation_rate: None,
                age_days: None,
                last_touch_days: None,
                explanation: None,
            })
            .collect();

        Snapshot {
            schema_version: 1,
            commit: CommitInfo {
                sha: "test".into(),
                parents: vec![],
                timestamp: 0,
                branch: None,
                message: None,
                author: None,
                is_fix_commit: None,
                is_revert_commit: None,
                ticket_ids: vec![],
            },
            analysis: AnalysisInfo {
                scope: "test".into(),
                tool_version: "0.0.0".into(),
            },
            functions,
            summary: None,
            aggregates: None,
        }
    }

    #[test]
    fn screener_skip_ft_below_threshold() {
        let snap = make_snapshot_with_activity(&[0.01, 0.01, 0.01]);
        let (verdict, mean_hs) = screen_repo(&snap);
        assert_eq!(verdict, ScreenerVerdict::SkipFt);
        assert!(mean_hs < SCREENER_SKIP_THRESHOLD);
    }

    #[test]
    fn screener_ambiguous_zone() {
        let snap = make_snapshot_with_activity(&[0.045, 0.045, 0.045]);
        let (verdict, mean_hs) = screen_repo(&snap);
        assert_eq!(verdict, ScreenerVerdict::Ambiguous);
        assert!((SCREENER_SKIP_THRESHOLD..SCREENER_RUN_THRESHOLD).contains(&mean_hs));
    }

    #[test]
    fn screener_run_ft_above_threshold() {
        let snap = make_snapshot_with_activity(&[0.15, 0.15, 0.15]);
        let (verdict, _) = screen_repo(&snap);
        assert_eq!(verdict, ScreenerVerdict::RunFt);
    }

    #[test]
    fn screener_empty_snapshot_is_skip() {
        let snap = make_snapshot_with_activity(&[]);
        let (verdict, mean_hs) = screen_repo(&snap);
        assert_eq!(verdict, ScreenerVerdict::SkipFt);
        assert_eq!(mean_hs, 0.0);
    }

    #[test]
    fn load_v3_model_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        std::fs::write(
            &path,
            r#"{"model_version":3,"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let err = RankerModel::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("retrain") || err.contains("model_version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_v4_model_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        std::fs::write(
            &path,
            r#"{"model_version":4,"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let err = RankerModel::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("retrain") || err.contains("model_version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_v5_model_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ranker.json");
        std::fs::write(
            &path,
            r#"{"model_version":5,"trees":[],"meta":{"n_samples":100,"n_pos":50,"n_neg":50,"label_window_days":365,"n_estimators":10,"max_depth":3}}"#,
        )
        .unwrap();
        let model = RankerModel::load(&path).expect("should load");
        assert_eq!(model.model_version, 5);
    }

    // ── cold-start routing (F62/F63) ────────────────────────────────────────────

    #[test]
    fn gini_uniform_distribution_is_zero() {
        assert_eq!(gini_coefficient(&[1.0, 1.0, 1.0, 1.0]), 0.0);
    }

    #[test]
    fn gini_single_dominant_value_matches_hand_computation() {
        let g = gini_coefficient(&[0.0, 0.0, 0.0, 10.0]);
        assert!((g - 0.75).abs() < 1e-9, "got {g}");
    }

    #[test]
    fn gini_ascending_series_matches_hand_computation() {
        let g = gini_coefficient(&[1.0, 2.0, 3.0, 4.0]);
        assert!((g - 0.25).abs() < 1e-9, "got {g}");
    }

    #[test]
    fn gini_empty_is_zero() {
        assert_eq!(gini_coefficient(&[]), 0.0);
    }

    fn make_snapshot_with_commit_counts(counts: &[u32]) -> Snapshot {
        use crate::language::Language;
        use crate::report::MetricsReport;
        use crate::risk::RiskBand;
        use crate::snapshot::{AnalysisInfo, CommitInfo, FunctionSnapshot};

        let functions = counts
            .iter()
            .enumerate()
            .map(|(i, &cc)| FunctionSnapshot {
                function_id: format!("f{i}"),
                file: format!("src/f{i}.rs"),
                line: 1,
                language: Language::Rust,
                metrics: MetricsReport {
                    cc: 1,
                    nd: 0,
                    fo: 0,
                    ns: 0,
                    loc: 10,
                },
                lrs: (i as f64) / (counts.len() as f64),
                band: RiskBand::Low,
                suppression_reason: None,
                churn: None,
                touch_count_30d: None,
                days_since_last_change: None,
                callgraph: None,
                activity_risk: Some((i as f64) / (counts.len() as f64)),
                risk_factors: None,
                percentile: None,
                driver: None,
                driver_detail: None,
                quadrant: None,
                patterns: vec![],
                pattern_details: None,
                subsystem: None,
                authors_90d: Some(1),
                directed_coupling: None,
                jaccard_label_stability: None,
                convention_bug_fix_count: None,
                burst_score: Some(1.0),
                commit_count: Some(cc),
                author_count: Some(1),
                author_entropy: Some(0.0),
                isolation_rate: Some(0.5),
                age_days: Some(30.0),
                last_touch_days: Some(1.0),
                explanation: None,
            })
            .collect();

        Snapshot {
            schema_version: 1,
            commit: CommitInfo {
                sha: "test".into(),
                parents: vec![],
                timestamp: 0,
                branch: None,
                message: None,
                author: None,
                is_fix_commit: None,
                is_revert_commit: None,
                ticket_ids: vec![],
            },
            analysis: AnalysisInfo {
                scope: "test".into(),
                tool_version: "0.0.0".into(),
            },
            functions,
            summary: None,
            aggregates: None,
        }
    }

    #[test]
    fn cold_start_uniform_prior_when_no_file_stands_out() {
        // Flat distribution: top-decile share = 0.10 < 0.20 guard.
        let counts = vec![1u32; 20];
        let snap = make_snapshot_with_commit_counts(&counts);
        let result = cold_start_rank(&snap);
        assert_eq!(result.route, ColdStartRoute::UniformPrior);
        assert!(result.ranked.is_empty());
    }

    #[test]
    fn cold_start_formula_route_ranks_by_activity_risk() {
        // One dominant file: gini ≈ 0.56 (>= HIGH_GINI's neighborhood), top-decile
        // share ≈ 0.63 (passes the guard).
        let mut counts = vec![1u32; 20];
        counts[0] = 30;
        let snap = make_snapshot_with_commit_counts(&counts);
        let result = cold_start_rank(&snap);
        assert_eq!(result.route, ColdStartRoute::Formula);
        assert_eq!(result.ranked.len(), 20);
        // Ranked descending by activity_risk (lrs proxy: i/20, so f19 has the highest).
        assert_eq!(result.ranked[0].function_id, "f19");
        for w in result.ranked.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn cold_start_anomaly_route_when_gini_below_low_threshold() {
        // Low-concentration distribution (gini ≈ 0.31 < LOW_GINI) with top-decile
        // share ≈ 0.41 (passes the guard) — routes to Anomaly.
        let mut counts = vec![2u32; 30];
        counts[0] = 15;
        counts[1] = 12;
        counts[2] = 10;
        let snap = make_snapshot_with_commit_counts(&counts);
        let result = cold_start_rank(&snap);
        assert_eq!(result.route, ColdStartRoute::Anomaly);
        assert_eq!(result.ranked.len(), 30);
        for w in result.ranked.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn cold_start_empty_snapshot_is_uniform_prior_and_does_not_panic() {
        let snap = make_snapshot_with_commit_counts(&[]);
        let result = cold_start_rank(&snap);
        assert_eq!(result.route, ColdStartRoute::UniformPrior);
        assert!(result.ranked.is_empty());
    }

    #[test]
    fn cold_start_features_never_panics_on_none_fields() {
        use crate::language::Language;
        use crate::report::MetricsReport;
        use crate::risk::RiskBand;
        use crate::snapshot::FunctionSnapshot;

        let func = FunctionSnapshot {
            function_id: "f0".into(),
            file: "src/f0.rs".into(),
            line: 1,
            language: Language::Rust,
            metrics: MetricsReport {
                cc: 1,
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 10,
            },
            lrs: 0.0,
            band: RiskBand::Low,
            suppression_reason: None,
            churn: None,
            touch_count_30d: None,
            days_since_last_change: None,
            callgraph: None,
            activity_risk: None,
            risk_factors: None,
            percentile: None,
            driver: None,
            driver_detail: None,
            quadrant: None,
            patterns: vec![],
            pattern_details: None,
            subsystem: None,
            authors_90d: None,
            directed_coupling: None,
            jaccard_label_stability: None,
            convention_bug_fix_count: None,
            burst_score: None,
            commit_count: None,
            author_count: None,
            author_entropy: None,
            isolation_rate: None,
            age_days: None,
            last_touch_days: None,
            explanation: None,
        };
        assert_eq!(cold_start_features(&func), [0.0; 8]);
    }
}
