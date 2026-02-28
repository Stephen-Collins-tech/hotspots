//! Hotspots core library - static analysis for TypeScript, JavaScript, Go, Java, Python, and Rust

#![deny(warnings)]

// Global invariants enforced in this crate:
// - Analysis is strictly per-function
// - No global mutable state
// - No randomness, clocks, threads, or async
// - Deterministic traversal order must be explicit
// - Formatting, comments, and whitespace must not affect results
// - Identical input yields byte-for-byte identical output

pub mod aggregates;
pub mod analysis;
pub mod ast;
pub mod callgraph;
pub mod cfg;
pub mod config;
pub mod delta;
pub mod discover;
pub mod git;
pub mod html;
pub mod imports;
pub mod language;
pub mod metrics;
pub mod parser;
pub mod patterns;
pub mod policy;
pub mod prune;
pub mod report;
pub mod risk;
pub mod scoring;
pub mod snapshot;
pub mod suppression;
pub mod touch_cache;
pub mod trends;

pub use callgraph::CallGraph;
pub use config::ResolvedConfig;
pub use git::GitContext;
pub use report::{render_json, render_text, sort_reports, FunctionRiskReport};

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
    let cm: Lrc<SourceMap> = Default::default();
    let mut all_reports = Vec::new();
    let mut file_index = 0;

    // Build weights/thresholds from config
    let weights = resolved_config.map(|c| risk::LrsWeights {
        cc: c.weight_cc,
        nd: c.weight_nd,
        fo: c.weight_fo,
        ns: c.weight_ns,
    });
    let thresholds = resolved_config.map(|c| risk::RiskThresholds {
        moderate: c.moderate_threshold,
        high: c.high_threshold,
        critical: c.critical_threshold,
    });

    // Collect source files (TypeScript, JavaScript, Go, Java, Python, Rust)
    let source_files = collect_source_files(path)?;

    // Analyze each file (applying include/exclude from config)
    let mut skipped_files: usize = 0;
    for file_path in source_files {
        // Apply config include/exclude filter
        if let Some(config) = resolved_config {
            if !config.should_include(&file_path) {
                continue;
            }
        }

        match analysis::analyze_file_with_config(
            &file_path,
            &cm,
            file_index,
            &options,
            weights.as_ref(),
            thresholds.as_ref(),
            resolved_config.map(|c| &c.pattern_thresholds),
        ) {
            Ok(reports) => {
                all_reports.extend(reports);
                file_index += 1;
            }
            Err(e) => {
                eprintln!("warning: skipping file {}: {}", file_path.display(), e);
                skipped_files += 1;
            }
        }
    }
    if skipped_files > 0 {
        eprintln!("Skipped {} file(s) due to analysis errors", skipped_files);
    }

    // Sort deterministically
    let sorted_reports = sort_reports(all_reports);

    // Apply top_n filter if specified
    let final_reports = if let Some(top_n) = options.top_n {
        sorted_reports.into_iter().take(top_n).collect()
    } else {
        sorted_reports
    };

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
fn collect_source_files(path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if is_supported_source_file(filename) {
                files.push(path.to_path_buf());
            }
        }
    } else if path.is_dir() {
        collect_source_files_recursive(path, &mut files)?;
    }

    // Sort files for deterministic order
    files.sort();

    Ok(files)
}

/// Returns true for directory names that should not be traversed
fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || name == "node_modules"
        || name == "dist"
        || name == "build"
        || name == "out"
        || name == "coverage"
        || name == "target"
}

/// Process one directory entry, pushing source files or recursing into dirs
fn process_dir_entry(
    path: std::path::PathBuf,
    metadata: std::fs::Metadata,
    files: &mut Vec<std::path::PathBuf>,
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
        collect_source_files_recursive(&path, files)?;
    } else if metadata.is_file() {
        if let Some(filename) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
            if is_supported_source_file(filename) {
                files.push(path);
            }
        }
    }

    Ok(())
}

/// Recursively collect supported source files from a directory
fn collect_source_files_recursive(
    dir: &std::path::Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    for entry_result in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry_result?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("Failed to read metadata: {}", path.display()))?;
        process_dir_entry(path, metadata, files)?;
    }

    Ok(())
}

/// Build nodes and the name→IDs reverse index for callee resolution
fn build_name_index(
    reports: &[FunctionRiskReport],
    graph: &mut callgraph::CallGraph,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut name_to_id: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for report in reports {
        let function_id = format!("{}::{}", report.file, report.function);
        graph.add_node(function_id.clone());
        name_to_id
            .entry(report.function.clone())
            .or_default()
            .push(function_id);
    }
    name_to_id
}

/// Resolve the best callee_id for a call site.
///
/// Priority 1: same-file callee.
/// Priority 2: callee in a file the caller explicitly imports.
/// Priority 3: first name match (fallback).
/// Returns None for self-calls or unresolved names.
fn resolve_callee(
    callee_name: &str,
    caller_id: &str,
    caller_file: &str,
    name_to_id: &std::collections::HashMap<String, Vec<String>>,
    import_map: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Option<String> {
    let possible_callees = name_to_id.get(callee_name)?;
    let normalized_caller_file = caller_file.replace('\\', "/");

    // Priority 1: same file
    for callee_id in possible_callees {
        let normalized_callee = callee_id.replace('\\', "/");
        if normalized_callee.starts_with(&format!("{}::", normalized_caller_file)) {
            return (callee_id != caller_id).then(|| callee_id.clone());
        }
    }

    // Priority 2: imported file
    if let Some(imports) = import_map.get(caller_file) {
        for callee_id in possible_callees {
            let callee_file = callee_id.split("::").next().unwrap_or("");
            if imports.contains(callee_file) && callee_id != caller_id {
                return Some(callee_id.clone());
            }
        }
    }

    // Priority 3: first match (fallback)
    possible_callees
        .first()
        .filter(|id| *id != caller_id)
        .cloned()
}

/// Add AST-derived edges to the graph; return (total_callee_names, resolved_callee_names)
fn add_callee_edges(
    reports: &[FunctionRiskReport],
    name_to_id: &std::collections::HashMap<String, Vec<String>>,
    import_map: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    graph: &mut callgraph::CallGraph,
) -> (usize, usize) {
    let mut total = 0usize;
    let mut resolved = 0usize;
    for report in reports {
        let caller_id = format!("{}::{}", report.file, report.function);
        let mut added_callees = std::collections::HashSet::new();
        for callee_name in &report.callees {
            total += 1;
            if name_to_id.contains_key(callee_name.as_str()) {
                resolved += 1;
                if let Some(callee_id) = resolve_callee(
                    callee_name,
                    &caller_id,
                    &report.file,
                    name_to_id,
                    import_map,
                ) {
                    if added_callees.insert(callee_id.clone()) {
                        graph.add_edge(caller_id.clone(), callee_id);
                    }
                }
            }
        }
    }
    (total, resolved)
}

/// Build a call graph from AST-derived callee names in function reports.
pub fn build_call_graph(
    reports: &[FunctionRiskReport],
    repo_root: &std::path::Path,
) -> Result<callgraph::CallGraph> {
    let mut graph = callgraph::CallGraph::new();
    let name_to_id = build_name_index(reports, &mut graph);

    // Build import map for import-guided resolution (priority 2 after same-file)
    let file_list: Vec<&str> = reports.iter().map(|r| r.file.as_str()).collect();
    let file_deps = crate::imports::resolve_file_deps(&file_list, repo_root);
    let mut import_map: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (from, to) in file_deps {
        import_map.entry(from).or_default().insert(to);
    }

    let (total, resolved) = add_callee_edges(reports, &name_to_id, &import_map, &mut graph);
    graph.total_callee_names = total;
    graph.resolved_callee_names = resolved;
    Ok(graph)
}
