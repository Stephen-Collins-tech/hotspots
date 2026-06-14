use hotspots_core::coordinate::{CoordinateReport, HIDDEN_JSON_CAP, SERIALIZE_THRESHOLD};
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
    #[serde(skip_serializing_if = "is_zero")]
    hidden_dependencies_omitted: usize,
    ownership: Vec<OwnershipSignal>,
    parallel_safe: Vec<String>,
    serialize: Vec<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
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
    let hidden_cap = r.hidden_dependencies.len().min(HIDDEN_JSON_CAP);
    let hidden_omitted = r.hidden_dependencies.len().saturating_sub(HIDDEN_JSON_CAP);
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
        hidden_dependencies: r.hidden_dependencies[..hidden_cap]
            .iter()
            .map(|h| HiddenDep {
                input_file: h.input_file.clone(),
                partner: h.partner.clone(),
                co_change_count: h.co_change_count,
                coupling_ratio: h.coupling_ratio,
            })
            .collect(),
        hidden_dependencies_omitted: hidden_omitted,
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

const HIDDEN_TEXT_CAP: usize = 15;
const MAX_PATH_WIDTH: usize = 50;

fn sep(width: usize) {
    println!("{}", "─".repeat(width));
}

fn print_text(out: &CoordinateReport) {
    println!("Coordination Signal Report");
    sep(60);

    // ── co-change pairs (within set) ─────────────────────────────
    println!();
    if out.pairs.is_empty() {
        println!("Co-change pairs     none");
    } else {
        let pair_w = out
            .pairs
            .iter()
            .map(|p| {
                crate::util::truncate_string(
                    &format!("{} ↔ {}", p.file_a, p.file_b),
                    MAX_PATH_WIDTH * 2 + 3,
                )
                .len()
            })
            .max()
            .unwrap_or(4)
            .max("pair".len());
        let total_w = pair_w + 8 + 7 + 6; // pair + count + ratio + spacing
        println!("Co-change pairs");
        println!(
            "  {:<pair_w$}  {:>5}  {:>5}  dep",
            "pair",
            "count",
            "ratio",
            pair_w = pair_w
        );
        sep(total_w + 2);
        for p in &out.pairs {
            let pair_str = crate::util::truncate_string(
                &format!("{} ↔ {}", p.file_a, p.file_b),
                MAX_PATH_WIDTH * 2 + 3,
            );
            let dep = if p.has_static_dep { "import" } else { "" };
            println!(
                "  {:<pair_w$}  {:>5}  {:>5.2}  {}",
                pair_str,
                p.co_change_count,
                p.coupling_ratio,
                dep,
                pair_w = pair_w
            );
        }
    }

    // ── hidden dependencies ──────────────────────────────────────
    println!();
    let shown_deps = out.hidden_dependencies.len().min(HIDDEN_TEXT_CAP);
    let overflow = out
        .hidden_dependencies
        .len()
        .saturating_sub(HIDDEN_TEXT_CAP);
    if out.hidden_dependencies.is_empty() {
        println!("Hidden dependencies  none");
    } else {
        let visible = &out.hidden_dependencies[..shown_deps];
        let input_w = visible
            .iter()
            .map(|h| crate::util::truncate_string(&h.input_file, MAX_PATH_WIDTH).len())
            .max()
            .unwrap_or(10)
            .max("input file".len());
        let partner_w = visible
            .iter()
            .map(|h| crate::util::truncate_string(&h.partner, MAX_PATH_WIDTH).len())
            .max()
            .unwrap_or(7)
            .max("partner".len());
        let total_w = input_w + partner_w + 8 + 7 + 6;
        println!("Hidden dependencies  (co-change partners outside input set)");
        println!(
            "  {:<input_w$}  {:<partner_w$}  {:>5}  {:>5}",
            "input file",
            "partner",
            "count",
            "ratio",
            input_w = input_w,
            partner_w = partner_w
        );
        sep(total_w + 2);
        for h in visible {
            println!(
                "  {:<input_w$}  {:<partner_w$}  {:>5}  {:>5.2}",
                crate::util::truncate_string(&h.input_file, MAX_PATH_WIDTH),
                crate::util::truncate_string(&h.partner, MAX_PATH_WIDTH),
                h.co_change_count,
                h.coupling_ratio,
                input_w = input_w,
                partner_w = partner_w
            );
        }
        if overflow > 0 {
            println!("  … {} more  (--json for full list)", overflow);
        }
    }

    // ── ownership ────────────────────────────────────────────────
    println!();
    let file_w = out
        .ownership
        .iter()
        .map(|o| crate::util::truncate_string(&o.file, MAX_PATH_WIDTH).len())
        .max()
        .unwrap_or(4)
        .max("file".len());
    let total_w = file_w + 8 + 13 + 6;
    println!("Ownership  (last 90 days)");
    println!(
        "  {:<file_w$}  {:>7}  {:>11}",
        "file",
        "authors",
        "top author %",
        file_w = file_w
    );
    sep(total_w + 2);
    for o in &out.ownership {
        println!(
            "  {:<file_w$}  {:>7}  {:>10.0}%",
            crate::util::truncate_string(&o.file, MAX_PATH_WIDTH),
            o.author_count,
            o.top_author_pct * 100.0,
            file_w = file_w
        );
    }

    // ── partition recommendation ─────────────────────────────────
    println!();
    println!("Partition recommendation");
    sep(60);
    if out.serialize.is_empty() {
        println!("  All input files are safe to modify in parallel.");
    } else {
        if !out.parallel_safe.is_empty() {
            println!("  parallel   {}", out.parallel_safe.join("  "));
        }
        println!(
            "  serialize  {}  (coupling ≥ {:.0}%)",
            out.serialize.join("  "),
            SERIALIZE_THRESHOLD * 100.0
        );
    }
}
