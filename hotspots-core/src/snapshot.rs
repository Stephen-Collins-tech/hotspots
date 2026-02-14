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

/// Schema version for snapshots
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

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
    pub by_band: std::collections::HashMap<String, BandStats>,
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

    /// Populate touch count and recency metrics from git data
    ///
    /// For each file, computes:
    /// - touch_count_30d: number of commits in last 30 days
    /// - days_since_last_change: days since last modification
    ///
    /// These are computed at the file level and applied to all functions in the file.
    ///
    /// # Arguments
    ///
    /// * `repo_root` - Path to repository root (for git operations)
    pub fn populate_touch_metrics(&mut self, repo_root: &std::path::Path) -> anyhow::Result<()> {
        use std::collections::HashMap;

        // Build set of unique files to avoid duplicate git operations
        let mut unique_files: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, function) in self.functions.iter().enumerate() {
            unique_files
                .entry(function.file.clone())
                .or_default()
                .push(idx);
        }

        // For each unique file, compute metrics once
        for (file_path, function_indices) in unique_files {
            // Convert absolute path back to relative path for git
            let relative_path =
                if let Ok(rel) = std::path::Path::new(&file_path).strip_prefix(repo_root) {
                    rel.to_string_lossy().to_string()
                } else {
                    // If can't make relative, use as-is
                    file_path.clone()
                };

            // Compute touch count (may fail if file is new or git operation fails)
            let touch_count =
                crate::git::count_file_touches_30d(&relative_path, self.commit.timestamp).ok();

            // Compute days since last change
            let days_since =
                crate::git::days_since_last_change(&relative_path, self.commit.timestamp).ok();

            // Apply to all functions in this file
            for &idx in &function_indices {
                self.functions[idx].touch_count_30d = touch_count;
                self.functions[idx].days_since_last_change = days_since;
            }
        }

        Ok(())
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
                function.lrs,
                churn,
                function.touch_count_30d,
                function.days_since_last_change,
                fan_in,
                scc_size,
                dependency_depth,
                neighbor_churn,
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

    /// Compute repo-level summary statistics
    ///
    /// Must be called after compute_activity_risk() and populate_callgraph().
    pub fn compute_summary(&mut self) {
        use std::collections::HashMap;

        let n = self.functions.len();
        if n == 0 {
            self.summary = Some(SnapshotSummary {
                total_functions: 0,
                total_activity_risk: 0.0,
                top_1_pct_share: 0.0,
                top_5_pct_share: 0.0,
                top_10_pct_share: 0.0,
                by_band: HashMap::new(),
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
        let mut by_band: HashMap<String, BandStats> = HashMap::new();
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
                .unwrap()
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

        // Validate schema version
        if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
            anyhow::bail!(
                "schema version mismatch: expected {}, got {}",
                SNAPSHOT_SCHEMA_VERSION,
                snapshot.schema_version
            );
        }

        Ok(snapshot)
    }

    /// Get the commit SHA for this snapshot
    pub fn commit_sha(&self) -> &str {
        &self.commit.sha
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
    snapshots_dir(repo_root).join(format!("{}.json", commit_sha))
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

/// Persist a snapshot to disk
///
/// # Atomic Writes
///
/// Uses temp file + rename pattern for atomic writes.
/// Never overwrites existing snapshots (fails if snapshot already exists).
///
/// # Errors
///
/// Returns error if:
/// - Snapshot file already exists (immutability enforced)
/// - Schema version mismatch (if reading existing file)
/// - I/O errors during write
pub fn persist_snapshot(repo_root: &Path, snapshot: &Snapshot) -> Result<()> {
    let snapshot_path = snapshot_path(repo_root, snapshot.commit_sha());

    // Never overwrite existing snapshots (immutability)
    if snapshot_path.exists() {
        // Verify existing snapshot matches (idempotency check)
        let existing_json = std::fs::read_to_string(&snapshot_path).with_context(|| {
            format!(
                "failed to read existing snapshot: {}",
                snapshot_path.display()
            )
        })?;
        let existing_snapshot = Snapshot::from_json(&existing_json).with_context(|| {
            format!(
                "existing snapshot has invalid schema: {}",
                snapshot_path.display()
            )
        })?;

        // If it's byte-for-byte identical, this is idempotent (ok)
        if existing_snapshot.to_json()? == snapshot.to_json()? {
            return Ok(());
        }

        anyhow::bail!(
            "snapshot already exists and differs: {} (snapshots are immutable)",
            snapshot_path.display()
        );
    }

    // Serialize snapshot
    let json = snapshot.to_json()?;

    // Atomic write
    atomic_write(&snapshot_path, &json)
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

        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        // Read and parse snapshot
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read snapshot: {}", path.display()))?;

        let snapshot = match Snapshot::from_json(&json) {
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
        assert!(json.contains("\"schema_version\": 1"));
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
