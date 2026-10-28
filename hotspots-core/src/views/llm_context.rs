//! LLM context view - bounded, prompt-ready projection of `FunctionSnapshot`
//! for inclusion in a hosted coding model's context window.
//!
//! A pure projection: selects, renames, and bounds fields already computed
//! by the analysis pipeline. Never recomputes a metric.

use crate::snapshot::FunctionSnapshot;
use serde::{Deserialize, Serialize};

/// Schema version for this view, independent of the master snapshot's
/// `schema_version` — this view's shape can change without a snapshot bump.
pub const LLM_CONTEXT_SCHEMA_VERSION: u32 = 1;

/// Number of functions included when `--top` is not specified.
pub const DEFAULT_TOP_N: usize = 20;

/// One function's risk context, sized for direct inclusion in a model prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LlmContextEntry {
    pub file: String,
    pub function: String,
    pub line: u32,
    pub band: String,
    pub risk_score: f64,
    /// Human-readable rationale from the `--explain` phrase table.
    /// `None` when `--explain` was not passed or no trained ranker was active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// Bounded, prompt-ready projection of the highest-risk functions in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LlmContextReport {
    pub schema_version: u32,
    pub entries: Vec<LlmContextEntry>,
}

/// Build a bounded LLM-context projection: the top `top_n` functions by
/// `activity_risk` (falling back to `lrs` when absent), descending.
pub fn to_llm_context_report(
    functions: &[FunctionSnapshot],
    repo_root: &std::path::Path,
    top_n: usize,
) -> LlmContextReport {
    let mut sorted: Vec<&FunctionSnapshot> = functions.iter().collect();
    sorted.sort_by(|a, b| {
        let a_score = a.activity_risk.unwrap_or(a.lrs);
        let b_score = b.activity_risk.unwrap_or(b.lrs);
        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let entries = sorted
        .into_iter()
        .take(top_n)
        .map(|func| {
            let function_name = func
                .function_id
                .split("::")
                .last()
                .unwrap_or(&func.function_id)
                .to_string();
            let file = crate::aggregates::normalize_path_relative_to_repo(&func.file, repo_root)
                .unwrap_or_else(|| func.file.clone());
            LlmContextEntry {
                file,
                function: function_name,
                line: func.line,
                band: func.band.as_str().to_string(),
                risk_score: func.activity_risk.unwrap_or(func.lrs),
                explanation: func.explanation.clone(),
            }
        })
        .collect();

    LlmContextReport {
        schema_version: LLM_CONTEXT_SCHEMA_VERSION,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::MetricsReport;
    use crate::risk::RiskBand;

    fn make_function(
        file: &str,
        function: &str,
        lrs: f64,
        activity_risk: Option<f64>,
    ) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: format!("{file}::{function}"),
            file: file.to_string(),
            line: 1,
            language: crate::language::Language::TypeScript,
            metrics: MetricsReport {
                cc: 1,
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 10,
                missing_docstring: false,
            },
            lrs,
            band: RiskBand::Moderate,
            suppression_reason: None,
            churn: None,
            touch_count: None,
            days_since_last_change: None,
            callgraph: None,
            activity_risk,
            risk_factors: None,
            percentile: None,
            driver: None,
            driver_detail: None,
            quadrant: None,
            patterns: vec![],
            pattern_details: None,
            subsystem: None,
            authors_90d: None,
            directed_coupling: None,
            convention_bug_fix_count: None,
            burst_score: None,
            commit_count: None,
            author_count: None,
            author_entropy: None,
            isolation_rate: None,
            age_days: None,
            last_touch_days: None,
            newcomer_rate: None,
            explanation: None,
        }
    }

    #[test]
    fn sorts_by_activity_risk_descending_falling_back_to_lrs() {
        let functions = vec![
            make_function("src/a.ts", "low", 2.0, Some(2.0)),
            make_function("src/b.ts", "high", 5.0, None), // falls back to lrs
            make_function("src/c.ts", "mid", 3.0, Some(3.5)),
        ];
        let report = to_llm_context_report(&functions, std::path::Path::new("."), 10);
        let names: Vec<&str> = report.entries.iter().map(|e| e.function.as_str()).collect();
        assert_eq!(names, vec!["high", "mid", "low"]);
    }

    #[test]
    fn bounds_output_to_top_n() {
        let functions = vec![
            make_function("src/a.ts", "a", 1.0, Some(1.0)),
            make_function("src/b.ts", "b", 2.0, Some(2.0)),
            make_function("src/c.ts", "c", 3.0, Some(3.0)),
        ];
        let report = to_llm_context_report(&functions, std::path::Path::new("."), 2);
        assert_eq!(report.entries.len(), 2);
    }

    #[test]
    fn carries_explanation_through_when_present() {
        let mut func = make_function("src/a.ts", "a", 9.0, Some(9.0));
        func.explanation = Some("high lifetime churn".to_string());
        let report = to_llm_context_report(&[func], std::path::Path::new("."), 10);
        assert_eq!(
            report.entries[0].explanation,
            Some("high lifetime churn".to_string())
        );
    }

    #[test]
    fn schema_version_is_stamped() {
        let report = to_llm_context_report(&[], std::path::Path::new("."), 10);
        assert_eq!(report.schema_version, LLM_CONTEXT_SCHEMA_VERSION);
        assert!(report.entries.is_empty());
    }
}
