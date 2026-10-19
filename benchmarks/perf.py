#!/usr/bin/env python3
"""
perf.py — hotspots macro-benchmark (hotspots#267 part 3)

Measures wall-clock and peak RSS for `hotspots analyze` against the same
corpus/SHAs used by the ranking-quality benchmark (corpus.json), so results
are comparable across repos over time. This is a different axis from
RESULTS.md (ranking quality, ρ/P@10) — this file tracks runtime performance.

Reuses corpus.json's clone/checkout machinery from _run_corpus.py rather than
duplicating it.

Usage:
    uv run python benchmarks/perf.py --hotspots /path/to/hotspots/binary

Long-running (clones + analyzes every corpus repo) — run via `runlog`:
    runlog run perf-bench -- uv run python benchmarks/perf.py --hotspots target/release/hotspots

Writes benchmarks/versions/perf-vX.Y.Z.json and appends a block to
benchmarks/PERF.md, mirroring RESULTS.md's format.
"""
from __future__ import annotations

import argparse
import json
import resource
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _run_corpus import ensure_bare_clone, log, resolve_feature_sha, run  # noqa: E402


def hotspots_version(repo_root: Path) -> str:
    text = (repo_root / "Cargo.toml").read_text()
    for line in text.splitlines():
        if line.startswith("version"):
            return line.split('"')[1]
    raise RuntimeError("could not find version in Cargo.toml")


def measure_one(hotspots_bin: str, checkout_dir: Path) -> dict:
    """Runs `hotspots analyze --timing-report json` once and measures
    wall-clock + peak RSS of the child process, folding in the per-phase
    breakdown from hotspots#267 part 1's --timing-report output."""
    cmd = [
        hotspots_bin, "analyze", str(checkout_dir),
        "--mode", "snapshot", "--format", "json", "--force",
        "--timing-report",
    ]
    log("+ " + " ".join(cmd))

    rusage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.monotonic()
    proc = subprocess.run(cmd, capture_output=True, text=True, check=True)
    wall_seconds = time.monotonic() - start
    rusage_after = resource.getrusage(resource.RUSAGE_CHILDREN)

    # ru_maxrss is cumulative across all children reaped so far on Linux/macOS,
    # not a delta — but since each repo's measurement is one subprocess call
    # in an otherwise-fresh perf.py invocation per run.sh/CI wiring, the
    # after-value already reflects this run's peak. Taking the delta guards
    # against the (rarer) case of multiple measurements sharing one process.
    peak_rss_kb = max(0, rusage_after.ru_maxrss - rusage_before.ru_maxrss)
    if peak_rss_kb == 0:
        peak_rss_kb = rusage_after.ru_maxrss  # first/only measurement in-process

    # print_timing_report (hotspots-cli/src/cmd/analyze.rs) is always the last
    # thing written to stderr, as a single pretty-printed JSON object. Find
    # where that final object starts (a line that is exactly "{") and parse
    # from there to the end of stderr.
    timing_report = None
    lines = proc.stderr.splitlines()
    try:
        # Unindented "{" — the top-level report object, not a nested phase entry.
        obj_start = max(i for i, line in enumerate(lines) if line == "{")
        timing_report = json.loads("\n".join(lines[obj_start:]))
    except (ValueError, json.JSONDecodeError):
        log("warning: could not parse --timing-report JSON from stderr; phase breakdown omitted")

    return {
        "wall_seconds": round(wall_seconds, 3),
        "peak_rss_kb": peak_rss_kb,
        "timing_report": timing_report,
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", default=str(Path(__file__).parent / "corpus.json"))
    p.add_argument("--clones", default="/tmp/hotspots-perf-bench/clones")
    p.add_argument("--workdir", default="/tmp/hotspots-perf-bench/work")
    p.add_argument("--hotspots", required=True)
    p.add_argument("--versions-dir", default=str(Path(__file__).parent / "versions"))
    p.add_argument("--perf-md", default=str(Path(__file__).parent / "PERF.md"))
    args = p.parse_args()

    repo_root = Path(__file__).parent.parent
    version = hotspots_version(repo_root)
    run_date = time.strftime("%Y-%m-%d")

    corpus = json.loads(Path(args.corpus).read_text())
    clones_dir = Path(args.clones)
    workdir = Path(args.workdir)
    clones_dir.mkdir(parents=True, exist_ok=True)
    workdir.mkdir(parents=True, exist_ok=True)

    results = []
    for repo in corpus["repos"]:
        slug = repo["slug"]
        log(f"=== {slug} ===")
        git_dir = clones_dir / (slug.replace("/", "__") + ".git")
        ensure_bare_clone(repo["url"], git_dir)

        feature_sha = repo.get("feature_sha") or resolve_feature_sha(git_dir, corpus["label_after"])

        checkout_dir = workdir / slug.replace("/", "__")
        if checkout_dir.exists():
            run(["git", "--git-dir", str(git_dir), "worktree", "remove", "--force", str(checkout_dir)],
                capture_output=True)
        run(["git", "--git-dir", str(git_dir), "worktree", "add", "--quiet", "--detach",
             str(checkout_dir), feature_sha])

        try:
            measured = measure_one(args.hotspots, checkout_dir)
        finally:
            run(["git", "--git-dir", str(git_dir), "worktree", "remove", "--force", str(checkout_dir)])

        results.append({
            "repo": slug,
            "language": repo["language"],
            **measured,
        })
        log(f"{slug}: {measured['wall_seconds']}s, {measured['peak_rss_kb']}KB peak RSS")

    version_out = {
        "hotspots_version": version,
        "run_date": run_date,
        "results": results,
    }
    versions_dir = Path(args.versions_dir)
    versions_dir.mkdir(parents=True, exist_ok=True)
    out_path = versions_dir / f"perf-v{version}.json"
    out_path.write_text(json.dumps(version_out, indent=2) + "\n")
    log(f"wrote {out_path}")

    walls = [r["wall_seconds"] for r in results]
    mean_wall = sum(walls) / len(walls) if walls else float("nan")

    lines = [f"## Latest — v{version} ({run_date})\n"]
    lines.append("| Repo | Language | Wall-clock (s) | Peak RSS (KB) |")
    lines.append("|---|---|---|---|")
    for r in results:
        lines.append(f"| {r['repo']} | {r['language']} | {r['wall_seconds']} | {r['peak_rss_kb']} |")
    lines.append(f"| **mean** | | **{mean_wall:.3f}** | |\n")
    lines.append(f"**Raw data (including per-phase breakdown):** [versions/perf-v{version}.json](versions/perf-v{version}.json)\n")
    block = "\n".join(lines)

    perf_md = Path(args.perf_md)
    if perf_md.exists():
        existing = perf_md.read_text()
        marker = "---\n"
        idx = existing.find(marker)
        if idx != -1:
            insert_at = idx + len(marker)
            existing = existing[:insert_at] + "\n" + block + "\n" + existing[insert_at:]
        else:
            existing = existing + "\n" + block + "\n"
        perf_md.write_text(existing)
    else:
        header = (
            "# Performance Benchmark Results\n\n"
            "Wall-clock and peak RSS for `hotspots analyze` against the same corpus/SHAs as "
            "[RESULTS.md](RESULTS.md) (ranking quality) — a different axis, runtime performance "
            "rather than ranking accuracy. See [hotspots#267](https://github.com/Stephen-Collins-tech/hotspots/issues/267).\n\n"
            "Per-phase timing breakdown (discover/parse/cfg/metrics/scoring/aggregate/report) is in "
            "each run's versions/perf-vX.Y.Z.json, from `--timing-report` (hotspots#268).\n\n"
            "---\n"
        )
        perf_md.write_text(header + "\n" + block + "\n")
    log(f"updated {perf_md}")


if __name__ == "__main__":
    main()
