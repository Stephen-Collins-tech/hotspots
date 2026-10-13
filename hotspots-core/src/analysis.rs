//! Analysis orchestration - ties together parsing, discovery, CFG, metrics, and reporting

use crate::ast::FunctionNode;
use crate::language::{self, Language, LanguageParser};
use crate::metrics;
use crate::metrics_cache;
use crate::report;
use crate::risk;
use anyhow::{Context, Result};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use swc_common::{sync::Lrc, SourceMap};

/// Analyze a source file (TypeScript, JavaScript, Go, or Rust)
pub fn analyze_file(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
) -> Result<Vec<report::FunctionRiskReport>> {
    analyze_file_with_config(path, source_map, file_index, options, None, None, None)
}

/// Analyze a file with optional custom weights, thresholds, and pattern thresholds
pub fn analyze_file_with_config(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
    weights: Option<&risk::LrsWeights>,
    thresholds: Option<&risk::RiskThresholds>,
    pattern_thresholds: Option<&crate::patterns::Thresholds>,
) -> Result<Vec<report::FunctionRiskReport>> {
    let default_weights = risk::LrsWeights::default();
    let default_thresholds = risk::RiskThresholds::default();
    let default_pattern_thresholds = crate::patterns::Thresholds::default();
    let w = weights.unwrap_or(&default_weights);
    let t = thresholds.unwrap_or(&default_thresholds);
    let pt = pattern_thresholds.unwrap_or(&default_pattern_thresholds);

    let src = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let (max_line, long_line_count) = long_line_stats(&src, 1000);
    if long_line_count >= 3 {
        eprintln!(
            "warning: skipping {} — looks minified or machine-generated \
             ({} lines exceed 1000 chars, max: {})",
            path.display(),
            long_line_count,
            max_line
        );
        return Ok(vec![]);
    } else if looks_vendored(path) {
        eprintln!(
            "warning: skipping {} — path suggests vendored or generated third-party code",
            path.display()
        );
        return Ok(vec![]);
    }

    let language = Language::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file type: {}", path.display()))?;
    let parser = create_parser(language, source_map)?;

    let parse_result = panic::catch_unwind(AssertUnwindSafe(|| {
        parser.parse(&src, &path.to_string_lossy())
    }));

    let module = match parse_result {
        Ok(result) => result?,
        Err(panic_payload) => {
            let msg = panic_payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            eprintln!(
                "warning: skipping {} — parser panicked: {}",
                path.display(),
                msg
            );
            return Ok(vec![]);
        }
    };
    let mut functions = module.discover_functions(file_index, &src);

    // Extract suppression comments here, uniformly across all languages —
    // language-specific parsers leave `suppression_reason: None` and rely on
    // this pass rather than each duplicating the (language-agnostic) logic.
    let comment_prefix = language.suppression_comment_prefix();
    for function in &mut functions {
        function.suppression_reason =
            crate::suppression::extract_suppression(&src, function.span, comment_prefix);
    }

    let func_cfg = FunctionAnalysisConfig {
        options,
        weights: w,
        thresholds: t,
        pattern_thresholds: pt,
        source_map,
    };
    // Hashed once per file (not per function): `RawMetrics` depends only on
    // file content, so this is the key into the on-disk metrics cache (see
    // metrics_cache.rs). Any change anywhere in the file invalidates every
    // function's cached metrics in it — see metrics_cache.rs's doc comment
    // for why whole-file hashing was chosen over a per-function slice hash.
    let content_hash = metrics_cache::hash_content(&src);
    let mut reports = Vec::new();
    for function in &functions {
        if let Some(report) = analyze_function(function, path, language, &func_cfg, &content_hash) {
            reports.push(report);
        }
    }
    Ok(reports)
}

/// Returns the length of the longest line and the count of lines exceeding `threshold` chars.
///
/// Used to detect minified or machine-generated files before full analysis.
fn long_line_stats(src: &str, threshold: usize) -> (usize, usize) {
    let mut max_len = 0;
    let mut count = 0;
    for line in src.lines() {
        let len = line.len();
        if len > max_len {
            max_len = len;
        }
        if len > threshold {
            count += 1;
        }
    }
    (max_len, count)
}

/// Returns true if a file path suggests it contains vendored or generated third-party code.
///
/// Checks for common directory conventions used to store vendor dependencies, static assets,
/// and generated files that are typically not authored code (e.g. `vendor/`, `assets/js/`,
/// `third_party/`, `fixtures/`).
fn looks_vendored(path: &Path) -> bool {
    const VENDORED_SEGMENTS: &[&str] = &[
        "vendor",
        "vendors",
        "third_party",
        "thirdparty",
        "assets/js",
        "static/js",
        "public/js",
        "dist/js",
    ];
    let path_str = path.to_string_lossy().to_lowercase();
    VENDORED_SEGMENTS.iter().any(|seg| {
        path_str.contains(&format!("/{seg}/")) || path_str.contains(&format!("/{seg}\\"))
    })
}

/// Instantiates the correct parser for the given language.
fn create_parser(
    language: Language,
    source_map: &Lrc<SourceMap>,
) -> Result<Box<dyn LanguageParser>> {
    let parser: Box<dyn LanguageParser> = match language {
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact => {
            Box::new(language::ECMAScriptParser::new(source_map.clone()))
        }
        Language::Go => Box::new(language::GoParser::new().context("Failed to create Go parser")?),
        Language::Java => {
            Box::new(language::JavaParser::new().context("Failed to create Java parser")?)
        }
        Language::Python => {
            Box::new(language::PythonParser::new().context("Failed to create Python parser")?)
        }
        Language::Rust => Box::new(language::RustParser),
        Language::Vue => Box::new(language::VueParser::new(source_map.clone())),
        Language::CSharp => {
            Box::new(language::CSharpParser::new().context("Failed to create C# parser")?)
        }
        Language::C | Language::CHeader => {
            Box::new(language::CParser::new().context("Failed to create C parser")?)
        }
    };
    Ok(parser)
}

struct FunctionAnalysisConfig<'a> {
    options: &'a crate::AnalysisOptions,
    weights: &'a risk::LrsWeights,
    thresholds: &'a risk::RiskThresholds,
    pattern_thresholds: &'a crate::patterns::Thresholds,
    source_map: &'a Lrc<SourceMap>,
}

/// Builds CFG, extracts metrics, computes risk and patterns for one function.
/// Returns None if the CFG is invalid or the function is filtered by min_lrs.
///
/// `content_hash` is the hash of the whole file's source text (see
/// `metrics_cache::hash_content`), used together with the function's own line
/// span as the cache key. On a cache hit, CFG construction and
/// `metrics::extract_metrics` are both skipped — see `metrics_cache.rs`'s doc
/// comment for why skipping CFG validation on a hit is safe: a hit only
/// exists for a (hash, span) pair that previously built and validated
/// successfully, and both are deterministic functions of file content.
fn analyze_function(
    function: &FunctionNode,
    path: &Path,
    language: Language,
    config: &FunctionAnalysisConfig<'_>,
    content_hash: &str,
) -> Option<report::FunctionRiskReport> {
    let w = config.weights;
    let t = config.thresholds;
    let pt = config.pattern_thresholds;
    let options = config.options;
    let source_map = config.source_map;

    let start_line = function.span.start_line;
    let end_line = function.span.end_line;
    let cache_key = metrics_cache::cache_key(content_hash, start_line, end_line);
    let mut cfg_invalid = false;
    let raw_metrics = {
        // Load (or lazily populate) this file's slot in the process-wide cache,
        // checking for a hit before doing any CFG/metrics work.
        if let Some(cached) = metrics_cache::peek(path, content_hash, &cache_key) {
            cached
        } else {
            let cfg = language::get_builder_for_function(function).build(function);
            if let Err(e) = cfg.validate() {
                eprintln!(
                    "warning: skipping function '{}' in {}: invalid CFG: {}",
                    function.name.as_deref().unwrap_or("<anonymous>"),
                    path.display(),
                    e
                );
                cfg_invalid = true;
                metrics::RawMetrics {
                    cc: 0,
                    nd: 0,
                    fo: 0,
                    ns: 0,
                    loc: 0,
                    callee_names: Vec::new(),
                }
            } else {
                let computed = metrics::extract_metrics(function, &cfg);
                metrics_cache::insert(path, content_hash, &cache_key, computed.clone());
                computed
            }
        }
    };
    if cfg_invalid {
        return None;
    }

    let (risk_components, lrs, band) = risk::analyze_risk_with_config(&raw_metrics, w, t);

    if options.min_lrs.is_some_and(|min| lrs < min) {
        return None;
    }

    let t1 = crate::patterns::Tier1Input {
        cc: raw_metrics.cc,
        nd: raw_metrics.nd,
        fo: raw_metrics.fo,
        ns: raw_metrics.ns,
        loc: raw_metrics.loc,
    };
    let t2 = crate::patterns::Tier2Input {
        fan_in: None,
        scc_size: None,
        churn_lines: None,
        days_since_last_change: None,
        neighbor_churn: None,
        is_entrypoint: false,
    };
    let patterns = crate::patterns::classify(&t1, &t2, pt);

    Some(report::FunctionRiskReport::new(
        function,
        path.to_string_lossy().to_string(),
        language,
        report::FunctionAnalysis {
            metrics: raw_metrics,
            risk: risk_components,
            lrs,
            band,
            patterns,
        },
        source_map,
    ))
}

#[cfg(test)]
mod metrics_cache_correctness_tests {
    use super::*;
    use crate::AnalysisOptions;

    fn options() -> AnalysisOptions {
        AnalysisOptions {
            min_lrs: None,
            top_n: None,
        }
    }

    /// Runs `analyze_file_with_config` twice on the same file content. The
    /// first run is a cache miss (cold); the second hits the process-wide
    /// metrics cache populated by the first run (warm — no disk round-trip
    /// needed since both calls share the same process). Cached output must
    /// be bit-for-bit identical to the freshly computed one, proving the
    /// cache lookup never returns stale or wrong data.
    fn assert_cache_hit_matches_fresh(ext: &str, src: &str) {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join(format!("sample.{ext}"));
        std::fs::write(&file_path, src).unwrap();

        let cm: Lrc<SourceMap> = Default::default();
        let cold = analyze_file_with_config(&file_path, &cm, 0, &options(), None, None, None)
            .expect("cold run should succeed");
        let cm2: Lrc<SourceMap> = Default::default();
        let warm = analyze_file_with_config(&file_path, &cm2, 0, &options(), None, None, None)
            .expect("warm run should succeed");

        assert_eq!(
            cold.len(),
            warm.len(),
            "cache hit must report the same number of functions as a fresh run"
        );
        for (c, w) in cold.iter().zip(warm.iter()) {
            assert_eq!(
                c.metrics, w.metrics,
                "cached RawMetrics-derived report must match a fresh (uncached) one"
            );
        }
    }

    #[test]
    fn ecmascript_cache_hit_matches_fresh_extraction() {
        assert_cache_hit_matches_fresh(
            "ts",
            "export function add(a: number, b: number): number {\n  if (a > 0) {\n    return a + b;\n  }\n  return b;\n}\n",
        );
    }

    #[test]
    fn python_cache_hit_matches_fresh_extraction() {
        assert_cache_hit_matches_fresh(
            "py",
            "def add(a, b):\n    if a > 0:\n        return a + b\n    return b\n",
        );
    }

    #[test]
    fn go_cache_hit_matches_fresh_extraction() {
        assert_cache_hit_matches_fresh(
            "go",
            "package main\n\nfunc add(a int, b int) int {\n\tif a > 0 {\n\t\treturn a + b\n\t}\n\treturn b\n}\n",
        );
    }

    /// Changing a file's content must change its content hash and produce a
    /// fresh, correct computation on the next analysis — never a stale hit
    /// from the previous content's cache entry.
    #[test]
    fn content_change_invalidates_cache_and_recomputes() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("sample.ts");

        std::fs::write(
            &file_path,
            "export function f(a: number): number {\n  return a;\n}\n",
        )
        .unwrap();
        let cm: Lrc<SourceMap> = Default::default();
        let before = analyze_file_with_config(&file_path, &cm, 0, &options(), None, None, None)
            .expect("first run should succeed");

        std::fs::write(
            &file_path,
            "export function f(a: number): number {\n  if (a > 0) {\n    return a;\n  }\n  return -a;\n}\n",
        )
        .unwrap();
        let cm2: Lrc<SourceMap> = Default::default();
        let after = analyze_file_with_config(&file_path, &cm2, 0, &options(), None, None, None)
            .expect("second run (changed content) should succeed");

        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_ne!(
            before[0].metrics, after[0].metrics,
            "changed content must produce fresh (different) metrics, not a stale cache hit"
        );
        // cc should have gone up: an unconditional return became an if/else branch.
        assert!(after[0].metrics.cc > before[0].metrics.cc);
    }
}
