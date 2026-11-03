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
use crate::language::Language;
use crate::report::{FunctionRiskReport, MetricsReport};
use crate::risk::RiskBand;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::report::RiskReport;

/// Schema version for snapshots.
/// v1: LRS + basic metrics only
/// v2: adds LOC, git churn/touch, call graph, activity risk, percentiles, summary
/// v5 (hotspots 2.0): merges the formerly-separate "agent-optimized" v4 envelope
/// (`triage`/`architecture`/`co_change`) into this struct as optional sections,
/// so there is one schema_version and one Rust type for every `analyze --mode
/// snapshot` JSON shape instead of two independently-versioned structs that
/// shared no serde-level relationship. v3/v4 never existed on this type (v4 was
/// the now-merged agent envelope's own counter) — the jump from 2 to 5 reflects
/// that merge, not three skipped revisions of this struct.
/// v6 (hotspots 3.0): removes `driver`, `patterns`, `percentile`, the call graph
/// metrics other than `fan_in`, `churn.net_change`, `summary.call_graph` and the
/// PR `pr_risk_score`/`size_band`; makes `file` and `function_id` repo-relative
/// (they were absolute paths); replaces `quadrant` with `tier`; renames
/// `risk_factors.activity` to `touch`; and moves `activity_risk` to the fixed-scale
/// formula. Older snapshots are not read: re-run `analyze --mode snapshot`.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 6;
const SNAPSHOT_SCHEMA_MIN_VERSION: u32 = 6;

/// Version of the scoring formula (`activity_risk` constants, `LrsWeights`,
/// `RiskThresholds`). Bumped whenever a default weight or threshold changes, or
/// whenever a scoring input's *computation* changes even if its name/shape does not
/// (e.g. 1->2: `churn`'s source switched from a single-commit diff to a 180-day
/// trailing window; 2->3: `activity_risk` moved to the fixed-scale formula) — so
/// `hotspots diff`/`trends` can distinguish a score delta caused
/// by a tool upgrade from one caused by a real code change.
pub const FORMULA_VERSION: u32 = 3;

fn default_formula_version() -> u32 {
    FORMULA_VERSION
}

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
    /// Version of the scoring formula that produced this snapshot's scores.
    /// See `FORMULA_VERSION`.
    #[serde(default = "default_formula_version")]
    pub formula_version: u32,
    /// Window, in days, that per-function `touch_count` covers. hotspots 2.0
    /// renamed `touch_count_30d` to `touch_count` plus this top-level field,
    /// since the window has actually been `crate::git::TOUCH_WINDOW_DAYS`
    /// (365 days) since before the old name was ever accurate.
    #[serde(default = "default_touch_window_days")]
    pub touch_window_days: u32,
}

fn default_touch_window_days() -> u32 {
    crate::git::TOUCH_WINDOW_DAYS as u32
}

/// Churn metrics for a file/function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChurnMetrics {
    pub lines_added: usize,
    pub lines_deleted: usize,
}

/// Call graph metrics for a function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CallGraphMetrics {
    pub fan_in: usize,
}

/// Function entry in snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FunctionSnapshot {
    pub function_id: String,
    pub file: String,
    pub line: u32,
    pub language: Language,
    pub metrics: MetricsReport,
    pub lrs: f64,
    pub band: RiskBand,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn: Option<ChurnMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_since_last_change: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callgraph: Option<CallGraphMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_risk: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_factors: Option<crate::scoring::RiskFactors>,
    /// Activity tier: "active" when the function's file has more touches than the
    /// repo median or changed within the last 30 days, otherwise "quiet".
    /// Says nothing about complexity; `band` carries that separately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Nearest manifest-root ancestor relative to repo root (e.g. "packages/next",
    /// "turbopack/crates/turbopack-ecmascript"). Empty string when the file belongs
    /// to the repo root itself. None when no repo_root was provided at snapshot time.
    ///
    /// Detected from: package.json, Cargo.toml, pyproject.toml, go.mod,
    /// setup.py, setup.cfg, Gemfile, build.gradle, pom.xml, mix.exs.
    /// Populated in `Snapshot::from_reports` when `repo_root` is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<String>,
    /// Number of distinct commit authors for this file in the last 90 days.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_authors_90d()`.
    /// Used as a training feature in `hotspots train` — author diversity is a
    /// non-windowed ownership signal that doesn't share the temporal leakage
    /// of touch_count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors_90d: Option<u32>,
    /// Directed coupling score for this file.
    /// dc(i) = Σ_j [co_changes(i,j) × hotspots_score(j)] / commit_count(i)
    /// Weights co-change relationships by partner defect density.
    /// Variant (full-history or 365d) selected by the Jaccard screener.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_directed_coupling()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directed_coupling: Option<f64>,
    /// Count of commits touching this file whose message matches fix-keyword conventions.
    /// Full-history count (no time window) — avoids temporal leakage when used as a
    /// training feature with a post-cutoff label window.
    /// File-level (shared by all functions in the same file).
    /// Populated only by `Snapshot::populate_convention_bug_fix_count()`, which is called
    /// solely inside `trainer::train()`'s own internal snapshot clone for feature
    /// extraction (`hotspots train`) — never during `analyze`/`--mode snapshot`. This
    /// field is therefore already always `None` (and so already absent, via
    /// `skip_serializing_if` below) in every snapshot `analyze` actually outputs; the
    /// hotspots 2.0 Decide-table item "move behind `--explain`, remove from default
    /// output" needs no code change here — that's already the real behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convention_bug_fix_count: Option<u32>,
    /// Sliding 30-day-window max/mean commit ratio for this file.
    /// Always at least 1.0; higher values indicate a burst of frantic commit activity.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_burst_score()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_score: Option<f64>,
    /// Total commits touching this file across full history.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_count: Option<u32>,
    /// Distinct commit-author emails touching this file across full history.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_count: Option<u32>,
    /// Shannon entropy of the commit-author distribution for this file.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_entropy: Option<f64>,
    /// Fraction of this file's commits where it was the only file touched.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation_rate: Option<f64>,
    /// Days between this file's first and last commit.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_days: Option<f64>,
    /// Days since this file's most recent commit.
    /// File-level (shared by all functions in the same file).
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_touch_days: Option<f64>,
    /// Fraction of this file's commits in the 90-day window before the snapshot
    /// cutoff authored by someone whose first-ever commit to the repo also falls
    /// in that window.
    /// File-level (shared by all functions in the same file).
    /// `None` when the file has no commits in the 90-day window (excluded, not
    /// zero-filled) — distinct from `Some(0.0)`, a file with window commits and
    /// no newcomers.
    /// Populated by `Snapshot::populate_history_signals()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newcomer_rate: Option<f64>,
    /// Human-readable explanation phrase derived from feature percentiles within this repo.
    /// Populated by the `--explain` path after the trained ranker is applied.
    /// None unless `--explain` was passed and a trained ranker is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// Risk distribution by band
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BandStats {
    pub count: usize,
    pub sum_risk: f64,
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
}

/// Complete snapshot for a commit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Snapshot {
    #[serde(rename = "schema_version")]
    pub schema_version: u32,
    pub commit: CommitInfo,
    pub analysis: AnalysisInfo,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub functions: Vec<FunctionSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SnapshotSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<crate::aggregates::SnapshotAggregates>,
    /// Functions grouped by activity tier (active/quiet), top-N per
    /// tier. Populated only for the slimmed default `analyze --mode
    /// snapshot --format json` output (i.e. without `--all-functions`), in
    /// which case `functions` above is empty and this carries the agent-sized
    /// view instead. Mutually exclusive with a populated `functions`/`aggregates`
    /// in practice, though nothing enforces that at the type level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage: Option<crate::aggregates::TriageView>,
    /// File/module-level risk rollup for the same slimmed default output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<crate::aggregates::AgentArchitectureView>,
    /// Hidden-coupling co-change summary for the same slimmed default output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_change: Option<crate::aggregates::AgentCoChangeView>,
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

/// Manifest filenames that mark a subsystem (package/crate/module) boundary.
const SUBSYSTEM_MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "setup.py",
    "setup.cfg",
    "Gemfile",
    "build.gradle",
    "pom.xml",
    "mix.exs",
];

/// Directories that are never subsystem roots (generated, vendored, hidden).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    "out",
    "target",
];

/// Build a cache of `absolute_dir → subsystem_rel_path` for every directory
/// in `repo_root` that contains a manifest file. Skips hidden and vendor dirs.
///
/// The cache maps an absolute directory path to its subsystem label (the
/// relative path from `repo_root`, using `/` separators, or `""` for root).
fn build_subsystem_cache(repo_root: &Path) -> HashMap<PathBuf, String> {
    let mut cache: HashMap<PathBuf, String> = HashMap::new();
    collect_manifest_dirs(repo_root, repo_root, &mut cache);
    cache
}

fn collect_manifest_dirs(dir: &Path, repo_root: &Path, cache: &mut HashMap<PathBuf, String>) {
    // Check if this directory contains any manifest file
    for manifest in SUBSYSTEM_MANIFESTS {
        if dir.join(manifest).exists() {
            let rel = dir
                .strip_prefix(repo_root)
                .unwrap_or(dir)
                .to_string_lossy()
                .replace('\\', "/");
            cache.entry(dir.to_path_buf()).or_insert(rel);
            break;
        }
    }

    // Recurse into subdirectories, skipping hidden and vendor dirs
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || SKIP_DIRS.contains(&name_str.as_ref()) {
            continue;
        }
        collect_manifest_dirs(&entry.path(), repo_root, cache);
    }
}

/// Given the absolute path of a source file and a pre-built subsystem cache,
/// return the label of the deepest manifest-containing ancestor directory.
///
/// Returns `Some("")` when the file belongs to the repo root itself (a
/// manifest exists at `repo_root` but nowhere deeper on this path), and
/// `None` when no manifest ancestor exists at all.
fn subsystem_for_file(
    abs_file: &Path,
    repo_root: &Path,
    cache: &HashMap<PathBuf, String>,
) -> Option<String> {
    let mut best: Option<&str> = None;
    let mut best_depth = 0usize;

    let mut dir = abs_file.parent()?;
    loop {
        if let Some(label) = cache.get(dir) {
            let depth = dir.components().count();
            if depth >= best_depth {
                best = Some(label.as_str());
                best_depth = depth;
            }
        }
        match dir.parent() {
            Some(p) if p != dir && dir.starts_with(repo_root) => dir = p,
            _ => break,
        }
    }

    best.map(|s| s.to_string())
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
                    churn: None,       // Churn will be populated separately if available
                    touch_count: None, // Touch count will be populated separately if available
                    days_since_last_change: None, // Days since last change will be populated separately if available
                    callgraph: None, // Call graph metrics will be populated separately if available
                    activity_risk: None,
                    risk_factors: None,
                    tier: None,
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
                formula_version: FORMULA_VERSION,
                touch_window_days: 365,
            },
            functions,
            summary: None,
            aggregates: None, // Aggregates are computed on-demand, not stored
            triage: None,
            architecture: None,
            co_change: None,
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
                function.churn = Some(ChurnMetrics {
                    lines_added: file_churn.lines_added,
                    lines_deleted: file_churn.lines_deleted,
                });
            }
        }
    }

    /// Rewrite every function's `file` and `function_id` to be relative to `repo_root`
    /// (forward slashes, no leading `./`), so snapshots and deltas do not depend on where
    /// the repo happens to be checked out. Call this after anything that looks functions up
    /// by their analysis-time (absolute) path, such as the call graph.
    pub fn relativize_paths(&mut self, repo_root: &Path) {
        let (prefix_can, prefix_raw) = crate::trainer::repo_prefixes(repo_root);
        for function in &mut self.functions {
            let rel = crate::trainer::make_rel(&function.file, &prefix_can, &prefix_raw);
            if rel == function.file {
                continue;
            }
            if let Some((_, symbol)) = function.function_id.split_once("::") {
                function.function_id = format!("{rel}::{symbol}");
            }
            function.file = rel;
        }
    }

    /// Populate `authors_90d` for every function.
    ///
    /// Runs one `git log` subprocess per unique file in the snapshot to count
    /// distinct commit-author emails in the last 90 days.  File-level: all
    /// functions in the same file receive the same value.  Functions in files
    /// with no matching history receive `Some(0)`.
    ///
    /// Errors are soft: a failed git call leaves the affected file's functions
    /// with `authors_90d = None`.
    pub fn populate_authors_90d(&mut self, repo_root: &Path) {
        use std::collections::{HashMap, HashSet};
        use std::process::Command;

        // Build unique-file → function-indices map
        let mut file_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, func) in self.functions.iter().enumerate() {
            file_indices.entry(func.file.clone()).or_default().push(i);
        }

        for (abs_path, indices) in &file_indices {
            let rel = if let Ok(r) = std::path::Path::new(abs_path).strip_prefix(repo_root) {
                r.to_string_lossy().to_string()
            } else {
                abs_path.clone()
            };

            let out = Command::new("git")
                .args(["log", "--after=90.days.ago", "--format=%ae", "--", &rel])
                .current_dir(repo_root)
                .output();

            let count = match out {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout);
                    text.lines()
                        .filter(|l| !l.is_empty())
                        .collect::<HashSet<_>>()
                        .len() as u32
                }
                _ => continue, // leave as None on error
            };

            for &i in indices {
                self.functions[i].authors_90d = Some(count);
            }
        }
    }

    /// Populate `convention_bug_fix_count` for every function.
    ///
    /// Counts full-history commits per file whose message matches fix-keyword conventions
    /// (same regex as `detect_fix_commit`). File-level: all functions in the same file
    /// receive the same value. Functions in files with no history receive `Some(0)`.
    ///
    /// Errors are soft: a failed git call leaves the affected file's functions with
    /// `convention_bug_fix_count = None`.
    pub fn populate_convention_bug_fix_count(&mut self, repo_root: &Path) {
        use std::collections::HashMap;
        use std::process::Command;

        let mut file_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, func) in self.functions.iter().enumerate() {
            file_indices.entry(func.file.clone()).or_default().push(i);
        }

        for (abs_path, indices) in &file_indices {
            let rel = if let Ok(r) = std::path::Path::new(abs_path).strip_prefix(repo_root) {
                r.to_string_lossy().to_string()
            } else {
                abs_path.clone()
            };

            let out = Command::new("git")
                .args(["log", "--format=%s", "--", &rel])
                .current_dir(repo_root)
                .output();

            let count = match out {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout);
                    text.lines()
                        .filter(|l| crate::git::detect_fix_commit(l))
                        .count() as u32
                }
                _ => continue,
            };

            for &i in indices {
                self.functions[i].convention_bug_fix_count = Some(count);
            }
        }
    }

    /// Populate `burst_score` for every function.
    ///
    /// Runs a single `git log --name-only` pass over the whole repository via
    /// `history_signals::load_commits_with_author` (the same loader
    /// `populate_history_signals` uses) and reads `burst_score` off the
    /// resulting per-file `HistorySignals` — no separate `git log` subprocess.
    /// File-level: all functions in the same file receive the same value.
    /// Files with fewer than 2 commits receive `Some(1.0)` (no burst signal).
    ///
    /// Errors are soft: if the git call fails, all functions keep `burst_score = None`.
    pub fn populate_burst_score(&mut self, repo_root: &Path) {
        use std::collections::HashMap;

        let git_dir = crate::coupling::git_dir(repo_root);
        let commits = crate::history_signals::load_commits_with_author(&git_dir);
        if commits.is_empty() {
            return;
        }
        let signals = crate::history_signals::compute_history_signals(&commits);

        let mut file_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, func) in self.functions.iter().enumerate() {
            let rel = if let Ok(r) = std::path::Path::new(&func.file).strip_prefix(repo_root) {
                r.to_string_lossy().to_string()
            } else {
                func.file.clone()
            };
            file_indices.entry(rel).or_default().push(i);
        }

        for (rel, indices) in &file_indices {
            let Some(s) = signals.get(rel) else {
                continue;
            };
            for &i in indices {
                self.functions[i].burst_score = Some(s.burst_score);
            }
        }
    }

    /// Populate `commit_count`, `author_count`, `author_entropy`, `isolation_rate`,
    /// `age_days`, and `last_touch_days` for every function. `burst_score` is not touched here — it is populated separately
    /// by `populate_burst_score` and reused as-is for cold-start features.
    ///
    /// Runs a single `git log --name-only` pass over the whole repository via
    /// `history_signals::load_commits_with_author`, then computes all six signals
    /// per file in one aggregation pass. File-level: all functions in the same file
    /// receive the same values. Files with no matching commit history keep all six
    /// fields as `None`.
    ///
    /// Errors are soft: if the git call fails, all functions keep these fields as `None`.
    pub fn populate_history_signals(&mut self, repo_root: &Path) {
        let git_dir = crate::coupling::git_dir(repo_root);
        let commits = crate::history_signals::load_commits_with_author(&git_dir);
        if commits.is_empty() {
            return;
        }
        let signals = crate::history_signals::compute_history_signals(&commits);

        let mut file_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, func) in self.functions.iter().enumerate() {
            let rel = if let Ok(r) = std::path::Path::new(&func.file).strip_prefix(repo_root) {
                r.to_string_lossy().to_string()
            } else {
                func.file.clone()
            };
            file_indices.entry(rel).or_default().push(i);
        }

        for (rel, indices) in &file_indices {
            let Some(s) = signals.get(rel) else {
                continue;
            };
            for &i in indices {
                self.functions[i].commit_count = Some(s.commit_count);
                self.functions[i].author_count = Some(s.author_count);
                self.functions[i].author_entropy = Some(s.author_entropy);
                self.functions[i].isolation_rate = Some(s.isolation_rate);
                self.functions[i].age_days = Some(s.age_days);
                self.functions[i].last_touch_days = Some(s.last_touch_days);
                self.functions[i].newcomer_rate = s.newcomer_rate;
            }
        }
    }

    /// Populate `directed_coupling` for every function.
    ///
    /// Calls `crate::coupling::compute_directed_coupling_for_repo` which:
    /// 1. Loads full commit history via git log.
    /// 2. Computes Jaccard label stability to select dc_full vs
    ///    dc_365d — purely an internal routing decision; hotspots 2.0 removed
    ///    the `jaccard_label_stability` output field once the stricter "only
    ///    fields proven to work" bar was adopted, since it was an internal
    ///    screener diagnostic, not an independently validated signal. The
    ///    returned `dc_scores` already reflect whichever variant the screener
    ///    picked, so nothing downstream needs the raw Jaccard value itself.
    /// 3. Computes directed coupling scores keyed by file path.
    ///
    /// All functions in the same file receive the same file-level DC score.
    /// Functions in files with no co-change history receive `directed_coupling = None`.
    ///
    /// `partner_scores` is typically `file → hotspots_score` from the snapshot itself
    /// (built before calling this method, or passed in from cheap_signals.csv).
    pub fn populate_directed_coupling(
        &mut self,
        repo_root: &Path,
        partner_scores: &std::collections::HashMap<String, f64>,
    ) {
        let (dc_scores, _jaccard) =
            crate::coupling::compute_directed_coupling_for_repo(repo_root, partner_scores);

        // Build file → function-indices map
        let mut file_indices: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, func) in self.functions.iter().enumerate() {
            // DC scores are keyed by repo-relative path; snapshot paths may be absolute.
            let rel = std::path::Path::new(&func.file)
                .strip_prefix(repo_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| func.file.replace('\\', "/"));
            file_indices.entry(rel).or_default().push(i);
        }

        for (rel_path, indices) in &file_indices {
            let score = dc_scores.get(rel_path).copied();
            for &i in indices {
                self.functions[i].directed_coupling = score;
            }
        }
    }

    /// Detect and populate the `subsystem` field for every function.
    ///
    /// Walks `repo_root` once to find all manifest files, then assigns each
    /// function its nearest manifest-containing ancestor directory (relative to
    /// `repo_root`, using `/` separators). Empty string means the file belongs
    /// to the repo root. Skips hidden and vendored directories.
    pub fn populate_subsystems(&mut self, repo_root: &Path) {
        let cache = build_subsystem_cache(repo_root);
        for function in &mut self.functions {
            // function.file may be absolute (--all-functions) or repo-relative.
            let path = Path::new(&function.file);
            let abs = if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            };
            function.subsystem = subsystem_for_file(&abs, repo_root, &cache);
        }
    }

    /// File-level touch metrics: one batched git call + per-file fallback
    fn populate_file_level_touch_metrics(
        &mut self,
        repo_root: &std::path::Path,
        window_days: u32,
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

        // One batched call for the touch window (replaces N×2 individual calls)
        let batched =
            crate::git::batch_touch_metrics_at(repo_root, self.commit.timestamp, window_days)
                .unwrap_or_else(|_| crate::git::BatchedTouchMetrics {
                    touch_count: HashMap::new(),
                    days_since_last_change: HashMap::new(),
                });

        // Collect files whose last-touch timestamp isn't in the touch window
        // and resolve them in a single streaming git log pass instead of one
        // subprocess per file.
        let stale_files: std::collections::HashSet<&str> = abs_to_rel
            .values()
            .map(|s| s.as_str())
            .filter(|rel| !batched.days_since_last_change.contains_key(*rel))
            .collect();

        let stale_days =
            crate::git::batch_last_touch_for_files(repo_root, &stale_files, self.commit.timestamp);

        // Apply results to all functions in each file
        for (abs_path, function_indices) in &unique_files {
            let rel = abs_to_rel
                .get(abs_path)
                .map(|s| s.as_str())
                .unwrap_or(abs_path);

            let touch_count = batched.touch_count.get(rel).copied().or(Some(0));
            let days_since = batched
                .days_since_last_change
                .get(rel)
                .copied()
                .or_else(|| stale_days.get(rel).copied());

            for &idx in function_indices {
                self.functions[idx].touch_count = touch_count;
                self.functions[idx].days_since_last_change = days_since;
            }
        }

        Ok(())
    }

    /// Populate touch count and recency metrics from git data
    ///
    /// For each file (or function when `per_function` is true), computes:
    /// - touch_count: number of commits in the last `git::TOUCH_WINDOW_DAYS` days (365)
    /// - days_since_last_change: days since last modification
    ///
    pub fn populate_touch_metrics(
        &mut self,
        repo_root: &std::path::Path,
        window_days: u32,
    ) -> anyhow::Result<()> {
        self.populate_file_level_touch_metrics(repo_root, window_days)
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

    /// Populate call graph metrics (fan-in).
    pub fn populate_callgraph(&mut self, call_graph: &crate::callgraph::CallGraph) {
        // Precompute fan-in counts in O(N+E) to avoid O(N*E) repeated fan_in() calls
        let fan_in_map = call_graph.build_fan_in_map();

        for function in &mut self.functions {
            if call_graph.contains(&function.function_id) {
                function.callgraph = Some(CallGraphMetrics {
                    fan_in: fan_in_map.get(&function.function_id).copied().unwrap_or(0),
                });
            }
        }
    }

    /// Compute and populate activity risk scores
    ///
    /// Should be called after populate_churn and populate_touch_metrics.
    pub fn compute_activity_risk(&mut self) {
        for function in &mut self.functions {
            let (activity_risk, risk_factors) =
                crate::scoring::compute_activity_risk(&crate::scoring::ActivityRiskInput {
                    lrs: function.lrs,
                    churn: function
                        .churn
                        .as_ref()
                        .map(|c| (c.lines_added, c.lines_deleted)),
                    touch_count: function.touch_count,
                    days_since_last_change: function.days_since_last_change,
                });
            function.activity_risk = Some(activity_risk);
            function.risk_factors = Some(risk_factors);
        }
    }

    /// Compute and populate the activity tier for all functions.
    ///
    /// `active` = touch_count above the repo median OR last changed within 30 days;
    /// everything else is `quiet`. Activity only: no complexity gate (the research
    /// found the gate adds nothing for predicting where fixes land).
    pub fn compute_tiers(&mut self) {
        if self.functions.is_empty() {
            return;
        }
        let mut touches: Vec<usize> = self
            .functions
            .iter()
            .map(|f| f.touch_count.unwrap_or(0))
            .collect();
        touches.sort_unstable();
        let touch_median = touches[(50 * (touches.len() - 1)) / 100];

        for function in &mut self.functions {
            let touched_often = function.touch_count.is_some_and(|t| t > touch_median);
            let recently_changed = function.days_since_last_change.is_some_and(|d| d <= 30);
            function.tier = Some(
                if touched_often || recently_changed {
                    "active"
                } else {
                    "quiet"
                }
                .to_string(),
            );
        }
    }

    /// Compute repo-level summary statistics
    ///
    /// Must be called after compute_activity_risk() and populate_callgraph().
    ///
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
            });
            return;
        }

        let mut scored: Vec<f64> = self
            .functions
            .iter()
            .map(|f| f.activity_risk.unwrap_or(f.lrs))
            .collect();
        scored.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        let total_risk: f64 = scored.iter().sum();
        let (top_1_pct_share, top_5_pct_share, top_10_pct_share) =
            compute_top_k_shares(&scored, total_risk);

        self.summary = Some(SnapshotSummary {
            total_functions: n,
            total_activity_risk: total_risk,
            top_1_pct_share,
            top_5_pct_share,
            top_10_pct_share,
            by_band: compute_band_distribution(&self.functions),
        });
    }

    /// Serialize snapshot to JSON string (deterministic ordering)
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize snapshot to JSON")
    }

    /// Write the snapshot as pretty-printed JSON directly to `writer` without
    /// building an intermediate `String`.
    ///
    /// Use this instead of `to_json()` + `println!` when outputting large
    /// snapshots — it avoids allocating a potentially hundreds-of-MB string.
    pub fn write_json_to<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        serde_json::to_writer_pretty(&mut *writer, self)
            .context("failed to write snapshot JSON")?;
        writeln!(writer).context("failed to write trailing newline")
    }

    /// Deserialize snapshot from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let snapshot: Snapshot =
            serde_json::from_str(json).context("failed to deserialize snapshot from JSON")?;

        // Validate schema version. Snapshots from earlier schema versions are rejected;
        // regenerate them with `analyze --mode snapshot`.
        if snapshot.schema_version < SNAPSHOT_SCHEMA_MIN_VERSION
            || snapshot.schema_version > SNAPSHOT_SCHEMA_VERSION
        {
            anyhow::bail!(
                "unsupported snapshot schema version {} (this build reads version {}); \
                 regenerate it with `hotspots analyze --mode snapshot --force`",
                snapshot.schema_version,
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

/// Returns (top_1_pct_share, top_5_pct_share, top_10_pct_share) from a
/// descending-sorted score slice and the pre-computed total.
fn compute_top_k_shares(scored: &[f64], total_risk: f64) -> (f64, f64, f64) {
    let n = scored.len();
    let safe_div = |a: f64, b: f64| if b > 0.0 { a / b } else { 0.0 };
    let top1_sum: f64 = scored.iter().take((n / 100).max(1)).sum();
    let top5_sum: f64 = scored.iter().take((n * 5 / 100).max(1)).sum();
    let top10_sum: f64 = scored.iter().take((n / 10).max(1)).sum();
    (
        safe_div(top1_sum, total_risk),
        safe_div(top5_sum, total_risk),
        safe_div(top10_sum, total_risk),
    )
}

/// Builds a band → BandStats map from the function list.
fn compute_band_distribution(
    functions: &[FunctionSnapshot],
) -> std::collections::BTreeMap<String, BandStats> {
    let mut by_band = std::collections::BTreeMap::new();
    for func in functions {
        let score = func.activity_risk.unwrap_or(func.lrs);
        let entry = by_band
            .entry(func.band.as_str().to_string())
            .or_insert(BandStats {
                count: 0,
                sum_risk: 0.0,
            });
        entry.count += 1;
        entry.sum_risk += score;
    }
    by_band
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

    /// Detect and populate the `subsystem` field for every function.
    ///
    /// Walks `repo_root` once to find manifest files (package.json, Cargo.toml,
    /// pyproject.toml, go.mod, etc.) and tags each function with its nearest
    /// manifest-containing ancestor directory relative to `repo_root`.
    /// No-op (returns self unchanged) if `repo_root` does not exist.
    pub fn with_subsystems(mut self, repo_root: &Path) -> Self {
        if repo_root.exists() {
            self.snapshot.populate_subsystems(repo_root);
        }
        self
    }

    /// Make every function's `file` and `function_id` repo-relative. Must come after the
    /// steps that key off the analysis-time paths (call graph, churn, touch).
    pub fn with_relative_paths(mut self, repo_root: &Path) -> Self {
        self.snapshot.relativize_paths(repo_root);
        self
    }

    /// Populate `burst_score` for every function.
    /// No-op if `repo_root` does not exist.
    pub fn with_burst_score(mut self, repo_root: &Path) -> Self {
        if repo_root.exists() {
            self.snapshot.populate_burst_score(repo_root);
        }
        self
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
    /// On error, emits a warning to stderr and continues.
    pub fn with_touch_metrics(mut self, repo_root: &Path, window_days: u32) -> Self {
        if let Err(e) = self.snapshot.populate_touch_metrics(repo_root, window_days) {
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

    /// Populate call graph metrics (fan-in).
    pub fn with_callgraph(mut self, call_graph: &crate::callgraph::CallGraph) -> Self {
        self.snapshot.populate_callgraph(call_graph);
        self
    }

    /// Compute activity risk, percentile flags, driver labels, and summary statistics.
    ///
    /// Must be called after with_churn, with_touch_metrics, and with_callgraph.
    pub fn enrich(mut self) -> Self {
        self.snapshot.compute_activity_risk();
        self.snapshot.compute_tiers();
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

/// Delta encoding schema version
const DELTA_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Delta between two snapshots — stores only what changed.
///
/// Stored as `<sha>.delta.json.zst` in the snapshots directory.
/// Reconstruct the full snapshot by loading the base and calling `apply_delta`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DeltaSnapshot {
    pub schema_version: u32,
    /// Schema version of the full snapshot this delta reconstructs.
    pub snapshot_schema_version: u32,
    pub commit: CommitInfo,
    pub analysis: AnalysisInfo,
    /// SHA of the snapshot used as the delta base.
    pub base_sha: String,
    pub added: Vec<FunctionSnapshot>,
    pub modified: Vec<FunctionSnapshot>,
    pub removed: Vec<String>,
    /// Preserved from the original snapshot so HTML history charts remain intact
    /// after reconstruction (callers filter on `summary.is_some()`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SnapshotSummary>,
}

/// Compute a delta from `base` to `current`.
///
/// The returned `DeltaSnapshot` encodes only the functions that were added,
/// modified, or removed relative to `base`.
pub fn compute_delta(base: &Snapshot, current: &Snapshot) -> DeltaSnapshot {
    use std::collections::HashMap;

    let base_map: HashMap<&str, &FunctionSnapshot> = base
        .functions
        .iter()
        .map(|f| (f.function_id.as_str(), f))
        .collect();
    let current_map: HashMap<&str, ()> = current
        .functions
        .iter()
        .map(|f| (f.function_id.as_str(), ()))
        .collect();

    let added: Vec<FunctionSnapshot> = current
        .functions
        .iter()
        .filter(|f| !base_map.contains_key(f.function_id.as_str()))
        .cloned()
        .collect();

    let modified: Vec<FunctionSnapshot> = current
        .functions
        .iter()
        .filter(|f| {
            base_map
                .get(f.function_id.as_str())
                .map(|bf| *bf != *f)
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let removed: Vec<String> = base
        .functions
        .iter()
        .filter(|f| !current_map.contains_key(f.function_id.as_str()))
        .map(|f| f.function_id.clone())
        .collect();

    DeltaSnapshot {
        schema_version: DELTA_SNAPSHOT_SCHEMA_VERSION,
        snapshot_schema_version: current.schema_version,
        commit: current.commit.clone(),
        analysis: current.analysis.clone(),
        base_sha: base.commit.sha.clone(),
        added,
        modified,
        removed,
        summary: current.summary.clone(),
    }
}

/// Reconstruct a full `Snapshot` by applying a delta on top of `base`.
pub fn apply_delta(base: Snapshot, delta: DeltaSnapshot) -> Snapshot {
    use std::collections::HashMap;

    let mut functions: HashMap<String, FunctionSnapshot> = base
        .functions
        .into_iter()
        .map(|f| (f.function_id.clone(), f))
        .collect();

    for id in &delta.removed {
        functions.remove(id);
    }
    for func in delta.modified {
        functions.insert(func.function_id.clone(), func);
    }
    for func in delta.added {
        functions.insert(func.function_id.clone(), func);
    }

    let mut result: Vec<FunctionSnapshot> = functions.into_values().collect();
    result.sort_by(|a, b| a.function_id.cmp(&b.function_id));

    Snapshot {
        schema_version: delta.snapshot_schema_version,
        commit: delta.commit,
        analysis: delta.analysis,
        functions: result,
        summary: delta.summary,
        aggregates: None,
        triage: None,
        architecture: None,
        co_change: None,
    }
}

/// Persist a delta snapshot to `<sha>.delta.json.zst`.
pub fn persist_delta(repo_root: &Path, delta: &DeltaSnapshot) -> Result<()> {
    let path = delta_snapshot_path(repo_root, &delta.commit.sha);
    let json = serde_json::to_string_pretty(delta).context("failed to serialize delta snapshot")?;
    let compressed =
        zstd::encode_all(json.as_bytes(), 3).context("failed to compress delta snapshot")?;
    atomic_write_bytes(&path, &compressed)
        .with_context(|| format!("failed to persist delta snapshot: {}", path.display()))
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

/// Get the path to a delta snapshot file for a given commit SHA
pub fn delta_snapshot_path(repo_root: &Path, commit_sha: &str) -> PathBuf {
    snapshots_dir(repo_root).join(format!("{}.delta.json.zst", commit_sha))
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
/// Handles full snapshots (`.json.zst`, `.json`), delta snapshots
/// (`.delta.json.zst`), and transparent reconstruction of delta chains.
/// Returns `None` if no snapshot or delta file exists for the SHA.
pub fn load_snapshot(repo_root: &Path, commit_sha: &str) -> Result<Option<Snapshot>> {
    // Full snapshot takes priority.
    if let Some(path) = snapshot_path_existing(repo_root, commit_sha) {
        return Ok(Some(read_snapshot_file(&path)?));
    }

    // Fall back to delta reconstruction.
    let dpath = delta_snapshot_path(repo_root, commit_sha);
    if dpath.exists() {
        let compressed = std::fs::read(&dpath)
            .with_context(|| format!("failed to read delta: {}", dpath.display()))?;
        let bytes = zstd::decode_all(compressed.as_slice())
            .with_context(|| format!("failed to decompress delta: {}", dpath.display()))?;
        let json = String::from_utf8(bytes).context("delta snapshot contains invalid UTF-8")?;
        let delta: DeltaSnapshot = serde_json::from_str(&json)
            .with_context(|| format!("failed to parse delta: {}", dpath.display()))?;
        let base = load_snapshot(repo_root, &delta.base_sha)?.ok_or_else(|| {
            anyhow::anyhow!(
                "base snapshot {} not found for delta {}",
                delta.base_sha,
                commit_sha
            )
        })?;
        return Ok(Some(apply_delta(base, delta)));
    }

    Ok(None)
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
            language: Language::TypeScript,
            metrics: MetricsReport {
                cc: 5,
                nd: 2,
                fo: 3,
                ns: 1,
                loc: 10,
                missing_docstring: false,
            },
            risk: RiskReport {
                r_cc: 2.0,
                r_nd: 1.0,
                r_fo: 1.0,
                r_ns: 1.0,
            },
            lrs: 4.8,
            band: RiskBand::Moderate,
            suppression_reason: None,
            callees: vec![],
            explanation: None,
        };

        Snapshot::new(git_context, vec![report])
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = create_test_snapshot();

        // Serialize
        let json = snapshot.to_json().expect("should serialize");
        assert!(json.contains(&format!("\"schema_version\": {SNAPSHOT_SCHEMA_VERSION}")));
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
    }

    #[test]
    fn test_snapshot_enricher_enrich_computes_summary() {
        let snapshot = create_test_snapshot();
        let snapshot = SnapshotEnricher::new(snapshot).enrich().build();
        let summary = snapshot.summary.as_ref().expect("summary should be set");
        assert_eq!(summary.total_functions, 1);
    }

    #[test]
    fn test_snapshot_enricher_enrich_computes_activity_risk_and_tier() {
        let snapshot = create_test_snapshot();
        let snapshot = SnapshotEnricher::new(snapshot).enrich().build();
        assert!(snapshot.functions[0].activity_risk.is_some());
        assert!(snapshot.functions[0].tier.is_some());
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

    // Cache key for the test snapshot's single function:
    //   sha="abc123", file="src/foo.ts", line=42, loc=10 → end=51

    #[test]
    fn test_relativize_paths_makes_file_and_function_id_repo_relative() {
        let mut snapshot = create_test_snapshot();
        for (file, expected) in [
            ("/repo/src/foo.ts", "src/foo.ts"),
            ("/repo/./src/foo.ts", "src/foo.ts"),
            ("src/foo.ts", "src/foo.ts"),
        ] {
            snapshot.functions[0].file = file.to_string();
            snapshot.functions[0].function_id = format!("{file}::foo");
            snapshot.relativize_paths(std::path::Path::new("/repo"));
            assert_eq!(snapshot.functions[0].file, expected);
            assert_eq!(
                snapshot.functions[0].function_id,
                format!("{expected}::foo")
            );
        }
    }
}
