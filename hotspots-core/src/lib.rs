//! Hotspots core library - static analysis for TypeScript, JavaScript, Go, Java, Python, and Rust

#![deny(warnings)]

// Global invariants enforced in this crate:
// - Analysis is strictly per-function
// - No global mutable state
// - No randomness, clocks, or async
// - File analysis is parallelized via rayon; all other logic is single-threaded
// - Deterministic traversal order must be explicit
// - Formatting, comments, and whitespace must not affect results
// - Identical input yields byte-for-byte identical output

pub mod aggregates;
pub mod analysis;
pub mod ast;
pub mod callgraph;
pub mod cfg;
pub mod compact;
pub mod config;
pub mod coupling;
pub mod db;
pub mod delta;
pub mod discover;
pub mod function_id;
pub mod gate;
pub mod git;
pub mod history_signals;
pub mod html;
pub mod imports;
pub mod isolation_forest;
pub mod language;
pub mod metrics;
pub mod parser;
pub mod phrases;
pub mod policy;
pub mod prune;
pub mod ranking;
pub mod report;
pub mod risk;
pub mod sarif;
pub mod scoring;
pub mod snapshot;
pub mod suppression;
pub mod trainer;
pub mod trends;
pub mod views;

pub use callgraph::CallGraph;
pub use config::ResolvedConfig;
pub use git::GitContext;
pub use report::{
    render_json, render_json_envelope, render_text, render_text_grouped, sort_reports,
    FunctionRiskReport,
};

use anyhow::{Context, Result};
use swc_common::{sync::Lrc, SourceMap};

pub struct AnalysisOptions {
    pub min_lrs: Option<f64>,
    pub top_n: Option<usize>,
}

/// Analyze files at the given path with default configuration
pub fn analyze(
    path: &std::path::Path,
    options: AnalysisOptions,
) -> anyhow::Result<Vec<FunctionRiskReport>> {
    analyze_with_config(path, options, None)
}

/// Analyze files at the given path with optional resolved configuration
pub fn analyze_with_config(
    path: &std::path::Path,
    options: AnalysisOptions,
    resolved_config: Option<&ResolvedConfig>,
) -> anyhow::Result<Vec<FunctionRiskReport>> {
    analyze_with_progress(path, options, resolved_config, None)
}

/// Like [`analyze_with_config`] but accepts an optional progress callback.
///
/// `progress`, when provided, is called as `(files_done, total_files)`:
/// - Once with `(0, total)` immediately after file discovery (skipped when no
///   source files are found)
/// - Once with `(n, total)` after each file is processed (order not guaranteed
///   across parallel workers)
pub fn analyze_with_progress(
    path: &std::path::Path,
    options: AnalysisOptions,
    resolved_config: Option<&ResolvedConfig>,
    progress: Option<&(dyn Fn(usize, usize) + Send + Sync)>,
) -> anyhow::Result<Vec<FunctionRiskReport>> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Collect and filter source files upfront so the total is known before analysis begins
    let (collected, ignored) = collect_source_files_with_ignored(path)?;
    let source_files: Vec<_> = collected
        .into_iter()
        .filter(|f| resolved_config.map_or(true, |c| c.should_include(f)))
        .collect();
    let ignored: Vec<_> = ignored
        .into_iter()
        .filter(|f| resolved_config.map_or(true, |c| c.should_include(f)))
        .collect();
    let total_files = source_files.len();

    if total_files > 0 {
        if let Some(f) = progress {
            f(0, total_files);
        }
    }

    // Parallel file analysis: each worker creates its own SourceMap (Lrc is !Send
    // so it cannot be shared, but creating one per-task on a single thread is safe).
    let counter = AtomicUsize::new(0);
    let mut raw_results: Vec<(usize, &std::path::Path, Result<Vec<FunctionRiskReport>>)> =
        source_files
            .par_iter()
            .enumerate()
            .map(|(file_index, file_path)| {
                let cm: Lrc<SourceMap> = Default::default();
                let result = analysis::analyze_file_with_config(
                    file_path, &cm, file_index, &options, None, None,
                );
                let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(f) = progress {
                    f(done, total_files);
                }
                (file_index, file_path.as_path(), result)
            })
            .collect();

    // Restore deterministic ordering (parallel workers complete out of order)
    raw_results.sort_by_key(|(idx, _, _)| *idx);

    let mut skipped_files: usize = 0;

    let final_reports = if let Some(top_n) = options.top_n {
        // Bounded min-heap: maintain at most top_n reports keyed by lrs ascending
        // so the root is always the lowest score seen so far.
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        struct MinByLrs(FunctionRiskReport);
        impl PartialEq for MinByLrs {
            fn eq(&self, other: &Self) -> bool {
                self.0.lrs == other.0.lrs
            }
        }
        impl Eq for MinByLrs {}
        impl PartialOrd for MinByLrs {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for MinByLrs {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse so BinaryHeap (max-heap) pops the lowest lrs first
                other
                    .0
                    .lrs
                    .partial_cmp(&self.0.lrs)
                    .unwrap_or(Ordering::Equal)
            }
        }

        let mut heap: BinaryHeap<MinByLrs> = BinaryHeap::with_capacity(top_n + 1);
        for (_file_index, file_path, result) in raw_results {
            match result {
                Ok(reports) => {
                    for r in reports {
                        heap.push(MinByLrs(r));
                        if heap.len() > top_n {
                            heap.pop();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("warning: skipping file {}: {}", file_path.display(), e);
                    skipped_files += 1;
                }
            }
        }

        let mut v: Vec<FunctionRiskReport> = heap.into_iter().map(|w| w.0).collect();
        v.sort_by(|a, b| b.lrs.partial_cmp(&a.lrs).unwrap_or(Ordering::Equal));
        v
    } else {
        let mut all_reports = Vec::new();
        for (_file_index, file_path, result) in raw_results {
            match result {
                Ok(reports) => all_reports.extend(reports),
                Err(e) => {
                    eprintln!("warning: skipping file {}: {}", file_path.display(), e);
                    skipped_files += 1;
                }
            }
        }
        sort_reports(all_reports)
    };

    if skipped_files > 0 {
        eprintln!("Skipped {} file(s) due to analysis errors", skipped_files);
    }
    if let Some(line) = summarize_unsupported_files(&ignored) {
        eprintln!("{line}");
    }

    Ok(final_reports)
}

/// Check if a file is a supported source file
fn is_supported_source_file(filename: &str) -> bool {
    // Skip TypeScript declaration files (.d.ts)
    if filename.ends_with(".d.ts") {
        return false;
    }

    // Use language detection to check if file is supported
    if let Some(ext) = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
    {
        language::Language::from_extension(ext).is_some()
    } else {
        false
    }
}

/// Collect all supported source files from a path (file or directory)
///
/// Supported languages and extensions:
/// - TypeScript: .ts, .mts, .cts (excludes .d.ts declaration files)
/// - TSX: .tsx, .mtsx, .ctsx
/// - JavaScript: .js, .mjs, .cjs
/// - JSX: .jsx, .mjsx, .cjsx
/// - Go: .go
/// - Java: .java
/// - Python: .py, .pyw
/// - Rust: .rs
///
/// Pure filesystem I/O: no git calls, no parsing.
pub fn collect_source_files(path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    Ok(collect_source_files_with_ignored(path)?.0)
}

/// Source files found while walking, split into those we can analyze and those written in
/// a language we recognise as source code but do not support.
#[derive(Default)]
struct SourceWalk {
    files: Vec<std::path::PathBuf>,
    unsupported: Vec<std::path::PathBuf>,
}

/// Like [`collect_source_files`], but also returns files in recognised-but-unsupported
/// languages (for example C++), so callers can say what was left out instead of ignoring
/// it silently. Same single walk and same pruned directories.
pub fn collect_source_files_with_ignored(
    path: &std::path::Path,
) -> Result<(Vec<std::path::PathBuf>, Vec<std::path::PathBuf>)> {
    let mut walk = SourceWalk::default();

    if path.is_file() {
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if is_supported_source_file(filename) {
                walk.files.push(path.to_path_buf());
            } else if unsupported_language_extension(filename).is_some() {
                walk.unsupported.push(path.to_path_buf());
            }
        }
    } else if path.is_dir() {
        collect_source_files_recursive(path, &mut walk)?;
    }

    // Sort files for deterministic order
    walk.files.sort();
    walk.unsupported.sort();

    Ok((walk.files, walk.unsupported))
}

/// Source-file extensions for languages hotspots does not analyze. Listing them lets a run
/// report what it left out; it is not a promise of future support. Kept to extensions that
/// almost always mean source code (so `.m`, which may be MATLAB or Objective-C, is in,
/// but `.h`, which we do parse as C, is not).
const UNSUPPORTED_SOURCE_EXTENSIONS: &[&str] = &[
    "kt", "kts", // Kotlin
    "swift", "rb", "scala", "dart", "lua", "m", "mm", "cu", "cuh",
];

/// The (lowercased) extension if `filename` is a source file in an unsupported language.
fn unsupported_language_extension(filename: &str) -> Option<String> {
    let ext = std::path::Path::new(filename)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    UNSUPPORTED_SOURCE_EXTENSIONS
        .contains(&ext.as_str())
        .then_some(ext)
}

/// One stderr line saying how many source files were not analyzed, by extension, largest
/// first. `None` when nothing was left out.
pub fn summarize_unsupported_files(unsupported: &[std::path::PathBuf]) -> Option<String> {
    if unsupported.is_empty() {
        return None;
    }
    let mut by_ext: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for path in unsupported {
        if let Some(ext) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(unsupported_language_extension)
        {
            *by_ext.entry(ext).or_default() += 1;
        }
    }
    let mut counts: Vec<(String, usize)> = by_ext.into_iter().collect();
    if counts.is_empty() {
        return None;
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    let detail = counts
        .iter()
        .map(|(ext, n)| format!("{n} .{ext}"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Not analyzed: {total} source file(s) in unsupported languages ({detail}). \
         Results cover only the supported languages; see docs/REFERENCE.md."
    ))
}

/// Returns true for directory names that should not be traversed.
/// These are pruned at walk time before any glob matching — keep this list
/// to things that are unambiguously never first-party source code.
fn is_skipped_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "dist"
            | "build"
            | "out"
            | "coverage"
            | "target"
            | "vendor"
            | "venv"
            | "__pycache__"
            | "storybook-static"
            | "generated"
            | "__generated__"
    ) || name.starts_with('.')
}

/// Process one directory entry, pushing source files or recursing into dirs
fn process_dir_entry(
    path: std::path::PathBuf,
    metadata: std::fs::Metadata,
    walk: &mut SourceWalk,
) -> Result<()> {
    use std::ffi::OsStr;

    if metadata.is_symlink() {
        return Ok(());
    }

    if metadata.is_dir() {
        if let Some(name) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
            if is_skipped_dir(name) {
                return Ok(());
            }
        }
        collect_source_files_recursive(&path, walk)?;
    } else if metadata.is_file() {
        if let Some(filename) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
            if is_supported_source_file(filename) {
                walk.files.push(path);
            } else if unsupported_language_extension(filename).is_some() {
                walk.unsupported.push(path);
            }
        }
    }

    Ok(())
}

/// Recursively collect supported source files from a directory
fn collect_source_files_recursive(dir: &std::path::Path, walk: &mut SourceWalk) -> Result<()> {
    for entry_result in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry_result?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("Failed to read metadata: {}", path.display()))?;
        process_dir_entry(path, metadata, walk)?;
    }

    Ok(())
}

/// Pre-compute all function IDs and build the name→indices reverse index.
///
/// Each function ID is allocated exactly once. `name_to_id` values store indices
/// into the returned `Vec<String>` rather than cloned Strings, avoiding O(N)
/// duplicate allocations during callee resolution.
///
/// Returns (function_ids, name_to_report_idx, report_to_graph_idx).
/// `report_to_graph_idx[i]` maps report index i to its graph node index — necessary
/// because intern() deduplicates identical file::function IDs, so graph node count
/// may be less than report count.
fn build_name_index<'r>(
    reports: &'r [FunctionRiskReport],
    graph: &mut callgraph::CallGraph,
) -> (
    Vec<String>,
    std::collections::HashMap<&'r str, Vec<usize>>,
    Vec<u32>,
) {
    let mut function_ids: Vec<String> = Vec::with_capacity(reports.len());
    let mut name_to_idx: std::collections::HashMap<&'r str, Vec<usize>> =
        std::collections::HashMap::new();
    let mut report_to_graph_idx: Vec<u32> = Vec::with_capacity(reports.len());
    for (i, report) in reports.iter().enumerate() {
        let function_id = format!("{}::{}", report.file, report.function);
        let graph_idx = graph.intern(function_id.clone());
        function_ids.push(function_id);
        report_to_graph_idx.push(graph_idx);
        name_to_idx
            .entry(report.function.as_str())
            .or_default()
            .push(i);
    }
    (function_ids, name_to_idx, report_to_graph_idx)
}

/// Resolve the best callee index for a call site.
///
/// Priority 1: same-file callee.
/// Priority 2: callee in a file the caller explicitly imports.
/// Priority 3: first name match (fallback).
/// Returns None for self-calls or unresolved names.
fn resolve_callee(
    callee_name: &str,
    caller_idx: usize,
    caller_file: &str,
    reports: &[FunctionRiskReport],
    name_to_idx: &std::collections::HashMap<&str, Vec<usize>>,
    import_map: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Option<usize> {
    let possible_indices = name_to_idx.get(callee_name)?;
    let normalized_caller_file = caller_file.replace('\\', "/");

    // Priority 1: same file
    for &idx in possible_indices {
        if idx == caller_idx {
            continue;
        }
        let normalized_callee = reports[idx].file.replace('\\', "/");
        if normalized_callee == normalized_caller_file {
            return Some(idx);
        }
    }

    // Priority 2: imported file
    if let Some(imports) = import_map.get(caller_file) {
        for &idx in possible_indices {
            if idx != caller_idx && imports.contains(&reports[idx].file) {
                return Some(idx);
            }
        }
    }

    // Priority 3: first match (fallback)
    possible_indices
        .first()
        .copied()
        .filter(|&idx| idx != caller_idx)
}

/// Add AST-derived edges to the graph; return (total_callee_names, resolved_callee_names)
fn add_callee_edges(
    reports: &[FunctionRiskReport],
    name_to_idx: &std::collections::HashMap<&str, Vec<usize>>,
    import_map: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    graph: &mut callgraph::CallGraph,
    report_to_graph_idx: &[u32],
) -> (usize, usize) {
    let mut total = 0usize;
    let mut resolved = 0usize;
    for (caller_report_idx, report) in reports.iter().enumerate() {
        let caller_graph_idx = report_to_graph_idx[caller_report_idx];
        let mut added_callees = std::collections::HashSet::<u32>::new();
        for callee_name in &report.callees {
            total += 1;
            if name_to_idx.contains_key(callee_name.as_str()) {
                resolved += 1;
                if let Some(callee_report_idx) = resolve_callee(
                    callee_name,
                    caller_report_idx,
                    &report.file,
                    reports,
                    name_to_idx,
                    import_map,
                ) {
                    let callee_graph_idx = report_to_graph_idx[callee_report_idx];
                    if added_callees.insert(callee_graph_idx) {
                        graph.add_adj(caller_graph_idx, callee_graph_idx);
                    }
                }
            }
        }
    }
    (total, resolved)
}

/// Build a call graph from lean DB rows instead of full FunctionRiskReport slices.
///
/// Loads only `(function_id, file, callees)` from the TempDb — ~2 MB for 51k functions
/// vs ~23 MB for the full reports Vec. The caller should have already dropped the reports
/// Vec before calling this.
///
/// Resolution priority is identical to `build_call_graph`: same-file first, then
/// imported-file, then first name match.
pub fn build_call_graph_from_db(
    db: &db::TempDb,
    sha: &str,
    repo_root: &std::path::Path,
) -> Result<callgraph::CallGraph> {
    let rows = db.load_callee_rows(sha)?;

    let mut graph = callgraph::CallGraph::new();

    // Intern all function IDs and build name → row-index map.
    // Extract the function name by stripping the "file::" prefix.
    let mut name_to_idx: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    let mut row_to_graph_idx: Vec<u32> = Vec::with_capacity(rows.len());

    for (i, (function_id, file, _)) in rows.iter().enumerate() {
        let graph_idx = graph.intern(function_id.clone());
        row_to_graph_idx.push(graph_idx);
        let name = function_id
            .get(file.len() + 2..)
            .unwrap_or(function_id.as_str())
            .to_string();
        name_to_idx.entry(name).or_default().push(i);
    }

    // Build import map for import-guided resolution.
    let file_list: Vec<&str> = rows.iter().map(|(_, f, _)| f.as_str()).collect();
    let file_deps = crate::imports::resolve_file_deps(&file_list, repo_root);
    let mut import_map: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (from, to) in file_deps {
        import_map.entry(from).or_default().insert(to);
    }

    // Add edges with same priority logic as build_call_graph.
    let mut total = 0usize;
    let mut resolved = 0usize;
    for (caller_idx, (_, caller_file, callees)) in rows.iter().enumerate() {
        let caller_graph_idx = row_to_graph_idx[caller_idx];
        let caller_file_norm = caller_file.replace('\\', "/");
        let mut added: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for callee_name in callees {
            total += 1;
            if let Some(candidates) = name_to_idx.get(callee_name.as_str()) {
                resolved += 1;
                // Priority 1: same file
                let mut chosen = None;
                for &idx in candidates {
                    if idx == caller_idx {
                        continue;
                    }
                    if rows[idx].1.replace('\\', "/") == caller_file_norm {
                        chosen = Some(idx);
                        break;
                    }
                }
                // Priority 2: imported file
                if chosen.is_none() {
                    if let Some(imports) = import_map.get(caller_file.as_str()) {
                        for &idx in candidates {
                            if idx != caller_idx && imports.contains(&rows[idx].1) {
                                chosen = Some(idx);
                                break;
                            }
                        }
                    }
                }
                // Priority 3: first match
                if chosen.is_none() {
                    chosen = candidates.iter().copied().find(|&idx| idx != caller_idx);
                }
                if let Some(callee_idx) = chosen {
                    let callee_graph_idx = row_to_graph_idx[callee_idx];
                    if added.insert(callee_graph_idx) {
                        graph.add_adj(caller_graph_idx, callee_graph_idx);
                    }
                }
            }
        }
    }
    graph.total_callee_names = total;
    graph.resolved_callee_names = resolved;
    Ok(graph)
}

/// Build a call graph from AST-derived callee names in function reports.
pub fn build_call_graph(
    reports: &[FunctionRiskReport],
    repo_root: &std::path::Path,
) -> Result<callgraph::CallGraph> {
    let mut graph = callgraph::CallGraph::new();
    let (_, name_to_idx, report_to_graph_idx) = build_name_index(reports, &mut graph);

    // Build import map for import-guided resolution (priority 2 after same-file)
    let file_list: Vec<&str> = reports.iter().map(|r| r.file.as_str()).collect();
    let file_deps = crate::imports::resolve_file_deps(&file_list, repo_root);
    let mut import_map: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (from, to) in file_deps {
        import_map.entry(from).or_default().insert(to);
    }

    let (total, resolved) = add_callee_edges(
        reports,
        &name_to_idx,
        &import_map,
        &mut graph,
        &report_to_graph_idx,
    );
    graph.total_callee_names = total;
    graph.resolved_callee_names = resolved;
    Ok(graph)
}

#[cfg(test)]
mod unsupported_language_tests {
    use super::*;
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn nothing_left_out_means_no_line() {
        assert_eq!(summarize_unsupported_files(&[]), None);
    }

    #[test]
    fn line_counts_by_extension_largest_first() {
        let line = summarize_unsupported_files(&paths(&[
            "a/x.rb",
            "a/y.rb",
            "a/z.rb",
            "b/w.swift",
            "c/q.kt",
        ]))
        .unwrap();
        assert!(line.contains("5 source file(s)"), "{line}");
        assert!(line.contains("3 .rb, 1 .kt, 1 .swift"), "{line}");
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        let line = summarize_unsupported_files(&paths(&["a/X.RB", "a/y.rb"])).unwrap();
        assert!(line.contains("2 .rb"), "{line}");
    }

    #[test]
    fn list_never_overlaps_a_supported_language() {
        // If a language is added to the registry (C++ was the last), this fails until
        // it is removed from UNSUPPORTED_SOURCE_EXTENSIONS, so the line never calls a
        // supported language unsupported.
        for ext in UNSUPPORTED_SOURCE_EXTENSIONS {
            assert!(
                language::Language::from_extension(ext).is_none(),
                ".{ext} is supported but still listed as unsupported"
            );
        }
    }

    #[test]
    fn walk_separates_supported_unsupported_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.py", "b.rb", "c.swift", "d.txt", "e.go"] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        let (files, ignored) = collect_source_files_with_ignored(dir.path()).unwrap();
        let names = |v: &[PathBuf]| -> Vec<String> {
            v.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(names(&files), ["a.py", "e.go"]);
        assert_eq!(names(&ignored), ["b.rb", "c.swift"]); // .txt is not source code
    }
}
