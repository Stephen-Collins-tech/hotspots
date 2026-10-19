//! Tests for per-phase timing instrumentation (hotspots#267 part 1).

use hotspots_core::{analyze_with_progress, analyze_with_progress_and_timing, AnalysisOptions};
use hotspots_core::{TimingCollector, TimingReport};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        min_lrs: None,
        top_n: None,
    }
}

/// Flag absent (no timing collector passed): behavior must be identical to
/// `analyze_with_progress`, and no timing is recorded anywhere to observe.
#[test]
fn analyze_with_progress_matches_timing_variant_when_timing_is_none() {
    let path = fixture_path("simple.ts");

    let reports_a = analyze_with_progress(&path, opts(), None, None).expect("analyze (plain)");
    let reports_b = analyze_with_progress_and_timing(&path, opts(), None, None, None)
        .expect("analyze (timing variant, no collector)");

    assert_eq!(reports_a.len(), reports_b.len());
    for (a, b) in reports_a.iter().zip(reports_b.iter()) {
        assert_eq!(a.lrs, b.lrs);
        assert_eq!(a.file, b.file);
    }
}

/// Flag present: the resulting `TimingReport` is well-formed — every expected
/// phase is present exactly once, all durations are non-negative, and the
/// files/functions processed counters reflect real work.
#[test]
fn timing_report_structure_is_well_formed_when_requested() {
    let path = fixture_path("simple.ts");
    let timing = TimingCollector::default();

    let reports = analyze_with_progress_and_timing(&path, opts(), None, None, Some(&timing))
        .expect("analyze with timing collector");
    assert!(
        !reports.is_empty(),
        "fixture should yield at least one function report"
    );

    let report: TimingReport = timing.report();

    let expected_phases = [
        "discover",
        "parse",
        "cfg",
        "metrics",
        "scoring",
        "aggregate",
        "report",
    ];
    assert_eq!(report.phases.len(), expected_phases.len());
    for name in expected_phases {
        let phase = report
            .phases
            .iter()
            .find(|p| p.phase == name)
            .unwrap_or_else(|| panic!("missing phase '{name}' in timing report"));
        assert!(
            phase.wall_ms >= 0.0,
            "phase '{name}' has a negative duration: {}",
            phase.wall_ms
        );
    }

    // `report` is a CLI-side phase (final rendering); hotspots-core itself
    // never records into it, so it stays at zero here.
    let report_phase = report.phases.iter().find(|p| p.phase == "report").unwrap();
    assert_eq!(report_phase.wall_ms, 0.0);

    assert!(report.files_processed >= 1);
    assert!(report.functions_processed >= 1);
}

/// Phase durations won't sum exactly to wall-clock time (rayon parallelism),
/// but core phases should be sane: non-negative and not wildly exceeding a
/// generous bound for a single small fixture file.
#[test]
fn timing_report_core_phase_durations_are_sane() {
    let path = fixture_path("simple.ts");
    let timing = TimingCollector::default();

    analyze_with_progress_and_timing(&path, opts(), None, None, Some(&timing))
        .expect("analyze with timing collector");

    let report = timing.report();
    let core_total_ms: f64 = report
        .phases
        .iter()
        .filter(|p| p.phase != "report")
        .map(|p| p.wall_ms)
        .sum();

    assert!(core_total_ms >= 0.0);
    assert!(
        core_total_ms < 30_000.0,
        "core phase total ({core_total_ms}ms) is implausibly large for a single small fixture file"
    );
}
