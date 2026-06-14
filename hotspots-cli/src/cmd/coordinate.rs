use hotspots_core::coordinate::{CoordinateReport, SERIALIZE_THRESHOLD};
use serde::Serialize;
use std::path::PathBuf;

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
struct JsonOutput {
    input_files: Vec<String>,
    pairs: Vec<PairSignal>,
    hidden_dependencies: Vec<HiddenDep>,
    ownership: Vec<OwnershipSignal>,
    parallel_safe: Vec<String>,
    serialize: Vec<String>,
}

pub(crate) fn handle_coordinate(args: CoordinateArgs) -> anyhow::Result<()> {
    let repo_root = crate::util::find_repo_root(&args.path)?;
    let report = hotspots_core::coordinate::coordinate(&repo_root, &args.files)?;

    if args.json {
        println!("{}", to_json(&report)?);
    } else {
        print_text(&report);
    }

    Ok(())
}

fn to_json(r: &CoordinateReport) -> anyhow::Result<String> {
    let out = JsonOutput {
        input_files: r.input_files.clone(),
        pairs: r
            .pairs
            .iter()
            .map(|p| PairSignal {
                files: [p.file_a.clone(), p.file_b.clone()],
                co_change_count: p.co_change_count,
                coupling_ratio: p.coupling_ratio,
                has_static_dep: p.has_static_dep,
            })
            .collect(),
        hidden_dependencies: r
            .hidden_dependencies
            .iter()
            .map(|h| HiddenDep {
                input_file: h.input_file.clone(),
                partner: h.partner.clone(),
                co_change_count: h.co_change_count,
                coupling_ratio: h.coupling_ratio,
            })
            .collect(),
        ownership: r
            .ownership
            .iter()
            .map(|o| OwnershipSignal {
                file: o.file.clone(),
                author_count: o.author_count,
                top_author_pct: o.top_author_pct,
            })
            .collect(),
        parallel_safe: r.parallel_safe.clone(),
        serialize: r.serialize.clone(),
    };
    Ok(serde_json::to_string_pretty(&out)?)
}

fn print_text(out: &CoordinateReport) {
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
            let pair_str = format!("{} ↔ {}", p.file_a, p.file_b);
            let dep_note = if p.has_static_dep { " [import]" } else { "" };
            println!(
                "  {:<45} {:>8}  {:>7.2}{}",
                pair_str, p.co_change_count, p.coupling_ratio, dep_note
            );
        }
    }

    const HIDDEN_TEXT_CAP: usize = 15;
    if out.hidden_dependencies.is_empty() {
        println!("\nHidden dependencies: none");
    } else {
        let shown = out.hidden_dependencies.len().min(HIDDEN_TEXT_CAP);
        let overflow = out
            .hidden_dependencies
            .len()
            .saturating_sub(HIDDEN_TEXT_CAP);
        println!("\nHidden dependencies (outside input set)");
        println!(
            "  {:<25} {:<25} {:>8}  {:>8}",
            "input file", "partner", "count", "ratio"
        );
        println!("  {}", "-".repeat(70));
        for h in &out.hidden_dependencies[..shown] {
            println!(
                "  {:<25} {:<25} {:>8}  {:>7.2}",
                h.input_file, h.partner, h.co_change_count, h.coupling_ratio
            );
        }
        if overflow > 0 {
            println!("  ... {} more — use --json for full list", overflow);
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
            "  Serialize (coupling ratio ≥ {:.0}%):",
            SERIALIZE_THRESHOLD * 100.0
        );
        for f in &out.serialize {
            println!("    {}", f);
        }
    }
}
