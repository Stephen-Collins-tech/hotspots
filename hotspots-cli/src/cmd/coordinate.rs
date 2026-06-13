use anyhow::Context;
use hotspots_core::git::{extract_co_change_pairs, CoChangePair};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

const CO_CHANGE_WINDOW_DAYS: u64 = 180;
const CO_CHANGE_MIN_COUNT: usize = 2;
/// coupling_ratio above this → serialize recommendation
const SERIALIZE_THRESHOLD: f64 = 0.4;
/// Author window for ownership computation
const OWNERSHIP_WINDOW_DAYS: u64 = 90;

pub(crate) struct CoordinateArgs {
    pub path: PathBuf,
    pub files: Vec<String>,
    pub json: bool,
}

#[derive(Serialize)]
struct PairSignal {
    files: [String; 2],
    co_change_count: usize,
    coupling_ratio: f64,
    has_static_dep: bool,
}

#[derive(Serialize)]
struct HiddenDep {
    input_file: String,
    partner: String,
    co_change_count: usize,
    coupling_ratio: f64,
}

#[derive(Serialize)]
struct OwnershipSignal {
    file: String,
    author_count: usize,
    top_author_pct: f64,
}

#[derive(Serialize)]
struct CoordinateOutput {
    input_files: Vec<String>,
    pairs: Vec<PairSignal>,
    hidden_dependencies: Vec<HiddenDep>,
    ownership: Vec<OwnershipSignal>,
    parallel_safe: Vec<String>,
    serialize: Vec<String>,
}

pub(crate) fn handle_coordinate(args: CoordinateArgs) -> anyhow::Result<()> {
    let repo_root = crate::util::find_repo_root(&args.path)?;

    let input: HashSet<String> = args.files.iter().cloned().collect();

    let all_pairs = extract_co_change_pairs(&repo_root, CO_CHANGE_WINDOW_DAYS, CO_CHANGE_MIN_COUNT)
        .context("failed to extract co-change pairs")?;

    let (within, hidden) = partition_pairs(&all_pairs, &input);

    let ownership = compute_ownership(&repo_root, &args.files)?;

    // Files that appear in any serialize-worthy pair
    let mut must_serialize: HashSet<&str> = HashSet::new();
    for p in &within {
        if p.coupling_ratio >= SERIALIZE_THRESHOLD {
            must_serialize.insert(p.file_a.as_str());
            must_serialize.insert(p.file_b.as_str());
        }
    }

    let mut parallel_safe: Vec<String> = args
        .files
        .iter()
        .filter(|f| !must_serialize.contains(f.as_str()))
        .cloned()
        .collect();
    let mut serialize: Vec<String> = args
        .files
        .iter()
        .filter(|f| must_serialize.contains(f.as_str()))
        .cloned()
        .collect();
    parallel_safe.sort();
    serialize.sort();

    let pairs: Vec<PairSignal> = within
        .iter()
        .map(|p| PairSignal {
            files: [p.file_a.clone(), p.file_b.clone()],
            co_change_count: p.co_change_count,
            coupling_ratio: p.coupling_ratio,
            has_static_dep: p.has_static_dep,
        })
        .collect();

    let hidden_deps: Vec<HiddenDep> = hidden
        .iter()
        .map(|(input_file, partner, pair)| HiddenDep {
            input_file: input_file.clone(),
            partner: partner.clone(),
            co_change_count: pair.co_change_count,
            coupling_ratio: pair.coupling_ratio,
        })
        .collect();

    let output = CoordinateOutput {
        input_files: args.files.clone(),
        pairs,
        hidden_dependencies: hidden_deps,
        ownership,
        parallel_safe,
        serialize,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_text(&output);
    }

    Ok(())
}

/// Split all co-change pairs into:
/// - within: both files in input set
/// - hidden: exactly one file in input set (the tuple is (input_file, partner, pair))
fn partition_pairs<'a>(
    pairs: &'a [CoChangePair],
    input: &HashSet<String>,
) -> (
    Vec<&'a CoChangePair>,
    Vec<(String, String, &'a CoChangePair)>,
) {
    let mut within = Vec::new();
    let mut hidden = Vec::new();

    for pair in pairs {
        let a_in = input.contains(&pair.file_a);
        let b_in = input.contains(&pair.file_b);
        match (a_in, b_in) {
            (true, true) => within.push(pair),
            (true, false) => hidden.push((pair.file_a.clone(), pair.file_b.clone(), pair)),
            (false, true) => hidden.push((pair.file_b.clone(), pair.file_a.clone(), pair)),
            (false, false) => {}
        }
    }

    within.sort_by(|a, b| b.coupling_ratio.partial_cmp(&a.coupling_ratio).unwrap());
    hidden.sort_by(|a, b| b.2.coupling_ratio.partial_cmp(&a.2.coupling_ratio).unwrap());

    (within, hidden)
}

/// Compute per-file ownership signals from git log over the last OWNERSHIP_WINDOW_DAYS.
fn compute_ownership(
    repo_root: &std::path::Path,
    files: &[String],
) -> anyhow::Result<Vec<OwnershipSignal>> {
    let mut result = Vec::new();
    let since = format!("{} days ago", OWNERSHIP_WINDOW_DAYS);

    for file in files {
        let out = Command::new("git")
            .args([
                "--git-dir",
                repo_root.join(".git").to_string_lossy().as_ref(),
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

fn print_text(out: &CoordinateOutput) {
    println!("Coordination Signal Report");
    println!("{}", "=".repeat(60));

    println!("\nInput files ({})", out.input_files.len());
    for f in &out.input_files {
        println!("  {}", f);
    }

    if out.pairs.is_empty() {
        println!("\nCo-change pairs (within set): none");
    } else {
        println!("\nCo-change pairs (within set)");
        println!("  {:<45} {:>8}  {:>8}", "pair", "count", "ratio");
        println!("  {}", "-".repeat(65));
        for p in &out.pairs {
            let pair_str = format!("{} ↔ {}", p.files[0], p.files[1]);
            let dep_note = if p.has_static_dep { " [import]" } else { "" };
            println!(
                "  {:<45} {:>8}  {:>7.2}{}",
                pair_str, p.co_change_count, p.coupling_ratio, dep_note
            );
        }
    }

    if out.hidden_dependencies.is_empty() {
        println!("\nHidden dependencies: none");
    } else {
        println!("\nHidden dependencies (outside input set)");
        println!(
            "  {:<25} {:<25} {:>8}  {:>8}",
            "input file", "partner", "count", "ratio"
        );
        println!("  {}", "-".repeat(70));
        for h in &out.hidden_dependencies {
            println!(
                "  {:<25} {:<25} {:>8}  {:>7.2}",
                h.input_file, h.partner, h.co_change_count, h.coupling_ratio
            );
        }
    }

    println!("\nOwnership (last 90 days)");
    println!("  {:<40} {:>8}  {:>12}", "file", "authors", "top author %");
    println!("  {}", "-".repeat(65));
    for o in &out.ownership {
        println!(
            "  {:<40} {:>8}  {:>11.0}%",
            o.file,
            o.author_count,
            o.top_author_pct * 100.0
        );
    }

    println!("\nPartition recommendation");
    if out.serialize.is_empty() {
        println!("  All files appear safe to modify in parallel.");
    } else {
        if !out.parallel_safe.is_empty() {
            println!("  Parallel-safe:");
            for f in &out.parallel_safe {
                println!("    {}", f);
            }
        }
        println!(
            "  Serialize (high coupling ratio ≥ {:.0}%):",
            SERIALIZE_THRESHOLD * 100.0
        );
        for f in &out.serialize {
            println!("    {}", f);
        }
    }
}
