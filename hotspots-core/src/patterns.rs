//! Pattern classification engine
//!
//! Pure, stateless pattern detection from function metrics.
//! No I/O. Same inputs always produce the same outputs.
//! See `docs/patterns.md` for the canonical specification.

use serde::{Deserialize, Serialize};

/// Input for Tier 1 (structural) pattern classification.
/// Available in all analysis modes from raw metrics.
pub struct Tier1Input {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
    pub ns: usize,
    pub loc: usize,
}

/// Input for Tier 2 (enriched) pattern classification.
/// All fields are `Option` — absent outside snapshot mode.
pub struct Tier2Input {
    pub fan_in: Option<usize>,
    pub scc_size: Option<usize>,
    pub churn_lines: Option<usize>,
    pub days_since_last_change: Option<u32>,
    pub neighbor_churn: Option<usize>,
    /// Suppresses `middle_man` and `neighbor_risk` when true.
    /// Set from call graph entry point detection.
    pub is_entrypoint: bool,
}

/// Default thresholds for all patterns. Values match `docs/patterns.md`.
///
/// Pass `&Thresholds::default()` unless the project has configured overrides
/// via `.hotspotsrc.json`. The `classify` functions accept this by reference
/// so the type signature accommodates overrides without any API change.
#[derive(Debug, Clone)]
pub struct Thresholds {
    pub complex_branching_cc: usize,
    pub complex_branching_nd: usize,
    pub deeply_nested_nd: usize,
    pub exit_heavy_ns: usize,
    pub god_function_loc: usize,
    pub god_function_fo: usize,
    pub long_function_loc: usize,
    pub churn_magnet_churn: usize,
    pub churn_magnet_cc: usize,
    pub cyclic_hub_scc: usize,
    pub cyclic_hub_fan_in: usize,
    pub hub_function_fan_in: usize,
    pub hub_function_cc: usize,
    pub middle_man_fan_in: usize,
    pub middle_man_fo: usize,
    pub middle_man_cc_max: usize,
    pub neighbor_risk_churn: usize,
    pub neighbor_risk_fo: usize,
    pub shotgun_target_fan_in: usize,
    pub shotgun_target_churn: usize,
    pub stale_complex_cc: usize,
    pub stale_complex_loc: usize,
    pub stale_complex_days: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            complex_branching_cc: 10,
            complex_branching_nd: 4,
            deeply_nested_nd: 5,
            exit_heavy_ns: 5,
            god_function_loc: 60,
            god_function_fo: 10,
            long_function_loc: 80,
            churn_magnet_churn: 200,
            churn_magnet_cc: 8,
            cyclic_hub_scc: 2,
            cyclic_hub_fan_in: 6,
            hub_function_fan_in: 10,
            hub_function_cc: 8,
            middle_man_fan_in: 8,
            middle_man_fo: 8,
            middle_man_cc_max: 4,
            neighbor_risk_churn: 400,
            neighbor_risk_fo: 8,
            shotgun_target_fan_in: 8,
            shotgun_target_churn: 150,
            stale_complex_cc: 10,
            stale_complex_loc: 60,
            stale_complex_days: 180,
        }
    }
}

/// A single metric condition that caused a pattern to fire.
/// Populated only when `--explain-patterns` is requested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggeredBy {
    pub metric: String,
    pub op: String,
    pub value: usize,
    pub threshold: usize,
}

/// Full detail for a single fired pattern.
/// Populated only when `--explain-patterns` is requested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternDetail {
    pub id: String,
    /// 1 = Tier 1 (structural), 2 = Tier 2 (enriched).
    pub tier: u8,
    /// "primitive" or "derived".
    pub kind: String,
    pub triggered_by: Vec<TriggeredBy>,
}

/// Classify patterns and return sorted IDs.
///
/// Ordering: Tier 1 patterns alphabetically, then Tier 2 alphabetically.
/// Delegates entirely to `classify_detailed` — no separate threshold logic.
pub fn classify(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Vec<String> {
    classify_detailed(t1, t2, th)
        .into_iter()
        .map(|d| d.id)
        .collect()
}

/// Classify patterns and return full detail for each.
///
/// This is the canonical implementation. `classify` delegates here.
/// Use this when `--explain-patterns` is active.
pub fn classify_detailed(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Vec<PatternDetail> {
    let mut results = Vec::new();

    // Compute primitives needed for derived pattern check up-front.
    let god = check_god_function(t1, th);
    let churn = check_churn_magnet(t1, t2, th);

    // Tier 1 — alphabetical
    if let Some(d) = check_complex_branching(t1, th) {
        results.push(d);
    }
    if let Some(d) = check_deeply_nested(t1, th) {
        results.push(d);
    }
    if let Some(d) = check_exit_heavy(t1, th) {
        results.push(d);
    }
    if let Some(d) = god.clone() {
        results.push(d);
    }
    if let Some(d) = check_long_function(t1, th) {
        results.push(d);
    }

    // Tier 2 — alphabetical
    if let Some(d) = churn.clone() {
        results.push(d);
    }
    if let Some(d) = check_cyclic_hub(t2, th) {
        results.push(d);
    }
    if let Some(d) = check_hub_function(t1, t2, th) {
        results.push(d);
    }
    if let Some(d) = check_middle_man(t1, t2, th) {
        results.push(d);
    }
    if let Some(d) = check_neighbor_risk(t1, t2, th) {
        results.push(d);
    }
    if let Some(d) = check_shotgun_target(t2, th) {
        results.push(d);
    }
    if let Some(d) = check_stale_complex(t1, t2, th) {
        results.push(d);
    }

    // volatile_god: derived — fires iff both god_function AND churn_magnet fired.
    // triggered_by is the union of both; no raw thresholds re-evaluated here.
    if let (Some(ref g), Some(ref c)) = (&god, &churn) {
        let mut triggered_by = g.triggered_by.clone();
        triggered_by.extend(c.triggered_by.clone());
        results.push(PatternDetail {
            id: "volatile_god".to_string(),
            tier: 2,
            kind: "derived".to_string(),
            triggered_by,
        });
    }

    results
}

// ---------- Tier 1 helpers ----------

fn check_complex_branching(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail> {
    if t.cc >= th.complex_branching_cc && t.nd >= th.complex_branching_nd {
        Some(PatternDetail {
            id: "complex_branching".to_string(),
            tier: 1,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("CC", ">=", t.cc, th.complex_branching_cc),
                tb("ND", ">=", t.nd, th.complex_branching_nd),
            ],
        })
    } else {
        None
    }
}

fn check_deeply_nested(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail> {
    if t.nd >= th.deeply_nested_nd {
        Some(PatternDetail {
            id: "deeply_nested".to_string(),
            tier: 1,
            kind: "primitive".to_string(),
            triggered_by: vec![tb("ND", ">=", t.nd, th.deeply_nested_nd)],
        })
    } else {
        None
    }
}

fn check_exit_heavy(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail> {
    if t.ns >= th.exit_heavy_ns {
        Some(PatternDetail {
            id: "exit_heavy".to_string(),
            tier: 1,
            kind: "primitive".to_string(),
            triggered_by: vec![tb("NS", ">=", t.ns, th.exit_heavy_ns)],
        })
    } else {
        None
    }
}

fn check_god_function(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail> {
    if t.loc >= th.god_function_loc && t.fo >= th.god_function_fo {
        Some(PatternDetail {
            id: "god_function".to_string(),
            tier: 1,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("LOC", ">=", t.loc, th.god_function_loc),
                tb("FO", ">=", t.fo, th.god_function_fo),
            ],
        })
    } else {
        None
    }
}

fn check_long_function(t: &Tier1Input, th: &Thresholds) -> Option<PatternDetail> {
    if t.loc >= th.long_function_loc {
        Some(PatternDetail {
            id: "long_function".to_string(),
            tier: 1,
            kind: "primitive".to_string(),
            triggered_by: vec![tb("LOC", ">=", t.loc, th.long_function_loc)],
        })
    } else {
        None
    }
}

// ---------- Tier 2 helpers ----------

fn check_churn_magnet(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    let churn = t2.churn_lines?;
    if churn >= th.churn_magnet_churn && t1.cc >= th.churn_magnet_cc {
        Some(PatternDetail {
            id: "churn_magnet".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("churn_lines", ">=", churn, th.churn_magnet_churn),
                tb("CC", ">=", t1.cc, th.churn_magnet_cc),
            ],
        })
    } else {
        None
    }
}

fn check_cyclic_hub(t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    let scc = t2.scc_size?;
    let fan_in = t2.fan_in?;
    if scc >= th.cyclic_hub_scc && fan_in >= th.cyclic_hub_fan_in {
        Some(PatternDetail {
            id: "cyclic_hub".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("scc_size", ">=", scc, th.cyclic_hub_scc),
                tb("fan_in", ">=", fan_in, th.cyclic_hub_fan_in),
            ],
        })
    } else {
        None
    }
}

fn check_hub_function(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    let fan_in = t2.fan_in?;
    if fan_in >= th.hub_function_fan_in && t1.cc >= th.hub_function_cc {
        Some(PatternDetail {
            id: "hub_function".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("fan_in", ">=", fan_in, th.hub_function_fan_in),
                tb("CC", ">=", t1.cc, th.hub_function_cc),
            ],
        })
    } else {
        None
    }
}

fn check_middle_man(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    if t2.is_entrypoint {
        return None;
    }
    let fan_in = t2.fan_in?;
    if fan_in >= th.middle_man_fan_in && t1.fo >= th.middle_man_fo && t1.cc <= th.middle_man_cc_max
    {
        Some(PatternDetail {
            id: "middle_man".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("fan_in", ">=", fan_in, th.middle_man_fan_in),
                tb("FO", ">=", t1.fo, th.middle_man_fo),
                tb("CC", "<=", t1.cc, th.middle_man_cc_max),
            ],
        })
    } else {
        None
    }
}

fn check_neighbor_risk(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    if t2.is_entrypoint {
        return None;
    }
    let nc = t2.neighbor_churn?;
    if nc >= th.neighbor_risk_churn && t1.fo >= th.neighbor_risk_fo {
        Some(PatternDetail {
            id: "neighbor_risk".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("neighbor_churn", ">=", nc, th.neighbor_risk_churn),
                tb("FO", ">=", t1.fo, th.neighbor_risk_fo),
            ],
        })
    } else {
        None
    }
}

fn check_shotgun_target(t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    let fan_in = t2.fan_in?;
    let churn = t2.churn_lines?;
    if fan_in >= th.shotgun_target_fan_in && churn >= th.shotgun_target_churn {
        Some(PatternDetail {
            id: "shotgun_target".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("fan_in", ">=", fan_in, th.shotgun_target_fan_in),
                tb("churn_lines", ">=", churn, th.shotgun_target_churn),
            ],
        })
    } else {
        None
    }
}

fn check_stale_complex(t1: &Tier1Input, t2: &Tier2Input, th: &Thresholds) -> Option<PatternDetail> {
    let days = t2.days_since_last_change?;
    if t1.cc >= th.stale_complex_cc
        && t1.loc >= th.stale_complex_loc
        && days >= th.stale_complex_days
    {
        Some(PatternDetail {
            id: "stale_complex".to_string(),
            tier: 2,
            kind: "primitive".to_string(),
            triggered_by: vec![
                tb("CC", ">=", t1.cc, th.stale_complex_cc),
                tb("LOC", ">=", t1.loc, th.stale_complex_loc),
                tb(
                    "days_since_last_change",
                    ">=",
                    days as usize,
                    th.stale_complex_days as usize,
                ),
            ],
        })
    } else {
        None
    }
}

/// Helper: construct a `TriggeredBy` record.
fn tb(metric: &str, op: &str, value: usize, threshold: usize) -> TriggeredBy {
    TriggeredBy {
        metric: metric.to_string(),
        op: op.to_string(),
        value,
        threshold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t1(cc: usize, nd: usize, fo: usize, ns: usize, loc: usize) -> Tier1Input {
        Tier1Input {
            cc,
            nd,
            fo,
            ns,
            loc,
        }
    }

    fn t2_none() -> Tier2Input {
        Tier2Input {
            fan_in: None,
            scc_size: None,
            churn_lines: None,
            days_since_last_change: None,
            neighbor_churn: None,
            is_entrypoint: false,
        }
    }

    fn t2(
        fan_in: usize,
        scc_size: usize,
        churn_lines: usize,
        days: u32,
        neighbor_churn: usize,
    ) -> Tier2Input {
        Tier2Input {
            fan_in: Some(fan_in),
            scc_size: Some(scc_size),
            churn_lines: Some(churn_lines),
            days_since_last_change: Some(days),
            neighbor_churn: Some(neighbor_churn),
            is_entrypoint: false,
        }
    }

    fn has(patterns: &[String], id: &str) -> bool {
        patterns.iter().any(|p| p == id)
    }

    fn th() -> Thresholds {
        Thresholds::default()
    }

    // ---------- complex_branching ----------

    #[test]
    fn complex_branching_below_threshold() {
        let p = classify(&t1(9, 4, 0, 0, 0), &t2_none(), &th());
        assert!(!has(&p, "complex_branching"));
        let p = classify(&t1(10, 3, 0, 0, 0), &t2_none(), &th());
        assert!(!has(&p, "complex_branching"));
    }

    #[test]
    fn complex_branching_at_threshold() {
        let p = classify(&t1(10, 4, 0, 0, 0), &t2_none(), &th());
        assert!(has(&p, "complex_branching"));
    }

    #[test]
    fn complex_branching_above_threshold() {
        let p = classify(&t1(20, 8, 0, 0, 0), &t2_none(), &th());
        assert!(has(&p, "complex_branching"));
    }

    // ---------- deeply_nested ----------

    #[test]
    fn deeply_nested_below_threshold() {
        let p = classify(&t1(0, 4, 0, 0, 0), &t2_none(), &th());
        assert!(!has(&p, "deeply_nested"));
    }

    #[test]
    fn deeply_nested_at_threshold() {
        let p = classify(&t1(0, 5, 0, 0, 0), &t2_none(), &th());
        assert!(has(&p, "deeply_nested"));
    }

    #[test]
    fn deeply_nested_above_threshold() {
        let p = classify(&t1(0, 10, 0, 0, 0), &t2_none(), &th());
        assert!(has(&p, "deeply_nested"));
    }

    // ---------- exit_heavy ----------

    #[test]
    fn exit_heavy_below_threshold() {
        let p = classify(&t1(0, 0, 0, 4, 0), &t2_none(), &th());
        assert!(!has(&p, "exit_heavy"));
    }

    #[test]
    fn exit_heavy_at_threshold() {
        let p = classify(&t1(0, 0, 0, 5, 0), &t2_none(), &th());
        assert!(has(&p, "exit_heavy"));
    }

    #[test]
    fn exit_heavy_above_threshold() {
        let p = classify(&t1(0, 0, 0, 10, 0), &t2_none(), &th());
        assert!(has(&p, "exit_heavy"));
    }

    // ---------- god_function ----------

    #[test]
    fn god_function_below_threshold() {
        // LOC below
        let p = classify(&t1(0, 0, 10, 0, 59), &t2_none(), &th());
        assert!(!has(&p, "god_function"));
        // FO below
        let p = classify(&t1(0, 0, 9, 0, 60), &t2_none(), &th());
        assert!(!has(&p, "god_function"));
    }

    #[test]
    fn god_function_at_threshold() {
        let p = classify(&t1(0, 0, 10, 0, 60), &t2_none(), &th());
        assert!(has(&p, "god_function"));
    }

    #[test]
    fn god_function_above_threshold() {
        let p = classify(&t1(0, 0, 20, 0, 120), &t2_none(), &th());
        assert!(has(&p, "god_function"));
    }

    // ---------- long_function ----------

    #[test]
    fn long_function_below_threshold() {
        let p = classify(&t1(0, 0, 0, 0, 79), &t2_none(), &th());
        assert!(!has(&p, "long_function"));
    }

    #[test]
    fn long_function_at_threshold() {
        let p = classify(&t1(0, 0, 0, 0, 80), &t2_none(), &th());
        assert!(has(&p, "long_function"));
    }

    #[test]
    fn long_function_above_threshold() {
        let p = classify(&t1(0, 0, 0, 0, 200), &t2_none(), &th());
        assert!(has(&p, "long_function"));
    }

    // ---------- churn_magnet ----------

    #[test]
    fn churn_magnet_below_threshold() {
        // churn below
        let t = Tier2Input {
            churn_lines: Some(199),
            ..t2(0, 1, 0, 0, 0)
        };
        let p = classify(&t1(8, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "churn_magnet"));
        // cc below
        let t = Tier2Input {
            churn_lines: Some(200),
            ..t2(0, 1, 0, 0, 0)
        };
        let p = classify(&t1(7, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "churn_magnet"));
    }

    #[test]
    fn churn_magnet_at_threshold() {
        let t = Tier2Input {
            churn_lines: Some(200),
            ..t2(0, 1, 0, 0, 0)
        };
        let p = classify(&t1(8, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "churn_magnet"));
    }

    #[test]
    fn churn_magnet_above_threshold() {
        let t = Tier2Input {
            churn_lines: Some(500),
            ..t2(0, 1, 0, 0, 0)
        };
        let p = classify(&t1(15, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "churn_magnet"));
    }

    // ---------- cyclic_hub ----------

    #[test]
    fn cyclic_hub_below_threshold() {
        // scc below
        let t = Tier2Input {
            scc_size: Some(1),
            fan_in: Some(6),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "cyclic_hub"));
        // fan_in below
        let t = Tier2Input {
            scc_size: Some(2),
            fan_in: Some(5),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "cyclic_hub"));
    }

    #[test]
    fn cyclic_hub_at_threshold() {
        let t = Tier2Input {
            scc_size: Some(2),
            fan_in: Some(6),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "cyclic_hub"));
    }

    #[test]
    fn cyclic_hub_above_threshold() {
        let t = Tier2Input {
            scc_size: Some(5),
            fan_in: Some(20),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "cyclic_hub"));
    }

    // ---------- hub_function ----------

    #[test]
    fn hub_function_below_threshold() {
        // fan_in below
        let t = Tier2Input {
            fan_in: Some(9),
            ..t2_none()
        };
        let p = classify(&t1(8, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "hub_function"));
        // cc below
        let t = Tier2Input {
            fan_in: Some(10),
            ..t2_none()
        };
        let p = classify(&t1(7, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "hub_function"));
    }

    #[test]
    fn hub_function_at_threshold() {
        let t = Tier2Input {
            fan_in: Some(10),
            ..t2_none()
        };
        let p = classify(&t1(8, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "hub_function"));
    }

    #[test]
    fn hub_function_above_threshold() {
        let t = Tier2Input {
            fan_in: Some(25),
            ..t2_none()
        };
        let p = classify(&t1(20, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "hub_function"));
    }

    // ---------- middle_man ----------

    #[test]
    fn middle_man_below_threshold() {
        // fan_in below
        let t = Tier2Input {
            fan_in: Some(7),
            ..t2_none()
        };
        let p = classify(&t1(2, 0, 8, 0, 0), &t, &th());
        assert!(!has(&p, "middle_man"));
        // fo below
        let t = Tier2Input {
            fan_in: Some(8),
            ..t2_none()
        };
        let p = classify(&t1(2, 0, 7, 0, 0), &t, &th());
        assert!(!has(&p, "middle_man"));
        // cc above max
        let t = Tier2Input {
            fan_in: Some(8),
            ..t2_none()
        };
        let p = classify(&t1(5, 0, 8, 0, 0), &t, &th());
        assert!(!has(&p, "middle_man"));
    }

    #[test]
    fn middle_man_at_threshold() {
        let t = Tier2Input {
            fan_in: Some(8),
            ..t2_none()
        };
        let p = classify(&t1(4, 0, 8, 0, 0), &t, &th());
        assert!(has(&p, "middle_man"));
    }

    #[test]
    fn middle_man_above_threshold() {
        let t = Tier2Input {
            fan_in: Some(20),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 15, 0, 0), &t, &th());
        assert!(has(&p, "middle_man"));
    }

    // ---------- neighbor_risk ----------

    #[test]
    fn neighbor_risk_below_threshold() {
        // churn below
        let t = Tier2Input {
            neighbor_churn: Some(399),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 8, 0, 0), &t, &th());
        assert!(!has(&p, "neighbor_risk"));
        // fo below
        let t = Tier2Input {
            neighbor_churn: Some(400),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 7, 0, 0), &t, &th());
        assert!(!has(&p, "neighbor_risk"));
    }

    #[test]
    fn neighbor_risk_at_threshold() {
        let t = Tier2Input {
            neighbor_churn: Some(400),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 8, 0, 0), &t, &th());
        assert!(has(&p, "neighbor_risk"));
    }

    #[test]
    fn neighbor_risk_above_threshold() {
        let t = Tier2Input {
            neighbor_churn: Some(1000),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 20, 0, 0), &t, &th());
        assert!(has(&p, "neighbor_risk"));
    }

    // ---------- shotgun_target ----------

    #[test]
    fn shotgun_target_below_threshold() {
        // fan_in below
        let t = Tier2Input {
            fan_in: Some(7),
            churn_lines: Some(150),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "shotgun_target"));
        // churn below
        let t = Tier2Input {
            fan_in: Some(8),
            churn_lines: Some(149),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(!has(&p, "shotgun_target"));
    }

    #[test]
    fn shotgun_target_at_threshold() {
        let t = Tier2Input {
            fan_in: Some(8),
            churn_lines: Some(150),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "shotgun_target"));
    }

    #[test]
    fn shotgun_target_above_threshold() {
        let t = Tier2Input {
            fan_in: Some(30),
            churn_lines: Some(500),
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 0, 0, 0), &t, &th());
        assert!(has(&p, "shotgun_target"));
    }

    // ---------- stale_complex ----------

    #[test]
    fn stale_complex_below_threshold() {
        // cc below
        let t = Tier2Input {
            days_since_last_change: Some(180),
            ..t2_none()
        };
        let p = classify(&t1(9, 0, 0, 0, 60), &t, &th());
        assert!(!has(&p, "stale_complex"));
        // loc below
        let t = Tier2Input {
            days_since_last_change: Some(180),
            ..t2_none()
        };
        let p = classify(&t1(10, 0, 0, 0, 59), &t, &th());
        assert!(!has(&p, "stale_complex"));
        // days below
        let t = Tier2Input {
            days_since_last_change: Some(179),
            ..t2_none()
        };
        let p = classify(&t1(10, 0, 0, 0, 60), &t, &th());
        assert!(!has(&p, "stale_complex"));
    }

    #[test]
    fn stale_complex_at_threshold() {
        let t = Tier2Input {
            days_since_last_change: Some(180),
            ..t2_none()
        };
        let p = classify(&t1(10, 0, 0, 0, 60), &t, &th());
        assert!(has(&p, "stale_complex"));
    }

    #[test]
    fn stale_complex_above_threshold() {
        let t = Tier2Input {
            days_since_last_change: Some(365),
            ..t2_none()
        };
        let p = classify(&t1(20, 0, 0, 0, 200), &t, &th());
        assert!(has(&p, "stale_complex"));
    }

    // ---------- volatile_god (derived) ----------

    #[test]
    fn volatile_god_only_god_does_not_fire() {
        // god_function fires, churn_magnet does not
        let t = Tier2Input {
            churn_lines: Some(100),
            ..t2_none()
        }; // churn < 200
        let p = classify(&t1(8, 0, 10, 0, 60), &t, &th());
        assert!(has(&p, "god_function"));
        assert!(!has(&p, "volatile_god"));
    }

    #[test]
    fn volatile_god_only_churn_does_not_fire() {
        // churn_magnet fires, god_function does not
        let t = Tier2Input {
            churn_lines: Some(200),
            ..t2_none()
        };
        let p = classify(&t1(8, 0, 5, 0, 30), &t, &th()); // loc=30, fo=5 — god doesn't fire
        assert!(has(&p, "churn_magnet"));
        assert!(!has(&p, "volatile_god"));
    }

    #[test]
    fn volatile_god_both_fire() {
        let t = Tier2Input {
            churn_lines: Some(200),
            ..t2_none()
        };
        let p = classify(&t1(8, 0, 10, 0, 60), &t, &th());
        assert!(has(&p, "god_function"));
        assert!(has(&p, "churn_magnet"));
        assert!(has(&p, "volatile_god"));
    }

    // ---------- entrypoint suppression ----------

    #[test]
    fn middle_man_suppressed_for_entrypoint() {
        let base = Tier2Input {
            fan_in: Some(8),
            is_entrypoint: false,
            ..t2_none()
        };
        let p = classify(&t1(4, 0, 8, 0, 0), &base, &th());
        assert!(has(&p, "middle_man"), "should fire when not entrypoint");

        let ep = Tier2Input {
            is_entrypoint: true,
            ..base
        };
        let p = classify(&t1(4, 0, 8, 0, 0), &ep, &th());
        assert!(!has(&p, "middle_man"), "should not fire for entrypoint");
    }

    #[test]
    fn neighbor_risk_suppressed_for_entrypoint() {
        let base = Tier2Input {
            neighbor_churn: Some(400),
            is_entrypoint: false,
            ..t2_none()
        };
        let p = classify(&t1(0, 0, 8, 0, 0), &base, &th());
        assert!(has(&p, "neighbor_risk"), "should fire when not entrypoint");

        let ep = Tier2Input {
            is_entrypoint: true,
            ..base
        };
        let p = classify(&t1(0, 0, 8, 0, 0), &ep, &th());
        assert!(!has(&p, "neighbor_risk"), "should not fire for entrypoint");
    }

    // ---------- ordering ----------

    #[test]
    fn all_tier1_ordering() {
        // Triggers: complex_branching (cc=10,nd=5), deeply_nested (nd=5),
        //           exit_heavy (ns=5), god_function (loc=80,fo=10), long_function (loc=80)
        let p = classify(&t1(10, 5, 10, 5, 80), &t2_none(), &th());
        assert_eq!(
            p,
            vec![
                "complex_branching",
                "deeply_nested",
                "exit_heavy",
                "god_function",
                "long_function",
            ]
        );
    }

    // ---------- classify_detailed ----------

    #[test]
    fn classify_detailed_god_function() {
        let details = classify_detailed(&t1(0, 0, 12, 0, 85), &t2_none(), &th());
        let god = details.iter().find(|d| d.id == "god_function").unwrap();
        assert_eq!(god.tier, 1);
        assert_eq!(god.kind, "primitive");
        assert_eq!(god.triggered_by.len(), 2);

        let loc_tb = god.triggered_by.iter().find(|t| t.metric == "LOC").unwrap();
        assert_eq!(loc_tb.value, 85);
        assert_eq!(loc_tb.threshold, 60);
        assert_eq!(loc_tb.op, ">=");

        let fo_tb = god.triggered_by.iter().find(|t| t.metric == "FO").unwrap();
        assert_eq!(fo_tb.value, 12);
        assert_eq!(fo_tb.threshold, 10);
    }

    #[test]
    fn classify_delegates_to_classify_detailed() {
        // classify() and classify_detailed() must agree on which IDs fire
        let t1_in = t1(10, 5, 10, 5, 80);
        let t2_in = t2_none();
        let ids = classify(&t1_in, &t2_in, &th());
        let detail_ids: Vec<String> = classify_detailed(&t1_in, &t2_in, &th())
            .into_iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(ids, detail_ids);
    }
}
