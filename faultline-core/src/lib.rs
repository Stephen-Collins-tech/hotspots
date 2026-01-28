//! Faultline core library - static analysis of TypeScript functions

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
pub mod cfg;
pub mod delta;
pub mod discover;
pub mod git;
pub mod metrics;
pub mod parser;
pub mod policy;
pub mod prune;
pub mod report;
pub mod risk;
pub mod snapshot;
pub mod trends;

pub use git::GitContext;
pub use report::{FunctionRiskReport, render_json, render_text, sort_reports};

use anyhow::{Context, Result};
use swc_common::{sync::Lrc, SourceMap};

pub struct AnalysisOptions {
    pub min_lrs: Option<f64>,
    pub top_n: Option<usize>,
}

pub fn analyze(path: &std::path::Path, options: AnalysisOptions) -> anyhow::Result<Vec<FunctionRiskReport>> {
    let cm: Lrc<SourceMap> = Default::default();
    let mut all_reports = Vec::new();
    let mut file_index = 0;

    // Collect TypeScript and JavaScript files
    let source_files = collect_source_files(path)?;

    // Analyze each file
    for file_path in source_files {
        let reports = analysis::analyze_file(&file_path, &cm, file_index, &options)
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
    // TypeScript files (.ts, .mts, .cts) but not declaration files (.d.ts)
    let is_ts = (filename.ends_with(".ts") || filename.ends_with(".mts") || filename.ends_with(".cts"))
        && !filename.ends_with(".d.ts");

    // TSX files (.tsx, .mtsx, .ctsx)
    let is_tsx = filename.ends_with(".tsx") || filename.ends_with(".mtsx") || filename.ends_with(".ctsx");

    // JavaScript files (.js, .mjs, .cjs)
    let is_js = filename.ends_with(".js") || filename.ends_with(".mjs") || filename.ends_with(".cjs");

    // JSX files (.jsx, .mjsx, .cjsx)
    let is_jsx = filename.ends_with(".jsx") || filename.ends_with(".mjsx") || filename.ends_with(".cjsx");

    is_ts || is_tsx || is_js || is_jsx
}

/// Collect all TypeScript, JavaScript, JSX, and TSX files from a path (file or directory)
///
/// Supported extensions:
/// - TypeScript: .ts, .mts, .cts (excludes .d.ts declaration files)
/// - TSX: .tsx, .mtsx, .ctsx
/// - JavaScript: .js, .mjs, .cjs
/// - JSX: .jsx, .mjsx, .cjsx
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

/// Recursively collect TypeScript and JavaScript files from a directory
fn collect_source_files_recursive(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    use std::ffi::OsStr;

    for entry_result in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry: std::fs::DirEntry = entry_result?;
        let path = entry.path();

        if path.is_dir() {
            // Skip node_modules and other common non-source directories
            if let Some(name) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
                if name == "node_modules" || name.starts_with('.') {
                    continue;
                }
            }
            collect_source_files_recursive(&path, files)?;
        } else if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n: &OsStr| n.to_str()) {
                if is_supported_source_file(filename) {
                    files.push(path);
                }
            }
        }
    }

    Ok(())
}

