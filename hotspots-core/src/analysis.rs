//! Analysis orchestration - ties together parsing, discovery, CFG, metrics, and reporting

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

    // Read file
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Detect language from file extension
    let language = Language::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file type: {}", path.display()))?;

    // Get appropriate parser for this language
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
    };

    // Parse source file
    let module = parser.parse(&src, &path.to_string_lossy())?;

    // Discover functions (with suppression extraction)
    let functions = module.discover_functions(file_index, &src);

    // Analyze each function
    let mut reports = Vec::new();
    for function in &functions {
        // Build CFG using language-specific builder
        let builder = language::get_builder_for_function(function);
        let cfg = builder.build(function);

        // Validate CFG
        if let Err(e) = cfg.validate() {
            eprintln!(
                "warning: skipping function '{}' in {}: invalid CFG: {}",
                function.name.as_deref().unwrap_or("<anonymous>"),
                path.display(),
                e
            );
            continue;
        }

        // Extract metrics
        let raw_metrics = metrics::extract_metrics(function, &cfg);

        // Calculate risk with configured weights/thresholds
        let (risk_components, lrs, band) = risk::analyze_risk_with_config(&raw_metrics, w, t);

        // Apply filters
        if let Some(min_lrs) = options.min_lrs {
            if lrs < min_lrs {
                continue;
            }
        }

        // Classify Tier 1 patterns (Tier 2 requires snapshot mode enrichment)
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

        // Create report
        let report = report::FunctionRiskReport::new(
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
        );

        reports.push(report);
    }

    Ok(reports)
}
