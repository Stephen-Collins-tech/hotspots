//! Scaffolding for issue #182's Part 1 empirical ratchet probe.
//!
//! Plan: `docs/.internal/ratchet-wiring-182-plan.md`. This file is the
//! harness referenced there ("Harness lives in `hotspots-core/tests/`
//! integration test ... one-time diagnostic, not shipped code"). It is
//! currently scaffolded and verified against one locally-trainable example
//! (`small_locally_trained_model_monotonicity`), NOT yet run against the
//! real `benchmarks/` corpus — that requires `hotspots train` runs against
//! real repos, which is a separate, longer-running pass.
//!
//! Decision rule (from the plan): if RandomForest is materially
//! non-monotonic for most sampled vectors/repos, scope the fix to the
//! Ridge-only regime. If RF also shows a dominant monotonic trend (>90%
//! never decrease), treat both model classes the same.

use hotspots_core::language::Language;
use hotspots_core::report::MetricsReport;
use hotspots_core::risk::RiskBand;
use hotspots_core::snapshot::{AnalysisInfo, CommitInfo, FunctionSnapshot, Snapshot};
use hotspots_core::trainer::{extract_features, score, train, ModelClass, TrainConfig};
use std::path::Path;
use std::process::Command;

// ── Helpers (mirrors trainer_tests.rs's pattern) ───────────────────────────────

fn git_cmd(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap_or_else(|_| panic!("git {:?} failed to spawn", args));
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path();
    git_cmd(p, &["init", "--initial-branch=main"]);
    git_cmd(p, &["config", "user.name", "Test"]);
    git_cmd(p, &["config", "user.email", "test@example.com"]);
    git_cmd(p, &["config", "commit.gpgsign", "false"]);
    dir
}

fn commit_file(repo: &Path, filename: &str, content: &str, message: &str) {
    std::fs::write(repo.join(filename), content).expect("write");
    git_cmd(repo, &["add", filename]);
    git_cmd(repo, &["commit", "-m", message]);
}

fn make_func(file: &str, name: &str, line: u32) -> FunctionSnapshot {
    FunctionSnapshot {
        function_id: name.to_string(),
        file: file.to_string(),
        line,
        language: Language::Python,
        metrics: MetricsReport {
            cc: 2,
            nd: 1,
            fo: 0,
            ns: 0,
            loc: 10,
        },
        lrs: 1.0,
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

fn make_snapshot(functions: Vec<FunctionSnapshot>) -> Snapshot {
    Snapshot {
        schema_version: 2,
        commit: CommitInfo {
            sha: "test".to_string(),
            parents: vec![],
            timestamp: 0,
            branch: None,
            message: None,
            author: None,
            is_fix_commit: None,
            is_revert_commit: None,
            ticket_ids: vec![],
        },
        analysis: AnalysisInfo {
            scope: "test".to_string(),
            tool_version: "0.0.0".to_string(),
            formula_version: 1,
        },
        functions,
        summary: None,
        aggregates: None,
    }
}

fn make_py_file(n: usize) -> String {
    (0..n)
        .map(|i| format!("def func_{i}():\n    x = {i}\n    return x\n\n"))
        .collect()
}

// ── Probe primitives ────────────────────────────────────────────────────────────

/// Sweep `total_churn` (feature index 6) across an observed percentile grid,
/// holding every other feature fixed by cloning `base` and mutating only
/// `churn`. Returns the sequence of scores in grid order.
fn sweep_total_churn(
    model: &hotspots_core::trainer::RankerModel,
    base: &FunctionSnapshot,
    grid: &[u32],
) -> Vec<f64> {
    grid.iter()
        .map(|&churn_val| {
            let mut f = base.clone();
            f.churn = Some(hotspots_core::snapshot::ChurnMetrics {
                lines_added: churn_val as usize,
                lines_deleted: 0,
                net_change: churn_val as i64,
            });
            score(model, &f)
        })
        .collect()
}

/// Sweep `convention_bug_fix_count` (feature index 9) across an observed grid.
fn sweep_convention_bug_fix_count(
    model: &hotspots_core::trainer::RankerModel,
    base: &FunctionSnapshot,
    grid: &[u32],
) -> Vec<f64> {
    grid.iter()
        .map(|&count| {
            let mut f = base.clone();
            f.convention_bug_fix_count = Some(count);
            score(model, &f)
        })
        .collect()
}

/// A sequence is "monotonic non-decreasing" if no later value is strictly
/// less than an earlier one (per plan step 3: "does score ever decrease
/// across the sweep").
fn is_monotonic_non_decreasing(scores: &[f64]) -> bool {
    scores.windows(2).all(|w| w[1] + 1e-12 >= w[0])
}

// ── Scaffolding test: verifies the harness against one locally-trainable repo ──
//
// This is NOT the benchmark-corpus run described in the plan — it's a
// smoke test that the probe plumbing (train -> load model -> sweep ->
// monotonicity check) actually works end-to-end against a real trained
// model, so the benchmark-corpus pass can reuse these primitives without
// re-deriving them.
#[test]
fn small_locally_trained_model_monotonicity_probe() {
    let dir = init_repo();
    let p = dir.path();

    commit_file(p, "buggy.py", &make_py_file(20), "feat: add buggy module");
    commit_file(
        p,
        "clean_a.py",
        &make_py_file(20),
        "feat: add clean_a module",
    );
    commit_file(
        p,
        "clean_b.py",
        &make_py_file(20),
        "feat: add clean_b module",
    );

    for i in 0..8 {
        let content = format!("{}\n# fix iteration {i}", make_py_file(20));
        commit_file(
            p,
            "buggy.py",
            &content,
            &format!("fix: patch issue #{i} in buggy"),
        );
    }

    let mut functions = Vec::new();
    for i in 0u32..20 {
        let mut f = make_func("buggy.py", &format!("buggy_func_{i}"), i * 4 + 1);
        f.metrics.cc = 10;
        f.lrs = 5.0;
        functions.push(f);
    }
    for i in 0u32..20 {
        functions.push(make_func(
            "clean_a.py",
            &format!("clean_a_func_{i}"),
            i * 4 + 1,
        ));
    }
    for i in 0u32..20 {
        functions.push(make_func(
            "clean_b.py",
            &format!("clean_b_func_{i}"),
            i * 4 + 1,
        ));
    }
    let snapshot = make_snapshot(functions);

    let cfg = TrainConfig {
        n_estimators: 50,
        ..Default::default()
    };

    let model = train(&snapshot, p, &cfg, None)
        .expect("train")
        .expect("model should be returned — enough training signal");

    // Open question #3 (plan): determining model class/regime is just
    // reading the field back off the loaded model — no extra screening
    // logic is needed in the probe itself.
    eprintln!(
        "model_class={:?} regime_verdict={:?} regime_delta={:.4}",
        model.model_class, model.meta.regime_verdict, model.meta.regime_delta
    );

    // Sanity: feature indices match the plan's assumption.
    let base = snapshot.functions[0].clone();
    let feats = extract_features(&base);
    assert_eq!(feats.len(), 10);

    let churn_grid: Vec<u32> = vec![0, 10, 50, 100, 500, 2000];
    let fix_count_grid: Vec<u32> = vec![0, 1, 2, 5, 10, 25];

    let churn_scores = sweep_total_churn(&model, &base, &churn_grid);
    let fix_count_scores = sweep_convention_bug_fix_count(&model, &base, &fix_count_grid);

    eprintln!("total_churn sweep: {:?} -> {:?}", churn_grid, churn_scores);
    eprintln!(
        "convention_bug_fix_count sweep: {:?} -> {:?}",
        fix_count_grid, fix_count_scores
    );

    // This probe run is diagnostic, not a pass/fail gate: on this one small
    // synthetic repo we only assert the harness *executes* without panicking
    // and produces a same-length score sequence. The actual monotonicity
    // verdict (used to decide Part 3's fix scope) requires running this
    // same sweep across the real `benchmarks/` corpus repos, per repo,
    // across both Ridge and RandomForest regimes — not yet done.
    assert_eq!(churn_scores.len(), churn_grid.len());
    assert_eq!(fix_count_scores.len(), fix_count_grid.len());

    match model.model_class {
        ModelClass::Ridge => {
            // Deterministic ground truth per the plan: a positive Ridge
            // coefficient guarantees monotonic non-decrease (modulo the
            // [0,1] clamp's saturation, which still counts as non-decreasing).
            let ridge = model.ridge.as_ref().expect("ridge present for Ridge class");
            eprintln!(
                "ridge coefficients[6] (total_churn)={:.4} [9] (convention_bug_fix_count)={:.4}",
                ridge.coefficients.get(6).copied().unwrap_or(0.0),
                ridge.coefficients.get(9).copied().unwrap_or(0.0),
            );
        }
        ModelClass::RandomForest => {
            eprintln!(
                "total_churn monotonic_non_decreasing={} convention_bug_fix_count monotonic_non_decreasing={}",
                is_monotonic_non_decreasing(&churn_scores),
                is_monotonic_non_decreasing(&fix_count_scores),
            );
        }
    }
}
