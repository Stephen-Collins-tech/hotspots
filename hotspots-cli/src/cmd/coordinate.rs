//! `hotspots coordinate` — coupling and ownership risk for a caller-specified
//! file set, before work starts.
//!
//! v1 minimal baseline (`hotspots-research/docs/promotion-briefs/coordinate-v1-minimal.md`):
//! given an explicit `--files` list, reports raw co-change coupling within the
//! set, files outside the set strongly coupled to something inside it
//! ("hidden dependencies"), per-file ownership signals, and a single
//! parallel-safety recommendation. Deliberately does not depend on any
//! snapshot or AST/CFG analysis — this command only needs git log.

use anyhow::{Context, Result};
use hotspots_core::coupling::compute_raw_coupling_for_repo;
use hotspots_core::history_signals::compute_history_signals_for_repo;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Files outside `--files` with coupling_ratio >= this to any input file are
/// reported as hidden dependencies.
const HIDDEN_DEP_THRESHOLD: f64 = 0.7;

/// Any within-set pair at or above this coupling_ratio makes the whole set
/// unsafe to parallelize.
const SERIALIZE_THRESHOLD: f64 = 0.7;

pub(crate) struct CoordinateArgs {
    pub files: String,
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

pub(crate) fn handle_coordinate(args: CoordinateArgs) -> Result<()> {
    let repo_root = args.path.canonicalize().context("resolve repo path")?;

    let input_files: Vec<String> = args
        .files
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

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
            FileOwnership {
                file: file.clone(),
                author_count: signals.map(|s| s.author_count).unwrap_or(0),
                author_entropy: signals.map(|s| s.author_entropy).unwrap_or(0.0),
                newcomer_rate: signals.and_then(|s| s.newcomer_rate),
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
