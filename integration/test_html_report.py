from __future__ import annotations

from pathlib import Path

from .utils import Runner, init_test_repo, write_ts


def test_snapshot_html_report(tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[1]
    r = Runner(repo_root)
    r.ensure_built()

    repo = init_test_repo(tmp_path)

    # Minimal source content
    write_ts(
        repo,
        """
// Simple
function a(x){return x+1}
""".strip(),
    )

    out = repo / "report.html"
    proc = r.run(
        [
            "analyze",
            "--mode",
            "snapshot",
            "--format",
            "html",
            "--output",
            str(out),
            "src/main.ts",
        ],
        repo,
    )
    # HTML generation writes to stderr; exit code should be 0
    assert proc.returncode == 0
    assert out.exists(), "HTML report was not generated"
    content = out.read_text(errors="ignore").lower()
    assert "<html" in content and "hotspots" in content

