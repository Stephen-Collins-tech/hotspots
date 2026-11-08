//! Control-flow shapes that used to leave an unreachable node, so the CFG failed validation and
//! the whole function was dropped from the ranking ("invalid CFG: nodes not reachable from entry").

use hotspots_core::analysis::analyze_file_with_config;
use hotspots_core::AnalysisOptions;
use std::path::PathBuf;
use swc_common::{sync::Lrc, SourceMap};

/// Number of functions analysed in `source` (a skipped function is not counted).
fn analyzed(name: &str, source: &str) -> usize {
    let dir = std::env::temp_dir().join(format!("hotspots-cfg-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let cm: Lrc<SourceMap> = Default::default();
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let n = analyze_file_with_config(&path, &cm, 0, &options, None, None)
        .expect("analysis must not error")
        .len();
    let _ = std::fs::remove_dir_all(&dir);
    n
}

// `do { ... } while (0);` whose body always breaks or returns never reaches the condition.
const C_DO_WHILE: &str =
    "int f(int x) {\n    do {\n        if (x) break;\n        return 1;\n    } while (0);\n    return 0;\n}\n";

#[test]
fn c_do_while_body_that_always_exits() {
    assert_eq!(analyzed("a.c", C_DO_WHILE), 1);
}

#[test]
fn cpp_do_while_body_that_always_exits() {
    assert_eq!(analyzed("a.cpp", C_DO_WHILE), 1);
}

#[test]
fn java_do_while_body_that_always_exits() {
    let src = "class A { int f(int x) { do { if (x > 0) break; return 1; } while (false); return 0; } }\n";
    assert_eq!(analyzed("A.java", src), 1);
}

#[test]
fn csharp_do_while_body_that_always_exits() {
    let src = "class A { int F(int x) { do { if (x > 0) break; return 1; } while (false); return 0; } }\n";
    assert_eq!(analyzed("A.cs", src), 1);
}

// A `break` with no enclosing loop comes from text the grammar mis-parsed (a multi-line
// `#define` containing comments).
#[test]
fn c_and_cpp_break_outside_any_loop() {
    let src = "int f(int x) {\n    x++;\n    break;\n    return x;\n}\n";
    assert_eq!(analyzed("b.c", src), 1);
    assert_eq!(analyzed("b.cpp", src), 1);
}

#[test]
fn ordinary_do_while_still_counts_as_a_loop() {
    let simple = "int f(int x) { return x; }\n";
    let looped = "int f(int x) { do { x--; } while (x > 0); return x; }\n";
    assert_eq!(analyzed("c1.c", simple), 1);
    assert_eq!(analyzed("c2.c", looped), 1);
}
