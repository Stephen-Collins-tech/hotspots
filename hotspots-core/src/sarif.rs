//! SARIF 2.1.0 output for GitHub code scanning integration
//!
//! Emits results for functions at moderate risk or above.
//! Maps risk bands to SARIF levels:
//!   critical → error
//!   high     → warning
//!   moderate → note

use crate::snapshot::Snapshot;
use serde::Serialize;

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

/// Render a snapshot as SARIF 2.1.0 JSON.
///
/// Only functions at moderate risk or above are emitted.
pub fn render_sarif(snapshot: &Snapshot) -> String {
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
                        band = f.band,
                    ),
                },
                locations: vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifact {
                            uri: f.file.clone(),
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
