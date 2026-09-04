//! Multi-axis hotspot ranking — F05 (independence hypothesis).
//!
//! Risk, Coupling, and Ownership axes rank files by different signals with
//! low pairwise correlation (F05: coupling |r|=0.075, ownership |r|=0.104 vs
//! defect risk). Each axis is ranked independently — do not blend them into
//! a composite score, see `docs/promotion-briefs/f05-multi-axis-report.md`.

use crate::snapshot::FunctionSnapshot;
use serde::Serialize;

/// A hotspot ranking axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotspotAxis {
    /// Defect risk, ranked by `activity_risk.unwrap_or(lrs)` (the CLI's
    /// `hotspots_score`).
    Risk,
    /// Co-change coupling, ranked by `directed_coupling`. Files with
    /// `directed_coupling == 0` (or absent) are excluded.
    Coupling,
    /// Ownership churn, ranked by `newcomer_rate`. Files with no commits in
    /// the 90-day newcomer window (`newcomer_rate == None`) are excluded.
    Ownership,
}

/// One ranked entry: a file path and the axis-specific score it was ranked by.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankedFile {
    pub file: String,
    pub score: f64,
}

/// Rank `entries` by `axis`'s signal field, descending, returning at most
/// `top_n` results — one entry per unique file. Entries lacking a defined
/// value for the axis are excluded rather than sorted to the bottom (see
/// `HotspotAxis` docs). Coupling and Ownership scores are file-level
/// (identical across every function in a file), so deduping is a no-op for
/// them; Risk (`activity_risk`/`lrs`) varies per function, so the max score
/// among a file's functions is used to represent the file.
pub fn rank_by_axis(
    entries: &[FunctionSnapshot],
    axis: HotspotAxis,
    top_n: usize,
) -> Vec<RankedFile> {
    let mut by_file: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for f in entries {
        let Some(score) = axis_score(f, axis) else {
            continue;
        };
        by_file
            .entry(f.file.as_str())
            .and_modify(|best| *best = best.max(score))
            .or_insert(score);
    }

    let mut scored: Vec<RankedFile> = by_file
        .into_iter()
        .map(|(file, score)| RankedFile {
            file: file.to_string(),
            score,
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
    });
    scored.truncate(top_n);
    scored
}

fn axis_score(f: &FunctionSnapshot, axis: HotspotAxis) -> Option<f64> {
    match axis {
        HotspotAxis::Risk => Some(f.activity_risk.unwrap_or(f.lrs)),
        HotspotAxis::Coupling => f.directed_coupling.filter(|&dc| dc != 0.0),
        HotspotAxis::Ownership => f.newcomer_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;
    use crate::report::MetricsReport;
    use crate::risk::RiskBand;

    fn fixture(file: &str) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: format!("{file}::f"),
            file: file.to_string(),
            line: 1,
            language: Language::Rust,
            metrics: MetricsReport {
                cc: 1,
                nd: 1,
                fo: 1,
                ns: 1,
                loc: 1,
            },
            lrs: 0.0,
            band: RiskBand::Low,
            suppression_reason: None,
            churn: None,
            touch_count_30d: None,
            days_since_last_change: None,
            callgraph: None,
            activity_risk: None,
            risk_factors: None,
            percentile: None,
            driver: None,
            driver_detail: None,
            quadrant: None,
            patterns: vec![],
            pattern_details: None,
            subsystem: None,
            authors_90d: None,
            directed_coupling: None,
            jaccard_label_stability: None,
            convention_bug_fix_count: None,
            burst_score: None,
            commit_count: None,
            author_count: None,
            author_entropy: None,
            isolation_rate: None,
            age_days: None,
            last_touch_days: None,
            newcomer_rate: None,
            explanation: None,
        }
    }

    #[test]
    fn multi_axis_risk_ordering() {
        let mut entries = vec![
            fixture("a.rs"),
            fixture("b.rs"),
            fixture("c.rs"),
            fixture("d.rs"),
            fixture("e.rs"),
        ];
        entries[0].activity_risk = Some(3.0);
        entries[1].activity_risk = Some(1.0);
        entries[2].activity_risk = Some(5.0);
        entries[3].activity_risk = Some(2.0);
        entries[4].activity_risk = Some(4.0);

        let ranked = rank_by_axis(&entries, HotspotAxis::Risk, 10);
        let files: Vec<&str> = ranked.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(files, vec!["c.rs", "e.rs", "a.rs", "d.rs", "b.rs"]);
    }

    #[test]
    fn multi_axis_coupling_excludes_zeros() {
        let mut entries = vec![fixture("a.rs"), fixture("b.rs"), fixture("c.rs")];
        entries[0].directed_coupling = Some(0.0);
        entries[1].directed_coupling = Some(2.5);
        entries[2].directed_coupling = None;

        let ranked = rank_by_axis(&entries, HotspotAxis::Coupling, 10);
        let files: Vec<&str> = ranked.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(files, vec!["b.rs"]);
    }

    #[test]
    fn multi_axis_ownership_excludes_no_window_data() {
        let mut entries = vec![fixture("a.rs"), fixture("b.rs"), fixture("c.rs")];
        entries[0].newcomer_rate = Some(0.5);
        entries[1].newcomer_rate = None;
        entries[2].newcomer_rate = Some(0.0); // real window data, zero newcomers — must be included

        let ranked = rank_by_axis(&entries, HotspotAxis::Ownership, 10);
        let files: Vec<&str> = ranked.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(files, vec!["a.rs", "c.rs"]);
    }

    #[test]
    fn multi_axis_top_n_respected() {
        let mut entries: Vec<FunctionSnapshot> = (0..20)
            .map(|i| {
                let mut f = fixture(&format!("f{i}.rs"));
                f.activity_risk = Some(i as f64);
                f
            })
            .collect();
        entries.shuffle_deterministic();

        let ranked = rank_by_axis(&entries, HotspotAxis::Risk, 5);
        assert_eq!(ranked.len(), 5);
    }

    /// Deterministic in-place "shuffle" (reverse) so the top-n test doesn't
    /// depend on input already being sorted.
    trait ShuffleDeterministic {
        fn shuffle_deterministic(&mut self);
    }
    impl<T> ShuffleDeterministic for Vec<T> {
        fn shuffle_deterministic(&mut self) {
            self.reverse();
        }
    }
}
