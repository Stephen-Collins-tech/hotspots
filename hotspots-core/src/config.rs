//! Configuration file support for Hotspots
//!
//! Loads project-specific configuration from JSON files.
//!
//! Search order:
//! 1. Explicit path (--config CLI flag)
//! 2. `.hotspotsrc.json` in project root
//! 3. `hotspots.config.json` in project root
//! 4. `"hotspots"` key in `package.json`
//!
//! All fields are optional. CLI flags take precedence over config file values.

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default exclude patterns applied when no config is specified
const DEFAULT_EXCLUDES: &[&str] = &[
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.test.js",
    "**/*.test.jsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/*.spec.js",
    "**/*.spec.jsx",
    "**/node_modules/**",
    "**/__tests__/**",
    "**/__mocks__/**",
    "**/dist/**",
    "**/build/**",
];

/// Hotspots configuration loaded from a JSON config file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotspotsConfig {
    /// Glob patterns for files to include (default: all supported extensions)
    #[serde(default)]
    pub include: Vec<String>,

    /// Glob patterns for files to exclude (default: test files, node_modules, dist)
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Custom risk band thresholds
    #[serde(default)]
    pub thresholds: Option<ThresholdConfig>,

    /// Custom metric weights for LRS calculation
    #[serde(default)]
    pub weights: Option<WeightConfig>,

    /// Warning thresholds for proactive alerts
    #[serde(default)]
    pub warning_thresholds: Option<WarningThresholdConfig>,

    /// Minimum LRS to report (default: 0.0, report all)
    #[serde(default)]
    pub min_lrs: Option<f64>,

    /// Maximum number of results to show
    #[serde(default)]
    pub top: Option<usize>,
}

/// Custom risk band thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThresholdConfig {
    /// LRS threshold for moderate risk (default: 3.0)
    pub moderate: Option<f64>,
    /// LRS threshold for high risk (default: 6.0)
    pub high: Option<f64>,
    /// LRS threshold for critical risk (default: 9.0)
    pub critical: Option<f64>,
}

/// Custom metric weights for LRS calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightConfig {
    /// Weight for cyclomatic complexity (default: 1.0)
    pub cc: Option<f64>,
    /// Weight for nesting depth (default: 0.8)
    pub nd: Option<f64>,
    /// Weight for fan-out (default: 0.6)
    pub fo: Option<f64>,
    /// Weight for non-structured exits (default: 0.7)
    pub ns: Option<f64>,
}

/// Warning thresholds for proactive alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningThresholdConfig {
    /// Watch threshold minimum - approaching moderate (default: 2.5)
    pub watch_min: Option<f64>,
    /// Watch threshold maximum - approaching moderate (default: 3.0)
    pub watch_max: Option<f64>,
    /// Attention threshold minimum - approaching high (default: 5.5)
    pub attention_min: Option<f64>,
    /// Attention threshold maximum - approaching high (default: 6.0)
    pub attention_max: Option<f64>,
    /// Rapid growth threshold - percent increase (default: 50.0)
    pub rapid_growth_percent: Option<f64>,
}

/// Resolved configuration with compiled glob patterns
#[derive(Debug)]
pub struct ResolvedConfig {
    /// Compiled include patterns (empty means include all)
    pub include: Option<GlobSet>,
    /// Compiled exclude patterns
    pub exclude: GlobSet,
    /// Risk band thresholds
    pub moderate_threshold: f64,
    pub high_threshold: f64,
    pub critical_threshold: f64,
    /// LRS weights
    pub weight_cc: f64,
    pub weight_nd: f64,
    pub weight_fo: f64,
    pub weight_ns: f64,
    /// Warning thresholds
    pub watch_min: f64,
    pub watch_max: f64,
    pub attention_min: f64,
    pub attention_max: f64,
    pub rapid_growth_percent: f64,
    /// Filters
    pub min_lrs: Option<f64>,
    pub top_n: Option<usize>,
    /// Path the config was loaded from (None if defaults)
    pub config_path: Option<PathBuf>,
}

impl HotspotsConfig {
    /// Validate the configuration for logical errors
    pub fn validate(&self) -> Result<()> {
        // Validate thresholds are positive and ordered
        if let Some(ref t) = self.thresholds {
            let moderate = t.moderate.unwrap_or(3.0);
            let high = t.high.unwrap_or(6.0);
            let critical = t.critical.unwrap_or(9.0);

            if moderate <= 0.0 {
                anyhow::bail!("thresholds.moderate must be positive (got {})", moderate);
            }
            if high <= 0.0 {
                anyhow::bail!("thresholds.high must be positive (got {})", high);
            }
            if critical <= 0.0 {
                anyhow::bail!("thresholds.critical must be positive (got {})", critical);
            }
            if moderate >= high {
                anyhow::bail!(
                    "thresholds.moderate ({}) must be less than thresholds.high ({})",
                    moderate,
                    high
                );
            }
            if high >= critical {
                anyhow::bail!(
                    "thresholds.high ({}) must be less than thresholds.critical ({})",
                    high,
                    critical
                );
            }
        }

        // Validate weights are non-negative
        if let Some(ref w) = self.weights {
            for (name, val) in [("cc", w.cc), ("nd", w.nd), ("fo", w.fo), ("ns", w.ns)] {
                if let Some(v) = val {
                    if v < 0.0 {
                        anyhow::bail!("weights.{} must be non-negative (got {})", name, v);
                    }
                    if v > 10.0 {
                        anyhow::bail!("weights.{} must be at most 10.0 (got {})", name, v);
                    }
                }
            }
        }

        // Validate warning thresholds
        if let Some(ref wt) = self.warning_thresholds {
            let watch_min = wt.watch_min.unwrap_or(2.5);
            let watch_max = wt.watch_max.unwrap_or(3.0);
            let attention_min = wt.attention_min.unwrap_or(5.5);
            let attention_max = wt.attention_max.unwrap_or(6.0);
            let rapid_growth = wt.rapid_growth_percent.unwrap_or(50.0);

            if watch_min <= 0.0 {
                anyhow::bail!(
                    "warning_thresholds.watch_min must be positive (got {})",
                    watch_min
                );
            }
            if watch_max <= 0.0 {
                anyhow::bail!(
                    "warning_thresholds.watch_max must be positive (got {})",
                    watch_max
                );
            }
            if attention_min <= 0.0 {
                anyhow::bail!(
                    "warning_thresholds.attention_min must be positive (got {})",
                    attention_min
                );
            }
            if attention_max <= 0.0 {
                anyhow::bail!(
                    "warning_thresholds.attention_max must be positive (got {})",
                    attention_max
                );
            }
            if rapid_growth <= 0.0 {
                anyhow::bail!(
                    "warning_thresholds.rapid_growth_percent must be positive (got {})",
                    rapid_growth
                );
            }

            if watch_min >= watch_max {
                anyhow::bail!(
                    "warning_thresholds.watch_min ({}) must be less than watch_max ({})",
                    watch_min,
                    watch_max
                );
            }
            if attention_min >= attention_max {
                anyhow::bail!(
                    "warning_thresholds.attention_min ({}) must be less than attention_max ({})",
                    attention_min,
                    attention_max
                );
            }
        }

        // Validate min_lrs is non-negative
        if let Some(min) = self.min_lrs {
            if min < 0.0 {
                anyhow::bail!("min_lrs must be non-negative (got {})", min);
            }
        }

        // Validate glob patterns compile
        for pattern in &self.include {
            Glob::new(pattern).with_context(|| format!("invalid include pattern: {}", pattern))?;
        }
        for pattern in &self.exclude {
            Glob::new(pattern).with_context(|| format!("invalid exclude pattern: {}", pattern))?;
        }

        Ok(())
    }

    /// Resolve config into compiled form ready for use
    pub fn resolve(&self) -> Result<ResolvedConfig> {
        self.validate()?;

        // Compile include patterns
        let include = if self.include.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in &self.include {
                builder.add(Glob::new(pattern)?);
            }
            Some(builder.build()?)
        };

        // Compile exclude patterns (merge with defaults if user didn't specify any)
        let exclude = {
            let mut builder = GlobSetBuilder::new();
            if self.exclude.is_empty() {
                // Use defaults when no excludes specified
                for pattern in DEFAULT_EXCLUDES {
                    builder.add(Glob::new(pattern)?);
                }
            } else {
                for pattern in &self.exclude {
                    builder.add(Glob::new(pattern)?);
                }
            }
            builder.build()?
        };

        let (moderate, high, critical) = match &self.thresholds {
            Some(t) => (
                t.moderate.unwrap_or(3.0),
                t.high.unwrap_or(6.0),
                t.critical.unwrap_or(9.0),
            ),
            None => (3.0, 6.0, 9.0),
        };

        let (w_cc, w_nd, w_fo, w_ns) = match &self.weights {
            Some(w) => (
                w.cc.unwrap_or(1.0),
                w.nd.unwrap_or(0.8),
                w.fo.unwrap_or(0.6),
                w.ns.unwrap_or(0.7),
            ),
            None => (1.0, 0.8, 0.6, 0.7),
        };

        let (watch_min, watch_max, attention_min, attention_max, rapid_growth_percent) =
            match &self.warning_thresholds {
                Some(wt) => (
                    wt.watch_min.unwrap_or(2.5),
                    wt.watch_max.unwrap_or(3.0),
                    wt.attention_min.unwrap_or(5.5),
                    wt.attention_max.unwrap_or(6.0),
                    wt.rapid_growth_percent.unwrap_or(50.0),
                ),
                None => (2.5, 3.0, 5.5, 6.0, 50.0),
            };

        Ok(ResolvedConfig {
            include,
            exclude,
            moderate_threshold: moderate,
            high_threshold: high,
            critical_threshold: critical,
            weight_cc: w_cc,
            weight_nd: w_nd,
            weight_fo: w_fo,
            weight_ns: w_ns,
            watch_min,
            watch_max,
            attention_min,
            attention_max,
            rapid_growth_percent,
            min_lrs: self.min_lrs,
            top_n: self.top,
            config_path: None,
        })
    }
}

impl ResolvedConfig {
    /// Check if a file path should be included based on include/exclude patterns
    pub fn should_include(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // Check exclude first
        if self.exclude.is_match(path_str.as_ref()) {
            return false;
        }

        // If include patterns exist, file must match at least one
        if let Some(ref include) = self.include {
            return include.is_match(path_str.as_ref());
        }

        true
    }

    /// Build a ResolvedConfig with all defaults (no config file)
    pub fn defaults() -> Result<Self> {
        HotspotsConfig::default().resolve()
    }
}

/// Discover and load a config file from the project root
///
/// Search order:
/// 1. `.hotspotsrc.json`
/// 2. `hotspots.config.json`
/// 3. `"hotspots"` key in `package.json`
///
/// Returns `None` if no config file is found (use defaults).
pub fn discover_config(project_root: &Path) -> Result<Option<(HotspotsConfig, PathBuf)>> {
    // 1. .hotspotsrc.json
    let rc_path = project_root.join(".hotspotsrc.json");
    if rc_path.exists() {
        let config = load_config_file(&rc_path)?;
        return Ok(Some((config, rc_path)));
    }

    // 2. hotspots.config.json
    let config_path = project_root.join("hotspots.config.json");
    if config_path.exists() {
        let config = load_config_file(&config_path)?;
        return Ok(Some((config, config_path)));
    }

    // 3. package.json "hotspots" key
    let pkg_path = project_root.join("package.json");
    if pkg_path.exists() {
        if let Some(config) = load_from_package_json(&pkg_path)? {
            return Ok(Some((config, pkg_path)));
        }
    }

    Ok(None)
}

/// Load config from an explicit file path
pub fn load_config_file(path: &Path) -> Result<HotspotsConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;

    let config: HotspotsConfig = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse config file: {}", path.display()))?;

    config
        .validate()
        .with_context(|| format!("invalid config in: {}", path.display()))?;

    Ok(config)
}

/// Load hotspots config from the "hotspots" key in package.json
fn load_from_package_json(path: &Path) -> Result<Option<HotspotsConfig>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let pkg: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    match pkg.get("hotspots") {
        Some(hotspots_value) => {
            let config: HotspotsConfig = serde_json::from_value(hotspots_value.clone())
                .with_context(|| format!("invalid hotspots config in {}", path.display()))?;
            config
                .validate()
                .with_context(|| format!("invalid hotspots config in {}", path.display()))?;
            Ok(Some(config))
        }
        None => Ok(None),
    }
}

/// Load and resolve config for a project
///
/// If `config_path` is provided, loads from that file.
/// Otherwise, discovers config from the project root.
/// Returns default config if nothing is found.
pub fn load_and_resolve(project_root: &Path, config_path: Option<&Path>) -> Result<ResolvedConfig> {
    let (config, source_path) = if let Some(path) = config_path {
        let config = load_config_file(path)?;
        (config, Some(path.to_path_buf()))
    } else {
        match discover_config(project_root)? {
            Some((config, path)) => (config, Some(path)),
            None => (HotspotsConfig::default(), None),
        }
    };

    let mut resolved = config.resolve()?;
    resolved.config_path = source_path;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_config_is_valid() {
        let config = HotspotsConfig::default();
        config.validate().expect("default config should be valid");
        let resolved = config.resolve().expect("default config should resolve");
        assert!(resolved.include.is_none());
        assert_eq!(resolved.weight_cc, 1.0);
        assert_eq!(resolved.weight_nd, 0.8);
        assert_eq!(resolved.weight_fo, 0.6);
        assert_eq!(resolved.weight_ns, 0.7);
        assert_eq!(resolved.moderate_threshold, 3.0);
        assert_eq!(resolved.high_threshold, 6.0);
        assert_eq!(resolved.critical_threshold, 9.0);
    }

    #[test]
    fn test_parse_minimal_config() {
        let json = r#"{}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
    }

    #[test]
    fn test_parse_full_config() {
        let json = r#"{
            "include": ["src/**/*.ts", "src/**/*.tsx"],
            "exclude": ["**/*.test.ts", "**/node_modules/**"],
            "thresholds": {
                "moderate": 4.0,
                "high": 7.0,
                "critical": 10.0
            },
            "weights": {
                "cc": 1.2,
                "nd": 0.9,
                "fo": 0.5,
                "ns": 0.8
            },
            "min_lrs": 2.0,
            "top": 20
        }"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let resolved = config.resolve().unwrap();
        assert!(resolved.include.is_some());
        assert_eq!(resolved.moderate_threshold, 4.0);
        assert_eq!(resolved.high_threshold, 7.0);
        assert_eq!(resolved.critical_threshold, 10.0);
        assert_eq!(resolved.weight_cc, 1.2);
        assert_eq!(resolved.min_lrs, Some(2.0));
        assert_eq!(resolved.top_n, Some(20));
    }

    #[test]
    fn test_reject_unknown_fields() {
        let json = r#"{"unknown_field": true}"#;
        let result: Result<HotspotsConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown fields should be rejected");
    }

    #[test]
    fn test_reject_negative_weight() {
        let json = r#"{"weights": {"cc": -1.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_weight_over_10() {
        let json = r#"{"weights": {"cc": 11.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_negative_threshold() {
        let json = r#"{"thresholds": {"moderate": -1.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_unordered_thresholds() {
        let json = r#"{"thresholds": {"moderate": 6.0, "high": 3.0, "critical": 9.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_invalid_glob_pattern() {
        let json = r#"{"include": ["[invalid"]}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_include_default_excludes() {
        let resolved = ResolvedConfig::defaults().unwrap();
        assert!(!resolved.should_include(Path::new("src/foo.test.ts")));
        assert!(!resolved.should_include(Path::new("node_modules/pkg/index.js")));
        assert!(!resolved.should_include(Path::new("dist/bundle.js")));
        assert!(resolved.should_include(Path::new("src/api.ts")));
        assert!(resolved.should_include(Path::new("src/components/Button.tsx")));
    }

    #[test]
    fn test_should_include_custom_patterns() {
        let config: HotspotsConfig = serde_json::from_str(
            r#"{
            "include": ["src/**/*.ts"],
            "exclude": ["src/generated/**"]
        }"#,
        )
        .unwrap();
        let resolved = config.resolve().unwrap();
        assert!(resolved.should_include(Path::new("src/api.ts")));
        assert!(!resolved.should_include(Path::new("lib/util.ts")));
        assert!(!resolved.should_include(Path::new("src/generated/types.ts")));
    }

    #[test]
    fn test_discover_hotspotsrc() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".hotspotsrc.json");
        fs::write(&config_path, r#"{"min_lrs": 5.0}"#).unwrap();

        let result = discover_config(dir.path()).unwrap();
        assert!(result.is_some());
        let (config, path) = result.unwrap();
        assert_eq!(config.min_lrs, Some(5.0));
        assert_eq!(path, config_path);
    }

    #[test]
    fn test_discover_hotspots_config_json() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hotspots.config.json");
        fs::write(&config_path, r#"{"top": 10}"#).unwrap();

        let result = discover_config(dir.path()).unwrap();
        assert!(result.is_some());
        let (config, _) = result.unwrap();
        assert_eq!(config.top, Some(10));
    }

    #[test]
    fn test_discover_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_path = dir.path().join("package.json");
        fs::write(
            &pkg_path,
            r#"{
            "name": "my-project",
            "version": "1.0.0",
            "hotspots": {
                "exclude": ["**/*.test.ts"],
                "min_lrs": 3.0
            }
        }"#,
        )
        .unwrap();

        let result = discover_config(dir.path()).unwrap();
        assert!(result.is_some());
        let (config, _) = result.unwrap();
        assert_eq!(config.min_lrs, Some(3.0));
        assert_eq!(config.exclude, vec!["**/*.test.ts"]);
    }

    #[test]
    fn test_discover_package_json_without_faultline_key() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_path = dir.path().join("package.json");
        fs::write(&pkg_path, r#"{"name": "my-project", "version": "1.0.0"}"#).unwrap();

        let result = discover_config(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_discover_priority_order() {
        let dir = tempfile::tempdir().unwrap();

        // Create both config files - .hotspotsrc.json should win
        fs::write(dir.path().join(".hotspotsrc.json"), r#"{"min_lrs": 1.0}"#).unwrap();
        fs::write(
            dir.path().join("hotspots.config.json"),
            r#"{"min_lrs": 2.0}"#,
        )
        .unwrap();

        let result = discover_config(dir.path()).unwrap();
        let (config, _) = result.unwrap();
        assert_eq!(
            config.min_lrs,
            Some(1.0),
            ".hotspotsrc.json should take priority"
        );
    }

    #[test]
    fn test_no_config_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let result = discover_config(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_and_resolve_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = load_and_resolve(dir.path(), None).unwrap();
        assert!(resolved.config_path.is_none());
        assert_eq!(resolved.weight_cc, 1.0);
    }

    #[test]
    fn test_load_and_resolve_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("custom.json");
        fs::write(&config_path, r#"{"weights": {"cc": 2.0}}"#).unwrap();

        let resolved = load_and_resolve(dir.path(), Some(&config_path)).unwrap();
        assert_eq!(resolved.weight_cc, 2.0);
        assert_eq!(resolved.config_path, Some(config_path));
    }

    #[test]
    fn test_partial_weights_use_defaults_for_rest() {
        let json = r#"{"weights": {"cc": 2.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.weight_cc, 2.0);
        assert_eq!(resolved.weight_nd, 0.8); // default
        assert_eq!(resolved.weight_fo, 0.6); // default
        assert_eq!(resolved.weight_ns, 0.7); // default
    }

    #[test]
    fn test_partial_thresholds_use_defaults_for_rest() {
        let json = r#"{"thresholds": {"critical": 12.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.moderate_threshold, 3.0); // default
        assert_eq!(resolved.high_threshold, 6.0); // default
        assert_eq!(resolved.critical_threshold, 12.0);
    }

    #[test]
    fn test_default_warning_thresholds() {
        let config = HotspotsConfig::default();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.watch_min, 2.5);
        assert_eq!(resolved.watch_max, 3.0);
        assert_eq!(resolved.attention_min, 5.5);
        assert_eq!(resolved.attention_max, 6.0);
        assert_eq!(resolved.rapid_growth_percent, 50.0);
    }

    #[test]
    fn test_parse_warning_thresholds() {
        let json = r#"{
            "warning_thresholds": {
                "watch_min": 2.0,
                "watch_max": 3.5,
                "attention_min": 5.0,
                "attention_max": 7.0,
                "rapid_growth_percent": 75.0
            }
        }"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.watch_min, 2.0);
        assert_eq!(resolved.watch_max, 3.5);
        assert_eq!(resolved.attention_min, 5.0);
        assert_eq!(resolved.attention_max, 7.0);
        assert_eq!(resolved.rapid_growth_percent, 75.0);
    }

    #[test]
    fn test_partial_warning_thresholds_use_defaults() {
        let json = r#"{"warning_thresholds": {"rapid_growth_percent": 100.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.watch_min, 2.5); // default
        assert_eq!(resolved.watch_max, 3.0); // default
        assert_eq!(resolved.attention_min, 5.5); // default
        assert_eq!(resolved.attention_max, 6.0); // default
        assert_eq!(resolved.rapid_growth_percent, 100.0);
    }

    #[test]
    fn test_reject_negative_watch_min() {
        let json = r#"{"warning_thresholds": {"watch_min": -1.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_negative_rapid_growth() {
        let json = r#"{"warning_thresholds": {"rapid_growth_percent": -10.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_unordered_watch_thresholds() {
        let json = r#"{"warning_thresholds": {"watch_min": 3.0, "watch_max": 2.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_reject_unordered_attention_thresholds() {
        let json = r#"{"warning_thresholds": {"attention_min": 7.0, "attention_max": 5.0}}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_full_config_with_warnings() {
        let json = r#"{
            "thresholds": {
                "moderate": 3.0,
                "high": 6.0,
                "critical": 9.0
            },
            "warning_thresholds": {
                "watch_min": 2.5,
                "watch_max": 3.0,
                "attention_min": 5.5,
                "attention_max": 6.0,
                "rapid_growth_percent": 50.0
            }
        }"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let resolved = config.resolve().unwrap();
        assert_eq!(resolved.watch_min, 2.5);
        assert_eq!(resolved.watch_max, 3.0);
        assert_eq!(resolved.attention_min, 5.5);
        assert_eq!(resolved.attention_max, 6.0);
        assert_eq!(resolved.rapid_growth_percent, 50.0);
    }
}
