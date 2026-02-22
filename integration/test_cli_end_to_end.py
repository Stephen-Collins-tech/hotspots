from __future__ import annotations

from pathlib import Path

from .utils import Runner, init_test_repo, write_ts, read_json


def test_snapshot_delta_trends(tmp_path: Path):
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
    assert "deltas" in d and isinstance(d["deltas"], list)
    assert d.get("aggregates") is not None

    # Trends JSON
    trends = repo / "trends.json"
    proc = r.run(["trends", "--window", "10", "--top", "5", "--format", "json", "."], repo, out=trends)
    assert proc.returncode == 0
    t = read_json(trends)
    assert set(["velocities", "hotspots", "refactors"]).issubset(t.keys())

