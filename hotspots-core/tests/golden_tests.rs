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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn read_golden(name: &str) -> String {
    let path = golden_path(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read golden file {}: {}", path.display(), e))
}

/// Normalize paths in JSON to use the actual project root for portability
/// Extracts the path segment after "hotspots/" and normalizes to use the current project root
fn normalize_paths(json: &mut serde_json::Value, project_root: &PathBuf) {
    match json {
        serde_json::Value::Array(arr) => {
            for item in arr {
                normalize_paths(item, project_root);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::String(path)) = obj.get_mut("file") {
                // Extract path segment after "hotspots/" - this is the relative path within the project
                let path_str = path.as_str();
                if let Some(idx) = path_str.find("hotspots/") {
                    let suffix = &path_str[idx + "hotspots/".len()..];
                    // Normalize to use the actual project root (no hardcoded paths)
                    *path = project_root.join(suffix).to_string_lossy().to_string();
                } else if let Some(idx) = path_str.find("hotspots") {
                    // Handle case where "hotspots" is not followed by "/"
                    if let Some(next_char) = path_str.chars().nth(idx + "hotspots".len()) {
                        if next_char == '/' || next_char == '\\' {
                            let suffix = &path_str[idx + "hotspots".len() + 1..];
                            *path = project_root.join(suffix).to_string_lossy().to_string();
                        }
                    }
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
    let mut output_json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
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

    assert_eq!(json1, json2, "Output must be byte-for-byte identical across runs");
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
    let mut output_json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
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

    assert_eq!(json1, json2, "Go output must be byte-for-byte identical across runs");
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
    let mut output_json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Output is not valid JSON: {}", e));
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

    assert_eq!(json1, json2, "Rust output must be byte-for-byte identical across runs");
}
