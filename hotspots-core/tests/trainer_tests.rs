//! Integration tests for hotspots-core trainer.
//!
//! Tests that require a real git repository (collect_fix_files,
//! collect_fix_functions, training guard, blame-based labelling).
//! Pure-unit tests (feature names, hunk parsing, nearest_function_above,
//! model versioning) live in trainer.rs itself.

use hotspots_core::language::Language;
use hotspots_core::report::MetricsReport;
use hotspots_core::risk::RiskBand;
use hotspots_core::snapshot::{
    AnalysisInfo, CallGraphMetrics, ChurnMetrics, CommitInfo, FunctionSnapshot, Snapshot,
};
use hotspots_core::trainer::{
    collect_fix_files, collect_fix_functions, extract_features, train, TrainConfig, FEATURE_NAMES,
};
use std::path::Path;
use std::process::Command;

// ── Helpers ───────────────────────────────────────────────────────────────────

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
        },
        functions,
        summary: None,
        aggregates: None,
    }
}

// ── Feature extraction ────────────────────────────────────────────────────────

#[test]
fn extract_features_baseline() {
    let func = make_func("src/foo.py", "foo", 1);
    let feats = extract_features(&func);
    assert_eq!(feats.len(), 8);
    assert_eq!(FEATURE_NAMES.len(), 8);
    // total_churn = 0 when no ChurnMetrics
    assert_eq!(feats[6], 0.0);
    // authors_90d = 0 by default
    assert_eq!(feats[7], 0.0);
}

#[test]
fn extract_features_with_churn_and_callgraph() {
    let mut func = make_func("src/foo.py", "foo", 1);
    func.churn = Some(ChurnMetrics {
        lines_added: 300,
        lines_deleted: 100,
        net_change: 200,
    });
    func.callgraph = Some(CallGraphMetrics {
        fan_in: 5,
        fan_out: 2,
        pagerank: 0.0,
        betweenness: 0.0,
        scc_id: 0,
        scc_size: 1,
        is_entrypoint: false,
        dependency_depth: None,
        neighbor_churn: None,
    });
    func.activity_risk = Some(3.5);

    let feats = extract_features(&func);
    assert_eq!(feats[5], 5.0); // fan_in
    assert_eq!(feats[6], 400.0); // total_churn = 300+100
    assert_eq!(feats[7], 0.0); // authors_90d
}

// ── collect_fix_files ─────────────────────────────────────────────────────────

#[test]
fn collect_fix_files_no_fix_commits() {
    let dir = init_repo();
    commit_file(dir.path(), "readme.txt", "hello", "initial commit");
    let files = collect_fix_files(dir.path(), 365).expect("collect_fix_files");
    assert!(files.is_empty());
}

#[test]
fn collect_fix_files_detects_fix_keyword() {
    let dir = init_repo();
    let p = dir.path();
    // First commit: create the files (Add, not Modify — excluded by --diff-filter=M)
    commit_file(p, "foo.rs", "fn foo() {}", "initial: add files");
    commit_file(p, "bar.rs", "fn bar() {}", "initial: add bar");
    commit_file(p, "baz.rs", "fn baz() {}", "initial: add baz");
    // Second pass: modify each file with semantically distinct commit types
    commit_file(p, "foo.rs", "fn foo() { 1 }", "feat: enhance foo");
    commit_file(
        p,
        "bar.rs",
        "fn bar() { 1 }",
        "fix: resolve null pointer in bar",
    );
    commit_file(p, "baz.rs", "fn baz() { 1 }", "chore: cleanup baz");

    let files = collect_fix_files(p, 365).expect("collect_fix_files");
    assert!(
        files.contains("bar.rs"),
        "fix commit file should be labelled"
    );
    assert!(
        !files.contains("foo.rs"),
        "feature commit should be excluded"
    );
    assert!(!files.contains("baz.rs"), "chore commit should be excluded");
}

#[test]
fn collect_fix_files_all_keywords() {
    let dir = init_repo();
    let p = dir.path();
    // Create files first (Add commits, not tracked by --diff-filter=M)
    for (file, _) in &[
        ("a.rs", ""),
        ("b.rs", ""),
        ("c.rs", ""),
        ("d.rs", ""),
        ("e.rs", ""),
    ] {
        commit_file(p, file, "content", &format!("initial: {}", file));
    }
    // Now modify each with a keyword-bearing fix commit
    for (file, msg) in &[
        ("a.rs", "bug: wrong output"),
        ("b.rs", "patch security hole"),
        ("c.rs", "regression in v2"),
        ("d.rs", "hotfix prod issue"),
        ("e.rs", "defect resolved"),
    ] {
        commit_file(p, file, "updated content", msg);
    }
    let files = collect_fix_files(p, 365).expect("collect_fix_files");
    for f in &["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        assert!(files.contains(*f), "{} should be labelled", f);
    }
}

// ── collect_fix_functions ─────────────────────────────────────────────────────

#[test]
fn collect_fix_functions_labels_correct_function() {
    let dir = init_repo();
    let p = dir.path();

    // Initial commit: file with two functions
    commit_file(
        p,
        "mod.py",
        "def alpha():\n    pass\n\ndef beta():\n    x = 1\n    return x\n",
        "feat: add mod",
    );
    // Fix commit: only beta() changes (line 4 onwards)
    commit_file(
        p,
        "mod.py",
        "def alpha():\n    pass\n\ndef beta():\n    x = 2  # fixed\n    return x\n",
        "fix: correct beta value",
    );

    // Snapshot: alpha at line 1, beta at line 4
    let snapshot = make_snapshot(vec![
        make_func("mod.py", "alpha", 1),
        make_func("mod.py", "beta", 4),
    ]);

    let labelled = collect_fix_functions(&snapshot, p, 365).expect("collect_fix_functions");

    assert!(
        labelled.contains(&("mod.py".to_string(), 4)),
        "beta should be labelled"
    );
    assert!(
        !labelled.contains(&("mod.py".to_string(), 1)),
        "alpha should NOT be labelled"
    );
}

#[test]
fn collect_fix_functions_ignores_non_fix_commits() {
    let dir = init_repo();
    let p = dir.path();

    commit_file(p, "mod.py", "def foo():\n    pass\n", "feat: add foo");
    commit_file(
        p,
        "mod.py",
        "def foo():\n    return 1\n",
        "refactor: simplify",
    );

    let snapshot = make_snapshot(vec![make_func("mod.py", "foo", 1)]);
    let labelled = collect_fix_functions(&snapshot, p, 365).expect("collect_fix_functions");
    assert!(labelled.is_empty());
}

// ── End-to-end model quality ──────────────────────────────────────────────────

/// Verify that a trained model assigns higher scores to functions labelled as
/// buggy than to clean functions.
///
/// Setup:
/// - 3 Python files, 20 functions each = 60 total (passes 50-func guard)
/// - "buggy.py" functions have cc=10 (high complexity)
/// - "clean_a.py" and "clean_b.py" functions have cc=2
/// - Initial commits create all 3 files, then fix commits modify "buggy.py"
///   multiple times — labelling its functions as positive
/// - After training, buggy.py functions should score higher on average
///
/// This is a weak statistical test (not per-function), but with cc=10 vs cc=2
/// and 20 positives vs 40 negatives the RandomForest reliably finds the signal.
#[test]
fn trained_model_ranks_buggy_functions_above_clean() {
    let dir = init_repo();
    let p = dir.path();

    // Generate a Python file with `n` stub functions, each occupying 3 lines.
    fn make_py_file(n: usize) -> String {
        (0..n)
            .map(|i| format!("def func_{i}():\n    x = {i}\n    return x\n\n"))
            .collect()
    }

    // Initial commits: create all three files
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

    // Fix commits: modify buggy.py multiple times to build up fix-commit labels.
    // Each commit changes the file slightly so it qualifies as a modification (M).
    for i in 0..8 {
        let content = format!("{}\n# fix iteration {i}", make_py_file(20));
        commit_file(
            p,
            "buggy.py",
            &content,
            &format!("fix: patch issue #{i} in buggy"),
        );
    }

    // Build snapshot: buggy.py functions get cc=10, clean files get cc=2.
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
    let model = train(&snapshot, p, &cfg)
        .expect("train")
        .expect("model should be returned — enough training signal");

    assert_eq!(model.model_version, 3);

    // Score all functions
    let scores: Vec<(String, f64)> = snapshot
        .functions
        .iter()
        .map(|f| (f.file.clone(), hotspots_core::trainer::score(&model, f)))
        .collect();

    let buggy_mean: f64 = {
        let v: Vec<f64> = scores
            .iter()
            .filter(|(file, _)| file == "buggy.py")
            .map(|(_, s)| *s)
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    };
    let clean_mean: f64 = {
        let v: Vec<f64> = scores
            .iter()
            .filter(|(file, _)| file != "buggy.py")
            .map(|(_, s)| *s)
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    };

    assert!(
        buggy_mean > clean_mean,
        "buggy functions (mean={buggy_mean:.3}) should score higher than clean (mean={clean_mean:.3})"
    );
}

// ── Training guard ────────────────────────────────────────────────────────────

#[test]
fn train_returns_none_below_threshold() {
    let dir = init_repo();
    let p = dir.path();
    commit_file(p, "a.py", "def foo(): pass", "fix: something");

    // Only 3 functions — well below the 50-function minimum
    let snapshot = make_snapshot(vec![
        make_func("a.py", "foo", 1),
        make_func("a.py", "bar", 5),
        make_func("a.py", "baz", 9),
    ]);

    let result = train(&snapshot, p, &TrainConfig::default()).expect("train");
    assert!(result.is_none(), "too few functions → None");
}
