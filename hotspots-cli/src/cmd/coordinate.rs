//! `hotspots coordinate` — coupling and ownership risk for a caller-specified
//! file set, before work starts.
//!
//! v1 minimal baseline (`hotspots-research/docs/promotion-briefs/coordinate-v1-minimal.md`):
//! given an explicit `--files` list, reports raw co-change coupling within the
//! set, files outside the set strongly coupled to something inside it
//! ("hidden dependencies"), per-file ownership signals, and a single
//! parallel-safety recommendation. Deliberately does not depend on any
//! snapshot or AST/CFG analysis — this command only needs git log.
//!
//! `knowledge_mode` (`hotspots-research/docs/promotion-briefs/coordinate-knowledge-mode.md`):
//! `FileOwnership.knowledge_mode` fires `"concentrated"` for narrow-ownership
//! files, derived purely from already-shipped `author_entropy`/`author_count`.
//!
//! `--diff`/`--staged` (`hotspots-research/docs/promotion-briefs/coordinate-diff-mode.md`):
//! alternate ways to derive the file set instead of `--files` — a unified
//! diff on stdin, or the current git staging area.

use anyhow::{bail, Context, Result};
use hotspots_core::coupling::compute_raw_coupling_for_repo;
use hotspots_core::history_signals::compute_history_signals_for_repo;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Files outside `--files` with coupling_ratio >= this to any input file are
/// reported as hidden dependencies.
const HIDDEN_DEP_THRESHOLD: f64 = 0.7;

/// Any within-set pair at or above this coupling_ratio makes the whole set
/// unsafe to parallelize.
const SERIALIZE_THRESHOLD: f64 = 0.7;

/// `knowledge_mode` fires "concentrated" when a file's ownership is this
/// narrow. Values from `coordinate-knowledge-mode.md` (F146) — 100%
/// cross-check match on every repo tested, not re-derived here.
const CONCENTRATED_MAX_AUTHOR_COUNT: u32 = 3;
const CONCENTRATED_MAX_NORMALIZED_ENTROPY: f64 = 0.5;

pub(crate) struct CoordinateArgs {
    pub files: Option<String>,
    /// Pre-read stdin contents from `--diff`; `None` if `--diff` wasn't passed.
    /// Read by `main.rs`, not here — keeps this module free of stdin I/O.
    pub diff: Option<String>,
    pub staged: bool,
    pub path: PathBuf,
}

#[derive(Debug, Serialize)]
struct CouplingPair {
    file_a: String,
    file_b: String,
    coupling_ratio: f64,
}

#[derive(Debug, Serialize)]
struct HiddenDependency {
    file: String,
    coupled_to: String,
    coupling_ratio: f64,
}

#[derive(Debug, Serialize)]
struct FileOwnership {
    file: String,
    author_count: u32,
    author_entropy: f64,
    newcomer_rate: Option<f64>,
    knowledge_mode: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct CoordinateOutput {
    schema_version: u32,
    input_files: Vec<String>,
    within_set: Vec<CouplingPair>,
    hidden_dependencies: Vec<HiddenDependency>,
    ownership: Vec<FileOwnership>,
    recommendation: String,
}

/// `knowledge_mode` classifier (F146) — fires "concentrated" only. "diffuse"
/// is deliberately unimplemented: F146 found it unreliable and repo-dependent
/// (5.9%-80% cross-check match), blocked on an author-identity-canonicalization
/// gap this brief does not attempt to fix.
fn classify_knowledge_mode(author_count: u32, author_entropy: f64) -> Option<&'static str> {
    if author_count <= 1 {
        return None;
    }
    let normalized_entropy = author_entropy / (author_count as f64).log2();
    if author_count <= CONCENTRATED_MAX_AUTHOR_COUNT
        && normalized_entropy < CONCENTRATED_MAX_NORMALIZED_ENTROPY
    {
        Some("concentrated")
    } else {
        None
    }
}

/// Extracts modified file paths from a unified diff via two rules: `+++ b/`
/// (additions, modifications, content-changing renames — new path only) and
/// `rename to` (pure renames with no content change, which emit no `+++`
/// line at all). Deduplicated, first-occurrence order preserved. Binary
/// files and deletions are intentionally excluded — verified against real
/// diffs in `coordinate-diff-input-pr-preflight.md`.
fn extract_files_from_diff(diff_text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    for line in diff_text.lines() {
        let path = if let Some(rest) = line.strip_prefix("+++ b/") {
            Some(rest)
        } else {
            line.strip_prefix("rename to ")
        };
        if let Some(path) = path {
            if !path.is_empty() && seen.insert(path.to_string()) {
                files.push(path.to_string());
            }
        }
    }
    files
}

/// `--staged`: `git diff --cached --name-only` already lists a pure rename's
/// new path natively — no regex parsing needed on this path (verified
/// directly, separate simpler code path from `--diff`'s text parsing).
fn staged_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("diff")
        .arg("--cached")
        .arg("--name-only")
        .current_dir(repo_root)
        .output()
        .context("run git diff --cached --name-only")?;
    if !output.status.success() {
        bail!(
            "git diff --cached --name-only failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

pub(crate) fn handle_coordinate(args: CoordinateArgs) -> Result<()> {
    let repo_root = args.path.canonicalize().context("resolve repo path")?;

    let provided = [args.files.is_some(), args.diff.is_some(), args.staged]
        .iter()
        .filter(|&&p| p)
        .count();
    if provided > 1 {
        bail!("--files, --diff, and --staged are mutually exclusive");
    }

    let input_files: Vec<String> = if let Some(files) = &args.files {
        files
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else if let Some(diff_text) = &args.diff {
        extract_files_from_diff(diff_text)
    } else if args.staged {
        staged_files(&repo_root)?
    } else {
        bail!("one of --files, --diff, or --staged is required");
    };

    let coupling = compute_raw_coupling_for_repo(&repo_root);
    let ownership_signals = compute_history_signals_for_repo(&repo_root);

    let input_set: std::collections::HashSet<&str> =
        input_files.iter().map(|s| s.as_str()).collect();

    let mut within_set = Vec::new();
    let mut hidden_dependencies: Vec<HiddenDependency> = Vec::new();
    // Track the max coupling_ratio seen for each hidden-dep candidate so a
    // file coupled to multiple input files is reported once, against its
    // strongest coupling partner.
    let mut hidden_dep_best: HashMap<String, HiddenDependency> = HashMap::new();

    for ((file_a, file_b), ratio) in &coupling {
        let a_in = input_set.contains(file_a.as_str());
        let b_in = input_set.contains(file_b.as_str());

        if a_in && b_in {
            within_set.push(CouplingPair {
                file_a: file_a.clone(),
                file_b: file_b.clone(),
                coupling_ratio: *ratio,
            });
        } else if *ratio >= HIDDEN_DEP_THRESHOLD && (a_in || b_in) {
            let (outside, inside) = if a_in {
                (file_b.clone(), file_a.clone())
            } else {
                (file_a.clone(), file_b.clone())
            };
            let better = hidden_dep_best
                .get(&outside)
                .map(|existing| *ratio > existing.coupling_ratio)
                .unwrap_or(true);
            if better {
                hidden_dep_best.insert(
                    outside.clone(),
                    HiddenDependency {
                        file: outside,
                        coupled_to: inside,
                        coupling_ratio: *ratio,
                    },
                );
            }
        }
    }
    hidden_dependencies.extend(hidden_dep_best.into_values());
    hidden_dependencies.sort_by(|a, b| a.file.cmp(&b.file));
    within_set.sort_by(|a, b| {
        a.file_a
            .cmp(&b.file_a)
            .then_with(|| a.file_b.cmp(&b.file_b))
    });

    let ownership: Vec<FileOwnership> = input_files
        .iter()
        .map(|file| {
            let signals = ownership_signals.get(file);
            let author_count = signals.map(|s| s.author_count).unwrap_or(0);
            let author_entropy = signals.map(|s| s.author_entropy).unwrap_or(0.0);
            FileOwnership {
                file: file.clone(),
                author_count,
                author_entropy,
                newcomer_rate: signals.and_then(|s| s.newcomer_rate),
                knowledge_mode: classify_knowledge_mode(author_count, author_entropy),
            }
        })
        .collect();

    let recommendation = if within_set
        .iter()
        .any(|p| p.coupling_ratio >= SERIALIZE_THRESHOLD)
    {
        "serialize"
    } else {
        "parallel_safe"
    };

    let output = CoordinateOutput {
        schema_version: 1,
        input_files,
        within_set,
        hidden_dependencies,
        ownership,
        recommendation: recommendation.to_string(),
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- classify_knowledge_mode (coordinate-knowledge-mode.md) --

    #[test]
    fn knowledge_mode_fires_concentrated() {
        // 2 authors, entropy 1.0 (50/50 split) -> normalized_entropy = 1.0,
        // NOT concentrated (fails the < 0.5 bound) despite low author_count.
        assert_eq!(classify_knowledge_mode(2, 1.0), None);
        // 2 authors, entropy near 0 (one author dominates almost entirely).
        assert_eq!(classify_knowledge_mode(2, 0.1), Some("concentrated"));
        // 3 authors at the author_count boundary, low entropy.
        assert_eq!(classify_knowledge_mode(3, 0.2), Some("concentrated"));
    }

    #[test]
    fn knowledge_mode_does_not_fire_above_author_count_bound() {
        // 5 authors, even with low raw entropy, exceeds CONCENTRATED_MAX_AUTHOR_COUNT.
        assert_eq!(classify_knowledge_mode(5, 0.2), None);
    }

    #[test]
    fn knowledge_mode_degenerate_single_author() {
        // author_count <= 1 must not panic on log2(1) == 0.0 division.
        assert_eq!(classify_knowledge_mode(1, 0.0), None);
        assert_eq!(classify_knowledge_mode(0, 0.0), None);
    }

    // -- extract_files_from_diff (coordinate-diff-mode.md) --

    #[test]
    fn extract_files_addition_or_modification() {
        let diff = "diff --git a/foo.rs b/foo.rs\n\
                     index 111..222 100644\n\
                     --- a/foo.rs\n\
                     +++ b/foo.rs\n\
                     @@ -1 +1 @@\n\
                     -old\n\
                     +new\n";
        assert_eq!(extract_files_from_diff(diff), vec!["foo.rs"]);
    }

    #[test]
    fn extract_files_excludes_deletion() {
        let diff = "diff --git a/gone.rs b/gone.rs\n\
                     deleted file mode 100644\n\
                     index 111..000\n\
                     --- a/gone.rs\n\
                     +++ /dev/null\n\
                     @@ -1 +0,0 @@\n\
                     -bye\n";
        assert!(extract_files_from_diff(diff).is_empty());
    }

    #[test]
    fn extract_files_content_changing_rename_uses_new_path_only() {
        let diff = "diff --git a/old_path.rs b/new_path.rs\n\
                     similarity index 90%\n\
                     rename from old_path.rs\n\
                     rename to new_path.rs\n\
                     index 111..222 100644\n\
                     --- a/old_path.rs\n\
                     +++ b/new_path.rs\n\
                     @@ -1 +1 @@\n\
                     -old\n\
                     +new\n";
        assert_eq!(extract_files_from_diff(diff), vec!["new_path.rs"]);
    }

    #[test]
    fn extract_files_pure_rename_has_no_plus_plus_plus_line() {
        // 100% similarity: git emits only rename from/to, no +++/--- at all.
        let diff = "diff --git a/old.rs b/new.rs\n\
                     similarity index 100%\n\
                     rename from old.rs\n\
                     rename to new.rs\n";
        assert_eq!(extract_files_from_diff(diff), vec!["new.rs"]);
    }

    #[test]
    fn extract_files_binary_addition_excluded() {
        let diff = "diff --git a/img.png b/img.png\n\
                     new file mode 100644\n\
                     index 000..111\n\
                     Binary files /dev/null and b/img.png differ\n";
        assert!(extract_files_from_diff(diff).is_empty());
    }

    #[test]
    fn extract_files_deduplicates_preserving_first_occurrence() {
        let diff = "+++ b/a.rs\n+++ b/a.rs\n+++ b/b.rs\n";
        assert_eq!(extract_files_from_diff(diff), vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn extract_files_ignores_malformed_empty_path_line() {
        // A "+++ b/" line with nothing after the prefix must not produce an
        // empty-string entry in the file list.
        let diff = "+++ b/\n+++ b/real.rs\n";
        assert_eq!(extract_files_from_diff(diff), vec!["real.rs"]);
    }

    // -- staged_files (coordinate-diff-mode.md, acceptance criterion 3 —
    // integration-style against a real git repo with a staged rename) --

    fn run_git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn staged_files_lists_pure_rename_new_path_natively() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        run_git(repo, &["init", "-q"]);
        run_git(repo, &["config", "user.email", "test@example.com"]);
        run_git(repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("old.rs"), "fn main() {}\n").unwrap();
        run_git(repo, &["add", "old.rs"]);
        run_git(repo, &["commit", "-q", "-m", "init"]);
        run_git(repo, &["mv", "old.rs", "new.rs"]);

        let files = staged_files(repo).unwrap();
        assert_eq!(files, vec!["new.rs".to_string()]);
    }
}
