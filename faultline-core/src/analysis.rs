//! Analysis orchestration - ties together parsing, discovery, CFG, metrics, and reporting

use crate::cfg::builder;
use crate::discover;
use crate::metrics;
use crate::parser;
use crate::report;
use crate::risk;
use anyhow::{Context, Result};
use std::path::Path;
use swc_common::{sync::Lrc, SourceMap};

/// Analyze a TypeScript or JavaScript file
pub fn analyze_file(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
) -> Result<Vec<report::FunctionRiskReport>> {
    // Read file
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Parse source file (TypeScript or JavaScript)
    let module = parser::parse_source(&src, source_map, &path.to_string_lossy())?;
    
    // Discover functions
    let functions = discover::discover_functions(&module, file_index);
    
    // Analyze each function
    let mut reports = Vec::new();
    for function in &functions {
        // Build CFG
        let cfg = builder::build_cfg(function);
        
        // Validate CFG
        cfg.validate()
            .map_err(|e| anyhow::anyhow!("Invalid CFG constructed: {}", e))?;
        
        // Extract metrics
        let raw_metrics = metrics::extract_metrics(function, &cfg);
        
        // Calculate risk
        let (risk_components, lrs, band) = risk::analyze_risk(&raw_metrics);
        
        // Apply filters
        if let Some(min_lrs) = options.min_lrs {
            if lrs < min_lrs {
                continue;
            }
        }
        
        // Create report
        let report = report::FunctionRiskReport::new(
            function,
            path.to_string_lossy().to_string(),
            raw_metrics,
            risk_components,
            lrs,
            band,
            source_map,
        );
        
        reports.push(report);
    }
    
    Ok(reports)
}
