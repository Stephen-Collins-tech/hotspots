//! Analysis orchestration - ties together parsing, discovery, CFG, metrics, and reporting

use crate::ast::FunctionNode;
use crate::language::{self, Language, LanguageParser};
use crate::metrics;
use crate::report;
use crate::risk;
use anyhow::{Context, Result};
use std::path::Path;
use swc_common::{sync::Lrc, SourceMap};

/// Analyze a source file (TypeScript, JavaScript, Go, or Rust)
pub fn analyze_file(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
) -> Result<Vec<report::FunctionRiskReport>> {
    analyze_file_with_config(path, source_map, file_index, options, None, None, None)
}

/// Analyze a file with optional custom weights, thresholds, and pattern thresholds
pub fn analyze_file_with_config(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
    weights: Option<&risk::LrsWeights>,
    thresholds: Option<&risk::RiskThresholds>,
    pattern_thresholds: Option<&crate::patterns::Thresholds>,
) -> Result<Vec<report::FunctionRiskReport>> {
    let default_weights = risk::LrsWeights::default();
    let default_thresholds = risk::RiskThresholds::default();
    let default_pattern_thresholds = crate::patterns::Thresholds::default();
    let w = weights.unwrap_or(&default_weights);
    let t = thresholds.unwrap_or(&default_thresholds);
    let pt = pattern_thresholds.unwrap_or(&default_pattern_thresholds);

    let src = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let (max_line, long_line_count) = long_line_stats(&src, 1000);
    if long_line_count >= 3 {
        eprintln!(
            "warning: skipping {} — looks minified or machine-generated \
             ({} lines exceed 1000 chars, max: {})",
            path.display(),
            long_line_count,
            max_line
        );
        return Ok(vec![]);
    } else if looks_vendored(path) {
        eprintln!(
            "warning: skipping {} — path suggests vendored or generated third-party code",
            path.display()
        );
        return Ok(vec![]);
    }

    let language = Language::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file type: {}", path.display()))?;
    let parser = create_parser(language, source_map)?;
    let module = parser.parse(&src, &path.to_string_lossy())?;
    let functions = module.discover_functions(file_index, &src);

    let func_cfg = FunctionAnalysisConfig {
        options,
        weights: w,
        thresholds: t,
        pattern_thresholds: pt,
        source_map,
    };
    let mut reports = Vec::new();
    for function in &functions {
        if let Some(report) = analyze_function(function, path, language, &func_cfg) {
            reports.push(report);
        }
    }
    Ok(reports)
}

/// Returns the length of the longest line and the count of lines exceeding `threshold` chars.
///
/// Used to detect minified or machine-generated files before full analysis.
fn long_line_stats(src: &str, threshold: usize) -> (usize, usize) {
    let mut max_len = 0;
    let mut count = 0;
    for line in src.lines() {
        let len = line.len();
        if len > max_len {
            max_len = len;
        }
        if len > threshold {
            count += 1;
        }
    }
    (max_len, count)
}

/// Returns true if a file path suggests it contains vendored or generated third-party code.
///
/// Checks for common directory conventions used to store vendor dependencies, static assets,
/// and generated files that are typically not authored code (e.g. `vendor/`, `assets/js/`,
/// `third_party/`, `fixtures/`).
fn looks_vendored(path: &Path) -> bool {
    const VENDORED_SEGMENTS: &[&str] = &[
        "vendor",
        "vendors",
        "third_party",
        "thirdparty",
        "assets/js",
        "static/js",
        "public/js",
        "dist/js",
    ];
    let path_str = path.to_string_lossy().to_lowercase();
    VENDORED_SEGMENTS.iter().any(|seg| {
        path_str.contains(&format!("/{seg}/")) || path_str.contains(&format!("/{seg}\\"))
    })
}

/// Instantiates the correct parser for the given language.
fn create_parser(
    language: Language,
    source_map: &Lrc<SourceMap>,
) -> Result<Box<dyn LanguageParser>> {
    let parser: Box<dyn LanguageParser> = match language {
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact => {
            Box::new(language::ECMAScriptParser::new(source_map.clone()))
        }
        Language::Go => Box::new(language::GoParser::new().context("Failed to create Go parser")?),
        Language::Java => {
            Box::new(language::JavaParser::new().context("Failed to create Java parser")?)
        }
        Language::Python => {
            Box::new(language::PythonParser::new().context("Failed to create Python parser")?)
        }
        Language::Rust => Box::new(language::RustParser),
        Language::Vue => Box::new(language::VueParser::new(source_map.clone())),
    };
    Ok(parser)
}

struct FunctionAnalysisConfig<'a> {
    options: &'a crate::AnalysisOptions,
    weights: &'a risk::LrsWeights,
    thresholds: &'a risk::RiskThresholds,
    pattern_thresholds: &'a crate::patterns::Thresholds,
    source_map: &'a Lrc<SourceMap>,
}

/// Builds CFG, extracts metrics, computes risk and patterns for one function.
/// Returns None if the CFG is invalid or the function is filtered by min_lrs.
fn analyze_function(
    function: &FunctionNode,
    path: &Path,
    language: Language,
    config: &FunctionAnalysisConfig<'_>,
) -> Option<report::FunctionRiskReport> {
    let w = config.weights;
    let t = config.thresholds;
    let pt = config.pattern_thresholds;
    let options = config.options;
    let source_map = config.source_map;
    let cfg = language::get_builder_for_function(function).build(function);
    if let Err(e) = cfg.validate() {
        eprintln!(
            "warning: skipping function '{}' in {}: invalid CFG: {}",
            function.name.as_deref().unwrap_or("<anonymous>"),
            path.display(),
            e
        );
        return None;
    }

    let raw_metrics = metrics::extract_metrics(function, &cfg);
    let (risk_components, lrs, band) = risk::analyze_risk_with_config(&raw_metrics, w, t);

    if options.min_lrs.is_some_and(|min| lrs < min) {
        return None;
    }

    let t1 = crate::patterns::Tier1Input {
        cc: raw_metrics.cc,
        nd: raw_metrics.nd,
        fo: raw_metrics.fo,
        ns: raw_metrics.ns,
        loc: raw_metrics.loc,
    };
    let t2 = crate::patterns::Tier2Input {
        fan_in: None,
        scc_size: None,
        churn_lines: None,
        days_since_last_change: None,
        neighbor_churn: None,
        is_entrypoint: false,
    };
    let patterns = crate::patterns::classify(&t1, &t2, pt);

    Some(report::FunctionRiskReport::new(
        function,
        path.to_string_lossy().to_string(),
        language.name().to_string(),
        report::FunctionAnalysis {
            metrics: raw_metrics,
            risk: risk_components,
            lrs,
            band,
            patterns,
        },
        source_map,
    ))
}
