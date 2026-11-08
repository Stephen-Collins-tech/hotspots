//! Flow-annotated JavaScript (`// @flow`): the subset that is also valid TypeScript is analysed;
//! the rest is skipped with a warning, as before.

use hotspots_core::analysis::analyze_file_with_config;
use hotspots_core::AnalysisOptions;
use std::path::PathBuf;
use swc_common::{sync::Lrc, SourceMap};

/// Functions analysed, or `None` if the file failed to parse (the CLI reports that as a skip).
fn analyzed(name: &str, source: &str) -> Option<usize> {
    let dir = std::env::temp_dir().join(format!("hotspots-flow-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let cm: Lrc<SourceMap> = Default::default();
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let n = analyze_file_with_config(&path, &cm, 0, &options, None, None)
        .ok()
        .map(|f| f.len());
    let _ = std::fs::remove_dir_all(&dir);
    n
}

const FLOW_TYPES: &str = "// @flow\ntype Point = {x: number, y: number};\n\
    function f(p: Point, n: number): number {\n  if (n > 0) { return p.x; }\n  return p.y;\n}\n";

#[test]
fn flow_file_using_typescript_compatible_types_is_analysed() {
    assert_eq!(analyzed("a.js", FLOW_TYPES), Some(1));
}

#[test]
fn same_source_without_the_flow_pragma_is_still_skipped() {
    // Without `@flow` this is simply invalid JavaScript, so nothing changes for it.
    let plain = FLOW_TYPES.replace("// @flow\n", "");
    assert_eq!(analyzed("b.js", &plain), None);
}

#[test]
fn flow_only_syntax_is_still_skipped() {
    // A nullable `?string` is Flow-only; TypeScript cannot parse it.
    let src =
        "// @flow\nfunction f(a: ?string): number {\n  if (a) { return 1; }\n  return 2;\n}\n";
    assert_eq!(analyzed("c.js", src), None);
}

#[test]
fn ordinary_javascript_is_unaffected() {
    assert_eq!(
        analyzed("d.js", "function f(a) { if (a) { return 1; } return 2; }\n"),
        Some(1)
    );
}
