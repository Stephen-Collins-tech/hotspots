//! Snapshot container and persistence
//!
//! Wraps analysis output in immutable, commit-scoped snapshots and persists them.
//!
//! Global invariants enforced:
//! - Snapshots are immutable (never overwrite existing snapshots)
//! - Commit hash is the sole identity (filename equals commit SHA)
//! - Byte-for-byte deterministic serialization
//! - Paths normalized to `/` (forward slashes only)
//! - ASCII lexical ordering (not locale-aware)

use crate::git::GitContext;
use crate::report::{FunctionRiskReport, MetricsReport};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::report::RiskReport;

/// Schema version for snapshots.
/// v1: LRS + basic metrics only
/// v2: adds LOC, git churn/touch, call graph, activity risk, percentiles, summary
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const SNAPSHOT_SCHEMA_MIN_VERSION: u32 = 1;

/// Schema version for index
const INDEX_SCHEMA_VERSION: u32 = 1;

/// Commit information in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CommitInfo {
    pub sha: String,
    pub parents: Vec<String>,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fix_commit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_revert_commit: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ticket_ids: Vec<String>,
}

impl From<GitContext> for CommitInfo {
    fn from(ctx: GitContext) -> Self {
        CommitInfo {
            sha: ctx.head_sha,
            parents: ctx.parent_shas,
            timestamp: ctx.timestamp,
            branch: ctx.branch,
            message: ctx.message,
            author: ctx.author,
            is_fix_commit: ctx.is_fix_commit,
            is_revert_commit: ctx.is_revert_commit,
            ticket_ids: ctx.ticket_ids,
        }
    }
}

/// Analysis metadata in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AnalysisInfo {
    pub scope: String,
    #[serde(rename = "tool_version")]
    pub tool_version: String,
}

/// Churn metrics for a file/function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChurnMetrics {
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub net_change: i64,
}

/// Per-function percentile flags based on activity_risk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PercentileFlags {
    pub is_top_10_pct: bool,
    pub is_top_5_pct: bool,
    pub is_top_1_pct: bool,
}

/// Call graph metrics for a function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CallGraphMetrics {
    pub fan_in: usize,
    pub fan_out: usize,
    pub pagerank: f64,
    pub betweenness: f64,
    pub scc_id: usize,
    pub scc_size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neighbor_churn: Option<usize>,
}

/// Function entry in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionSnapshot {
    pub function_id: String,
    pub file: String,
    pub line: u32,
    pub language: String,
    pub metrics: MetricsReport,
    pub lrs: f64,
    pub band: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn: Option<ChurnMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_count_30d: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_since_last_change: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callgraph: Option<CallGraphMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_risk: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_factors: Option<crate::scoring::RiskFactors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentile: Option<PercentileFlags>,
    /// Primary driving dimension label (e.g. "high_complexity", "high_churn_low_cc").
    /// Populated by the enricher after activity_risk is computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// Near-miss detail for composite functions: top dimensions that almost fired,
    /// with their percentile rank. E.g. "cc (P72), nd (P68)".
    /// None for non-composite labels or when no metric is near-threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_detail: Option<String>,
    /// Triage quadrant. Values: "fire", "debt", "watch", "ok".
    /// fire  = high/critical + active (touches > p50 or changed ≤30d)
    /// debt  = high/critical + not active
    /// watch = moderate/low  + active
    /// ok    = everything else
    /// Populated by the enricher after driver labels. None before enrichment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quadrant: Option<String>,
}

/// Risk distribution by band
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BandStats {
    pub count: usize,
    pub sum_risk: f64,
}

/// Call graph statistics for the whole repo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CallGraphStats {
    pub total_edges: usize,
    pub avg_fan_in: f64,
    pub scc_count: usize,
    pub largest_scc_size: usize,
}

/// Repo-level summary statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SnapshotSummary {
    pub total_functions: usize,
    pub total_activity_risk: f64,
    pub top_1_pct_share: f64,
    pub top_5_pct_share: f64,
    pub top_10_pct_share: f64,
    pub by_band: std::collections::BTreeMap<String, BandStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_graph: Option<CallGraphStats>,
}

/// Complete snapshot for a commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Snapshot {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    pub commit: CommitInfo,
    pub analysis: AnalysisInfo,
    pub functions: Vec<FunctionSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SnapshotSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<crate::aggregates::SnapshotAggregates>,
}

/// Index entry for a commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct IndexEntry {
    pub sha: String,
    pub parents: Vec<String>,
    pub timestamp: i64,
}

/// Index containing all tracked commits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Index {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    /// Compaction level (0 = full snapshots, 1 = deltas only, 2 = band transitions only)
    /// None means Level 0 (full snapshots) - for backward compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_level: Option<u32>,
    pub commits: Vec<IndexEntry>,
}

impl Snapshot {
    /// Create a new snapshot from git context and function reports
    ///
    /// # Arguments
    ///
    /// * `git_context` - Git context for the current commit
    /// * `reports` - Function risk reports from analysis
    ///
    /// # Function ID Format
    ///
    /// Function ID is `<relative_file_path>::<symbol>` where:
    /// - `relative_file_path` is normalized to use `/` separators
    /// - `symbol` is the function name (or `<anonymous>` for anonymous functions)
    pub fn new(git_context: GitContext, reports: Vec<FunctionRiskReport>) -> Self {
        // Normalize paths and build function snapshots
        let mut functions: Vec<FunctionSnapshot> = reports
            .into_iter()
            .map(|report| {
                // Normalize file path to use `/` separators
                let normalized_file = report.file.replace('\\', "/");

                // Extract function name for function_id
                // Use the function name from report, or derive from file/line if needed
                let function_symbol = if report.function.starts_with("<anonymous>") {
                    "<anonymous>"
                } else {
                    &report.function
                };

                // Build function_id: <relative_file_path>::<symbol>
                let function_id = format!("{}::{}", normalized_file, function_symbol);

                FunctionSnapshot {
                    function_id,
                    file: normalized_file,
                    line: report.line,
                    language: report.language,
                    metrics: report.metrics,
                    lrs: report.lrs,
                    band: report.band,
                    suppression_reason: report.suppression_reason,
                    churn: None, // Churn will be populated separately if available
                    touch_count_30d: None, // Touch count will be populated separately if available
                    days_since_last_change: None, // Days since last change will be populated separately if available
                    callgraph: None, // Call graph metrics will be populated separately if available
                    activity_risk: None,
                    risk_factors: None,
                    percentile: None,
                    driver: None,
                    driver_detail: None,
                    quadrant: None,
                }
            })
            .collect();

        // Sort functions deterministically by function_id (ASCII lexical ordering)
        functions.sort_by(|a, b| a.function_id.cmp(&b.function_id));

        Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            commit: CommitInfo::from(git_context),
            analysis: AnalysisInfo {
                scope: "full".to_string(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            functions,
            summary: None,
            aggregates: None, // Aggregates are computed on-demand, not stored
        }
    }

    /// Populate churn metrics from git data
    ///
    /// Maps file-level churn to all functions in each file.
    /// Files not in the churn map will have churn remain as None.
    ///
    /// # Arguments
    ///
    /// * `file_churns` - Map from file path to churn metrics
    pub fn populate_churn(
        &mut self,
        file_churns: &std::collections::HashMap<String, crate::git::FileChurn>,
    ) {
        for function in &mut self.functions {
            // Normalize file path for lookup (already normalized in constructor)
            let file_path = &function.file;

            if let Some(file_churn) = file_churns.get(file_path) {
                let net_change = file_churn.lines_added as i64 - file_churn.lines_deleted as i64;
                function.churn = Some(ChurnMetrics {
                    lines_added: file_churn.lines_added,
                    lines_deleted: file_churn.lines_deleted,
                    net_change,
                });
            }
        }
    }

    // Per-function touch metrics: one `git log -L` subprocess per function (~9 ms each).
    // A disk cache keyed by (sha, file, start, end) avoids re-running subprocesses for
    // functions whose line ranges have not changed since the last run (warm path).
    fn populate_per_function_touch_metrics(
        &mut self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<()> {
        let sha = self.commit.sha.clone();
        let mut cache = crate::touch_cache::read_touch_cache(repo_root).unwrap_or_default();
        let mut dirty = false;

        for function in &mut self.functions {
            let rel = if let Ok(r) = std::path::Path::new(&function.file).strip_prefix(repo_root) {
                r.to_string_lossy().replace('\\', "/")
            } else {
                function.file.replace('\\', "/")
            };

            let start_line = function.line;
            let end_line =
                (start_line + (function.metrics.loc as u32).saturating_sub(1)).max(start_line);
            let key = crate::touch_cache::cache_key(&sha, &rel, start_line, end_line);

            if let Some(&(count, days)) = cache.get(&key) {
                function.touch_count_30d = Some(count);
                function.days_since_last_change = days;
            } else {
                match crate::git::function_touch_metrics_at(
                    repo_root,
                    &rel,
                    start_line,
                    end_line,
                    self.commit.timestamp,
                ) {
                    Ok((count, days)) => {
                        function.touch_count_30d = Some(count);
                        function.days_since_last_change = days;
                        cache.insert(key, (count, days));
                        dirty = true;
                    }
                    Err(_) => {
                        function.touch_count_30d = Some(0);
                        cache.insert(key, (0, None));
                        dirty = true;
                    }
                }
            }
        }

        if dirty {
            // Evict stale entries (most-recent SHAs first) then write.
            // Always include the current SHA first so we don't evict entries we just
            // wrote — this matters when --no-persist is used and the SHA isn't in the index yet.
            let known_shas: Vec<String> = {
                let mut commits = Index::load_or_new(&index_path(repo_root))
                    .map(|idx| idx.commits)
                    .unwrap_or_default();
                commits.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                let mut shas = vec![sha.clone()];
                shas.extend(commits.into_iter().map(|e| e.sha).filter(|s| s != &sha));
                shas
            };
            crate::touch_cache::evict_old_entries(&mut cache, &known_shas);
            if let Err(e) = crate::touch_cache::write_touch_cache(repo_root, &cache) {
                eprintln!("warning: failed to write touch cache: {e}");
            }
        }

        Ok(())
    }

    /// File-level touch metrics: one batched git call + per-file fallback
    fn populate_file_level_touch_metrics(
        &mut self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<()> {
        use std::collections::HashMap;

        // Build map of absolute path -> function indices
        let mut unique_files: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, function) in self.functions.iter().enumerate() {
            unique_files
                .entry(function.file.clone())
                .or_default()
                .push(idx);
        }

        // Convert all absolute paths to relative paths for git
        let abs_to_rel: HashMap<String, String> = unique_files
            .keys()
            .map(|abs| {
                let rel = if let Ok(r) = std::path::Path::new(abs).strip_prefix(repo_root) {
                    r.to_string_lossy().to_string()
                } else {
                    abs.clone()
                };
                (abs.clone(), rel)
            })
            .collect();

        // One batched call for the 30-day window (replaces N×2 individual calls)
        let batched = crate::git::batch_touch_metrics_at(repo_root, self.commit.timestamp)
            .unwrap_or_else(|_| crate::git::BatchedTouchMetrics {
                touch_count_30d: HashMap::new(),
                days_since_last_change: HashMap::new(),
            });

        // Apply batched results; fall back per-file for anything not in the window
        for (abs_path, function_indices) in &unique_files {
            let rel = abs_to_rel
                .get(abs_path)
                .map(|s| s.as_str())
                .unwrap_or(abs_path);

            let touch_count = batched.touch_count_30d.get(rel).copied().or(Some(0));
            let days_since = batched
                .days_since_last_change
                .get(rel)
                .copied()
                .or_else(|| {
                    crate::git::days_since_last_change_at(repo_root, rel, self.commit.timestamp)
                        .ok()
                });

            for &idx in function_indices {
                self.functions[idx].touch_count_30d = touch_count;
                self.functions[idx].days_since_last_change = days_since;
            }
        }

        Ok(())
    }

    /// Populate touch count and recency metrics from git data
    ///
    /// For each file (or function when `per_function` is true), computes:
    /// - touch_count_30d: number of commits in last 30 days
    /// - days_since_last_change: days since last modification
    ///
    /// When `per_function` is false (default), metrics are file-level and applied to all
    /// functions in the file (fast, O(1) git calls via batching).
    /// When `per_function` is true, metrics use `git log -L` per function (accurate but slow,
    /// O(functions) subprocess calls — ~50× slower than file-level).
    ///
    /// # Arguments
    ///
    /// * `repo_root` - Path to repository root (for git operations)
    /// * `per_function` - Use per-function `git log -L` instead of file-level batching
    pub fn populate_touch_metrics(
        &mut self,
        repo_root: &std::path::Path,
        per_function: bool,
    ) -> anyhow::Result<()> {
        if per_function {
            self.populate_per_function_touch_metrics(repo_root)
        } else {
            self.populate_file_level_touch_metrics(repo_root)
        }
    }

    /// Replace branch-inflated recency values with pre-branch last-change dates.
    ///
    /// For functions touched only on this branch (days_since_last_change < branch age),
    /// replaces the inflated recency with the last-change date before the branch diverged.
    /// One git call per unique file that needs a lookup; no-op when called without a merge base.
    pub fn adjust_recency_for_branch(
        &mut self,
        repo_root: &std::path::Path,
        merge_base_sha: &str,
        merge_base_ts: i64,
    ) {
        let merge_base_age_days = ((self.commit.timestamp - merge_base_ts).max(0) / 86400) as u32;

        // Identify unique files touched only on this branch
        let mut files_needing_lookup: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for func in &self.functions {
            if func
                .days_since_last_change
                .is_some_and(|d| d < merge_base_age_days)
            {
                files_needing_lookup.insert(func.file.clone());
            }
        }

        // One git call per unique file — get last-change date before branch diverged
        let mut pre_branch: std::collections::HashMap<String, Option<u32>> =
            std::collections::HashMap::new();
        for abs_file in &files_needing_lookup {
            let rel = std::path::Path::new(abs_file)
                .strip_prefix(repo_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| abs_file.replace('\\', "/"));
            let days = crate::git::days_since_last_change_at_sha(
                repo_root,
                &rel,
                merge_base_sha,
                self.commit.timestamp,
            );
            pre_branch.insert(abs_file.clone(), days);
        }

        // Replace inflated recency with pre-branch value
        for func in &mut self.functions {
            if let Some(Some(pre_days)) = pre_branch.get(&func.file) {
                func.days_since_last_change = Some(*pre_days);
            }
        }
    }

    /// Populate call graph metrics
    ///
    /// Computes PageRank, betweenness centrality, fan-in, fan-out, SCC, dependency depth,
    /// and neighbor churn metrics for all functions.
    ///
    /// # Arguments
    ///
    /// * `call_graph` - Pre-computed call graph for the codebase
    pub fn populate_callgraph(&mut self, call_graph: &crate::callgraph::CallGraph) {
        use std::collections::HashMap;

        // Compute global metrics once
        let pagerank_scores = call_graph.pagerank(0.85, 30);
        let betweenness_scores = call_graph.betweenness_centrality();
        let scc_info = call_graph.find_strongly_connected_components();
        let dependency_depths = call_graph.compute_dependency_depth();

        // Build a map of function_id -> total churn (lines_added + lines_deleted)
        let mut churn_map: HashMap<String, usize> = HashMap::new();
        for function in &self.functions {
            if let Some(ref churn) = function.churn {
                let total_churn = churn.lines_added + churn.lines_deleted;
                churn_map.insert(function.function_id.clone(), total_churn);
            }
        }

        // Populate metrics for each function
        for function in &mut self.functions {
            let function_id = &function.function_id;

            // Only populate if function is in the call graph
            if call_graph.nodes.contains(function_id) {
                let (scc_id, scc_size) = scc_info.get(function_id).copied().unwrap_or((0, 1));
                let dependency_depth = dependency_depths.get(function_id).copied().flatten();

                // Compute neighbor churn: sum of churn for all callees
                let neighbor_churn = if let Some(callees) = call_graph.edges.get(function_id) {
                    let total: usize = callees
                        .iter()
                        .filter_map(|callee_id| churn_map.get(callee_id))
                        .sum();
                    if total > 0 {
                        Some(total)
                    } else {
                        None
                    }
                } else {
                    None
                };

                function.callgraph = Some(CallGraphMetrics {
                    fan_in: call_graph.fan_in(function_id),
                    fan_out: call_graph.fan_out(function_id),
                    pagerank: pagerank_scores.get(function_id).copied().unwrap_or(0.0),
                    betweenness: betweenness_scores.get(function_id).copied().unwrap_or(0.0),
                    scc_id,
                    scc_size,
                    dependency_depth,
                    neighbor_churn,
                });
            }
        }
    }

    /// Compute and populate activity risk scores
    ///
    /// Combines LRS with activity metrics and call graph metrics to produce
    /// a unified risk score. Should be called after populate_churn, populate_touch_metrics,
    /// and populate_callgraph have been called.
    ///
    /// # Arguments
    ///
    /// * `weights` - Optional weights for risk factors (uses defaults if None)
    pub fn compute_activity_risk(&mut self, weights: Option<&crate::scoring::ScoringWeights>) {
        let default_weights = crate::scoring::ScoringWeights::default();
        let weights = weights.unwrap_or(&default_weights);

        for function in &mut self.functions {
            // Extract churn data
            let churn = function
                .churn
                .as_ref()
                .map(|c| (c.lines_added, c.lines_deleted));

            // Extract call graph data
            let (fan_in, scc_size, dependency_depth, neighbor_churn) =
                if let Some(ref cg) = function.callgraph {
                    (
                        Some(cg.fan_in),
                        Some(cg.scc_size),
                        cg.dependency_depth,
                        cg.neighbor_churn,
                    )
                } else {
                    (None, None, None, None)
                };

            // Compute activity risk
            let (activity_risk, risk_factors) = crate::scoring::compute_activity_risk(
                &crate::scoring::ActivityRiskInput {
                    lrs: function.lrs,
                    churn,
                    touch_count_30d: function.touch_count_30d,
                    days_since_last_change: function.days_since_last_change,
                    fan_in,
                    scc_size,
                    dependency_depth,
                    neighbor_churn,
                },
                weights,
            );

            // Only populate if there are additional risk factors beyond base LRS
            if activity_risk > function.lrs || risk_factors.churn > 0.0 {
                function.activity_risk = Some(activity_risk);
                function.risk_factors = Some(risk_factors);
            }
        }
    }

    /// Compute and populate percentile flags for all functions
    ///
    /// Must be called after compute_activity_risk().
    /// Flags: is_top_1_pct, is_top_5_pct, is_top_10_pct based on activity_risk.
    pub fn compute_percentiles(&mut self) {
        let n = self.functions.len();
        if n == 0 {
            return;
        }

        // Collect all activity_risk scores (falling back to lrs)
        let mut scores: Vec<f64> = self
            .functions
            .iter()
            .map(|f| f.activity_risk.unwrap_or(f.lrs))
            .collect();
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Compute threshold values via quantile index
        let threshold_10 = scores[n.saturating_sub(1) * 90 / 100];
        let threshold_5 = scores[n.saturating_sub(1) * 95 / 100];
        let threshold_1 = scores[n.saturating_sub(1) * 99 / 100];

        for function in &mut self.functions {
            let score = function.activity_risk.unwrap_or(function.lrs);
            function.percentile = Some(PercentileFlags {
                is_top_10_pct: score >= threshold_10,
                is_top_5_pct: score >= threshold_5,
                is_top_1_pct: score >= threshold_1,
            });
        }
    }

    /// Populate driver labels for all functions using driving_dimension_label.
    ///
    /// Must be called after compute_activity_risk() and populate_callgraph().
    pub fn populate_driver_labels(&mut self, percentile: u8) {
        let thresholds = compute_dimension_thresholds(&self.functions, percentile);

        let mut sorted_cc: Vec<usize> = self.functions.iter().map(|f| f.metrics.cc).collect();
        let mut sorted_nd: Vec<usize> = self.functions.iter().map(|f| f.metrics.nd).collect();
        let mut sorted_fo: Vec<usize> = self
            .functions
            .iter()
            .map(|f| f.callgraph.as_ref().map(|cg| cg.fan_out).unwrap_or(0))
            .collect();
        let mut sorted_fi: Vec<usize> = self
            .functions
            .iter()
            .map(|f| f.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0))
            .collect();
        let mut sorted_touch: Vec<usize> = self
            .functions
            .iter()
            .map(|f| f.touch_count_30d.unwrap_or(0))
            .collect();
        sorted_cc.sort_unstable();
        sorted_nd.sort_unstable();
        sorted_fo.sort_unstable();
        sorted_fi.sort_unstable();
        sorted_touch.sort_unstable();

        for function in &mut self.functions {
            let label = driving_dimension_label(function, &thresholds).to_string();
            function.driver_detail = if label == "composite" {
                compute_near_miss_detail(
                    function,
                    &sorted_cc,
                    &sorted_nd,
                    &sorted_fo,
                    &sorted_fi,
                    &sorted_touch,
                )
            } else {
                None
            };
            function.driver = Some(label);
        }
    }

    /// Compute and populate triage quadrant for all functions.
    ///
    /// Quadrant logic (Option C — combines both signals):
    ///   is_active = touches_30d > touch_p50 OR days_since_last_change <= 30
    ///   fire  = high/critical + is_active
    ///   debt  = high/critical + !is_active
    ///   watch = moderate/low  + is_active
    ///   ok    = everything else
    ///
    /// Must be called after populate_driver_labels().
    pub fn compute_quadrants(&mut self, driver_threshold_percentile: u8) {
        if self.functions.is_empty() {
            return;
        }
        let thresholds = compute_dimension_thresholds(&self.functions, driver_threshold_percentile);
        let touch_p50 = thresholds.touch_med;

        for function in &mut self.functions {
            let touch_above_p50 = function
                .touch_count_30d
                .map(|t| t > touch_p50)
                .unwrap_or(false);
            let recently_changed = function
                .days_since_last_change
                .map(|d| d <= 30)
                .unwrap_or(false);
            let is_active = touch_above_p50 || recently_changed;
            let is_high_risk = matches!(function.band.as_str(), "critical" | "high");

            function.quadrant = Some(
                match (is_high_risk, is_active) {
                    (true, true) => "fire",
                    (true, false) => "debt",
                    (false, true) => "watch",
                    (false, false) => "ok",
                }
                .to_string(),
            );
        }
    }

    /// Compute repo-level summary statistics
    ///
    /// Must be called after compute_activity_risk() and populate_callgraph().
    pub fn compute_summary(&mut self) {
        let n = self.functions.len();
        if n == 0 {
            self.summary = Some(SnapshotSummary {
                total_functions: 0,
                total_activity_risk: 0.0,
                top_1_pct_share: 0.0,
                top_5_pct_share: 0.0,
                top_10_pct_share: 0.0,
                by_band: std::collections::BTreeMap::new(),
                call_graph: None,
            });
            return;
        }

        // Collect scores sorted descending
        let mut scored: Vec<f64> = self
            .functions
            .iter()
            .map(|f| f.activity_risk.unwrap_or(f.lrs))
            .collect();
        scored.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        let total_risk: f64 = scored.iter().sum();

        // Top-K share calculations
        let top1_n = (n / 100).max(1);
        let top5_n = (n * 5 / 100).max(1);
        let top10_n = (n / 10).max(1);

        let top1_sum: f64 = scored.iter().take(top1_n).sum();
        let top5_sum: f64 = scored.iter().take(top5_n).sum();
        let top10_sum: f64 = scored.iter().take(top10_n).sum();

        let safe_div = |a: f64, b: f64| if b > 0.0 { a / b } else { 0.0 };

        // Band distribution
        let mut by_band: std::collections::BTreeMap<String, BandStats> =
            std::collections::BTreeMap::new();
        for func in &self.functions {
            let score = func.activity_risk.unwrap_or(func.lrs);
            let entry = by_band.entry(func.band.clone()).or_insert(BandStats {
                count: 0,
                sum_risk: 0.0,
            });
            entry.count += 1;
            entry.sum_risk += score;
        }

        // Call graph stats
        let has_callgraph = self.functions.iter().any(|f| f.callgraph.is_some());
        let call_graph = if has_callgraph {
            let total_edges: usize = self
                .functions
                .iter()
                .filter_map(|f| f.callgraph.as_ref())
                .map(|cg| cg.fan_out)
                .sum();
            let total_fan_in: usize = self
                .functions
                .iter()
                .filter_map(|f| f.callgraph.as_ref())
                .map(|cg| cg.fan_in)
                .sum();
            let avg_fan_in = total_fan_in as f64 / n as f64;

            // SCC analysis: group by scc_id, count SCCs with size > 1
            let mut scc_sizes: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::new();
            for func in &self.functions {
                if let Some(ref cg) = func.callgraph {
                    if cg.scc_size > 1 {
                        scc_sizes.insert(cg.scc_id, cg.scc_size);
                    }
                }
            }
            let scc_count = scc_sizes.len();
            let largest_scc_size = scc_sizes.values().copied().max().unwrap_or(0);

            Some(CallGraphStats {
                total_edges,
                avg_fan_in,
                scc_count,
                largest_scc_size,
            })
        } else {
            None
        };

        self.summary = Some(SnapshotSummary {
            total_functions: n,
            total_activity_risk: total_risk,
            top_1_pct_share: safe_div(top1_sum, total_risk),
            top_5_pct_share: safe_div(top5_sum, total_risk),
            top_10_pct_share: safe_div(top10_sum, total_risk),
            by_band,
            call_graph,
        });
    }

    /// Serialize snapshot as JSONL (one JSON object per line, no outer array)
    ///
    /// Each line embeds the commit context alongside function data,
    /// suitable for streaming ingestion (DuckDB, jq -s, etc.)
    pub fn to_jsonl(&self) -> Result<String> {
        let commit_json =
            serde_json::to_value(&self.commit).context("failed to serialize commit")?;

        let mut lines = Vec::with_capacity(self.functions.len());
        for func in &self.functions {
            let mut obj = serde_json::to_value(func).context("failed to serialize function")?;
            // Embed commit context in each row
            obj.as_object_mut()
                .context("serialized function is not a JSON object")?
                .insert("commit".to_string(), commit_json.clone());
            lines.push(serde_json::to_string(&obj).context("failed to serialize JSONL line")?);
        }

        Ok(lines.join("\n"))
    }

    /// Serialize snapshot to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        // Use serde_json with pretty printing for readability
        // Keys are automatically sorted by serde when using BTreeMap-like structures
        // For deterministic ordering, we rely on serde's default behavior with sorted keys
        serde_json::to_string_pretty(self).context("failed to serialize snapshot to JSON")
    }

    /// Deserialize snapshot from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let snapshot: Snapshot =
            serde_json::from_str(json).context("failed to deserialize snapshot from JSON")?;

        // Validate schema version (accept v1 snapshots — missing fields default to None)
        if snapshot.schema_version < SNAPSHOT_SCHEMA_MIN_VERSION
            || snapshot.schema_version > SNAPSHOT_SCHEMA_VERSION
        {
            anyhow::bail!(
                "unsupported schema version: got {}, supported range {}-{}",
                snapshot.schema_version,
                SNAPSHOT_SCHEMA_MIN_VERSION,
                SNAPSHOT_SCHEMA_VERSION
            );
        }

        Ok(snapshot)
    }

    /// Get the commit SHA for this snapshot
    pub fn commit_sha(&self) -> &str {
        &self.commit.sha
    }
}

/// Percentile-derived thresholds for driving dimension detection.
/// Computed once per snapshot from the distribution of all functions.
pub struct DimensionThresholds {
    pub cc_high: usize,      // Pth percentile of cc — "high_complexity" gate
    pub cc_med: usize,       // 50th percentile of cc — floor for "high_fanin_complex"
    pub cc_low: usize,       // (100-P)th percentile of cc — "low cc" in "high_churn_low_cc"
    pub nd_high: usize,      // Pth percentile of nd — "deep_nesting" gate
    pub fan_out_high: usize, // Pth percentile of fan_out — "high_fanout_churning" gate
    pub fan_in_high: usize,  // Pth percentile of fan_in — "high_fanin_complex" gate
    pub touch_high: usize,   // Pth percentile of touch_count — "high churn" gate
    pub touch_med: usize,    // 50th percentile of touch_count — floor for "high_fanout_churning"
}

/// Compute percentile-derived thresholds from a slice of function snapshots.
pub fn compute_dimension_thresholds(
    functions: &[FunctionSnapshot],
    percentile: u8,
) -> DimensionThresholds {
    let n = functions.len();
    if n == 0 {
        return DimensionThresholds {
            cc_high: 0,
            cc_med: 0,
            cc_low: 0,
            nd_high: 0,
            fan_out_high: 0,
            fan_in_high: 0,
            touch_high: 0,
            touch_med: 0,
        };
    }

    let p = percentile as usize;
    let anti_p = 100 - p;

    let percentile_idx = |pct: usize| (pct * (n - 1)) / 100;

    let mut cc_vals: Vec<usize> = functions.iter().map(|f| f.metrics.cc).collect();
    cc_vals.sort_unstable();
    let cc_high = cc_vals[percentile_idx(p)];
    let cc_med = cc_vals[percentile_idx(50)];
    let cc_low = cc_vals[percentile_idx(anti_p)];

    let mut nd_vals: Vec<usize> = functions.iter().map(|f| f.metrics.nd).collect();
    nd_vals.sort_unstable();
    let nd_high = nd_vals[percentile_idx(p)];

    let mut fo_vals: Vec<usize> = functions
        .iter()
        .map(|f| f.callgraph.as_ref().map(|cg| cg.fan_out).unwrap_or(0))
        .collect();
    fo_vals.sort_unstable();
    let fan_out_high = fo_vals[percentile_idx(p)];

    let mut fi_vals: Vec<usize> = functions
        .iter()
        .map(|f| f.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0))
        .collect();
    fi_vals.sort_unstable();
    let fan_in_high = fi_vals[percentile_idx(p)];

    let mut touch_vals: Vec<usize> = functions
        .iter()
        .map(|f| f.touch_count_30d.unwrap_or(0))
        .collect();
    touch_vals.sort_unstable();
    let touch_high = touch_vals[percentile_idx(p)];
    let touch_med = touch_vals[percentile_idx(50)];

    DimensionThresholds {
        cc_high,
        cc_med,
        cc_low,
        nd_high,
        fan_out_high,
        fan_in_high,
        touch_high,
        touch_med,
    }
}

/// Normalize a driver label string to a canonical `'static` str.
pub fn normalize_driver_label(label: &str) -> &'static str {
    match label {
        "cyclic_dep" => "cyclic_dep",
        "high_complexity" => "high_complexity",
        "high_churn_low_cc" => "high_churn_low_cc",
        "high_fanout_churning" => "high_fanout_churning",
        "deep_nesting" => "deep_nesting",
        "high_fanin_complex" => "high_fanin_complex",
        _ => "composite",
    }
}

/// Map a (driver, quadrant) pair to a recommended action string.
///
/// `quadrant` is one of `"fire"`, `"debt"`, `"watch"`, `"ok"`, or `""` (unknown).
/// When quadrant context is available the action is more specific; the generic
/// driver-only text is used as a fallback.
pub fn driver_action_for_quadrant(driver: &str, quadrant: &str) -> &'static str {
    match (driver, quadrant) {
        ("cyclic_dep", "fire") => "Break cycle now — circular dep is actively changing",
        ("cyclic_dep", _) => "Resolve dependency cycle",
        ("high_complexity", "fire") => "Extract sub-functions now — actively changing",
        ("high_complexity", "debt") => "Schedule CC reduction — stable, plan for next sprint",
        ("high_complexity", _) => "Reduce cyclomatic complexity",
        ("high_churn_low_cc", "fire") => "Add tests now — churning without a safety net",
        ("high_churn_low_cc", _) => "Add tests before next change",
        ("high_fanout_churning", "fire") => {
            "Extract interface boundary — high coupling + active change"
        }
        ("high_fanout_churning", _) => "Consider extracting an interface boundary",
        ("deep_nesting", "fire") => "Flatten nesting before next change",
        ("deep_nesting", "debt") => "Schedule flattening — deep nesting, currently quiet",
        ("deep_nesting", _) => "Flatten nesting depth",
        ("high_fanin_complex", "fire") => "Stabilize interface — many callers + active changes",
        ("high_fanin_complex", _) => "Stabilize interface — high fan-in makes changes risky",
        (_, "fire") => "Actively risky — plan refactor this sprint",
        _ => "Monitor: review complexity trends before next modification",
    }
}

/// Map a driver label to its recommended action text (quadrant-agnostic fallback).
pub fn driver_action(label: &str) -> &'static str {
    driver_action_for_quadrant(label, "")
}

/// Identify the primary driving dimension for a function's risk.
///
/// Returns a stable label: one of `"cyclic_dep"`, `"high_complexity"`,
/// `"high_churn_low_cc"`, `"high_fanout_churning"`, `"deep_nesting"`,
/// `"high_fanin_complex"`, or `"composite"`. Uses percentile-relative thresholds
/// derived from the snapshot's own distribution; `cyclic_dep` stays absolute.
pub fn driving_dimension_label(
    func: &FunctionSnapshot,
    thresholds: &DimensionThresholds,
) -> &'static str {
    let in_cycle = func
        .callgraph
        .as_ref()
        .map(|cg| cg.scc_size > 1)
        .unwrap_or(false);
    let fan_out = func.callgraph.as_ref().map(|cg| cg.fan_out).unwrap_or(0);
    let fan_in = func.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0);
    let touch_count = func.touch_count_30d.unwrap_or(0);
    let cc = func.metrics.cc;
    let nd = func.metrics.nd;

    if in_cycle {
        "cyclic_dep"
    } else if cc > thresholds.cc_high {
        "high_complexity"
    } else if touch_count > thresholds.touch_high && cc < thresholds.cc_low {
        "high_churn_low_cc"
    } else if fan_out > thresholds.fan_out_high && touch_count > thresholds.touch_med {
        "high_fanout_churning"
    } else if nd > thresholds.nd_high {
        "deep_nesting"
    } else if fan_in > thresholds.fan_in_high && cc > thresholds.cc_med {
        "high_fanin_complex"
    } else {
        "composite"
    }
}

/// Compute near-miss detail string for composite-labeled functions.
///
/// Returns a string like "cc (P72), nd (P68)" listing the top dimensions that
/// are above the 40th percentile (above median) but below the firing threshold.
/// Returns None when no dimension is notable.
fn compute_near_miss_detail(
    func: &FunctionSnapshot,
    sorted_cc: &[usize],
    sorted_nd: &[usize],
    sorted_fo: &[usize],
    sorted_fi: &[usize],
    sorted_touch: &[usize],
) -> Option<String> {
    let pct_rank = |v: usize, sorted: &[usize]| -> u8 {
        if sorted.is_empty() {
            return 0;
        }
        ((sorted.partition_point(|&x| x < v) * 100) / sorted.len()) as u8
    };

    let mut near: Vec<(&str, u8)> = vec![
        ("cc", pct_rank(func.metrics.cc, sorted_cc)),
        ("nd", pct_rank(func.metrics.nd, sorted_nd)),
        (
            "fan_out",
            pct_rank(
                func.callgraph.as_ref().map(|cg| cg.fan_out).unwrap_or(0),
                sorted_fo,
            ),
        ),
        (
            "fan_in",
            pct_rank(
                func.callgraph.as_ref().map(|cg| cg.fan_in).unwrap_or(0),
                sorted_fi,
            ),
        ),
        (
            "touch",
            pct_rank(func.touch_count_30d.unwrap_or(0), sorted_touch),
        ),
    ]
    .into_iter()
    .filter(|(_, rank)| *rank >= 40)
    .collect();

    near.sort_by(|a, b| b.1.cmp(&a.1));
    near.truncate(3);

    if near.is_empty() {
        return None;
    }
    Some(
        near.iter()
            .map(|(name, rank)| format!("{} (P{})", name, rank))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// churn → touch_metrics → callgraph → activity_risk + percentiles + summary.
pub struct SnapshotEnricher {
    snapshot: Snapshot,
}

impl SnapshotEnricher {
    /// Create a new enricher wrapping the given snapshot.
    pub fn new(snapshot: Snapshot) -> Self {
        SnapshotEnricher { snapshot }
    }

    /// Populate churn metrics from a file churn map.
    pub fn with_churn(
        mut self,
        file_churns: &std::collections::HashMap<String, crate::git::FileChurn>,
    ) -> Self {
        self.snapshot.populate_churn(file_churns);
        self
    }

    /// Populate touch count and recency metrics from git.
    ///
    /// When `per_function` is true, uses `git log -L` per function (accurate, slow).
    /// On error, emits a warning to stderr and continues.
    pub fn with_touch_metrics(mut self, repo_root: &Path, per_function: bool) -> Self {
        if let Err(e) = self
            .snapshot
            .populate_touch_metrics(repo_root, per_function)
        {
            eprintln!("Warning: failed to populate touch metrics: {}", e);
        }
        self
    }

    /// Replace branch-inflated recency with pre-branch last-change dates.
    /// No-op when merge_base is None (on main, or no divergence).
    pub fn with_branch_recency_adjustment(
        mut self,
        repo_root: &Path,
        merge_base: Option<&(String, i64)>,
    ) -> Self {
        if let Some((sha, ts)) = merge_base {
            self.snapshot.adjust_recency_for_branch(repo_root, sha, *ts);
        }
        self
    }

    /// Populate call graph metrics (PageRank, fan-in, SCC, etc).
    pub fn with_callgraph(mut self, call_graph: &crate::callgraph::CallGraph) -> Self {
        self.snapshot.populate_callgraph(call_graph);
        self
    }

    /// Compute activity risk, percentile flags, driver labels, and summary statistics.
    ///
    /// Must be called after with_churn, with_touch_metrics, and with_callgraph.
    pub fn enrich(
        mut self,
        weights: Option<&crate::scoring::ScoringWeights>,
        driver_threshold_percentile: u8,
    ) -> Self {
        self.snapshot.compute_activity_risk(weights);
        self.snapshot.compute_percentiles();
        self.snapshot
            .populate_driver_labels(driver_threshold_percentile);
        self.snapshot.compute_quadrants(driver_threshold_percentile);
        self.snapshot.compute_summary();
        self
    }

    /// Consume the enricher and return the fully enriched snapshot.
    pub fn build(self) -> Snapshot {
        self.snapshot
    }
}

impl Index {
    /// Create a new empty index (default compaction level 0 - full snapshots)
    pub fn new() -> Self {
        Index {
            schema_version: INDEX_SCHEMA_VERSION,
            compaction_level: Some(0), // Default: full snapshots
            commits: Vec::new(),
        }
    }

    /// Get compaction level (defaults to 0 if not set for backward compatibility)
    pub fn compaction_level(&self) -> u32 {
        self.compaction_level.unwrap_or(0)
    }

    /// Set compaction level
    pub fn set_compaction_level(&mut self, level: u32) {
        self.compaction_level = Some(level);
    }

    /// Load index from JSON file, or create new if file doesn't exist
    pub fn load_or_new(path: &Path) -> Result<Self> {
        if path.exists() {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read index file: {}", path.display()))?;
            Self::from_json(&json)
        } else {
            Ok(Self::new())
        }
    }

    /// Deserialize index from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let index: Index =
            serde_json::from_str(json).context("failed to deserialize index from JSON")?;

        // Validate schema version
        if index.schema_version != INDEX_SCHEMA_VERSION {
            anyhow::bail!(
                "index schema version mismatch: expected {}, got {}",
                INDEX_SCHEMA_VERSION,
                index.schema_version
            );
        }

        Ok(index)
    }

    /// Serialize index to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize index to JSON")
    }

    /// Add or update a commit entry in the index
    ///
    /// If the commit already exists, this is idempotent (no-op).
    /// Entries are kept sorted deterministically by timestamp, then SHA.
    pub fn add_commit(&mut self, entry: IndexEntry) {
        // Check if entry already exists
        if !self.commits.iter().any(|c| c.sha == entry.sha) {
            self.commits.push(entry);

            // Sort deterministically: timestamp ascending, then SHA ASCII ascending
            self.commits.sort_by(|a, b| {
                a.timestamp
                    .cmp(&b.timestamp)
                    .then_with(|| a.sha.cmp(&b.sha))
            });
        }
    }

    /// Remove a commit entry by SHA
    pub fn remove_commit(&mut self, sha: &str) {
        self.commits.retain(|c| c.sha != sha);
    }

    /// Check if index contains a commit
    pub fn contains(&self, sha: &str) -> bool {
        self.commits.iter().any(|c| c.sha == sha)
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the path to the `.hotspots` directory in the repository root
pub fn hotspots_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".hotspots")
}

/// Get the path to the snapshots directory
pub fn snapshots_dir(repo_root: &Path) -> PathBuf {
    hotspots_dir(repo_root).join("snapshots")
}

/// Get the path to the index file
pub fn index_path(repo_root: &Path) -> PathBuf {
    hotspots_dir(repo_root).join("index.json")
}

/// Get the path to a snapshot file for a given commit SHA
pub fn snapshot_path(repo_root: &Path, commit_sha: &str) -> PathBuf {
    snapshots_dir(repo_root).join(format!("{}.json.zst", commit_sha))
}

/// Return the path of the snapshot file that actually exists on disk,
/// trying `.json.zst` (new) before `.json` (legacy).  Returns `None` if
/// neither exists.
pub fn snapshot_path_existing(repo_root: &Path, commit_sha: &str) -> Option<PathBuf> {
    let zst = snapshot_path(repo_root, commit_sha);
    if zst.exists() {
        return Some(zst);
    }
    let json = snapshots_dir(repo_root).join(format!("{}.json", commit_sha));
    if json.exists() {
        return Some(json);
    }
    None
}

/// Load a snapshot for the given commit SHA from disk.
///
/// Handles both compressed (`.json.zst`) and legacy plain (`.json`) formats.
/// Returns `None` if no snapshot file exists for the SHA.
pub fn load_snapshot(repo_root: &Path, commit_sha: &str) -> Result<Option<Snapshot>> {
    let path = match snapshot_path_existing(repo_root, commit_sha) {
        Some(p) => p,
        None => return Ok(None),
    };

    let snapshot = read_snapshot_file(&path)?;
    Ok(Some(snapshot))
}

/// Read and parse a snapshot from an arbitrary path, auto-detecting compression.
fn read_snapshot_file(path: &Path) -> Result<Snapshot> {
    let is_compressed = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".json.zst"))
        .unwrap_or(false);

    let json: String = if is_compressed {
        let compressed = std::fs::read(path)
            .with_context(|| format!("failed to read snapshot: {}", path.display()))?;
        let bytes = zstd::decode_all(compressed.as_slice())
            .with_context(|| format!("failed to decompress snapshot: {}", path.display()))?;
        String::from_utf8(bytes).context("snapshot contains invalid UTF-8")?
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read snapshot: {}", path.display()))?
    };

    Snapshot::from_json(&json)
        .with_context(|| format!("failed to parse snapshot: {}", path.display()))
}

/// Write data to file atomically using temp file + rename
pub fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    use std::fs;
    use std::io::Write;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Create temp file in same directory
    let temp_path = path.with_extension("tmp");

    // Write to temp file
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("failed to create temp file: {}", temp_path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write to temp file: {}", temp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temp file: {}", temp_path.display()))?;
    drop(file);

    // Atomic rename
    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to rename temp file to: {}", path.display()))?;

    Ok(())
}

/// Write binary data to file atomically using temp file + rename
pub fn atomic_write_bytes(path: &Path, contents: &[u8]) -> Result<()> {
    use std::fs;
    use std::io::Write;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let temp_path = path.with_extension("tmp");

    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("failed to create temp file: {}", temp_path.display()))?;
    file.write_all(contents)
        .with_context(|| format!("failed to write to temp file: {}", temp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temp file: {}", temp_path.display()))?;
    drop(file);

    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to rename temp file to: {}", path.display()))?;

    Ok(())
}

/// Persist a snapshot to disk
///
/// # Atomic Writes
///
/// Uses temp file + rename pattern for atomic writes.
/// Persists a snapshot to disk.
///
/// When `force` is false, never overwrites an existing snapshot (fails if one already exists
/// and differs). When `force` is true, overwrites any existing snapshot.
///
/// # Errors
///
/// Returns error if:
/// - `force` is false and snapshot file already exists with different content
/// - Schema version mismatch (if reading existing file)
/// - I/O errors during write
pub fn persist_snapshot(repo_root: &Path, snapshot: &Snapshot, force: bool) -> Result<()> {
    let snapshot_path = snapshot_path(repo_root, snapshot.commit_sha());

    // Normalize through a parse-reserialize cycle to produce a canonical form.
    // This handles float serialization quirks where serde_json may parse a float
    // string to a slightly different f64 than what was computed (e.g. a 1-ULP
    // difference due to the float parser's rounding). Both the on-disk snapshot
    // (already round-tripped once) and the freshly-computed snapshot are brought
    // to the same canonical representation before comparing.
    let canonical_json = Snapshot::from_json(&snapshot.to_json()?)
        .context("failed to normalize snapshot for canonical form")?
        .to_json()?;

    if !force {
        if let Some(existing) = load_snapshot(repo_root, snapshot.commit_sha())? {
            // Compare canonical forms (both normalized through one parse-reserialize cycle)
            if existing.to_json()? == canonical_json {
                return Ok(());
            }
            anyhow::bail!(
                "snapshot already exists and differs: {} (snapshots are immutable; use --force to overwrite)",
                snapshot_path.display()
            );
        }
    }

    // Compress and write atomically (zstd level 3 — fast with good ratio)
    let compressed =
        zstd::encode_all(canonical_json.as_bytes(), 3).context("failed to compress snapshot")?;
    atomic_write_bytes(&snapshot_path, &compressed)
        .with_context(|| format!("failed to persist snapshot: {}", snapshot_path.display()))?;

    Ok(())
}

/// Append snapshot entry to index
///
/// Loads existing index, adds entry, and persists atomically.
pub fn append_to_index(repo_root: &Path, snapshot: &Snapshot) -> Result<()> {
    let index_path = index_path(repo_root);

    // Load existing index or create new
    let mut index = Index::load_or_new(&index_path)?;

    // Add commit entry
    index.add_commit(IndexEntry {
        sha: snapshot.commit.sha.clone(),
        parents: snapshot.commit.parents.clone(),
        timestamp: snapshot.commit.timestamp,
    });

    // Serialize and write atomically
    let json = index.to_json()?;
    atomic_write(&index_path, &json)
        .with_context(|| format!("failed to update index: {}", index_path.display()))?;

    Ok(())
}

/// Rebuild index from snapshots directory
///
/// Scans `.hotspots/snapshots/` and rebuilds `index.json` with deterministic ordering.
/// Useful for recovery if index is corrupted or missing.
///
/// # Ordering
///
/// Index entries are sorted by timestamp (ascending), then SHA (ASCII ascending),
/// ensuring byte-for-byte deterministic output.
pub fn rebuild_index(repo_root: &Path) -> Result<Index> {
    let snapshots_dir = snapshots_dir(repo_root);

    if !snapshots_dir.exists() {
        return Ok(Index::new());
    }

    let mut index = Index::new();

    // Read all snapshot files
    let entries = std::fs::read_dir(&snapshots_dir).with_context(|| {
        format!(
            "failed to read snapshots directory: {}",
            snapshots_dir.display()
        )
    })?;

    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();

        // Only process snapshot files (.json.zst or legacy .json)
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !file_name.ends_with(".json.zst") && !file_name.ends_with(".json") {
            continue;
        }

        // Read and parse snapshot (auto-detects compression)
        let snapshot = match read_snapshot_file(&path) {
            Ok(s) => s,
            Err(e) => {
                // Log error but continue (some snapshots may be corrupted)
                eprintln!(
                    "Warning: failed to parse snapshot {}: {}",
                    path.display(),
                    e
                );
                continue;
            }
        };

        // Add to index
        index.add_commit(IndexEntry {
            sha: snapshot.commit.sha,
            parents: snapshot.commit.parents,
            timestamp: snapshot.commit.timestamp,
        });
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::MetricsReport;

    fn create_test_snapshot() -> Snapshot {
        let git_context = GitContext {
            head_sha: "abc123".to_string(),
            parent_shas: vec!["def456".to_string()],
            timestamp: 1705600000,
            branch: Some("main".to_string()),
            is_detached: false,
            message: Some("test commit".to_string()),
            author: Some("Test Author".to_string()),
            is_fix_commit: Some(false),
            is_revert_commit: Some(false),
            ticket_ids: vec![],
        };

        let report = FunctionRiskReport {
            file: "src/foo.ts".to_string(),
            function: "handler".to_string(),
            line: 42,
            language: "TypeScript".to_string(),
            metrics: MetricsReport {
                cc: 5,
                nd: 2,
                fo: 3,
                ns: 1,
                loc: 10,
            },
            risk: RiskReport {
                r_cc: 2.0,
                r_nd: 1.0,
                r_fo: 1.0,
                r_ns: 1.0,
            },
            lrs: 4.8,
            band: "moderate".to_string(),
            suppression_reason: None,
            callees: vec![],
        };

        Snapshot::new(git_context, vec![report])
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = create_test_snapshot();

        // Serialize
        let json = snapshot.to_json().expect("should serialize");
        assert!(json.contains("\"schema_version\": 2"));
        assert!(json.contains("\"sha\": \"abc123\""));
        assert!(json.contains("\"function_id\""));

        // Deserialize
        let deserialized = Snapshot::from_json(&json).expect("should deserialize");
        assert_eq!(deserialized.commit.sha, snapshot.commit.sha);
        assert_eq!(deserialized.functions.len(), snapshot.functions.len());
    }

    #[test]
    fn test_function_id_format() {
        let snapshot = create_test_snapshot();
        assert_eq!(snapshot.functions[0].function_id, "src/foo.ts::handler");
    }

    #[test]
    fn test_snapshot_enricher_with_churn() {
        use crate::git::FileChurn;
        let snapshot = create_test_snapshot();
        let mut churn_map = std::collections::HashMap::new();
        churn_map.insert(
            "src/foo.ts".to_string(),
            FileChurn {
                file: "src/foo.ts".to_string(),
                lines_added: 10,
                lines_deleted: 5,
            },
        );
        let snapshot = SnapshotEnricher::new(snapshot)
            .with_churn(&churn_map)
            .build();
        let churn = snapshot.functions[0]
            .churn
            .as_ref()
            .expect("churn should be set");
        assert_eq!(churn.lines_added, 10);
        assert_eq!(churn.lines_deleted, 5);
        assert_eq!(churn.net_change, 5);
    }

    #[test]
    fn test_snapshot_enricher_enrich_computes_summary() {
        let snapshot = create_test_snapshot();
        let snapshot = SnapshotEnricher::new(snapshot).enrich(None, 75).build();
        let summary = snapshot.summary.as_ref().expect("summary should be set");
        assert_eq!(summary.total_functions, 1);
    }

    #[test]
    fn test_snapshot_enricher_enrich_computes_percentiles() {
        let snapshot = create_test_snapshot();
        let snapshot = SnapshotEnricher::new(snapshot).enrich(None, 75).build();
        assert!(snapshot.functions[0].percentile.is_some());
    }

    #[test]
    fn test_snapshot_enricher_build_passthrough() {
        let snapshot = create_test_snapshot();
        let built = SnapshotEnricher::new(snapshot.clone()).build();
        assert_eq!(
            built.functions[0].function_id,
            snapshot.functions[0].function_id
        );
        assert_eq!(built.commit.sha, snapshot.commit.sha);
    }

    #[test]
    fn test_index_ordering() {
        let mut index = Index::new();

        index.add_commit(IndexEntry {
            sha: "zzz".to_string(),
            parents: vec![],
            timestamp: 2000,
        });

        index.add_commit(IndexEntry {
            sha: "aaa".to_string(),
            parents: vec![],
            timestamp: 1000,
        });

        index.add_commit(IndexEntry {
            sha: "mmm".to_string(),
            parents: vec![],
            timestamp: 2000,
        });

        // Should be sorted by timestamp, then SHA
        assert_eq!(index.commits[0].sha, "aaa");
        assert_eq!(index.commits[1].sha, "mmm");
        assert_eq!(index.commits[2].sha, "zzz");
    }
}
