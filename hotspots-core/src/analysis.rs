//! Analysis orchestration - ties together parsing, discovery, CFG, metrics, and reporting

use crate::ast::FunctionNode;
use crate::language::{self, Language, LanguageParser};
use crate::metrics;
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
    analyze_file_with_config(path, source_map, file_index, options, None, None)
}

/// Analyze a file with optional custom weights and thresholds
pub fn analyze_file_with_config(
    path: &Path,
    source_map: &Lrc<SourceMap>,
    file_index: usize,
    options: &crate::AnalysisOptions,
    weights: Option<&risk::LrsWeights>,
    thresholds: Option<&risk::RiskThresholds>,
) -> Result<Vec<report::FunctionRiskReport>> {
    let default_weights = risk::LrsWeights::default();
    let default_thresholds = risk::RiskThresholds::default();
    let w = weights.unwrap_or(&default_weights);
    let t = thresholds.unwrap_or(&default_thresholds);

    let src = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    if src.len() > MAX_SOURCE_BYTES {
        eprintln!(
            "warning: skipping {} — file is {} MiB (limit {} MiB); likely generated",
            path.display(),
            src.len() / (1024 * 1024),
            MAX_SOURCE_BYTES / (1024 * 1024)
        );
        return Ok(vec![]);
    }
    if let Some(marker) = generated_file_marker(&src) {
        eprintln!(
            "warning: skipping {} — header marks it as generated code ({})",
            path.display(),
            marker
        );
        return Ok(vec![]);
    }

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

    let mut language = Language::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file type: {}", path.display()))?;

    // `.h` is ambiguous: a header that uses C++ syntax goes to the C++ grammar instead
    // of the C one, which would drop or mis-parse its member functions (issue #204).
    if language == Language::CHeader && looks_like_cpp(&src) {
        language = Language::Cpp;
    }

    if let Some(limit) = nesting_limit(language) {
        let depth = max_bracket_depth(&src, limit);
        if depth > limit {
            eprintln!(
                "warning: skipping {} — nesting is deeper than {} levels, which can overflow the stack \
                 in the {} parser",
                path.display(),
                limit,
                language.name()
            );
            return Ok(vec![]);
        }
    }

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
    let line_starts = crate::suppression::line_starts(&src);
    for function in &mut functions {
        function.suppression_reason = crate::suppression::extract_suppression_indexed(
            &src,
            &line_starts,
            function.span,
            comment_prefix,
        );
    }

    let func_cfg = FunctionAnalysisConfig {
        options,
        weights: w,
        thresholds: t,
        source_map,
    };
    let mut reports = Vec::new();
    for function in &functions {
        if let Some(report) = analyze_function(function, path, language, &func_cfg) {
            reports.push(report);
        }
    }
    Ok(reports)
}

/// Source files larger than this are skipped. Real hand-written files are far smaller; the cases
/// above this are generated code (e.g. a 12 MB Thrift-generated Java file took the analyzer to
/// ~120 GB resident and made a whole-repo run time out).
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// How much of a file's start is searched for a "generated" marker.
const GENERATED_HEADER_BYTES: usize = 2048;

/// Returns the marker found if the file's header says it is generated code that humans don't author:
/// `@generated`, `Autogenerated by ...` (Thrift and others), or Go's `Code generated ... DO NOT EDIT`.
fn generated_file_marker(src: &str) -> Option<&'static str> {
    let mut end = src.len().min(GENERATED_HEADER_BYTES);
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    let header = &src[..end];
    if header.contains("@generated") {
        Some("@generated")
    } else if header.contains("Autogenerated by") {
        Some("Autogenerated by")
    } else if header.contains("Code generated") && header.contains("DO NOT EDIT") {
        Some("Code generated ... DO NOT EDIT")
    } else {
        None
    }
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

/// Bracket-nesting depth beyond which a language's parser can overflow its thread stack.
///
/// The Rust (`syn`) and ECMAScript (`swc`) parsers are recursive descent and Go's later stages
/// recurse too; none of that is ours to make iterative. Measured on the 32 MiB worker stacks the
/// CLI uses, overflow starts near 1,600 nested blocks for Rust, 2,600 nested parentheses for
/// TypeScript and 11,000 nested blocks for Go. These limits sit well below those, and far above
/// anything hand-written. This is a mitigation, not a fix: a single expression with tens of
/// thousands of operators and no brackets can still overflow.
fn nesting_limit(language: Language) -> Option<usize> {
    match language {
        Language::Rust => Some(1_000),
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact
        | Language::Vue
        | Language::Astro => Some(1_500),
        Language::Go => Some(5_000),
        _ => None,
    }
}

/// Maximum nesting of `()`, `[]` and `{}` in `src`, stopping as soon as it exceeds `stop_after`.
///
/// A plain byte scan that ignores strings and comments: brackets inside them can only raise the
/// count, which errs towards skipping a file, never towards a crash. Unmatched closers do not
/// drive the depth negative.
fn max_bracket_depth(src: &str, stop_after: usize) -> usize {
    let mut depth = 0usize;
    let mut max = 0usize;
    for b in src.bytes() {
        match b {
            b'(' | b'[' | b'{' => {
                depth += 1;
                if depth > max {
                    max = depth;
                    if max > stop_after {
                        return max;
                    }
                }
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
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

/// Returns true if the content of a `.h` file looks like C++ rather than C.
///
/// `.h` files default to the C grammar (`Language::CHeader`), but a large
/// share of `.h` files in C++ projects are actually C++ (member functions, templates,
/// range-based `for`, `namespace`, `::`-qualified names). The C grammar either drops
/// those constructs silently or produces a confusing "invalid CFG" error (see issue
/// #204), so such files are detected here and parsed as C++ instead.
fn looks_like_cpp(src: &str) -> bool {
    use regex::Regex;

    const KEYWORD_MARKERS: &[&str] = &[
        "class ",
        "template<",
        "template <",
        "namespace ",
        "public:",
        "private:",
        "protected:",
    ];
    if KEYWORD_MARKERS.iter().any(|kw| src.contains(kw)) {
        return true;
    }

    // `::`-qualified names (e.g. `std::vector`, `Foo::bar`) are not valid C.
    if src.contains("::") {
        return true;
    }

    // Range-based for loop: `for (... : ...)` — not recognized by the C grammar,
    // which is why a `break` inside one has no enclosing loop target (issue #204).
    static RANGE_FOR_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let range_for_re =
        RANGE_FOR_RE.get_or_init(|| Regex::new(r"for\s*\([^;()]*:[^;()]*\)").unwrap());
    if range_for_re.is_match(src) {
        return true;
    }

    false
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
        Language::Astro => Box::new(language::AstroParser::new(source_map.clone())),
        Language::CSharp => {
            Box::new(language::CSharpParser::new().context("Failed to create C# parser")?)
        }
        Language::C | Language::CHeader => {
            Box::new(language::CParser::new().context("Failed to create C parser")?)
        }
        Language::Cpp => {
            Box::new(language::CppParser::new().context("Failed to create C++ parser")?)
        }
        Language::Php => {
            Box::new(language::PhpParser::new().context("Failed to create PHP parser")?)
        }
    };
    Ok(parser)
}

struct FunctionAnalysisConfig<'a> {
    options: &'a crate::AnalysisOptions,
    weights: &'a risk::LrsWeights,
    thresholds: &'a risk::RiskThresholds,
    source_map: &'a Lrc<SourceMap>,
}

/// Builds CFG, extracts metrics, computes risk for one function.
/// Returns None if the CFG is invalid or the function is filtered by min_lrs.
fn analyze_function(
    function: &FunctionNode,
    path: &Path,
    language: Language,
    config: &FunctionAnalysisConfig<'_>,
) -> Option<report::FunctionRiskReport> {
    let w = config.weights;
    let t = config.thresholds;
    let options = config.options;
    let source_map = config.source_map;

    let cfg = language::get_builder_for_function(function).build(function);
    let cfg_valid = cfg.validate();
    if let Err(e) = cfg_valid {
        eprintln!(
            "warning: skipping function '{}' in {}: invalid CFG: {}",
            function.name.as_deref().unwrap_or("<anonymous>"),
            path.display(),
            e
        );
        return None;
    }

    let raw_metrics = metrics::extract_metrics(function, &cfg);

    let (risk_components, lrs, band) = risk::analyze_risk_with_config(&raw_metrics, w, t);

    if options.min_lrs.is_some_and(|min| lrs < min) {
        return None;
    }

    Some(report::FunctionRiskReport::new(
        function,
        path.to_string_lossy().to_string(),
        language,
        report::FunctionAnalysis {
            metrics: raw_metrics,
            risk: risk_components,
            lrs,
            band,
        },
        source_map,
    ))
}

#[cfg(test)]
mod generated_file_tests {
    use super::*;

    #[test]
    fn generated_header_markers_are_detected() {
        assert_eq!(
            generated_file_marker(
                "/**\n * Autogenerated by Thrift Compiler (0.16.0)\n */\nclass A {}"
            ),
            Some("Autogenerated by")
        );
        assert_eq!(
            generated_file_marker("// @generated by tool\nfn x() {}"),
            Some("@generated")
        );
        assert_eq!(
            generated_file_marker("// Code generated by protoc-gen-go. DO NOT EDIT.\npackage x"),
            Some("Code generated ... DO NOT EDIT")
        );
    }

    #[test]
    fn ordinary_files_are_not_marked_generated() {
        assert_eq!(
            generated_file_marker("package main\nfunc main() {}\n"),
            None
        );
        // "Code generated" alone (no DO NOT EDIT) is not enough
        assert_eq!(
            generated_file_marker("// Code generated lazily at runtime\npackage x"),
            None
        );
        // a marker far past the header is ignored
        let late = format!("{}// @generated\n", "x\n".repeat(2000));
        assert_eq!(generated_file_marker(&late), None);
    }

    #[test]
    fn header_scan_does_not_split_multibyte_chars() {
        let src = "é".repeat(GENERATED_HEADER_BYTES);
        assert_eq!(generated_file_marker(&src), None);
    }

    #[test]
    fn max_bracket_depth_counts_nesting_and_stops_early() {
        assert_eq!(max_bracket_depth("fn f() { if x { (1) } }", 100), 3);
        assert_eq!(max_bracket_depth("}}}{", 100), 1);
        let deep = "{".repeat(5000);
        assert!(max_bracket_depth(&deep, 100) > 100);
    }

    #[test]
    fn nesting_limits_cover_the_recursive_parsers() {
        assert!(nesting_limit(Language::Rust).is_some());
        assert!(nesting_limit(Language::TypeScript).is_some());
        assert!(nesting_limit(Language::Go).is_some());
        assert!(nesting_limit(Language::Java).is_none());
    }
}
