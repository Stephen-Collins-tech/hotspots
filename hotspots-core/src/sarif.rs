//! SARIF 2.1.0 output for GitHub code scanning integration
//!
//! Emits results for functions at moderate risk or above.
//! Maps risk bands to SARIF levels:
//!   critical → error
//!   high     → warning
//!   moderate → note

use crate::snapshot::Snapshot;
use serde::Serialize;
use std::path::Path;

const SARIF_SCHEMA: &str =
    "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";

// Rule IDs
const RULE_CRITICAL: &str = "hotspots/critical-risk";
const RULE_HIGH: &str = "hotspots/high-risk";
const RULE_MODERATE: &str = "hotspots/moderate-risk";

#[derive(Serialize)]
struct SarifOutput {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun>,
}

#[derive(Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
struct SarifDriver {
    name: &'static str,
    version: String,
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
    rules: Vec<SarifRule>,
}

#[derive(Serialize)]
struct SarifRule {
    id: &'static str,
    name: &'static str,
    #[serde(rename = "shortDescription")]
    short_description: SarifMessage,
    #[serde(rename = "fullDescription")]
    full_description: SarifMessage,
    #[serde(rename = "defaultConfiguration")]
    default_configuration: SarifRuleConfig,
    #[serde(rename = "helpUri")]
    help_uri: &'static str,
}

#[derive(Serialize)]
struct SarifRuleConfig {
    level: &'static str,
}

#[derive(Serialize)]
struct SarifResult {
    #[serde(rename = "ruleId")]
    rule_id: &'static str,
    level: &'static str,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifact,
    region: SarifRegion,
}

#[derive(Serialize)]
struct SarifArtifact {
    uri: String,
    #[serde(rename = "uriBaseId")]
    uri_base_id: &'static str,
}

#[derive(Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: u32,
}

fn rules() -> Vec<SarifRule> {
    vec![
        SarifRule {
            id: RULE_CRITICAL,
            name: "CriticalRiskFunction",
            short_description: SarifMessage {
                text: "Critical-risk function detected".to_string(),
            },
            full_description: SarifMessage {
                text: "This function has a critical Logical Risk Score (LRS). It combines high cyclomatic complexity with heavy git churn and/or many contributors, making it a likely bug source.".to_string(),
            },
            default_configuration: SarifRuleConfig { level: "error" },
            help_uri: "https://hotspots.dev",
        },
        SarifRule {
            id: RULE_HIGH,
            name: "HighRiskFunction",
            short_description: SarifMessage {
                text: "High-risk function detected".to_string(),
            },
            full_description: SarifMessage {
                text: "This function has a high Logical Risk Score (LRS). It has elevated cyclomatic complexity combined with significant git activity.".to_string(),
            },
            default_configuration: SarifRuleConfig { level: "warning" },
            help_uri: "https://hotspots.dev",
        },
        SarifRule {
            id: RULE_MODERATE,
            name: "ModerateRiskFunction",
            short_description: SarifMessage {
                text: "Moderate-risk function detected".to_string(),
            },
            full_description: SarifMessage {
                text: "This function has a moderate Logical Risk Score (LRS). Consider reviewing for refactoring opportunities.".to_string(),
            },
            default_configuration: SarifRuleConfig { level: "note" },
            help_uri: "https://hotspots.dev",
        },
    ]
}

/// Strip `repo_root` from an absolute file path to produce a repo-relative URI.
/// Falls back to the original path if stripping fails (e.g. path is already relative).
fn to_relative_uri(file: &str, repo_root: &Path) -> String {
    let path = Path::new(file);
    // Normalize away any `.` components before stripping
    let stripped = path
        .strip_prefix(repo_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| file.replace('\\', "/"));
    // Remove any leading "./" that may remain
    stripped.trim_start_matches("./").to_string()
}

/// Render a snapshot as SARIF 2.1.0 JSON.
///
/// Only functions at moderate risk or above are emitted.
/// `repo_root` is used to convert absolute file paths to repo-relative URIs,
/// which is required for GitHub code scanning to resolve locations correctly.
pub fn render_sarif(snapshot: &Snapshot, repo_root: &Path) -> String {
    let tool_version = snapshot.analysis.tool_version.clone();

    let results: Vec<SarifResult> = snapshot
        .functions
        .iter()
        .filter_map(|f| {
            let (rule_id, level) = match f.band.as_str() {
                "critical" => (RULE_CRITICAL, "error"),
                "high" => (RULE_HIGH, "warning"),
                "moderate" => (RULE_MODERATE, "note"),
                _ => return None,
            };

            let name = f.function_id.rsplit("::").next().unwrap_or("<anonymous>");
            let lrs = f.lrs;
            let cc = f.metrics.cc;

            Some(SarifResult {
                rule_id,
                level,
                message: SarifMessage {
                    text: format!(
                        "Function `{name}` has a {band} risk score (LRS={lrs:.2}, CC={cc}).",
                        band = f.band.as_str(),
                    ),
                },
                locations: vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifact {
                            uri: to_relative_uri(&f.file, repo_root),
                            uri_base_id: "%SRCROOT%",
                        },
                        region: SarifRegion {
                            start_line: f.line.max(1),
                        },
                    },
                }],
            })
        })
        .collect();

    let output = SarifOutput {
        schema: SARIF_SCHEMA,
        version: SARIF_VERSION,
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "hotspots",
                    version: tool_version,
                    information_uri: "https://hotspots.dev",
                    rules: rules(),
                },
            },
            results,
        }],
    };

    serde_json::to_string_pretty(&output).expect("SARIF serialization is infallible")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::MetricsReport;
    use crate::snapshot::{AnalysisInfo, CommitInfo, FunctionSnapshot, Snapshot};

    fn make_snapshot(functions: Vec<FunctionSnapshot>) -> Snapshot {
        Snapshot {
            schema_version: 2,
            commit: CommitInfo {
                sha: "abc123".to_string(),
                parents: vec![],
                timestamp: 0,
                branch: None,
                message: None,
                author: None,
                is_fix_commit: None,
                is_revert_commit: None,
                ticket_ids: vec![],
            },
            analysis: AnalysisInfo {
                scope: ".".to_string(),
                tool_version: "1.0.0".to_string(),
            },
            functions,
            summary: None,
            aggregates: None,
        }
    }

    fn make_function(file: &str, name: &str, band: &str, lrs: f64, cc: u32) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: format!("{}::{}", file, name),
            file: file.to_string(),
            line: 10,
            language: crate::language::Language::Rust,
            metrics: MetricsReport {
                cc,
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 10,
            },
            lrs,
            band: crate::risk::RiskBand::parse(band).unwrap_or(crate::risk::RiskBand::Low),
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
            quadrant: None,
            patterns: vec![],
            pattern_details: None,
        }
    }

    #[test]
    fn test_sarif_schema_and_version() {
        let snapshot = make_snapshot(vec![]);
        let json = render_sarif(&snapshot, Path::new("/repo"));
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["version"], "2.1.0");
        assert!(val["$schema"]
            .as_str()
            .unwrap()
            .contains("sarif-schema-2.1.0"));
    }

    #[test]
    fn test_sarif_low_risk_functions_omitted() {
        let snapshot = make_snapshot(vec![
            make_function("/repo/src/lib.rs", "low_fn", "low", 1.0, 2),
            make_function("/repo/src/lib.rs", "moderate_fn", "moderate", 4.0, 5),
        ]);
        let json = render_sarif(&snapshot, Path::new("/repo"));
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = &val["runs"][0]["results"];
        assert_eq!(results.as_array().unwrap().len(), 1);
        assert_eq!(results[0]["ruleId"], "hotspots/moderate-risk");
    }

    #[test]
    fn test_sarif_band_to_level_mapping() {
        let snapshot = make_snapshot(vec![
            make_function("/repo/a.rs", "critical_fn", "critical", 10.0, 15),
            make_function("/repo/b.rs", "high_fn", "high", 7.0, 10),
            make_function("/repo/c.rs", "moderate_fn", "moderate", 4.0, 5),
        ]);
        let json = render_sarif(&snapshot, Path::new("/repo"));
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = val["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        let levels: Vec<&str> = results
            .iter()
            .map(|r| r["level"].as_str().unwrap())
            .collect();
        assert!(levels.contains(&"error"));
        assert!(levels.contains(&"warning"));
        assert!(levels.contains(&"note"));
    }

    #[test]
    fn test_sarif_path_made_relative() {
        let snapshot = make_snapshot(vec![make_function(
            "/repo/src/main.rs",
            "my_fn",
            "high",
            7.0,
            10,
        )]);
        let json = render_sarif(&snapshot, Path::new("/repo"));
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let uri = val["runs"][0]["results"][0]["locations"][0]["physicalLocation"]
            ["artifactLocation"]["uri"]
            .as_str()
            .unwrap();
        assert_eq!(uri, "src/main.rs");
    }

    #[test]
    fn test_sarif_rules_present() {
        let snapshot = make_snapshot(vec![]);
        let json = render_sarif(&snapshot, Path::new("/repo"));
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let rules = val["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap();
        assert_eq!(rules.len(), 3);
        let ids: Vec<&str> = rules.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"hotspots/critical-risk"));
        assert!(ids.contains(&"hotspots/high-risk"));
        assert!(ids.contains(&"hotspots/moderate-risk"));
    }
}
