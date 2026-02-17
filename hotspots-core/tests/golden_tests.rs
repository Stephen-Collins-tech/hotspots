//! Golden file tests - verify output matches expected snapshots

use hotspots_core::{analyze, render_json, AnalysisOptions};
use std::fs;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("golden")
        .join(name)
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_golden(name: &str) -> String {
    let path = golden_path(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read golden file {}: {}", path.display(), e))
}

/// Normalize paths in JSON to use relative paths for cross-platform portability
/// Strips the project root prefix to get a relative path that works everywhere
fn normalize_paths(json: &mut serde_json::Value, project_root: &PathBuf) {
    match json {
        serde_json::Value::Array(arr) => {
            for item in arr {
                normalize_paths(item, project_root);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::String(path)) = obj.get_mut("file") {
                let path_buf = PathBuf::from(path.as_str());
                // Strip the project root prefix to get the relative path
                if let Ok(relative) = path_buf.strip_prefix(project_root) {
                    // Use relative path with forward slashes for cross-platform compatibility
                    *path = relative.to_string_lossy().replace('\\', "/");
                }
            }
            for (_, value) in obj {
                normalize_paths(value, project_root);
            }
        }
        _ => {}
    }
}

fn test_golden(fixture_name: &str) {
    let fixture = fixture_path(&format!("{}.ts", fixture_name));
    let golden = golden_path(&format!("{}.json", fixture_name));
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden(&format!("{}.json", fixture_name));

    // Parse both as JSON for comparison (handles formatting differences)
    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    // Normalize paths in both JSON values before comparison
    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for {}",
        fixture_name
    );
}

#[test]
fn test_golden_simple() {
    test_golden("simple");
}

#[test]
fn test_golden_nested_branching() {
    test_golden("nested-branching");
}

#[test]
fn test_golden_loop_breaks() {
    test_golden("loop-breaks");
}

#[test]
fn test_golden_try_catch_finally() {
    test_golden("try-catch-finally");
}

#[test]
fn test_golden_pathological() {
    test_golden("pathological");
}

#[test]
fn test_golden_determinism() {
    // Test that running analysis twice produces identical output
    let fixture = fixture_path("simple.ts");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports1 = analyze(&fixture, options1).unwrap();
    let reports2 = analyze(&fixture, options2).unwrap();

    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);

    assert_eq!(
        json1, json2,
        "Output must be byte-for-byte identical across runs"
    );
}

// Go language golden tests

fn test_go_golden(fixture_name: &str) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("go")
        .join(format!("{}.go", fixture_name));
    let golden = golden_path(&format!("go-{}.json", fixture_name));
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden(&format!("go-{}.json", fixture_name));

    // Parse both as JSON for comparison (handles formatting differences)
    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    // Normalize paths in both JSON values before comparison
    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for {}",
        fixture_name
    );
}

#[test]
fn test_go_golden_simple() {
    test_go_golden("simple");
}

#[test]
fn test_go_golden_loops() {
    test_go_golden("loops");
}

#[test]
fn test_go_golden_switch() {
    test_go_golden("switch");
}

#[test]
fn test_go_golden_specific() {
    test_go_golden("go_specific");
}

#[test]
fn test_go_golden_determinism() {
    // Test that running Go analysis twice produces identical output
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("go")
        .join("simple.go");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports1 = analyze(&fixture, options1).unwrap();
    let reports2 = analyze(&fixture, options2).unwrap();

    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);

    assert_eq!(
        json1, json2,
        "Go output must be byte-for-byte identical across runs"
    );
}

// Rust language golden tests

fn test_rust_golden(fixture_name: &str) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("rust")
        .join(format!("{}.rs", fixture_name));
    let golden = golden_path(&format!("rust-{}.json", fixture_name));
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden(&format!("rust-{}.json", fixture_name));

    // Parse both as JSON for comparison (handles formatting differences)
    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    // Normalize paths in both JSON values before comparison
    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for {}",
        fixture_name
    );
}

#[test]
fn test_rust_golden_simple() {
    test_rust_golden("simple");
}

#[test]
fn test_rust_golden_loops() {
    test_rust_golden("loops");
}

#[test]
fn test_rust_golden_match() {
    test_rust_golden("match");
}

#[test]
fn test_rust_golden_specific() {
    test_rust_golden("rust_specific");
}

#[test]
fn test_rust_golden_determinism() {
    // Test that running Rust analysis twice produces identical output
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("rust")
        .join("simple.rs");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports1 = analyze(&fixture, options1).unwrap();
    let reports2 = analyze(&fixture, options2).unwrap();

    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);

    assert_eq!(
        json1, json2,
        "Rust output must be byte-for-byte identical across runs"
    );
}

// Java language golden tests

fn test_java_golden(fixture_name: &str) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("java")
        .join(format!("{}.java", fixture_name));
    // Golden files use lowercase/snake_case names
    let golden_name = fixture_name.to_lowercase();
    let golden = golden_path(&format!("java-{}.json", golden_name));
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden(&format!("java-{}.json", golden_name));

    // Parse both as JSON for comparison (handles formatting differences)
    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    // Normalize paths in both JSON values before comparison
    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for {}",
        fixture_name
    );
}

#[test]
fn test_java_golden_simple() {
    test_java_golden("Simple");
}

#[test]
fn test_java_golden_loops() {
    test_java_golden("Loops");
}

#[test]
fn test_java_golden_exceptions() {
    test_java_golden("Exceptions");
}

#[test]
fn test_java_golden_classes() {
    test_java_golden("Classes");
}

#[test]
fn test_java_golden_anonymous_class() {
    // Fixture: AnonymousClass.java, Golden: java-anonymous_class.json
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("java")
        .join("AnonymousClass.java");
    let golden = golden_path("java-anonymous_class.json");
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden("java-anonymous_class.json");

    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for anonymous_class"
    );
}

#[test]
fn test_java_golden_java_specific() {
    // Fixture: JavaSpecific.java, Golden: java-java_specific.json
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("java")
        .join("JavaSpecific.java");
    let golden = golden_path("java-java_specific.json");
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden("java-java_specific.json");

    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for java_specific"
    );
}

#[test]
fn test_java_golden_switch_and_ternary() {
    // Fixture: SwitchAndTernary.java, Golden: java-switch_and_ternary.json
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("java")
        .join("SwitchAndTernary.java");
    let golden = golden_path("java-switch_and_ternary.json");
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden("java-switch_and_ternary.json");

    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for switch_and_ternary"
    );
}

#[test]
fn test_java_golden_determinism() {
    // Test that running Java analysis twice produces identical output
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("java")
        .join("Simple.java");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports1 = analyze(&fixture, options1).unwrap();
    let reports2 = analyze(&fixture, options2).unwrap();

    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);

    assert_eq!(
        json1, json2,
        "Java output must be byte-for-byte identical across runs"
    );
}

// Python language golden tests

fn test_python_golden(fixture_name: &str) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("python")
        .join(format!("{}.py", fixture_name));
    let golden = golden_path(&format!("python-{}.json", fixture_name));
    let project_root = project_root();

    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&fixture, options)
        .unwrap_or_else(|e| panic!("Failed to analyze {}: {}", fixture.display(), e));

    let output = render_json(&reports);
    let expected = read_golden(&format!("python-{}.json", fixture_name));

    // Parse both as JSON for comparison (handles formatting differences)
    let mut output_json: serde_json::Value =
        serde_json::from_str(&output).unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
    let mut expected_json: serde_json::Value = serde_json::from_str(&expected)
        .unwrap_or_else(|e| panic!("Golden file {} is not valid JSON: {}", golden.display(), e));

    // Normalize paths in both JSON values before comparison
    normalize_paths(&mut output_json, &project_root);
    normalize_paths(&mut expected_json, &project_root);

    assert_eq!(
        output_json, expected_json,
        "Output does not match golden file for {}",
        fixture_name
    );
}

#[test]
fn test_python_golden_simple() {
    test_python_golden("simple");
}

#[test]
fn test_python_golden_loops() {
    test_python_golden("loops");
}

#[test]
fn test_python_golden_exceptions() {
    test_python_golden("exceptions");
}

#[test]
fn test_python_golden_classes() {
    test_python_golden("classes");
}

#[test]
fn test_python_golden_boolean_ops() {
    test_python_golden("boolean_ops");
}

#[test]
fn test_python_golden_comprehensions() {
    test_python_golden("comprehensions");
}

#[test]
fn test_python_golden_python_specific() {
    test_python_golden("python_specific");
}

#[test]
fn test_python_golden_determinism() {
    // Test that running Python analysis twice produces identical output
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("python")
        .join("simple.py");
    let options1 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let options2 = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports1 = analyze(&fixture, options1).unwrap();
    let reports2 = analyze(&fixture, options2).unwrap();

    let json1 = render_json(&reports1);
    let json2 = render_json(&reports2);

    assert_eq!(
        json1, json2,
        "Python output must be byte-for-byte identical across runs"
    );
}

// Call graph golden tests — verify fan-out deduplication and LOC

#[test]
fn test_golden_call_graph() {
    test_golden("call-graph");
}

/// Verify that `fo` reflects deduplicated unique callees (helper called twice → fo=1)
#[test]
fn test_golden_call_graph_deduplication() {
    let fixture = fixture_path("call-graph.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let reports = analyze(&fixture, options).expect("analysis should succeed");

    let helper = reports.iter().find(|r| r.function == "helper").unwrap();
    let middle = reports.iter().find(|r| r.function == "middle").unwrap();
    let top = reports.iter().find(|r| r.function == "top").unwrap();

    // helper has no calls
    assert_eq!(helper.metrics.fo, 0, "helper fo should be 0");
    // middle calls helper() twice — deduplicated to 1 unique callee
    assert_eq!(middle.metrics.fo, 1, "middle fo should be 1 (deduplicated)");
    // top calls helper() + middle() = 2 unique callees
    assert_eq!(top.metrics.fo, 2, "top fo should be 2");

    // LOC: each 3-line function body (open brace, body, close brace)
    assert_eq!(helper.metrics.loc, 3);
    assert_eq!(middle.metrics.loc, 3);
    assert_eq!(top.metrics.loc, 3);
}

#[test]
fn test_go_golden_call_graph() {
    test_go_golden("call_graph");
}

/// Verify Go fan-out deduplication mirrors TypeScript behavior
#[test]
fn test_go_golden_call_graph_deduplication() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("go")
        .join("call_graph.go");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };
    let reports = analyze(&fixture, options).expect("analysis should succeed");

    let helper = reports.iter().find(|r| r.function == "helper").unwrap();
    let middle = reports.iter().find(|r| r.function == "middle").unwrap();
    let top = reports.iter().find(|r| r.function == "top").unwrap();

    assert_eq!(helper.metrics.fo, 0);
    assert_eq!(
        middle.metrics.fo, 1,
        "Go middle fo should be 1 (deduplicated)"
    );
    assert_eq!(top.metrics.fo, 2);
}

/// Cross-language extended metrics determinism: LOC and fo must be identical across runs
#[test]
fn test_extended_metrics_determinism() {
    let fixtures: &[(&str, &str)] = &[
        ("simple.ts", "ts"),
        ("nested-branching.ts", "ts"),
        ("loop-breaks.ts", "ts"),
    ];

    for (fixture_name, _lang) in fixtures {
        let fixture = fixture_path(fixture_name);
        let options1 = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };
        let options2 = AnalysisOptions {
            min_lrs: None,
            top_n: None,
        };
        let reports1 = analyze(&fixture, options1)
            .unwrap_or_else(|e| panic!("failed to analyze {}: {}", fixture_name, e));
        let reports2 = analyze(&fixture, options2)
            .unwrap_or_else(|e| panic!("failed to analyze {}: {}", fixture_name, e));

        for (r1, r2) in reports1.iter().zip(reports2.iter()) {
            assert_eq!(
                r1.metrics.loc, r2.metrics.loc,
                "LOC not deterministic for {} in {}",
                r1.function, fixture_name
            );
            assert_eq!(
                r1.metrics.fo, r2.metrics.fo,
                "fan-out not deterministic for {} in {}",
                r1.function, fixture_name
            );
        }
    }
}
