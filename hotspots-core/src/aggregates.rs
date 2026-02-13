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

/// Snapshot aggregates container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SnapshotAggregates {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileAggregates>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<DirectoryAggregates>,
}

/// Delta aggregates for a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct FileDeltaAggregates {
    pub file: String,
    pub net_lrs_delta: f64,
    pub regression_count: usize,
}

/// Delta aggregates container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DeltaAggregates {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileDeltaAggregates>,
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

/// Compute snapshot aggregates
///
/// # Arguments
///
/// * `snapshot` - Snapshot to compute aggregates for
/// * `repo_root` - Repository root path for normalizing directory paths
pub fn compute_snapshot_aggregates(
    snapshot: &Snapshot,
    repo_root: &std::path::Path,
) -> SnapshotAggregates {
    let files = compute_file_aggregates(&snapshot.functions);
    let directories = compute_directory_aggregates(&files, repo_root);

    SnapshotAggregates { files, directories }
}

/// Compute delta aggregates from delta entries
pub fn compute_delta_aggregates(delta: &Delta) -> DeltaAggregates {
    let mut file_data: HashMap<String, (f64, usize)> = HashMap::new();

    for entry in &delta.deltas {
        // Extract file path from function_id (format: "path/to/file.ts::function")
        let file = if let Some(sep_pos) = entry.function_id.rfind("::") {
            entry.function_id[..sep_pos].to_string()
        } else {
            continue; // Skip malformed function_id
        };

        let file_entry = file_data.entry(file).or_insert((0.0, 0));

        // Compute net LRS delta
        if let Some(delta_val) = &entry.delta {
            file_entry.0 += delta_val.lrs;

            // Count regressions (delta.lrs > 0)
            if delta_val.lrs > 0.0 {
                file_entry.1 += 1;
            }
        } else {
            // For New/Deleted functions, compute delta from before/after states
            match entry.status {
                crate::delta::FunctionStatus::New => {
                    if let Some(after) = &entry.after {
                        file_entry.0 += after.lrs;
                    }
                }
                crate::delta::FunctionStatus::Deleted => {
                    if let Some(before) = &entry.before {
                        file_entry.0 -= before.lrs;
                    }
                }
                _ => {}
            }
        }
    }

    let mut aggregates: Vec<FileDeltaAggregates> = file_data
        .into_iter()
        .map(
            |(file, (net_lrs_delta, regression_count))| FileDeltaAggregates {
                file,
                net_lrs_delta,
                regression_count,
            },
        )
        .collect();

    // Sort deterministically by file path
    aggregates.sort_by(|a, b| a.file.cmp(&b.file));

    DeltaAggregates { files: aggregates }
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
