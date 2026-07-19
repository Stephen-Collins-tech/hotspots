//! Cold-start history signals — F63 signal-porting prerequisite.
//!
//! Seven file-level signals (six F62/F63 cold-start signals plus `burst_score`,
//! F93), computed from a single `git log` pass over the whole repo (not one
//! subprocess per file, unlike `populate_authors_90d`/`populate_convention_bug_fix_count`).
//! `burst_score`'s formula lives here too (moved from `snapshot.rs`) so
//! `Snapshot::populate_burst_score` can reuse this module's loader instead of
//! spawning its own redundant full-history `git log` walk — see
//! `hotspots-research/docs/promotion-briefs/burst-score-history-signals-shared-walk.md`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Separator used in git log --format to delimit commits from file lists.
const SEP: &str = "@@HC@@";

/// A single commit: timestamp, author email, and touched files.
pub struct CommitRecord {
    pub ts: i64,
    pub author: String,
    pub files: Vec<String>,
}

/// Per-file cold-start signals (F62/F63 feature set) plus `burst_score` (F93).
pub struct HistorySignals {
    pub commit_count: u32,
    pub author_count: u32,
    pub author_entropy: f64,
    pub isolation_rate: f64,
    pub age_days: f64,
    pub last_touch_days: f64,
    pub burst_score: f64,
}

/// Load full commit history as `(timestamp, author_email, files)` via a single
/// `git log` subprocess call. Returns an empty vec on any error (caller treats
/// as no-op, matching `coupling::load_commits`'s soft-failure convention).
pub(crate) fn load_commits_with_author(git_dir: &Path) -> Vec<CommitRecord> {
    let format = format!("{SEP}%at {SEP}%ae {SEP}%s");
    let out = Command::new("git")
        .args([
            "--git-dir",
            &git_dir.to_string_lossy(),
            "log",
            "--name-only",
            "--diff-filter=ACDMRT",
            &format!("--format={format}"),
        ])
        .output();

    let stdout = match out {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => o.stdout,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&stdout);

    let mut commits: Vec<CommitRecord> = Vec::new();
    let mut cur_ts: i64 = 0;
    let mut cur_author = String::new();
    let mut cur_files: Vec<String> = Vec::new();
    let mut in_commit = false;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(SEP) {
            if in_commit && !cur_files.is_empty() {
                commits.push(CommitRecord {
                    ts: cur_ts,
                    author: cur_author.clone(),
                    files: cur_files.clone(),
                });
            }
            let (ts_str, tail) = rest.split_once(' ').unwrap_or((rest, ""));
            cur_ts = ts_str.parse().unwrap_or(0);
            let author = tail.strip_prefix(SEP).unwrap_or(tail);
            let author = author.split(SEP).next().unwrap_or("").trim();
            cur_author = author.to_lowercase();
            cur_files = Vec::new();
            in_commit = true;
        } else if in_commit && !line.trim().is_empty() {
            cur_files.push(line.trim().to_string());
        }
    }
    if in_commit && !cur_files.is_empty() {
        commits.push(CommitRecord {
            ts: cur_ts,
            author: cur_author,
            files: cur_files,
        });
    }

    commits
}

/// Shannon entropy of the commit-author distribution. Mirrors
/// `cheap_signals.py::_author_entropy`.
fn author_entropy(authors: &[&str]) -> f64 {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for &a in authors {
        *counts.entry(a).or_insert(0) += 1;
    }
    let total = authors.len() as f64;
    if total == 0.0 {
        return 0.0;
    }
    -counts
        .values()
        .map(|&c| {
            let p = c as f64 / total;
            p * p.log2()
        })
        .sum::<f64>()
}

/// Sliding 30-day-window max/mean commit ratio (F93). Moved from `snapshot.rs`
/// so it shares this module's single-pass commit load with the other six
/// cold-start signals instead of triggering its own `git log` walk.
///
/// For each commit, counts how many commits (including itself) fall within the
/// following 30-day window, then divides the maximum such count by the mean.
/// Mirrors `cheap_signals.py::_burst_score` in `hotspots-research`. Returns
/// `1.0` (no burst signal) when fewer than 2 timestamps are given.
fn burst_score(timestamps: &[i64]) -> f64 {
    const BURST_WINDOW_DAYS: i64 = 30;
    const BURST_WINDOW_SECS: i64 = BURST_WINDOW_DAYS * 86400;

    if timestamps.len() < 2 {
        return 1.0;
    }

    let mut sorted = timestamps.to_vec();
    sorted.sort_unstable();

    let mut counts = Vec::with_capacity(sorted.len());
    let mut j = 0usize;
    for (i, &t) in sorted.iter().enumerate() {
        while j < sorted.len() && sorted[j] < t + BURST_WINDOW_SECS {
            j += 1;
        }
        counts.push((j - i) as f64);
    }

    let mean_c = counts.iter().sum::<f64>() / counts.len() as f64;
    if mean_c == 0.0 {
        return 1.0;
    }

    counts.iter().cloned().fold(f64::MIN, f64::max) / mean_c
}

/// Single pass over `commits`, aggregating per-file cold-start signals plus
/// `burst_score`. Mirrors `cheap_signals.py::compute_file_signals` (minus
/// `co_change_partners`, `mean_co_change_size` — out of scope, see brief).
pub fn compute_history_signals(commits: &[CommitRecord]) -> HashMap<String, HistorySignals> {
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // commit index -> number of files touched (for isolation_rate)
    let commit_file_count: Vec<usize> = commits.iter().map(|c| c.files.len()).collect();

    let mut file_commit_idxs: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, commit) in commits.iter().enumerate() {
        for f in &commit.files {
            file_commit_idxs.entry(f.as_str()).or_default().push(idx);
        }
    }

    let mut signals = HashMap::with_capacity(file_commit_idxs.len());
    for (file, idxs) in file_commit_idxs {
        let timestamps: Vec<i64> = idxs.iter().map(|&i| commits[i].ts).collect();
        let authors: Vec<&str> = idxs.iter().map(|&i| commits[i].author.as_str()).collect();

        let n = idxs.len();
        let isolated = idxs.iter().filter(|&&i| commit_file_count[i] == 1).count();
        let isolation_rate = isolated as f64 / n as f64;

        let unique_authors: HashSet<&str> = authors.iter().copied().collect();

        let first_ts = *timestamps.iter().min().unwrap_or(&0);
        let last_ts = *timestamps.iter().max().unwrap_or(&0);

        signals.insert(
            file.to_string(),
            HistorySignals {
                commit_count: n as u32,
                author_count: unique_authors.len() as u32,
                author_entropy: author_entropy(&authors),
                isolation_rate,
                age_days: (last_ts - first_ts) as f64 / 86400.0,
                last_touch_days: (now_ts - last_ts) as f64 / 86400.0,
                burst_score: burst_score(&timestamps),
            },
        );
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(ts: i64, author: &str, files: &[&str]) -> CommitRecord {
        CommitRecord {
            ts,
            author: author.to_string(),
            files: files.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Hand-computed fixture: 3 files, ≤10 commits, matching the acceptance
    /// criteria in the F63 signal-porting-prerequisite brief.
    ///
    /// Commits (ts in days since epoch, converted to seconds):
    ///   d0: alice   -> a.rs, b.rs   (co-touch, both non-isolated)
    ///   d1: alice   -> a.rs         (isolated)
    ///   d5: bob     -> a.rs         (isolated)
    ///   d5: bob     -> c.rs         (isolated)
    ///   d10: alice  -> b.rs         (isolated)
    fn fixture_commits() -> Vec<CommitRecord> {
        const DAY: i64 = 86400;
        vec![
            commit(0, "alice@example.com", &["a.rs", "b.rs"]),
            commit(DAY, "alice@example.com", &["a.rs"]),
            commit(5 * DAY, "bob@example.com", &["a.rs"]),
            commit(5 * DAY, "bob@example.com", &["c.rs"]),
            commit(10 * DAY, "alice@example.com", &["b.rs"]),
        ]
    }

    #[test]
    fn author_entropy_matches_hand_computation() {
        // a.rs touched by alice, alice, bob -> counts {alice:2, bob:1}, total=3
        // H = -[ (2/3)log2(2/3) + (1/3)log2(1/3) ] ≈ 0.9183
        let authors = vec!["alice", "alice", "bob"];
        let h = author_entropy(&authors);
        assert!((h - 0.9182958).abs() < 1e-6, "got {h}");
    }

    #[test]
    fn author_entropy_single_author_is_zero() {
        let authors = vec!["alice", "alice", "alice"];
        assert_eq!(author_entropy(&authors), 0.0);
    }

    #[test]
    fn compute_history_signals_matches_fixture() {
        const DAY: i64 = 86400;
        let commits = fixture_commits();
        let signals = compute_history_signals(&commits);

        // a.rs: commits at d0 (2-file, not isolated), d1 (isolated), d5 (isolated)
        let a = signals.get("a.rs").expect("a.rs present");
        assert_eq!(a.commit_count, 3);
        assert_eq!(a.author_count, 2); // alice, bob
        assert!((a.isolation_rate - (2.0 / 3.0)).abs() < 1e-9);
        assert!((a.age_days - 5.0).abs() < 1e-9); // (5*DAY - 0) / DAY
        let expected_last_touch = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 5 * DAY) as f64
            / 86400.0;
        assert!((a.last_touch_days - expected_last_touch).abs() < 1.0);

        // b.rs: commits at d0 (2-file, not isolated), d10 (isolated)
        let b = signals.get("b.rs").expect("b.rs present");
        assert_eq!(b.commit_count, 2);
        assert_eq!(b.author_count, 1); // alice only (case-folded)
        assert!((b.isolation_rate - 0.5).abs() < 1e-9);
        assert!((b.age_days - 10.0).abs() < 1e-9);

        // c.rs: single commit at d5, isolated
        let c = signals.get("c.rs").expect("c.rs present");
        assert_eq!(c.commit_count, 1);
        assert_eq!(c.author_count, 1);
        assert_eq!(c.isolation_rate, 1.0);
        assert_eq!(c.age_days, 0.0); // only one timestamp -> first == last
    }

    #[test]
    fn compute_history_signals_no_entry_for_untouched_file() {
        let commits = fixture_commits();
        let signals = compute_history_signals(&commits);
        assert!(!signals.contains_key("d.rs"));
    }

    #[test]
    fn load_commits_with_author_returns_empty_on_bad_path() {
        let bad = Path::new("/nonexistent/path/that/does/not/exist/.git");
        let commits = load_commits_with_author(bad);
        assert!(commits.is_empty());
    }

    #[test]
    fn burst_score_single_commit_is_baseline() {
        assert_eq!(burst_score(&[0]), 1.0);
    }

    #[test]
    fn burst_score_evenly_spaced_commits_is_baseline() {
        // Commits 60 days apart never share a 30-day window with another commit,
        // so max == mean == 1.0 for every commit.
        const DAY: i64 = 86400;
        let timestamps = vec![0, 60 * DAY, 120 * DAY, 180 * DAY];
        assert_eq!(burst_score(&timestamps), 1.0);
    }

    #[test]
    fn burst_score_detects_a_burst() {
        // 5 commits clustered on day 0, then 1 commit alone on day 60: the
        // clustered commits' windows each see all 5, giving max=5, mean=(5*5+1)/6.
        const DAY: i64 = 86400;
        let timestamps = vec![0, 0, 0, 0, 0, 60 * DAY];
        let score = burst_score(&timestamps);
        assert!(score > 1.0, "expected a burst signal, got {score}");
    }

    #[test]
    fn compute_history_signals_matches_burst_score_fixture() {
        let commits = fixture_commits();
        let signals = compute_history_signals(&commits);
        // a.rs: commits at d0, d1, d5 — all within 30 days of each other, so
        // the forward-window counts are [3, 2, 1] (max=3, mean=2, ratio=1.5).
        let a = signals.get("a.rs").expect("a.rs present");
        assert!((a.burst_score - 1.5).abs() < 1e-9, "got {}", a.burst_score);
        // c.rs: single commit -> baseline 1.0.
        let c = signals.get("c.rs").expect("c.rs present");
        assert_eq!(c.burst_score, 1.0);
    }
}
