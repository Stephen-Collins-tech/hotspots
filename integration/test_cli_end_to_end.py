from __future__ import annotations

from pathlib import Path

from .utils import Runner, init_test_repo, write_ts, read_json


def test_snapshot_delta(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)

    # Commit 1: simple + moderate
    write_ts(
        repo,
        """
// Low complexity function
function simpleFunction() { return 42; }

// Moderate function
function moderateFunction(x) { return x > 0 ? x * 2 : -x; }
""".strip(),
    )

    # Snapshot JSON (all functions view)
    snap1 = repo / "snap1.json"
    proc = r.run(["analyze", "--mode", "snapshot", "--format", "json", "--all-functions", "src/main.ts"], repo, out=snap1)
    assert proc.returncode == 0
    s1 = read_json(snap1)
    assert "functions" in s1 and isinstance(s1["functions"], list)
    assert s1.get("aggregates") is not None

    # Commit 2: add high complexity function
    write_ts(
        repo,
        (repo / "src" / "main.ts").read_text()
        + """
function highComplexityFunction(x, y, z) {
  if (x>0){ if (y>0){ if (z>0){ return x+y+z;} else { return x+y;} } else { if (z>0){ return x+z;} else { return x; } } } 
  else { if (y>0){ if (z>0){ return y+z;} else { return y;} } else { return z>0 ? z : 0; } }
}
""",
    )

    # Snapshot 2
    snap2 = repo / "snap2.json"
    proc = r.run(["analyze", "--mode", "snapshot", "--format", "json", "--all-functions", "src/main.ts"], repo, out=snap2)
    assert proc.returncode == 0
    s2 = read_json(snap2)
    assert len(s2.get("functions", [])) >= 2

    # Delta + policy JSON
    delta = repo / "delta.json"
    proc = r.run(["analyze", "--mode", "delta", "--policy", "--format", "json", "src/main.ts"], repo, out=delta)
    assert proc.returncode in (0, 1)  # policy may fail with exit 1
    d = read_json(delta)
    delta_section = d["delta"]
    assert "deltas" in delta_section and isinstance(delta_section["deltas"], list)
    assert delta_section.get("aggregates") is not None


def test_snapshot_mode_format_error_does_not_persist(tmp_path: Path):
    # Regression test for #222: `--mode snapshot` without `--format json`/
    # `--explain` must error out (exit 1) *and* leave no durable state behind
    # (no .hotspots/index.json, no .hotspots/snapshots/, no gate_history.json).
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(
        repo,
        """
function simpleFunction() { return 42; }
""".strip(),
    )

    assert not (repo / ".hotspots").exists()

    proc = r.run(["analyze", "--mode", "snapshot", "--force", "src/main.ts"], repo)
    assert proc.returncode == 1
    assert "text format without --explain is not supported" in proc.stderr

    assert not (repo / ".hotspots" / "index.json").exists()
    assert not (repo / ".hotspots" / "snapshots").exists()
    assert not (repo / ".hotspots" / "gate_history.json").exists()



def test_html_output_writes_report_and_json_from_one_run(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(repo, "function simpleFunction() { return 42; }")

    snap = repo / "snap.json"
    report = repo / "out" / "report.html"
    proc = r.run(
        ["analyze", "--mode", "snapshot", "--format", "json", "--no-persist",
         "--html-output", str(report), "src/main.ts"],
        repo,
        out=snap,
    )
    assert proc.returncode == 0

    # JSON on stdout is the normal snapshot: the HTML path must not leak aggregates into it.
    s = read_json(snap)
    assert s["schema_version"] == 6
    assert s.get("aggregates") is None

    html = report.read_text()
    assert "<html" in html.lower()
    assert "simpleFunction" in html
    assert "HTML report written to" in proc.stderr


def test_html_output_rejects_invalid_combinations(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(repo, "function simpleFunction() { return 42; }")
    report = str(repo / "report.html")

    no_mode = r.run(["analyze", "--html-output", report, "src/main.ts"], repo)
    assert no_mode.returncode != 0
    assert "--html-output is only valid with --mode snapshot" in no_mode.stderr

    with_html = r.run(
        ["analyze", "--mode", "snapshot", "--format", "html", "--html-output", report, "src/main.ts"], repo
    )
    assert with_html.returncode != 0
    assert "redundant with --format html" in with_html.stderr
    assert not (repo / "report.html").exists()


def test_non_tty_progress_goes_to_stderr_with_phase_lines(tmp_path: Path):
    # stderr is a pipe here (not a terminal), so analyze should print labelled phase
    # lines with elapsed time, and keep stdout clean for the JSON.
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(repo, "function simpleFunction() { return 42; }")

    snap = repo / "snap.json"
    proc = r.run(["analyze", "--mode", "snapshot", "--format", "json", "--no-persist", "src/main.ts"], repo, out=snap)
    assert proc.returncode == 0
    assert read_json(snap)["schema_version"] == 6  # stdout is still pure JSON

    assert "Analyzing: 0/1 files" in proc.stderr
    assert "[done in" in proc.stderr
    assert "hotspots: reading git history (churn)" in proc.stderr
    assert "hotspots: building call graph" in proc.stderr
    assert "scoring functions" in proc.stderr
    assert "elapsed)" in proc.stderr


def test_unsupported_language_files_are_reported_on_stderr(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(repo, "function simpleFunction() { return 42; }")
    (repo / "src" / "engine.cpp").write_text("int main() { return 0; }\n")
    (repo / "src" / "engine.hpp").write_text("int f();\n")

    snap = repo / "snap.json"
    proc = r.run(["analyze", "--mode", "snapshot", "--format", "json", "--no-persist", "src"], repo, out=snap)
    assert proc.returncode == 0
    assert read_json(snap)["schema_version"] == 6  # stdout is still pure JSON
    assert "Not analyzed: 2 source file(s) in unsupported languages (1 .cpp, 1 .hpp)" in proc.stderr


def test_no_unsupported_line_when_nothing_was_left_out(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)
    write_ts(repo, "function simpleFunction() { return 42; }")

    snap = repo / "snap.json"
    proc = r.run(["analyze", "--mode", "snapshot", "--format", "json", "--no-persist", "src"], repo, out=snap)
    assert proc.returncode == 0
    assert "Not analyzed" not in proc.stderr
