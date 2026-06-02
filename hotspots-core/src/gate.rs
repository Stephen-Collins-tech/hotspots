//! Suppression gate: detect when the activity-based ranker has failed on this repo
//! and recommend falling back to a code-semantic classifier.
//!
//! ## How it works
//!
//! 1. Scan the last 90 days of git history for commits whose message matches a
//!    fix-keyword heuristic (same as `detect_fix_commit`).
//! 2. Collect the set of files touched by those fix commits.
//! 3. Take the top-N functions by their current hotspot score (activity_risk or lrs).
//! 4. Compute P@10: of the top-10 functions, how many are in fix-touched files?
//! 5. If P@10 < threshold → the ranker has failed; return `GateVerdict::Suppressed`.
//!
//! ## Why calibrate on the top of the ranking?
//!
//! A random calibration sample has ~base_rate positives by chance, so it looks
//! fine even when the ranker is broken. Evaluating the top-N exposes failure
//! directly: if the ranker is working, the top functions should be bug-prone;
//! if it's not, they won't be.
//!
//! ## Promotion note
//!
//! The binary labels come from an on-the-fly commit scan — no pre-built holdout
//! required. The 90-day window matches the prediction horizon in both the ranker
//! and the LLM fallback prompt.

use crate::snapshot::FunctionSnapshot;
use crate::trainer::{make_rel, repo_prefixes};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

/// Result of the suppression gate check.
#[derive(Debug, Clone, PartialEq)]
pub enum GateVerdict {
    /// Ranker appears to be working — P@10 ≥ threshold.
    Pass {
        p_at_10: f64,
        fix_files_found: usize,
    },
    /// Ranker has failed — P@10 < threshold. Recommend LLM fallback.
    Suppressed {
        p_at_10: f64,
        fix_files_found: usize,
        threshold: f64,
    },
    /// Not enough data to calibrate (too few fix commits or functions).
    Inconclusive { reason: String },
}

impl GateVerdict {
    pub fn is_suppressed(&self) -> bool {
        matches!(self, GateVerdict::Suppressed { .. })
    }

    pub fn p_at_10(&self) -> Option<f64> {
        match self {
            GateVerdict::Pass { p_at_10, .. } => Some(*p_at_10),
            GateVerdict::Suppressed { p_at_10, .. } => Some(*p_at_10),
            GateVerdict::Inconclusive { .. } => None,
        }
    }
}

/// Configuration for the suppression gate.
#[derive(Debug, Clone)]
pub struct GateConfig {
    /// Number of days of git history to scan for fix commits.
    pub window_days: u32,
    /// Number of top-ranked functions to use for calibration.
    pub cal_n: usize,
    /// P@10 below this → suppress the ranker.
    pub threshold: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            window_days: 90,
            cal_n: 50,
            threshold: 0.5,
        }
    }
}

/// Run the suppression gate against the current repo and scored functions.
///
/// `functions` must already be scored (activity_risk populated where available)
/// and sorted descending by score — i.e. the output of `sort_reports` or equivalent.
pub fn check_gate(
    repo_root: &Path,
    functions: &[FunctionSnapshot],
    config: &GateConfig,
) -> GateVerdict {
    let fix_files = match scan_fix_files(repo_root, config.window_days) {
        Ok(f) => f,
        Err(e) => {
            return GateVerdict::Inconclusive {
                reason: format!("git scan failed: {e}"),
            }
        }
    };

    if fix_files.is_empty() {
        return GateVerdict::Inconclusive {
            reason: format!("no fix commits found in last {} days", config.window_days),
        };
    }

    if functions.len() < 10 {
        return GateVerdict::Inconclusive {
            reason: format!("too few functions to compute P@10 ({})", functions.len()),
        };
    }

    // Score top-cal_n functions descending (functions is already sorted).
    // Snapshot paths are absolute; fix_files has repo-relative paths from git log.
    // Normalise before comparison so the sets can actually intersect.
    let (prefix_can, prefix_raw) = repo_prefixes(repo_root);
    let cal_n = config.cal_n.min(functions.len());
    let top_n = &functions[..cal_n];

    // P@10: of top-10, how many files are in the fix-touched set?
    let p_at_10 = precision_at_k(top_n, &fix_files, 10, &prefix_can, &prefix_raw);

    if p_at_10 < config.threshold {
        GateVerdict::Suppressed {
            p_at_10,
            fix_files_found: fix_files.len(),
            threshold: config.threshold,
        }
    } else {
        GateVerdict::Pass {
            p_at_10,
            fix_files_found: fix_files.len(),
        }
    }
}

/// Scan git history for files touched by fix commits in the last `window_days` days.
fn scan_fix_files(repo_root: &Path, window_days: u32) -> anyhow::Result<HashSet<String>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let since = now - (window_days as i64 * 24 * 60 * 60);
    let since_arg = format!("--since={since}");

    let result = Command::new("git")
        .args(["log", "--format=COMMIT %s", "--name-only", &since_arg])
        .current_dir(repo_root)
        .output()?;

    if !result.status.success() {
        anyhow::bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let output = String::from_utf8_lossy(&result.stdout).into_owned();

    let mut fix_files = HashSet::new();
    let mut in_fix_commit = false;

    for line in output.lines() {
        if let Some(subject) = line.strip_prefix("COMMIT ") {
            in_fix_commit = crate::git::detect_fix_commit(subject);
        } else if in_fix_commit && !line.trim().is_empty() {
            fix_files.insert(line.trim().to_string());
        }
    }

    Ok(fix_files)
}

/// Precision at K: of the top-K functions by score, what fraction are in fix-touched files?
fn precision_at_k(
    functions: &[FunctionSnapshot],
    fix_files: &HashSet<String>,
    k: usize,
    prefix_can: &str,
    prefix_raw: &str,
) -> f64 {
    let k = k.min(functions.len());
    if k == 0 {
        return 0.0;
    }
    let hits = functions[..k]
        .iter()
        .filter(|f| fix_files.contains(&make_rel(&f.file, prefix_can, prefix_raw)))
        .count();
    hits as f64 / k as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix_set(files: &[&str]) -> HashSet<String> {
        files.iter().map(|s| s.to_string()).collect()
    }

    fn stub_fns(files: &[&str]) -> Vec<FunctionSnapshot> {
        // Build minimal FunctionSnapshot values using serde_json round-trip
        // to avoid depending on internal constructors.
        files
            .iter()
            .enumerate()
            .map(|(i, &file)| {
                let score = (files.len() - i) as f64;
                let json = serde_json::json!({
                    "function_id": format!("fn_{i}"),
                    "file": file,
                    "line": 1u32,
                    "language": "Rust",
                    "metrics": {"cc":1,"nd":0,"fo":0,"ns":0,"loc":5},
                    "lrs": score,
                    "band": "low",
                    "activity_risk": score,
                });
                serde_json::from_value(json).unwrap()
            })
            .collect()
    }

    #[test]
    fn test_precision_at_k_perfect() {
        let files = [
            "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs", "i.rs", "j.rs",
        ];
        let fns = stub_fns(&files);
        let fix_files = fix_set(&files);
        assert_eq!(precision_at_k(&fns, &fix_files, 10, "", ""), 1.0);
    }

    #[test]
    fn test_precision_at_k_zero() {
        let files = [
            "s1.rs", "s2.rs", "s3.rs", "s4.rs", "s5.rs", "s6.rs", "s7.rs", "s8.rs", "s9.rs",
            "s10.rs",
        ];
        let fns = stub_fns(&files);
        let fix_files = fix_set(&["bug.rs"]);
        assert_eq!(precision_at_k(&fns, &fix_files, 10, "", ""), 0.0);
    }

    #[test]
    fn test_precision_at_k_partial() {
        let files = [
            "bug.rs", "s2.rs", "s3.rs", "s4.rs", "s5.rs", "s6.rs", "s7.rs", "s8.rs", "s9.rs",
            "s10.rs",
        ];
        let fns = stub_fns(&files);
        let fix_files = fix_set(&["bug.rs"]);
        assert!((precision_at_k(&fns, &fix_files, 10, "", "") - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_gate_verdict_pass() {
        assert!(!GateVerdict::Pass {
            p_at_10: 0.8,
            fix_files_found: 5
        }
        .is_suppressed());
    }

    #[test]
    fn test_gate_verdict_suppressed() {
        assert!(GateVerdict::Suppressed {
            p_at_10: 0.0,
            fix_files_found: 3,
            threshold: 0.5
        }
        .is_suppressed());
    }
}
