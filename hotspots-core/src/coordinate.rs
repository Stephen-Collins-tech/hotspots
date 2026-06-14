use crate::git::{extract_co_change_pairs, CoChangePair};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

pub const CO_CHANGE_WINDOW_DAYS: u64 = 180;
pub const CO_CHANGE_MIN_COUNT: usize = 2;
/// coupling_ratio at or above this → serialize recommendation
pub const SERIALIZE_THRESHOLD: f64 = 0.4;
pub const OWNERSHIP_WINDOW_DAYS: u64 = 90;
/// Hidden deps with fewer co-changes than this are likely mass-commit artifacts
pub const HIDDEN_DEP_MIN_COUNT: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct PairSignal {
    pub file_a: String,
    pub file_b: String,
    pub co_change_count: usize,
    pub coupling_ratio: f64,
    pub has_static_dep: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HiddenDep {
    pub input_file: String,
    pub partner: String,
    pub co_change_count: usize,
    pub coupling_ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnershipSignal {
    pub file: String,
    pub author_count: usize,
    pub top_author_pct: f64,
}

#[derive(Debug, Clone)]
pub struct CoordinateReport {
    pub input_files: Vec<String>,
    pub pairs: Vec<PairSignal>,
    pub hidden_dependencies: Vec<HiddenDep>,
    pub ownership: Vec<OwnershipSignal>,
    pub parallel_safe: Vec<String>,
    pub serialize: Vec<String>,
}

pub fn coordinate(repo_root: &Path, files: &[String]) -> Result<CoordinateReport> {
    let input: HashSet<&str> = files.iter().map(|s| s.as_str()).collect();

    let all_pairs = extract_co_change_pairs(repo_root, CO_CHANGE_WINDOW_DAYS, CO_CHANGE_MIN_COUNT)
        .unwrap_or_default();

    let (pairs, hidden) = partition_pairs(&all_pairs, &input);
    let ownership = compute_ownership(repo_root, files)?;

    let must_serialize: HashSet<&str> = pairs
        .iter()
        .filter(|p| p.coupling_ratio >= SERIALIZE_THRESHOLD)
        .flat_map(|p| [p.file_a.as_str(), p.file_b.as_str()])
        .collect();

    let mut parallel_safe: Vec<String> = files
        .iter()
        .filter(|f| !must_serialize.contains(f.as_str()))
        .cloned()
        .collect();
    let mut serialize: Vec<String> = files
        .iter()
        .filter(|f| must_serialize.contains(f.as_str()))
        .cloned()
        .collect();
    parallel_safe.sort();
    serialize.sort();

    Ok(CoordinateReport {
        input_files: files.to_vec(),
        pairs,
        hidden_dependencies: hidden,
        ownership,
        parallel_safe,
        serialize,
    })
}

/// Split co-change pairs into within-set and hidden-dep sets.
///
/// Hidden deps are filtered to source-like files with at least HIDDEN_DEP_MIN_COUNT
/// co-changes. Low-count pairs at ratio 1.0 are typically mass-commit artifacts
/// (release bumps, reformats) that touch non-source files alongside everything else.
pub fn partition_pairs(
    pairs: &[CoChangePair],
    input: &HashSet<&str>,
) -> (Vec<PairSignal>, Vec<HiddenDep>) {
    let mut within: Vec<PairSignal> = Vec::new();
    let mut hidden: Vec<HiddenDep> = Vec::new();

    for pair in pairs {
        let a_in = input.contains(pair.file_a.as_str());
        let b_in = input.contains(pair.file_b.as_str());
        match (a_in, b_in) {
            (true, true) => within.push(PairSignal {
                file_a: pair.file_a.clone(),
                file_b: pair.file_b.clone(),
                co_change_count: pair.co_change_count,
                coupling_ratio: pair.coupling_ratio,
                has_static_dep: pair.has_static_dep,
            }),
            (true, false) => {
                if pair.co_change_count >= HIDDEN_DEP_MIN_COUNT && is_source_file(&pair.file_b) {
                    hidden.push(HiddenDep {
                        input_file: pair.file_a.clone(),
                        partner: pair.file_b.clone(),
                        co_change_count: pair.co_change_count,
                        coupling_ratio: pair.coupling_ratio,
                    });
                }
            }
            (false, true) => {
                if pair.co_change_count >= HIDDEN_DEP_MIN_COUNT && is_source_file(&pair.file_a) {
                    hidden.push(HiddenDep {
                        input_file: pair.file_b.clone(),
                        partner: pair.file_a.clone(),
                        co_change_count: pair.co_change_count,
                        coupling_ratio: pair.coupling_ratio,
                    });
                }
            }
            (false, false) => {}
        }
    }

    within.sort_by(|a, b| b.coupling_ratio.partial_cmp(&a.coupling_ratio).unwrap());
    // Secondary sort by count so ties are deterministic
    hidden.sort_by(|a, b| {
        b.coupling_ratio
            .partial_cmp(&a.coupling_ratio)
            .unwrap()
            .then(b.co_change_count.cmp(&a.co_change_count))
    });

    (within, hidden)
}

/// Returns false for file extensions that are never meaningful coupling partners:
/// data files, docs, scripts, lock files, CI config, and generated assets.
fn is_source_file(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    !matches!(
        ext,
        "json"
            | "md"
            | "toml"
            | "lock"
            | "yaml"
            | "yml"
            | "sh"
            | "txt"
            | "png"
            | "svg"
            | "html"
            | "css"
            | "map"
            | "d.ts"
    ) && !path.ends_with(".d.ts")
}

/// Compute per-file ownership signals from git log over OWNERSHIP_WINDOW_DAYS.
pub fn compute_ownership(repo_root: &Path, files: &[String]) -> Result<Vec<OwnershipSignal>> {
    let git_dir = repo_root.join(".git");
    let since = format!("{} days ago", OWNERSHIP_WINDOW_DAYS);
    let mut result = Vec::new();

    for file in files {
        let out = Command::new("git")
            .args([
                "--git-dir",
                git_dir.to_string_lossy().as_ref(),
                "--work-tree",
                repo_root.to_string_lossy().as_ref(),
                "log",
                "--format=%ae",
                &format!("--since={}", since),
                "--",
                file,
            ])
            .output();

        let emails: Vec<String> = match out {
            Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_lowercase())
                .collect(),
            _ => vec![],
        };

        if emails.is_empty() {
            result.push(OwnershipSignal {
                file: file.clone(),
                author_count: 0,
                top_author_pct: 0.0,
            });
            continue;
        }

        let mut counts: HashMap<String, usize> = HashMap::new();
        for email in &emails {
            *counts.entry(email.clone()).or_insert(0) += 1;
        }
        let author_count = counts.len();
        let top = counts.values().copied().max().unwrap_or(0);
        let top_author_pct = top as f64 / emails.len() as f64;

        result.push(OwnershipSignal {
            file: file.clone(),
            author_count,
            top_author_pct,
        });
    }

    Ok(result)
}
