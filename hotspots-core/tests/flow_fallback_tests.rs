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

/// Flow-only forms the rewrite pass turns into valid TypeScript. Each wraps one function so the
/// count shows the file parsed.
#[test]
fn flow_only_syntax_is_analysed() {
    let body = "function f(a: number): number {\n  if (a) { return 1; }\n  return 2;\n}\n";
    for (name, head) in [
        (
            "nullable.js",
            "// @flow\nfunction g(a: ?string, b: Array<?number>) { return a; }\n",
        ),
        (
            "exact.js",
            "// @flow\ntype P = {| x: number, +y: string |};\n",
        ),
        ("opaque.js", "// @flow\nopaque type ID = string;\n"),
        ("cast.js", "// @flow\nconst v = g as ?string;\n"),
        ("inexact.js", "// @flow\ntype P = {a: number, ...};\n"),
        ("spread.js", "// @flow\ntype P = {...Base, a: number};\n"),
        (
            "extends.js",
            "// @flow\ntype D<T> = T extends {...} ? 1 : 2;\n",
        ),
        (
            "importtypeof.js",
            "// @flow\nimport typeof Foo from './foo';\n",
        ),
    ] {
        let n = analyzed(name, &format!("{head}{body}"));
        assert!(n.is_some_and(|n| n >= 1), "{name}: {n:?}");
    }
}

#[test]
fn flow_component_declaration_is_a_function() {
    let src = "// @flow\nexport default component Box(label: string, ...rest: Props) {\n  if (label) { return 1; }\n  return 2;\n}\n";
    assert_eq!(analyzed("comp.js", src), Some(1));
}

#[test]
fn jsx_spread_attributes_survive_the_rewrite() {
    let src = "// @flow\nfunction View(props: ?Props) {\n  if (props) { return <div {...props} />; }\n  return <div />;\n}\n";
    assert_eq!(analyzed("v.js", src), Some(1));
}

#[test]
fn unsupported_flow_syntax_is_still_skipped() {
    // A `component(...)` type is not rewritten, so the file still fails to parse.
    let src = "// @flow\ntype C = component(x: number);\nfunction f() { return 1; }\n";
    assert_eq!(analyzed("c.js", src), None);
}

#[test]
fn ordinary_javascript_is_unaffected() {
    assert_eq!(
        analyzed("d.js", "function f(a) { if (a) { return 1; } return 2; }\n"),
        Some(1)
    );
}
