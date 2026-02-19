//! Aggregation views - expose architectural concentration
//!
//! Computes derived aggregates from snapshots and deltas without modifying core data.
//!
//! Global invariants enforced:
//! - Aggregates are strictly derived (never stored, always computed)
//! - Deterministic ordering
//! - No modification of existing function data

use crate::delta::Delta;
use crate::snapshot::{FunctionSnapshot, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// File-level aggregates for a snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FileAggregates {
    pub file: String,
    pub sum_lrs: f64,
    pub max_lrs: f64,
    pub high_plus_count: usize,
}

/// Directory-level aggregates (recursive rollup)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryAggregates {
    pub directory: String,
    pub sum_lrs: f64,
    pub max_lrs: f64,
    pub high_plus_count: usize,
}

/// File-level risk view
///
/// Richer than `FileAggregates` — includes CC, LOC, function density, and a composite
/// file_risk_score derived from:
///   max_cc × 0.4 + avg_cc × 0.3 + log2(function_count + 1) × 0.2 + churn_factor × 0.1
/// where churn_factor = (file_churn / 100).min(10.0)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FileRiskView {
    pub file: String,
    pub function_count: usize,
    pub loc: usize,
    pub max_cc: usize,
    pub avg_cc: f64,
    pub critical_count: usize,
    pub file_churn: u64,
    pub file_risk_score: f64,
}

/// Module (directory) instability metric (Robert Martin's Ca/Ce)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ModuleInstability {
    /// Directory path relative to repo root
    pub module: String,
    pub file_count: usize,
    pub function_count: usize,
    pub avg_complexity: f64,
    /// Afferent coupling: external modules that depend on this one
    pub afferent: usize,
    /// Efferent coupling: modules this one depends on externally
    pub efferent: usize,
    /// instability = efferent / (afferent + efferent); 0.5 if both == 0 (undefined)
    pub instability: f64,
    /// "high" if instability < 0.3 and avg_complexity > 10, else "low"
    pub module_risk: String,
}

/// Snapshot aggregates container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SnapshotAggregates {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileAggregates>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<DirectoryAggregates>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_risk: Vec<FileRiskView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub co_change: Vec<crate::git::CoChangePair>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ModuleInstability>,
}

/// Delta aggregates for a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FileDeltaAggregates {
    pub file: String,
    pub net_lrs_delta: f64,
    pub regression_count: usize,
    pub improvement_count: usize,
}

/// A co-change pair entry in the delta diff
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CoChangeDeltaEntry {
    pub file_a: String,
    pub file_b: String,
    /// "new" | "dropped" | "risk_increased" | "risk_decreased"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_risk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curr_risk: Option<String>,
    pub co_change_count: usize,
    pub coupling_ratio: f64,
    pub has_static_dep: bool,
}

/// Delta aggregates container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DeltaAggregates {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileDeltaAggregates>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub co_change_delta: Vec<CoChangeDeltaEntry>,
}

/// Check if a band is High+ (high or critical)
fn is_high_plus(band: &str) -> bool {
    band == "high" || band == "critical"
}

/// Extract directory path from file path
fn extract_directory(file_path: &str) -> String {
    if let Some(last_slash) = file_path.rfind('/') {
        file_path[..last_slash].to_string()
    } else {
        ".".to_string()
    }
}

/// Normalize file path relative to repo root
/// Returns None if path is outside repo root
fn normalize_path_relative_to_repo(file_path: &str, repo_root: &std::path::Path) -> Option<String> {
    let file_path_buf = std::path::PathBuf::from(file_path);

    // Try to make path relative to repo root
    if let Ok(relative) = file_path_buf.strip_prefix(repo_root) {
        // Convert to string with forward slashes
        Some(relative.to_string_lossy().replace('\\', "/"))
    } else {
        // If path doesn't start with repo_root, check if it's already relative
        if !file_path_buf.is_absolute() {
            Some(file_path.to_string())
        } else {
            // Absolute path outside repo - return None to filter it out
            None
        }
    }
}

/// Compute file aggregates from snapshot functions
pub fn compute_file_aggregates(functions: &[FunctionSnapshot]) -> Vec<FileAggregates> {
    let mut file_data: HashMap<String, (f64, f64, usize)> = HashMap::new();

    for func in functions {
        let entry = file_data.entry(func.file.clone()).or_insert((0.0, 0.0, 0));

        // Sum LRS
        entry.0 += func.lrs;

        // Max LRS
        if func.lrs > entry.1 {
            entry.1 = func.lrs;
        }

        // Count High+ functions
        if is_high_plus(&func.band) {
            entry.2 += 1;
        }
    }

    let mut aggregates: Vec<FileAggregates> = file_data
        .into_iter()
        .map(
            |(file, (sum_lrs, max_lrs, high_plus_count))| FileAggregates {
                file,
                sum_lrs,
                max_lrs,
                high_plus_count,
            },
        )
        .collect();

    // Sort deterministically by file path
    aggregates.sort_by(|a, b| a.file.cmp(&b.file));

    aggregates
}

/// Compute directory aggregates from file aggregates (recursive rollup)
///
/// # Arguments
///
/// * `file_aggregates` - File aggregates to roll up
/// * `repo_root` - Repository root path for normalizing absolute paths
pub fn compute_directory_aggregates(
    file_aggregates: &[FileAggregates],
    repo_root: &std::path::Path,
) -> Vec<DirectoryAggregates> {
    let mut dir_data: HashMap<String, (f64, f64, usize)> = HashMap::new();

    for file_agg in file_aggregates {
        // Normalize path relative to repo root
        let normalized_file = match normalize_path_relative_to_repo(&file_agg.file, repo_root) {
            Some(path) => path,
            None => continue, // Skip files outside repo root
        };

        // Recursive rollup: aggregate to all parent directories
        let mut current_path = normalized_file.clone();
        loop {
            let dir = extract_directory(&current_path);
            if dir == current_path || dir.is_empty() {
                // Reached root or empty, stop
                break;
            }

            // Skip if directory is outside repo (starts with / or contains ..)
            if dir.starts_with('/') && !dir.starts_with("./") {
                break;
            }

            let entry = dir_data.entry(dir.clone()).or_insert((0.0, 0.0, 0));

            // Sum LRS
            entry.0 += file_agg.sum_lrs;

            // Max LRS (max across all files in directory)
            if file_agg.max_lrs > entry.1 {
                entry.1 = file_agg.max_lrs;
            }

            // Sum High+ counts
            entry.2 += file_agg.high_plus_count;

            current_path = dir;
        }
    }

    let mut aggregates: Vec<DirectoryAggregates> = dir_data
        .into_iter()
        .map(
            |(directory, (sum_lrs, max_lrs, high_plus_count))| DirectoryAggregates {
                directory,
                sum_lrs,
                max_lrs,
                high_plus_count,
            },
        )
        .collect();

    // Sort deterministically by directory path
    aggregates.sort_by(|a, b| a.directory.cmp(&b.directory));

    aggregates
}

/// Compute file risk views from snapshot functions
///
/// Ranked descending by `file_risk_score`. Score formula:
///   max_cc × 0.4 + avg_cc × 0.3 + log2(function_count + 1) × 0.2 + churn_factor × 0.1
pub fn compute_file_risk_views(functions: &[FunctionSnapshot]) -> Vec<FileRiskView> {
    // Accumulate (sum_cc, max_cc, count, critical_count, loc, file_churn) per file
    let mut file_data: HashMap<String, (usize, usize, usize, usize, usize, u64)> = HashMap::new();
    for func in functions {
        let e = file_data
            .entry(func.file.clone())
            .or_insert((0, 0, 0, 0, 0, 0));
        e.0 += func.metrics.cc;
        e.1 = e.1.max(func.metrics.cc);
        e.2 += 1;
        if func.band == "critical" {
            e.3 += 1;
        }
        e.4 += func.metrics.loc;
        if let Some(churn) = &func.churn {
            let lines = (churn.lines_added + churn.lines_deleted) as u64;
            e.5 = e.5.max(lines);
        }
    }

    let mut views: Vec<FileRiskView> = file_data
        .into_iter()
        .map(
            |(file, (sum_cc, max_cc, function_count, critical_count, loc, file_churn))| {
                let avg_cc = if function_count > 0 {
                    sum_cc as f64 / function_count as f64
                } else {
                    0.0
                };
                let churn_factor = (file_churn as f64 / 100.0).min(10.0);
                let score = max_cc as f64 * 0.4
                    + avg_cc * 0.3
                    + (function_count as f64 + 1.0).log2() * 0.2
                    + churn_factor * 0.1;
                FileRiskView {
                    file,
                    function_count,
                    loc,
                    max_cc,
                    avg_cc: (avg_cc * 100.0).round() / 100.0,
                    critical_count,
                    file_churn,
                    file_risk_score: (score * 100.0).round() / 100.0,
                }
            },
        )
        .collect();

    views.sort_by(|a, b| {
        b.file_risk_score
            .partial_cmp(&a.file_risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.file.cmp(&b.file))
    });

    views
}

/// Annotate co-change pairs with `has_static_dep` from the import edge set.
///
/// For each pair, checks whether a direct import exists in either direction.
/// Pairs with a static dependency are reclassified as `"expected"`.
pub fn annotate_static_deps(
    pairs: &mut [crate::git::CoChangePair],
    edges: &[(String, String)],
    repo_root: &std::path::Path,
) {
    // Build a set of normalized (relative) edge pairs, both directions
    let mut edge_set: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for (a, b) in edges {
        let a_rel = normalize_path_relative_to_repo(a, repo_root).unwrap_or_else(|| a.clone());
        let b_rel = normalize_path_relative_to_repo(b, repo_root).unwrap_or_else(|| b.clone());
        edge_set.insert((a_rel.clone(), b_rel.clone()));
        edge_set.insert((b_rel, a_rel));
    }

    for pair in pairs.iter_mut() {
        let has_dep = edge_set.contains(&(pair.file_a.clone(), pair.file_b.clone()));
        pair.has_static_dep = has_dep;
        if has_dep {
            pair.risk = "expected".to_string();
        }
    }
}

/// Compute module (directory) instability from a pre-computed import edge list.
fn compute_module_instability_from_edges(
    functions: &[FunctionSnapshot],
    edges: &[(String, String)],
    repo_root: &std::path::Path,
) -> Vec<ModuleInstability> {
    // Helper: extract directory from a file path (relative to repo_root)
    let file_dir = |file: &str| -> Option<String> {
        let normalized = normalize_path_relative_to_repo(file, repo_root)?;
        Some(extract_directory(&normalized))
    };

    // Count cross-directory edges
    let mut efferent: HashMap<String, usize> = HashMap::new();
    let mut afferent: HashMap<String, usize> = HashMap::new();

    for (from_file, to_file) in edges {
        let from_dir = match file_dir(from_file) {
            Some(d) => d,
            None => continue,
        };
        let to_dir = match file_dir(to_file) {
            Some(d) => d,
            None => continue,
        };
        if from_dir != to_dir {
            *efferent.entry(from_dir.clone()).or_insert(0) += 1;
            *afferent.entry(to_dir.clone()).or_insert(0) += 1;
        }
    }

    // Aggregate function metrics per directory
    struct DirStats {
        files: std::collections::HashSet<String>,
        function_count: usize,
        sum_cc: usize,
    }
    let mut dir_stats: HashMap<String, DirStats> = HashMap::new();

    for func in functions {
        let dir = match file_dir(&func.file) {
            Some(d) => d,
            None => continue,
        };
        let stats = dir_stats.entry(dir).or_insert_with(|| DirStats {
            files: std::collections::HashSet::new(),
            function_count: 0,
            sum_cc: 0,
        });
        stats.files.insert(func.file.clone());
        stats.function_count += 1;
        stats.sum_cc += func.metrics.cc;
    }

    // Collect all directory names seen in any of the three maps
    let all_dirs: std::collections::HashSet<String> = dir_stats
        .keys()
        .chain(efferent.keys())
        .chain(afferent.keys())
        .cloned()
        .collect();

    let mut modules: Vec<ModuleInstability> = all_dirs
        .into_iter()
        .filter_map(|dir| {
            let stats = dir_stats.get(&dir)?;
            let eff = *efferent.get(&dir).unwrap_or(&0);
            let aff = *afferent.get(&dir).unwrap_or(&0);
            let instability = if eff + aff == 0 {
                0.5 // undefined — treat as neutral
            } else {
                eff as f64 / (eff + aff) as f64
            };
            let avg_complexity = if stats.function_count > 0 {
                stats.sum_cc as f64 / stats.function_count as f64
            } else {
                0.0
            };
            let module_risk = if instability < 0.3 && avg_complexity > 10.0 {
                "high".to_string()
            } else {
                "low".to_string()
            };
            Some(ModuleInstability {
                module: dir,
                file_count: stats.files.len(),
                function_count: stats.function_count,
                avg_complexity: (avg_complexity * 100.0).round() / 100.0,
                afferent: aff,
                efferent: eff,
                instability: (instability * 1000.0).round() / 1000.0,
                module_risk,
            })
        })
        .collect();

    // Sort: high-risk first, then by instability ascending (most stable / highest-risk first)
    modules.sort_by(|a, b| {
        b.module_risk
            .cmp(&a.module_risk) // "high" > "low"
            .then(
                a.instability
                    .partial_cmp(&b.instability)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.module.cmp(&b.module))
    });

    modules
}

/// Compute module instability from snapshot functions (computes import edges internally).
///
/// Exposed as a public API for callers that don't have pre-computed edges.
pub fn compute_module_instability(
    functions: &[FunctionSnapshot],
    repo_root: &std::path::Path,
) -> Vec<ModuleInstability> {
    let mut unique_files: Vec<String> = functions
        .iter()
        .map(|f| f.file.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    unique_files.sort();
    let files_as_str: Vec<&str> = unique_files.iter().map(|s| s.as_str()).collect();
    let edges = crate::imports::resolve_file_deps(&files_as_str, repo_root);
    compute_module_instability_from_edges(functions, &edges, repo_root)
}

/// Compute snapshot aggregates
///
/// # Arguments
///
/// * `snapshot` - Snapshot to compute aggregates for
/// * `repo_root` - Repository root path for normalizing directory paths
pub fn compute_snapshot_aggregates(
    snapshot: &Snapshot,
    repo_root: &std::path::Path,
    co_change_window_days: u64,
    co_change_min_count: usize,
) -> SnapshotAggregates {
    let files = compute_file_aggregates(&snapshot.functions);
    let directories = compute_directory_aggregates(&files, repo_root);
    let file_risk = compute_file_risk_views(&snapshot.functions);

    // Compute import edges once — shared by module instability and co-change annotation
    let mut unique_files: Vec<String> = snapshot
        .functions
        .iter()
        .map(|f| f.file.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    unique_files.sort();
    let files_as_str: Vec<&str> = unique_files.iter().map(|s| s.as_str()).collect();
    let import_edges = crate::imports::resolve_file_deps(&files_as_str, repo_root);

    let mut co_change =
        crate::git::extract_co_change_pairs(repo_root, co_change_window_days, co_change_min_count)
            .unwrap_or_default();
    annotate_static_deps(&mut co_change, &import_edges, repo_root);

    let modules =
        compute_module_instability_from_edges(&snapshot.functions, &import_edges, repo_root);

    SnapshotAggregates {
        files,
        directories,
        file_risk,
        co_change,
        modules,
    }
}

/// Numeric rank for risk strings (higher = worse).
fn risk_rank(risk: &str) -> u8 {
    match risk {
        "critical" => 4,
        "high" => 3,
        "moderate" => 2,
        "low" => 1,
        _ => 0,
    }
}

/// Diff two co-change pair lists and return change entries.
///
/// Pairs are keyed by `(min(file_a, file_b), max(file_a, file_b))` so ordering
/// within a pair does not affect matching.
pub fn diff_co_change_pairs(
    prev: &[crate::git::CoChangePair],
    curr: &[crate::git::CoChangePair],
) -> Vec<CoChangeDeltaEntry> {
    // Normalise key: always (smaller, larger) for consistent lookup
    let normalize = |a: &str, b: &str| -> (String, String) {
        if a <= b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        }
    };

    let prev_map: HashMap<(String, String), &crate::git::CoChangePair> = prev
        .iter()
        .map(|p| (normalize(&p.file_a, &p.file_b), p))
        .collect();
    let curr_map: HashMap<(String, String), &crate::git::CoChangePair> = curr
        .iter()
        .map(|p| (normalize(&p.file_a, &p.file_b), p))
        .collect();

    let mut result: Vec<CoChangeDeltaEntry> = Vec::new();

    // New pairs and risk changes
    for pair in curr {
        let key = normalize(&pair.file_a, &pair.file_b);
        let status = if let Some(prev_pair) = prev_map.get(&key) {
            let pr = risk_rank(&prev_pair.risk);
            let cr = risk_rank(&pair.risk);
            if cr > pr {
                "risk_increased"
            } else if cr < pr {
                "risk_decreased"
            } else {
                continue; // unchanged — skip
            }
        } else {
            "new"
        };
        let prev_risk = prev_map.get(&key).map(|p| p.risk.clone());
        result.push(CoChangeDeltaEntry {
            file_a: pair.file_a.clone(),
            file_b: pair.file_b.clone(),
            status: status.to_string(),
            prev_risk,
            curr_risk: Some(pair.risk.clone()),
            co_change_count: pair.co_change_count,
            coupling_ratio: pair.coupling_ratio,
            has_static_dep: pair.has_static_dep,
        });
    }

    // Dropped pairs
    for pair in prev {
        let key = normalize(&pair.file_a, &pair.file_b);
        if !curr_map.contains_key(&key) {
            result.push(CoChangeDeltaEntry {
                file_a: pair.file_a.clone(),
                file_b: pair.file_b.clone(),
                status: "dropped".to_string(),
                prev_risk: Some(pair.risk.clone()),
                curr_risk: None,
                co_change_count: pair.co_change_count,
                coupling_ratio: pair.coupling_ratio,
                has_static_dep: pair.has_static_dep,
            });
        }
    }

    // Sort: dropped last, then by risk rank desc, then alphabetically
    result.sort_by(|a, b| {
        let a_dropped = a.status == "dropped";
        let b_dropped = b.status == "dropped";
        a_dropped
            .cmp(&b_dropped)
            .then_with(|| {
                let ar = a.curr_risk.as_deref().map(risk_rank).unwrap_or(0);
                let br = b.curr_risk.as_deref().map(risk_rank).unwrap_or(0);
                br.cmp(&ar)
            })
            .then(a.file_a.cmp(&b.file_a))
            .then(a.file_b.cmp(&b.file_b))
    });

    result
}

/// Compute delta aggregates from delta entries
///
/// Sorted by `net_lrs_delta` descending (worst regressions first).
/// Ties broken by file path for determinism.
pub fn compute_delta_aggregates(
    delta: &Delta,
    current_co_change: &[crate::git::CoChangePair],
    prev_co_change: &[crate::git::CoChangePair],
) -> DeltaAggregates {
    // (net_lrs_delta, regression_count, improvement_count)
    let mut file_data: HashMap<String, (f64, usize, usize)> = HashMap::new();

    for entry in &delta.deltas {
        // Extract file path from function_id (format: "path/to/file.ts::function")
        let file = if let Some(sep_pos) = entry.function_id.rfind("::") {
            entry.function_id[..sep_pos].to_string()
        } else {
            continue; // Skip malformed function_id
        };

        let e = file_data.entry(file).or_insert((0.0, 0, 0));

        if let Some(delta_val) = &entry.delta {
            e.0 += delta_val.lrs;
            if delta_val.lrs > 0.0 {
                e.1 += 1; // regression
            } else if delta_val.lrs < 0.0 {
                e.2 += 1; // improvement
            }
        } else {
            match entry.status {
                crate::delta::FunctionStatus::New => {
                    if let Some(after) = &entry.after {
                        e.0 += after.lrs;
                    }
                }
                crate::delta::FunctionStatus::Deleted => {
                    if let Some(before) = &entry.before {
                        e.0 -= before.lrs;
                        e.2 += 1; // deleted function = improvement
                    }
                }
                _ => {}
            }
        }
    }

    let mut aggregates: Vec<FileDeltaAggregates> = file_data
        .into_iter()
        .map(
            |(file, (net_lrs_delta, regression_count, improvement_count))| FileDeltaAggregates {
                file,
                net_lrs_delta,
                regression_count,
                improvement_count,
            },
        )
        .collect();

    // Sort by net_lrs_delta descending (worst regressions first), then file path
    aggregates.sort_by(|a, b| {
        b.net_lrs_delta
            .partial_cmp(&a.net_lrs_delta)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.file.cmp(&b.file))
    });

    let co_change_delta = diff_co_change_pairs(prev_co_change, current_co_change);

    DeltaAggregates {
        files: aggregates,
        co_change_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::MetricsReport;
    use crate::snapshot::FunctionSnapshot;

    fn create_test_function(file: &str, function: &str, lrs: f64, band: &str) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: format!("{}::{}", file, function),
            file: file.to_string(),
            line: 1,
            language: "TypeScript".to_string(),
            metrics: MetricsReport {
                cc: 1,
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 10,
            },
            lrs,
            band: band.to_string(),
            suppression_reason: None,
            churn: None,
            touch_count_30d: None,
            days_since_last_change: None,
            callgraph: None,
            activity_risk: None,
            risk_factors: None,
            percentile: None,
            driver: None,
            driver_detail: None,
        }
    }

    #[test]
    fn test_file_aggregates() {
        let functions = vec![
            create_test_function("src/foo.ts", "func1", 5.0, "moderate"),
            create_test_function("src/foo.ts", "func2", 8.0, "high"),
            create_test_function("src/bar.ts", "func3", 3.0, "low"),
        ];

        let aggregates = compute_file_aggregates(&functions);
        assert_eq!(aggregates.len(), 2);

        let foo_agg = aggregates.iter().find(|a| a.file == "src/foo.ts").unwrap();
        assert_eq!(foo_agg.sum_lrs, 13.0);
        assert_eq!(foo_agg.max_lrs, 8.0);
        assert_eq!(foo_agg.high_plus_count, 1);

        let bar_agg = aggregates.iter().find(|a| a.file == "src/bar.ts").unwrap();
        assert_eq!(bar_agg.sum_lrs, 3.0);
        assert_eq!(bar_agg.max_lrs, 3.0);
        assert_eq!(bar_agg.high_plus_count, 0);
    }

    #[test]
    fn test_directory_aggregates() {
        let file_aggregates = vec![
            FileAggregates {
                file: "src/api/handler.ts".to_string(),
                sum_lrs: 10.0,
                max_lrs: 8.0,
                high_plus_count: 1,
            },
            FileAggregates {
                file: "src/api/router.ts".to_string(),
                sum_lrs: 5.0,
                max_lrs: 5.0,
                high_plus_count: 0,
            },
            FileAggregates {
                file: "src/utils.ts".to_string(),
                sum_lrs: 3.0,
                max_lrs: 3.0,
                high_plus_count: 0,
            },
        ];

        // Use a dummy repo root for testing (relative paths)
        let repo_root = std::path::Path::new("/test/repo");
        let dir_aggregates = compute_directory_aggregates(&file_aggregates, repo_root);

        let api_dir = dir_aggregates
            .iter()
            .find(|a| a.directory == "src/api")
            .unwrap();
        assert_eq!(api_dir.sum_lrs, 15.0);
        assert_eq!(api_dir.max_lrs, 8.0);
        assert_eq!(api_dir.high_plus_count, 1);

        // Recursive rollup: "src" directory should include both "src/api" and "src/utils"
        let src_dir = dir_aggregates
            .iter()
            .find(|a| a.directory == "src")
            .unwrap();
        assert_eq!(src_dir.sum_lrs, 18.0); // 15.0 (api) + 3.0 (utils)
        assert_eq!(src_dir.max_lrs, 8.0);
        assert_eq!(src_dir.high_plus_count, 1);
    }

    #[test]
    fn test_is_high_plus() {
        assert!(is_high_plus("high"));
        assert!(is_high_plus("critical"));
        assert!(!is_high_plus("moderate"));
        assert!(!is_high_plus("low"));
    }
}
