//! Integration tests for `hotspots analyze --timing-report` (hotspots#267 part 1).
//!
//! These write a fixture into a fresh tempdir rather than using
//! `tests/fixtures/simple.ts` directly: the project's own `.hotspotsrc.json`
//! excludes `**/tests/fixtures/**`, which would silently analyze zero files
//! when invoked (as these tests do) through the real CLI binary with config
//! auto-discovery, instead of the fixture path used by in-process core tests.

use std::process::Command;

fn write_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    std::fs::write(
        dir.path().join("simple.ts"),
        "function add(a: number, b: number): number {\n  return a + b;\n}\n",
    )
    .expect("write fixture file");
    dir
}

fn hotspots_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hotspots"))
}

/// Flag absent: stdout is unchanged, and no timing JSON appears on stderr.
#[test]
fn timing_report_flag_absent_leaves_output_unchanged() {
    let dir = write_fixture();
    let path = dir.path();

    let without_flag = hotspots_cmd()
        .arg("analyze")
        .arg(path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("run hotspots analyze");
    assert!(without_flag.status.success(), "{:?}", without_flag);

    let with_flag_false = hotspots_cmd()
        .arg("analyze")
        .arg(path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("run hotspots analyze (no flag, second run)");

    assert_eq!(without_flag.stdout, with_flag_false.stdout);

    let stderr = String::from_utf8_lossy(&without_flag.stderr);
    assert!(
        !stderr.contains("\"phases\""),
        "timing report JSON should not appear on stderr without --timing-report: {stderr}"
    );
}

/// Flag present: stdout is byte-for-byte identical to the no-flag run (the
/// report goes to stderr only), and stderr carries a well-formed JSON object
/// with the expected phase names and sane (non-negative) values.
#[test]
fn timing_report_flag_present_emits_well_formed_json_on_stderr_without_touching_stdout() {
    let dir = write_fixture();
    let path = dir.path();

    let baseline = hotspots_cmd()
        .arg("analyze")
        .arg(path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("run hotspots analyze (baseline)");
    assert!(baseline.status.success());

    let with_timing = hotspots_cmd()
        .arg("analyze")
        .arg(path)
        .arg("--format")
        .arg("json")
        .arg("--timing-report")
        .output()
        .expect("run hotspots analyze --timing-report");
    assert!(with_timing.status.success(), "{:?}", with_timing);

    assert_eq!(
        baseline.stdout, with_timing.stdout,
        "--timing-report must not change stdout"
    );

    let stderr = String::from_utf8_lossy(&with_timing.stderr);
    let json_start = stderr
        .find('{')
        .unwrap_or_else(|| panic!("no JSON object found on stderr: {stderr}"));
    let report: serde_json::Value =
        serde_json::from_str(&stderr[json_start..]).expect("timing report should be valid JSON");

    let phases = report["phases"]
        .as_array()
        .expect("timing report should have a `phases` array");
    let expected_phases = [
        "discover",
        "parse",
        "cfg",
        "metrics",
        "scoring",
        "aggregate",
        "report",
    ];
    assert_eq!(phases.len(), expected_phases.len());
    for name in expected_phases {
        let phase = phases
            .iter()
            .find(|p| p["phase"] == name)
            .unwrap_or_else(|| panic!("missing phase '{name}' in timing report: {report}"));
        let wall_ms = phase["wall_ms"]
            .as_f64()
            .unwrap_or_else(|| panic!("phase '{name}' has non-numeric wall_ms: {phase}"));
        assert!(
            wall_ms >= 0.0,
            "phase '{name}' has negative wall_ms: {wall_ms}"
        );
    }

    assert!(report["files_processed"].as_u64().unwrap() >= 1);
    assert!(report["functions_processed"].as_u64().unwrap() >= 1);
}
