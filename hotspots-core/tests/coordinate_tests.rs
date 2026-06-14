//! Integration tests for `hotspots coordinate`
//!
//! Uses real temp git repos with scripted commit histories so assertions
//! are against deterministic co-change signals, not heuristics.

use hotspots_core::coordinate::{coordinate, partition_pairs, SERIALIZE_THRESHOLD};
use hotspots_core::git::CoChangePair;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

// ── helpers ──────────────────────────────────────────────────────────────────

fn create_temp_git_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let p = tmp.path();
    git(p, &["init", "--initial-branch=main"]);
    git(p, &["config", "user.name", "Test User"]);
    git(p, &["config", "user.email", "test@example.com"]);
    git(p, &["config", "commit.gpgsign", "false"]);
    tmp
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap_or_else(|_| panic!("git {:?}", args));
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn touch(repo: &Path, path: &str, content: &str) {
    let full = repo.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
}

fn commit(repo: &Path, msg: &str, files: &[&str]) {
    for f in files {
        touch(repo, f, &format!("{}\n{}", f, msg));
    }
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", msg]);
}

fn make_pair(a: &str, b: &str, count: usize, ratio: f64) -> CoChangePair {
    CoChangePair {
        file_a: a.to_string(),
        file_b: b.to_string(),
        co_change_count: count,
        coupling_ratio: ratio,
        risk: if ratio >= 0.5 {
            "high".into()
        } else if ratio >= 0.25 {
            "moderate".into()
        } else {
            "low".into()
        },
        has_static_dep: false,
    }
}

// ── partition_pairs ───────────────────────────────────────────────────────────

#[test]
fn within_pair_both_in_set() {
    let pairs = vec![make_pair("auth.rs", "session.rs", 5, 0.8)];
    let input: HashSet<&str> = ["auth.rs", "session.rs"].into();
    let (within, hidden) = partition_pairs(&pairs, &input);
    assert_eq!(within.len(), 1);
    assert_eq!(within[0].file_a, "auth.rs");
    assert!(hidden.is_empty());
}

#[test]
fn hidden_dep_when_one_file_outside_set() {
    let pairs = vec![make_pair("auth.rs", "token.rs", 3, 0.6)];
    let input: HashSet<&str> = ["auth.rs"].into();
    let (within, hidden) = partition_pairs(&pairs, &input);
    assert!(within.is_empty());
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].input_file, "auth.rs");
    assert_eq!(hidden[0].partner, "token.rs");
}

#[test]
fn hidden_dep_direction_normalised_when_b_is_input() {
    // file_b is in the input set — partner should still be file_a
    let pairs = vec![make_pair("token.rs", "session.rs", 3, 0.6)];
    let input: HashSet<&str> = ["session.rs"].into();
    let (_, hidden) = partition_pairs(&pairs, &input);
    assert_eq!(hidden[0].input_file, "session.rs");
    assert_eq!(hidden[0].partner, "token.rs");
}

#[test]
fn pairs_both_outside_set_are_ignored() {
    let pairs = vec![make_pair("x.rs", "y.rs", 10, 0.9)];
    let input: HashSet<&str> = ["auth.rs"].into();
    let (within, hidden) = partition_pairs(&pairs, &input);
    assert!(within.is_empty());
    assert!(hidden.is_empty());
}

#[test]
fn within_pairs_sorted_by_coupling_ratio_descending() {
    let pairs = vec![
        make_pair("a.rs", "b.rs", 2, 0.3),
        make_pair("a.rs", "c.rs", 5, 0.9),
        make_pair("b.rs", "c.rs", 3, 0.6),
    ];
    let input: HashSet<&str> = ["a.rs", "b.rs", "c.rs"].into();
    let (within, _) = partition_pairs(&pairs, &input);
    assert_eq!(within.len(), 3);
    assert!(within[0].coupling_ratio >= within[1].coupling_ratio);
    assert!(within[1].coupling_ratio >= within[2].coupling_ratio);
}

#[test]
fn hidden_deps_sorted_by_coupling_ratio_descending() {
    let pairs = vec![
        make_pair("auth.rs", "x.rs", 3, 0.2),
        make_pair("auth.rs", "y.rs", 5, 0.8),
    ];
    let input: HashSet<&str> = ["auth.rs"].into();
    let (_, hidden) = partition_pairs(&pairs, &input);
    assert_eq!(hidden[0].coupling_ratio, 0.8);
    assert_eq!(hidden[1].coupling_ratio, 0.2);
}

// ── serialize / parallel_safe split ──────────────────────────────────────────

#[test]
fn high_coupling_pair_drives_serialize() {
    let pairs = vec![make_pair("auth.rs", "session.rs", 5, SERIALIZE_THRESHOLD)];
    let input: HashSet<&str> = ["auth.rs", "session.rs", "middleware.rs"].into();
    let (within, _) = partition_pairs(&pairs, &input);

    let must_serialize: HashSet<&str> = within
        .iter()
        .filter(|p| p.coupling_ratio >= SERIALIZE_THRESHOLD)
        .flat_map(|p| [p.file_a.as_str(), p.file_b.as_str()])
        .collect();

    assert!(must_serialize.contains("auth.rs"));
    assert!(must_serialize.contains("session.rs"));
    assert!(!must_serialize.contains("middleware.rs"));
}

#[test]
fn low_coupling_pair_is_parallel_safe() {
    let pairs = vec![make_pair("auth.rs", "session.rs", 2, 0.1)];
    let input: HashSet<&str> = ["auth.rs", "session.rs"].into();
    let (within, _) = partition_pairs(&pairs, &input);

    let must_serialize: HashSet<&str> = within
        .iter()
        .filter(|p| p.coupling_ratio >= SERIALIZE_THRESHOLD)
        .flat_map(|p| [p.file_a.as_str(), p.file_b.as_str()])
        .collect();

    assert!(must_serialize.is_empty());
}

// ── coordinate() end-to-end with real git repo ────────────────────────────────

#[test]
fn coordinate_surfaces_within_pair_from_real_git_history() {
    let repo = create_temp_git_repo();
    let root = repo.path();

    // auth.rs and session.rs co-change 3 times — enough to exceed MIN_COUNT=2
    for i in 0..3 {
        commit(
            root,
            &format!("feat: change {}", i),
            &["auth.rs", "session.rs"],
        );
    }
    // middleware.rs changes alone — no co-change relationship
    commit(root, "chore: standalone middleware", &["middleware.rs"]);

    let files = vec![
        "auth.rs".to_string(),
        "session.rs".to_string(),
        "middleware.rs".to_string(),
    ];
    let report = coordinate(root, &files).expect("coordinate failed");

    let within_pair = report
        .pairs
        .iter()
        .find(|p| {
            (p.file_a == "auth.rs" && p.file_b == "session.rs")
                || (p.file_a == "session.rs" && p.file_b == "auth.rs")
        })
        .expect("auth.rs ↔ session.rs pair should appear");

    assert!(within_pair.co_change_count >= 3);
    assert!(within_pair.coupling_ratio > 0.0);
}

#[test]
fn coordinate_surfaces_hidden_dep() {
    let repo = create_temp_git_repo();
    let root = repo.path();

    // auth.rs and token.rs co-change — token.rs is NOT in the input set
    for i in 0..3 {
        commit(
            root,
            &format!("feat: auth+token {}", i),
            &["auth.rs", "token.rs"],
        );
    }
    commit(root, "feat: auth alone", &["auth.rs"]);

    let files = vec!["auth.rs".to_string()];
    let report = coordinate(root, &files).expect("coordinate failed");

    let hidden = report
        .hidden_dependencies
        .iter()
        .find(|h| h.input_file == "auth.rs" && h.partner == "token.rs")
        .expect("token.rs should appear as hidden dep for auth.rs");

    assert!(hidden.co_change_count >= 3);
}

#[test]
fn coordinate_parallel_safe_excludes_high_coupling_files() {
    let repo = create_temp_git_repo();
    let root = repo.path();

    // Make auth.rs and session.rs co-change many times so coupling_ratio is high
    for i in 0..10 {
        commit(root, &format!("feat: {}", i), &["auth.rs", "session.rs"]);
    }
    // middleware.rs only changes alone — low/no coupling
    for i in 0..5 {
        commit(root, &format!("chore: mw {}", i), &["middleware.rs"]);
    }

    let files = vec![
        "auth.rs".to_string(),
        "session.rs".to_string(),
        "middleware.rs".to_string(),
    ];
    let report = coordinate(root, &files).expect("coordinate failed");

    // middleware should be parallel-safe; auth+session should serialize
    assert!(
        report.parallel_safe.contains(&"middleware.rs".to_string()),
        "middleware.rs should be parallel_safe"
    );
    assert!(
        report.serialize.contains(&"auth.rs".to_string())
            || report.serialize.contains(&"session.rs".to_string()),
        "auth.rs or session.rs should be in serialize"
    );
    assert!(
        !report.parallel_safe.contains(&"auth.rs".to_string())
            || !report.parallel_safe.contains(&"session.rs".to_string()),
        "auth.rs and session.rs should not both be parallel_safe"
    );
}

#[test]
fn coordinate_empty_file_list_returns_empty_report() {
    let repo = create_temp_git_repo();
    commit(repo.path(), "init", &["README.md"]);

    let report = coordinate(repo.path(), &[]).expect("coordinate failed");
    assert!(report.pairs.is_empty());
    assert!(report.hidden_dependencies.is_empty());
    assert!(report.ownership.is_empty());
    assert!(report.parallel_safe.is_empty());
    assert!(report.serialize.is_empty());
}

#[test]
fn coordinate_file_not_in_history_has_zero_ownership() {
    let repo = create_temp_git_repo();
    commit(repo.path(), "init", &["other.rs"]);

    let files = vec!["nonexistent.rs".to_string()];
    let report = coordinate(repo.path(), &files).expect("coordinate failed");

    let o = &report.ownership[0];
    assert_eq!(o.author_count, 0);
    assert_eq!(o.top_author_pct, 0.0);
}
