//! Hotspots core library - static analysis for TypeScript, JavaScript, Go, and Rust

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
pub mod language;
pub mod metrics;
pub mod parser;
pub mod policy;
pub mod prune;
pub mod report;
pub mod risk;
pub mod scoring;
pub mod snapshot;
pub mod suppression;
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

    // Collect source files (TypeScript, JavaScript, Go, Rust)
    let source_files = collect_source_files(path)?;

    // Analyze each file (applying include/exclude from config)
    for file_path in source_files {
        // Apply config include/exclude filter
        if let Some(config) = resolved_config {
            if !config.should_include(&file_path) {
                continue;
            }
        }

        let reports = analysis::analyze_file_with_config(
            &file_path,
            &cm,
            file_index,
            &options,
            weights.as_ref(),
            thresholds.as_ref(),
        )
        .with_context(|| format!("Failed to analyze file: {}", file_path.display()))?;
        all_reports.extend(reports);
        file_index += 1;
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

/// Recursively collect supported source files from a directory
fn collect_source_files_recursive(
    dir: &std::path::Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    use std::ffi::OsStr;

    for entry_result in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry: std::fs::DirEntry = entry_result?;
        let path = entry.path();

        // Use symlink_metadata to detect symlinks without following them
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("Failed to read metadata: {}", path.display()))?;

        // Skip symlinks to prevent infinite loops
        if metadata.is_symlink() {
            continue;
        }

        if metadata.is_dir() {
            // Skip common non-source directories
            if let Some(name) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
                // Skip hidden directories, node_modules, and build artifacts
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "dist"
                    || name == "build"
                    || name == "out"
                    || name == "coverage"
                    || name == "target"
                {
                    continue;
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
    }

    Ok(())
}

/// Build a call graph from source files and function reports
///
/// This uses a simple regex-based approach to extract function calls.
/// It's not perfect but provides a reasonable best-effort approximation.
pub fn build_call_graph(
    path: &std::path::Path,
    reports: &[FunctionRiskReport],
    resolved_config: Option<&ResolvedConfig>,
) -> Result<callgraph::CallGraph> {
    use regex::Regex;
    use std::collections::HashMap;

    let mut graph = callgraph::CallGraph::new();

    // Add all functions as nodes
    let mut name_to_id: HashMap<String, Vec<String>> = HashMap::new();
    for report in reports {
        let function_id = format!("{}::{}", report.file, report.function);
        graph.add_node(function_id.clone());

        // Build reverse mapping: simple name -> list of full IDs
        // This handles name collisions across files
        name_to_id
            .entry(report.function.clone())
            .or_default()
            .push(function_id);
    }

    // First pass: add AST-derived edges for reports that have callee names
    for report in reports {
        let caller_id = format!("{}::{}", report.file, report.function);
        if !report.callees.is_empty() {
            let mut added_callees = std::collections::HashSet::new();
            for callee_name in &report.callees {
                if let Some(possible_callees) = name_to_id.get(callee_name) {
                    let normalized_caller_file = report.file.replace('\\', "/");
                    let mut found = false;
                    for callee_id in possible_callees {
                        let normalized_callee = callee_id.replace('\\', "/");
                        if normalized_callee.starts_with(&format!("{}::", normalized_caller_file)) {
                            if callee_id != &caller_id && !added_callees.contains(callee_id) {
                                graph.add_edge(caller_id.clone(), callee_id.clone());
                                added_callees.insert(callee_id.clone());
                            }
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        if let Some(callee_id) = possible_callees.first() {
                            if callee_id != &caller_id && !added_callees.contains(callee_id) {
                                graph.add_edge(caller_id.clone(), callee_id.clone());
                                added_callees.insert(callee_id.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect source files for regex-based fallback (ECMAScript/Rust)
    let source_files = collect_source_files(path)?;

    // Regex to match function calls: name(...)
    let call_pattern = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();

    // Build set of files that already have AST-derived edges
    let ast_covered_files: std::collections::HashSet<String> = reports
        .iter()
        .filter(|r| !r.callees.is_empty())
        .map(|r| r.file.replace('\\', "/"))
        .collect();

    for file_path in source_files {
        // Apply config include/exclude filter
        if let Some(config) = resolved_config {
            if !config.should_include(&file_path) {
                continue;
            }
        }

        // Normalize file path for matching (use / separators)
        let normalized_path = file_path.to_string_lossy().replace('\\', "/");

        // Skip files already covered by AST-derived edges
        if ast_covered_files.contains(&normalized_path) {
            continue;
        }

        // Read source
        let source = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Find all functions in this file with their line ranges
        let mut file_functions: Vec<(String, String, u32)> = Vec::new();
        for report in reports {
            let report_file = report.file.replace('\\', "/");
            if report_file == normalized_path {
                let function_id = format!("{}::{}", report.file, report.function);
                file_functions.push((function_id, report.function.clone(), report.line));
            }
        }

        // Sort by line number to process in order
        file_functions.sort_by_key(|(_, _, line)| *line);

        // Split source into lines for per-function analysis
        let source_lines: Vec<&str> = source.lines().collect();

        // For each function, estimate its source range and find calls
        for i in 0..file_functions.len() {
            let (caller_id, _caller_name, start_line) = &file_functions[i];

            // Estimate end line: either the start of the next function, or end of file
            let end_line = if i + 1 < file_functions.len() {
                file_functions[i + 1].2
            } else {
                source_lines.len() as u32
            };

            // Extract function source (rough approximation)
            let function_source = source_lines
                .iter()
                .skip((start_line.saturating_sub(1)) as usize)
                .take((end_line - start_line) as usize)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");

            // Find calls in this function's source
            let mut added_callees = std::collections::HashSet::new();
            for cap in call_pattern.captures_iter(&function_source) {
                if let Some(called_name_match) = cap.get(1) {
                    let called_name = called_name_match.as_str();

                    // Try to resolve the call to a function ID
                    if let Some(possible_callees) = name_to_id.get(called_name) {
                        // Prefer functions in the same file
                        let mut found = false;
                        for callee_id in possible_callees {
                            if callee_id.starts_with(&format!("{}::", normalized_path)) {
                                // Don't add self-calls or duplicates
                                if callee_id != caller_id && !added_callees.contains(callee_id) {
                                    graph.add_edge(caller_id.clone(), callee_id.clone());
                                    added_callees.insert(callee_id.clone());
                                }
                                found = true;
                                break;
                            }
                        }

                        // If no same-file match, add edge to first match (cross-file call)
                        if !found {
                            if let Some(callee_id) = possible_callees.first() {
                                if callee_id != caller_id && !added_callees.contains(callee_id) {
                                    graph.add_edge(caller_id.clone(), callee_id.clone());
                                    added_callees.insert(callee_id.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(graph)
}
