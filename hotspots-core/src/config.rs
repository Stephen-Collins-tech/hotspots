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

/// Default exclude patterns always applied (merged with any user-specified excludes).
const DEFAULT_EXCLUDES: &[&str] = &[
    // Test files
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.test.js",
    "**/*.test.jsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/*.spec.js",
    "**/*.spec.jsx",
    "**/__tests__/**",
    "**/__mocks__/**",
    "**/__snapshots__/**",
    // Python test conventions
    "**/test_*.py",
    "**/*_test.py",
    // C/C++ test conventions (googletest and similar: `foo_test.cc`, `foo_unittest.cpp`).
    // These files define test cases through macros such as `TEST(...)`, which would
    // otherwise be ranked as ordinary functions.
    "**/*_test.{c,cc,cpp,cxx}",
    "**/*_tests.{c,cc,cpp,cxx}",
    "**/*_unittest.{cc,cpp,cxx}",
    // Go test and generated conventions
    "**/*_test.go",
    "**/vendor/**",
    "**/*.pb.go",
    "**/zz_generated*.go",
    "**/mock_*.go",
    // JS/TS build output and generated code
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/.next/**",
    "**/.nuxt/**",
    "**/.output/**",
    "**/.cache/**",
    "**/.turbo/**",
    "**/storybook-static/**",
    "**/*.min.js",
    "**/*.min.ts",
    "**/*.bundle.js",
    "**/__generated__/**",
    "**/generated/**",
    "**/*.generated.ts",
    "**/*.generated.js",
    "**/*.pb.ts",
    "**/*.pb.js",
    // Python virtualenvs and cache
    "**/venv/**",
    "**/.venv/**",
    "**/__pycache__/**",
    // Django auto-generated migrations
    "**/migrations/**",
    // Java/Kotlin build output
    "**/target/**",
    "**/out/**",
    // C/C++ vendored dependencies (common bundling conventions)
    "**/deps/**",
    "**/third_party/**",
    "**/thirdparty/**",
    "**/external/**",
    "**/extern/**",
    "**/contrib/**",
];

/// Current schema version for `.hotspotsrc.json` / `hotspots.config.json` files.
///
/// Mirrors `SNAPSHOT_SCHEMA_VERSION` (the unified master-schema envelope version,
/// which also covers what used to be the separate `DELTA_SCHEMA_VERSION` before
/// hotspots 2.0): bump this when a breaking change to weight/threshold semantics
/// requires migration or explicit detection rather than silent reinterpretation.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

fn default_config_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

/// Hotspots configuration loaded from a JSON config file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotspotsConfig {
    /// Schema version of this config file (default: `CONFIG_SCHEMA_VERSION`).
    /// Existing config files on disk predate this field, so it defaults to the
    /// current version rather than requiring every user config to set it.
    #[serde(default = "default_config_schema_version")]
    pub schema_version: u32,

    /// Glob patterns for files to include (default: all supported extensions)
    #[serde(default)]
    pub include: Vec<String>,

    /// Glob patterns for files to exclude (default: test files, node_modules, dist)
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Minimum LRS to report (default: 0.0, report all)
    #[serde(default)]
    pub min_lrs: Option<f64>,

    /// Maximum number of results to show
    #[serde(default)]
    pub top: Option<usize>,

    /// Per-repo severity overrides for blocking policies.
    #[serde(default)]
    pub policy: Option<PolicyConfig>,

    /// Dead-zone width (in Gini points) subtracted from `LOW_GINI` before the
    /// cold-start router switches from the Formula route to the
    /// Anomaly route (default: 0.0, no dead zone). Widens the ambiguous middle
    /// zone that already defaults to Formula, so a repo whose commit-count Gini
    /// sits just under 0.55 doesn't flip ranking strategy on a marginal commit.
    /// Does not change the `HIGH_GINI` (0.60) or `LOW_GINI` (0.55) constants
    /// themselves. Must be non-negative and less than `LOW_GINI`.
    #[serde(default)]
    pub cold_start_gini_dead_zone: Option<f64>,
}

impl Default for HotspotsConfig {
    fn default() -> Self {
        HotspotsConfig {
            schema_version: CONFIG_SCHEMA_VERSION,
            include: Vec::new(),
            exclude: Vec::new(),
            min_lrs: None,
            top: None,
            policy: None,
            cold_start_gini_dead_zone: None,
        }
    }
}

/// Severity for a blocking policy, as configured per-repo.
///
/// A repo whose baseline LRS naturally runs high (e.g. a research repo with
/// dense, one-shot scripts) can downgrade a policy from `block` to `warn`, or
/// disable it with `off`, without changing what counts as Critical (see
/// `thresholds.critical`) or which findings are reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    Block,
    Warn,
    Off,
}

impl PolicyMode {
    fn parse(field: &str, s: &str) -> Result<Self> {
        match s {
            "block" => Ok(PolicyMode::Block),
            "warn" => Ok(PolicyMode::Warn),
            "off" => Ok(PolicyMode::Off),
            other => anyhow::bail!(
                "policy.{} must be one of \"block\", \"warn\", \"off\" (got \"{}\")",
                field,
                other
            ),
        }
    }
}

/// Per-repo severity overrides for blocking policies.
///
/// Both `critical-introduction` (a function becomes Critical) and
/// `excessive-risk-regression` (an existing function jumps by >= 1.0 LRS)
/// default to `block`. Note: `critical-introduction` fires identically
/// whether the function is brand-new or an existing function regressed —
/// a Critical function needs review either way, so there is deliberately
/// no separate "new vs. regressed" knob here, only an overall severity.
///
/// Downgrading either policy below `block` requires a `_reason` string
/// (mirroring the `// hotspots-ignore: <reason>` convention for per-function
/// suppression), so a reviewer of the `.hotspotsrc.json` diff sees *why* a
/// blocking gate was weakened, not just that it was.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    /// Severity for `critical-introduction`: "block" | "warn" | "off" (default: "block")
    pub critical_introduction: Option<String>,
    /// Required when `critical_introduction` is not "block"
    pub critical_introduction_reason: Option<String>,
    /// Severity for `excessive-risk-regression`: "block" | "warn" | "off" (default: "block")
    pub excessive_risk_regression: Option<String>,
    /// Required when `excessive_risk_regression` is not "block"
    pub excessive_risk_regression_reason: Option<String>,
}

/// Resolved configuration with compiled glob patterns
#[derive(Debug)]
pub struct ResolvedConfig {
    /// Compiled include patterns (empty means include all)
    pub include: Option<GlobSet>,
    /// Compiled exclude patterns
    pub exclude: GlobSet,
    /// Filters
    pub min_lrs: Option<f64>,
    pub top_n: Option<usize>,
    /// Cold-start Gini-routing dead-zone width (see `HotspotsConfig::cold_start_gini_dead_zone`)
    pub cold_start_gini_dead_zone: f64,
    /// Severity for the `critical-introduction` policy (default: Block)
    pub critical_introduction_mode: PolicyMode,
    /// Reason given for downgrading `critical_introduction_mode` below Block (None if Block)
    pub critical_introduction_reason: Option<String>,
    /// Severity for the `excessive-risk-regression` policy (default: Block)
    pub excessive_risk_regression_mode: PolicyMode,
    /// Reason given for downgrading `excessive_risk_regression_mode` below Block (None if Block)
    pub excessive_risk_regression_reason: Option<String>,
    /// Path the config was loaded from (None if defaults)
    pub config_path: Option<PathBuf>,
    /// Whether the loaded config file set an `exclude` key specifically
    /// (as opposed to `exclude` being active only via built-in defaults)
    pub exclude_is_custom: bool,
}

impl HotspotsConfig {
    /// Validate the configuration for logical errors
    pub fn validate(&self) -> Result<()> {
        if self.schema_version > CONFIG_SCHEMA_VERSION {
            anyhow::bail!(
                "config schema_version {} is newer than supported version {}; \
                 upgrade hotspots to read this config",
                self.schema_version,
                CONFIG_SCHEMA_VERSION
            );
        }
        if let Some(ref p) = self.policy {
            validate_policy_config(p)?;
        }
        validate_scalar_fields(self)?;
        validate_glob_patterns(&self.include, &self.exclude)
    }
}

fn validate_scalar_fields(c: &HotspotsConfig) -> Result<()> {
    if let Some(min) = c.min_lrs {
        if min < 0.0 {
            anyhow::bail!("min_lrs must be non-negative (got {})", min);
        }
    }
    if let Some(dz) = c.cold_start_gini_dead_zone {
        if dz < 0.0 {
            anyhow::bail!(
                "cold_start_gini_dead_zone must be non-negative (got {})",
                dz
            );
        }
        if dz >= crate::trainer::LOW_GINI {
            anyhow::bail!(
                "cold_start_gini_dead_zone ({}) must be less than LOW_GINI ({})",
                dz,
                crate::trainer::LOW_GINI
            );
        }
    }
    Ok(())
}

fn validate_glob_patterns(include: &[String], exclude: &[String]) -> Result<()> {
    for pattern in include {
        Glob::new(pattern).with_context(|| format!("invalid include pattern: {}", pattern))?;
    }
    for pattern in exclude {
        Glob::new(pattern).with_context(|| format!("invalid exclude pattern: {}", pattern))?;
    }
    Ok(())
}

fn validate_policy_config(p: &PolicyConfig) -> Result<()> {
    if let Some(ref s) = p.critical_introduction {
        let mode = PolicyMode::parse("critical_introduction", s)?;
        require_reason_if_downgraded(
            "critical_introduction",
            mode,
            p.critical_introduction_reason.as_deref(),
        )?;
    }
    if let Some(ref s) = p.excessive_risk_regression {
        let mode = PolicyMode::parse("excessive_risk_regression", s)?;
        require_reason_if_downgraded(
            "excessive_risk_regression",
            mode,
            p.excessive_risk_regression_reason.as_deref(),
        )?;
    }
    Ok(())
}

/// A policy downgraded below `block` must carry a non-empty `<field>_reason`,
/// mirroring the `// hotspots-ignore: <reason>` convention — so a reviewer of
/// the config diff sees why a blocking gate was weakened.
fn require_reason_if_downgraded(field: &str, mode: PolicyMode, reason: Option<&str>) -> Result<()> {
    if mode == PolicyMode::Block {
        return Ok(());
    }
    match reason {
        Some(r) if !r.trim().is_empty() => Ok(()),
        _ => anyhow::bail!(
            "policy.{field}_reason is required when policy.{field} is not \"block\" (explain why this gate is downgraded)",
            field = field
        ),
    }
}

impl HotspotsConfig {
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

        // Compile exclude patterns: defaults always apply; user patterns are additive.
        let exclude = {
            let mut builder = GlobSetBuilder::new();
            for pattern in DEFAULT_EXCLUDES {
                builder.add(Glob::new(pattern)?);
            }
            for pattern in &self.exclude {
                builder.add(Glob::new(pattern)?);
            }
            builder.build()?
        };

        let (
            critical_introduction_mode,
            critical_introduction_reason,
            excessive_risk_regression_mode,
            excessive_risk_regression_reason,
        ) = match &self.policy {
            Some(p) => (
                match &p.critical_introduction {
                    Some(s) => PolicyMode::parse("critical_introduction", s)?,
                    None => PolicyMode::Block,
                },
                p.critical_introduction_reason.clone(),
                match &p.excessive_risk_regression {
                    Some(s) => PolicyMode::parse("excessive_risk_regression", s)?,
                    None => PolicyMode::Block,
                },
                p.excessive_risk_regression_reason.clone(),
            ),
            None => (PolicyMode::Block, None, PolicyMode::Block, None),
        };

        Ok(ResolvedConfig {
            include,
            exclude,
            min_lrs: self.min_lrs,
            top_n: self.top,
            critical_introduction_mode,
            critical_introduction_reason,
            excessive_risk_regression_mode,
            excessive_risk_regression_reason,
            cold_start_gini_dead_zone: self.cold_start_gini_dead_zone.unwrap_or(0.0),
            config_path: None,
            exclude_is_custom: !self.exclude.is_empty(),
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
    fn test_parse_minimal_config() {
        let json = r#"{}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
    }

    #[test]
    fn test_reject_unknown_fields() {
        let json = r#"{"unknown_field": true}"#;
        let result: Result<HotspotsConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown fields should be rejected");
    }

    #[test]
    fn test_schema_version_defaults_when_omitted() {
        // Existing config files on disk predate schema_version and must keep working.
        let json = r#"{}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        config.validate().unwrap();
    }

    #[test]
    fn test_schema_version_newer_than_supported_rejected() {
        let json = r#"{"schema_version": 999}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("schema_version"));
    }

    #[test]
    fn test_accept_unset_weights_use_nonzero_defaults() {
        // No weights key at all — every field defaults to a nonzero value,
        // so this must not be flagged as degenerate.
        let json = r#"{}"#;
        let config: HotspotsConfig = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_ok());
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
        // C/C++ vendored directories
        assert!(!resolved.should_include(Path::new("deps/jemalloc/src/arena.c")));
        assert!(!resolved.should_include(Path::new("third_party/abseil/absl/base/base.cc")));
        assert!(!resolved.should_include(Path::new("thirdparty/openssl/ssl/ssl_lib.c")));
        assert!(!resolved.should_include(Path::new("external/zlib/deflate.c")));
        assert!(!resolved.should_include(Path::new("extern/lua/lvm.c")));
        assert!(!resolved.should_include(Path::new("contrib/hiredis/net.c")));
        // C/C++ test files (googletest-style) are excluded; test-like names in source are not
        assert!(!resolved.should_include(Path::new("tests/db/segment_test.cc")));
        assert!(!resolved.should_include(Path::new("src/foo_unittest.cpp")));
        assert!(!resolved.should_include(Path::new("src/foo_tests.cxx")));
        assert!(!resolved.should_include(Path::new("src/util_test.c")));
        assert!(resolved.should_include(Path::new("src/testing.cc")));
        assert!(resolved.should_include(Path::new("src/contest.cc")));
        // Own source should still be included
        assert!(resolved.should_include(Path::new("src/server.c")));
        assert!(resolved.should_include(Path::new("src/networking.c")));
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
    fn test_discover_package_json_without_hotspots_key() {
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
    fn test_cold_start_gini_dead_zone_defaults_to_zero() {
        let resolved = HotspotsConfig::default().resolve().unwrap();
        assert_eq!(resolved.cold_start_gini_dead_zone, 0.0);
    }

    #[test]
    fn test_cold_start_gini_dead_zone_resolves_from_config() {
        let raw = HotspotsConfig {
            cold_start_gini_dead_zone: Some(0.05),
            ..Default::default()
        };
        let resolved = raw.resolve().unwrap();
        assert_eq!(resolved.cold_start_gini_dead_zone, 0.05);
    }

    #[test]
    fn test_cold_start_gini_dead_zone_rejects_negative() {
        let raw = HotspotsConfig {
            cold_start_gini_dead_zone: Some(-0.1),
            ..Default::default()
        };
        assert!(raw.validate().is_err());
    }

    #[test]
    fn test_cold_start_gini_dead_zone_rejects_at_or_above_low_gini() {
        let raw = HotspotsConfig {
            cold_start_gini_dead_zone: Some(crate::trainer::LOW_GINI),
            ..Default::default()
        };
        assert!(raw.validate().is_err());
    }

    #[test]
    fn test_exclude_is_custom_when_config_sets_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("custom.json");
        fs::write(&config_path, r#"{"exclude": ["**/legacy/**"]}"#).unwrap();

        let resolved = load_and_resolve(dir.path(), Some(&config_path)).unwrap();
        assert!(resolved.exclude_is_custom);
    }

    #[test]
    fn test_exclude_is_not_custom_with_no_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = load_and_resolve(dir.path(), None).unwrap();
        assert!(!resolved.exclude_is_custom);
    }

    #[test]
    fn test_default_config_resolves() {
        let resolved = HotspotsConfig::default().resolve().unwrap();
        assert!(resolved.min_lrs.is_none());
        assert!(resolved.top_n.is_none());
    }

    #[test]
    fn test_removed_tuning_keys_are_rejected() {
        for key in [
            r#"{"weights": {"cc": 1.0}}"#,
            r#"{"thresholds": {"high": 6.0}}"#,
            r#"{"warning_thresholds": {"rapid_growth_percent": 50.0}}"#,
            r#"{"scoring": {"churn": 0.5}}"#,
            r#"{"patterns": {}}"#,
            r#"{"co_change_window_days": 90}"#,
            r#"{"touch_window_days": 365}"#,
            r#"{"driver_threshold_percentile": 75}"#,
        ] {
            assert!(
                serde_json::from_str::<HotspotsConfig>(key).is_err(),
                "{key} should be rejected as an unknown key"
            );
        }
    }
}
