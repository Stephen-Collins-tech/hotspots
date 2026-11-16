//! Agent-optimized triage view - slim projection of `FunctionSnapshot` for
//! the `triage`/`architecture` sections of the unified envelope.

use crate::snapshot::FunctionSnapshot;
use serde::{Deserialize, Serialize};

/// Slim complexity metrics for agent-optimized function view
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentMetrics {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
}

/// Slim function view for agent-optimized triage output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentFunctionView {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub band: String,
    pub tier: String,
    pub lrs: f64,
    pub activity_risk: f64,
    pub metrics: AgentMetrics,
    // hotspots 2.0: renamed from `touches_30d`/`days_since_changed` to match
    // `FunctionSnapshot`'s names exactly, now that this struct lives inside the
    // same unified envelope (Phase 1) — these were the "triage-mode spelling"
    // the master-schema-spec's duplicate-naming audit flagged (section 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_since_last_change: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_in: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// Per-prediction feature contributions from the trained ranker.
    /// Each entry is `[feature_name, contribution]`, sorted by |contribution| desc.
    /// Null until TreeSHAP is implemented.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shap: Option<Vec<(String, f64)>>,
}

/// Convert a slice of function snapshots into slim `AgentFunctionView` entries (top N).
pub(crate) fn to_agent_view(
    fns: &[&FunctionSnapshot],
    repo_root: &std::path::Path,
    top_n: usize,
) -> Vec<AgentFunctionView> {
    fns.iter()
        .take(top_n)
        .map(|func| {
            let function_name = crate::function_id::short_name(&func.function_id).to_string();
            let file = crate::aggregates::normalize_path_relative_to_repo(&func.file, repo_root)
                .unwrap_or_else(|| func.file.clone());
            AgentFunctionView {
                function: function_name,
                file,
                line: func.line,
                band: func.band.as_str().to_string(),
                tier: func.tier.clone().unwrap_or_else(|| "quiet".to_string()),
                lrs: func.lrs,
                activity_risk: func.activity_risk.unwrap_or(func.lrs),
                metrics: AgentMetrics {
                    cc: func.metrics.cc as usize,
                    nd: func.metrics.nd as usize,
                    fo: func.metrics.fo as usize,
                },
                touch_count: func.touch_count,
                days_since_last_change: func.days_since_last_change,
                fan_in: func.callgraph.as_ref().map(|cg| cg.fan_in),
                explanation: func.explanation.clone(),
                shap: None,
            }
        })
        .collect()
}
