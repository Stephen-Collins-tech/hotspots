//! Directed coupling signal — F37/F38/F39.
//!
//! Directed coupling weights co-change relationships by partner defect score,
//! filtering infrastructure noise: coupling to defect-prone files is more
//! ominous than coupling to healthy utility code.
//!
//! Formula per file i:
//!   dc(i) = Σ_j [co_changes(i,j) × partner_score(j)] / commit_count(i)
//!
//! The Jaccard screener gates which variant to use:
//!   Jaccard < DC_JACCARD_THRESHOLD → dc_365d (architecturally volatile repo)
//!   Jaccard ≥ DC_JACCARD_THRESHOLD → dc_full (stable repo, full history is an asset)

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Minimum commit appearances for a file to receive a directed coupling score.
pub const MIN_DC_APPEARANCES: usize = 10;

/// Jaccard threshold: below this, use the 365-day window variant.
pub const DC_JACCARD_THRESHOLD: f64 = 0.15;

/// Train/holdout split fraction for Jaccard stability computation.
pub const DC_TRAIN_PCT: f64 = 0.8;

/// Top-file fraction used to compute Jaccard overlap.
pub const DC_TOP_FILE_PCT: f64 = 0.2;

/// 365-day window constant for windowed DC.
pub const DC_WINDOW_365D: u32 = 365;

/// Separator used in git log --format to delimit commits from file lists.
const SEP: &str = "@@C@@";

/// Regex patterns for fix-commit detection (mirrors the Python research scripts).
fn is_fix_subject(subject: &str) -> bool {
    let lower = subject.to_lowercase();
    // Keyword match
    if lower.contains("fix")
        || lower.contains("bug")
        || lower.contains("patch")
        || lower.contains("regression")
        || lower.contains("defect")
        || lower.contains("hotfix")
    {
        return true;
    }
    // Conventional commits: "fix(...):..." or "fix!:..."
    if let Some(rest) = lower.strip_prefix("fix") {
        let rest = rest
            .trim_start_matches(|c: char| c == '(' || c.is_alphanumeric() || c == '_' || c == '-');
        let rest = rest.trim_start_matches(')');
        let rest = rest.trim_start_matches('!');
        if rest.starts_with(':') {
            return true;
        }
    }
    false
}

/// Load all first-parent commits as `(timestamp_secs, subject, [file_paths])`.
///
/// Uses `git log --first-parent --name-only --diff-filter=ACDMRT` against the
/// repo at `git_dir`. Returns an empty vec on any error (caller treats as no-op).
fn load_commits(git_dir: &Path) -> Vec<(i64, String, Vec<String>)> {
    let format = format!("{}%at %s", SEP);
    let out = Command::new("git")
        .args([
            "--git-dir",
            &git_dir.to_string_lossy(),
            "log",
            "--first-parent",
            "--name-only",
            "--diff-filter=ACDMRT",
            &format!("--format={}", format),
        ])
        .output();

    let stdout = match out {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&stdout);

    let mut commits: Vec<(i64, String, Vec<String>)> = Vec::new();
    let mut cur_ts: i64 = 0;
    let mut cur_subj = String::new();
    let mut cur_files: Vec<String> = Vec::new();
    let mut in_commit = false;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(SEP) {
            if in_commit && !cur_files.is_empty() {
                commits.push((cur_ts, cur_subj.clone(), cur_files.clone()));
            }
            let (ts_str, subj) = rest.split_once(' ').unwrap_or((rest, ""));
            cur_ts = ts_str.parse().unwrap_or(0);
            cur_subj = subj.to_string();
            cur_files = Vec::new();
            in_commit = true;
        } else if in_commit && !line.trim().is_empty() {
            cur_files.push(line.trim().to_string());
        }
    }
    if in_commit && !cur_files.is_empty() {
        commits.push((cur_ts, cur_subj, cur_files));
    }

    commits.sort_by_key(|c| c.0);
    commits
}

/// Resolve the git directory path for a repo root.
///
/// Bare clones end in `.git`; working trees have a `.git` subdirectory.
pub fn git_dir(repo_root: &Path) -> std::path::PathBuf {
    if repo_root
        .file_name()
        .map(|n| n.to_string_lossy().ends_with(".git"))
        .unwrap_or(false)
    {
        repo_root.to_path_buf()
    } else {
        repo_root.join(".git")
    }
}

/// Compute directed coupling scores for all files in the commit list.
///
/// # Arguments
/// - `commits`: list of `(timestamp, subject, files)` — only commits in the
///   desired window should be passed; filtering by timestamp is the caller's job.
/// - `partner_scores`: map of `file → defect_score` used to weight co-changes.
/// - `min_appearances`: files appearing fewer times are excluded from output.
/// - `window_days`: when `Some(n)`, use only commits within the last `n` days
///   before the most-recent commit timestamp. When `None`, use all commits.
///
/// Returns a map of `file → directed_coupling_score`.
pub fn compute_directed_coupling(
    commits: &[(i64, String, Vec<String>)],
    partner_scores: &HashMap<String, f64>,
    min_appearances: usize,
    window_days: Option<u32>,
) -> HashMap<String, f64> {
    let filtered: Vec<&(i64, String, Vec<String>)> = if let Some(days) = window_days {
        let cutoff_ts = commits.last().map(|c| c.0).unwrap_or(0) - days as i64 * 86_400;
        commits.iter().filter(|c| c.0 >= cutoff_ts).collect()
    } else {
        commits.iter().collect()
    };

    let mut appearances: HashMap<String, usize> = HashMap::new();
    let mut weighted_co: HashMap<String, f64> = HashMap::new();

    for (_, _, files) in &filtered {
        let mut fs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
        fs.sort_unstable();
        fs.dedup();

        for &f in &fs {
            *appearances.entry(f.to_string()).or_insert(0) += 1;
        }

        for i in 0..fs.len() {
            for j in (i + 1)..fs.len() {
                let fa = fs[i];
                let fb = fs[j];
                if let Some(&sb) = partner_scores.get(fb) {
                    if sb > 0.0 {
                        *weighted_co.entry(fa.to_string()).or_insert(0.0) += sb;
                    }
                }
                if let Some(&sa) = partner_scores.get(fa) {
                    if sa > 0.0 {
                        *weighted_co.entry(fb.to_string()).or_insert(0.0) += sa;
                    }
                }
            }
        }
    }

    appearances
        .into_iter()
        .filter(|(_, count)| *count >= min_appearances)
        .map(|(f, count)| {
            let score = weighted_co.get(&f).copied().unwrap_or(0.0) / count as f64;
            (f, score)
        })
        .collect()
}

/// Compute Jaccard label stability for a repo.
///
/// Splits commit history at `train_pct`, computes the top `top_pct` fraction
/// of files by fix-commit count in each window, and returns the Jaccard overlap.
///
/// High Jaccard → same files are risky across time → use `dc_full`.
/// Low Jaccard → defect-prone files rotate → use `dc_365d`.
///
/// Returns `None` if either window has no fix commits.
pub fn compute_jaccard_stability(
    commits: &[(i64, String, Vec<String>)],
    train_pct: f64,
    top_pct: f64,
) -> Option<f64> {
    if commits.is_empty() {
        return None;
    }

    let cutoff_idx = (commits.len() as f64 * train_pct) as usize;
    let train = &commits[..cutoff_idx];
    let holdout = &commits[cutoff_idx..];

    let fix_counts = |window: &[(i64, String, Vec<String>)]| -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, subj, files) in window {
            if is_fix_subject(subj) {
                for f in files {
                    *counts.entry(f.clone()).or_insert(0) += 1;
                }
            }
        }
        counts
    };

    let train_fixes = fix_counts(train);
    let holdout_fixes = fix_counts(holdout);

    if train_fixes.is_empty() || holdout_fixes.is_empty() {
        return None;
    }

    let n_top = ((train_fixes.len() as f64 * top_pct) as usize).max(1);

    let mut train_ranked: Vec<(&String, usize)> =
        train_fixes.iter().map(|(f, &c)| (f, c)).collect();
    train_ranked.sort_by_key(|a| std::cmp::Reverse(a.1));
    let train_top: std::collections::HashSet<&str> = train_ranked
        .iter()
        .take(n_top)
        .map(|(f, _)| f.as_str())
        .collect();

    let n_holdout_top = ((holdout_fixes.len() as f64 * top_pct) as usize).max(1);
    let mut holdout_ranked: Vec<(&String, usize)> =
        holdout_fixes.iter().map(|(f, &c)| (f, c)).collect();
    holdout_ranked.sort_by_key(|a| std::cmp::Reverse(a.1));
    let holdout_top: std::collections::HashSet<&str> = holdout_ranked
        .iter()
        .take(n_holdout_top)
        .map(|(f, _)| f.as_str())
        .collect();

    let intersection = train_top.intersection(&holdout_top).count();
    let union = train_top.union(&holdout_top).count();

    if union == 0 {
        return None;
    }

    Some(intersection as f64 / union as f64)
}

/// High-level entry point: load commits from `repo_root`, compute Jaccard,
/// select the correct DC variant, and return scores keyed by file path.
///
/// Also returns the Jaccard value (for snapshot observability).
///
/// `partner_scores` is a map of `file → hotspots_score` from the snapshot.
pub fn compute_directed_coupling_for_repo(
    repo_root: &Path,
    partner_scores: &HashMap<String, f64>,
) -> (HashMap<String, f64>, Option<f64>) {
    let gd = git_dir(repo_root);
    if !gd.exists() {
        return (HashMap::new(), None);
    }

    let commits = load_commits(&gd);
    if commits.is_empty() {
        return (HashMap::new(), None);
    }

    // On large repos the git log traversal can take several seconds.
    // Surface this so users aren't surprised by latency in hotspots analyze.
    if commits.len() > 50_000 {
        eprintln!(
            "hotspots: directed coupling loading {} commits (large repo — may be slow)",
            commits.len()
        );
    }

    let jaccard = compute_jaccard_stability(&commits, DC_TRAIN_PCT, DC_TOP_FILE_PCT);

    let window_days = match jaccard {
        Some(j) if j < DC_JACCARD_THRESHOLD => Some(DC_WINDOW_365D),
        _ => None,
    };

    let scores =
        compute_directed_coupling(&commits, partner_scores, MIN_DC_APPEARANCES, window_days);

    (scores, jaccard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(ts: i64, subj: &str, files: &[&str]) -> (i64, String, Vec<String>) {
        (
            ts,
            subj.to_string(),
            files.iter().map(|f| f.to_string()).collect(),
        )
    }

    // ── is_fix_subject ────────────────────────────────────────────────────────

    #[test]
    fn fix_subject_keyword_variants() {
        assert!(is_fix_subject("fix: null deref"));
        assert!(is_fix_subject("bug in parser"));
        assert!(is_fix_subject("hotfix prod"));
        assert!(is_fix_subject("patch security hole"));
        assert!(is_fix_subject("regression in v2"));
        assert!(is_fix_subject("defect resolved"));
        assert!(!is_fix_subject("feat: add login"));
        assert!(!is_fix_subject("refactor: cleanup"));
        assert!(!is_fix_subject("chore: bump deps"));
    }

    // ── compute_directed_coupling ─────────────────────────────────────────────

    #[test]
    fn dc_empty_commits_returns_empty() {
        let scores = compute_directed_coupling(&[], &HashMap::new(), 1, None);
        assert!(scores.is_empty());
    }

    #[test]
    fn dc_single_file_commits_no_co_change() {
        let commits = vec![
            commit(1, "fix: a", &["a.rs"]),
            commit(2, "fix: a", &["a.rs"]),
            commit(3, "fix: a", &["a.rs"]),
        ];
        let partner_scores: HashMap<String, f64> = [("a.rs".to_string(), 1.0)].into();
        // a.rs never co-changes with anything, so weighted_co stays 0 → dc = 0
        let scores = compute_directed_coupling(&commits, &partner_scores, 1, None);
        assert_eq!(scores.get("a.rs").copied(), Some(0.0));
    }

    #[test]
    fn dc_co_change_weights_by_partner_score() {
        // a.rs and b.rs co-change in 10 commits; b.rs has score 2.0
        let commits: Vec<_> = (0..10)
            .map(|i| commit(i, "fix: x", &["a.rs", "b.rs"]))
            .collect();
        let partner_scores: HashMap<String, f64> = [("b.rs".to_string(), 2.0)].into();
        let scores = compute_directed_coupling(&commits, &partner_scores, 1, None);
        // a.rs appears 10 times; accumulated weight = 10 × 2.0 = 20.0; dc = 20/10 = 2.0
        assert!((scores["a.rs"] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dc_min_appearances_filters_rare_files() {
        let commits: Vec<_> = (0..5)
            .map(|i| commit(i, "fix: x", &["a.rs", "b.rs"]))
            .collect();
        let partner_scores: HashMap<String, f64> = [("b.rs".to_string(), 1.0)].into();
        // require 10 appearances — both files appear only 5 times
        let scores = compute_directed_coupling(&commits, &partner_scores, 10, None);
        assert!(scores.is_empty());
    }

    #[test]
    fn dc_window_restricts_to_recent_commits() {
        // 5 old commits outside window, 5 new commits inside window
        let mut commits: Vec<_> = (0..5)
            .map(|i| commit(i * 86_400, "chore: old", &["a.rs", "b.rs"]))
            .collect();
        let base = 1_000 * 86_400_i64;
        commits.extend((0..5).map(|i| commit(base + i * 86_400, "fix: new", &["a.rs", "b.rs"])));
        let partner_scores: HashMap<String, f64> = [("b.rs".to_string(), 1.0)].into();
        // window=10 days from last commit — only the 5 recent commits fall in
        let scores = compute_directed_coupling(&commits, &partner_scores, 1, Some(10));
        // a.rs appears 5 times in the window; dc = 5/5 = 1.0
        assert!((scores["a.rs"] - 1.0).abs() < 1e-9);
    }

    // ── compute_jaccard_stability ─────────────────────────────────────────────

    #[test]
    fn jaccard_empty_returns_none() {
        assert_eq!(compute_jaccard_stability(&[], 0.8, 0.2), None);
    }

    #[test]
    fn jaccard_no_fix_commits_returns_none() {
        let commits = vec![
            commit(1, "feat: add login", &["a.rs"]),
            commit(2, "chore: cleanup", &["b.rs"]),
        ];
        assert_eq!(compute_jaccard_stability(&commits, 0.8, 0.2), None);
    }

    #[test]
    fn jaccard_same_files_in_both_windows_returns_one() {
        // 10 fix commits on a.rs spread across both train and holdout windows
        let commits: Vec<_> = (0..10).map(|i| commit(i, "fix: x", &["a.rs"])).collect();
        let j = compute_jaccard_stability(&commits, 0.8, 0.2).unwrap();
        assert!((j - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_disjoint_files_returns_zero() {
        // train window: only a.rs; holdout window: only b.rs
        let mut commits: Vec<_> = (0..8).map(|i| commit(i, "fix: x", &["a.rs"])).collect();
        commits.extend((8..10).map(|i| commit(i, "fix: y", &["b.rs"])));
        let j = compute_jaccard_stability(&commits, 0.8, 0.2).unwrap();
        assert!((j - 0.0).abs() < 1e-9);
    }
}
