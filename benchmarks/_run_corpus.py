#!/usr/bin/env python3
"""
Orchestrates one full benchmark run across every repo in corpus.json.
Invoked by run.sh; not intended to be run standalone (but can be).
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

FEATURES_ACTIVE = [
    "lrs", "cc", "nd", "loc", "fo", "fan_in", "total_churn", "authors_90d",
    "directed_coupling", "convention_bug_fix_count", "burst_score",
]


def log(msg: str) -> None:
    print(f"[bench] {msg}", file=sys.stderr, flush=True)


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    log("+ " + " ".join(cmd))
    return subprocess.run(cmd, check=True, **kwargs)


def ensure_bare_clone(url: str, dest: Path) -> None:
    if dest.exists():
        log(f"reusing existing clone {dest}")
        run(["git", "--git-dir", str(dest), "fetch", "--quiet", "origin"])
        return
    run(["git", "clone", "--bare", "--quiet", url, str(dest)])


def default_branch(git_dir: Path) -> str:
    out = subprocess.run(
        ["git", "--git-dir", str(git_dir), "symbolic-ref", "refs/remotes/origin/HEAD"],
        capture_output=True, text=True,
    )
    if out.returncode == 0 and out.stdout.strip():
        return out.stdout.strip().rsplit("/", 1)[-1]
    return "HEAD"


def resolve_feature_sha(git_dir: Path, before: str) -> str:
    branch = default_branch(git_dir)
    out = run(
        ["git", "--git-dir", str(git_dir), "log", "-1", f"--before={before}",
         f"--format=%H", branch],
        capture_output=True, text=True,
    )
    sha = out.stdout.strip()
    if not sha:
        raise RuntimeError(f"no commit before {before} found in {git_dir}")
    return sha


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", required=True)
    p.add_argument("--clones", required=True)
    p.add_argument("--workdir", required=True)
    p.add_argument("--hotspots", required=True)
    p.add_argument("--hotspots-version", required=True)
    p.add_argument("--run-date", required=True)
    p.add_argument("--label-py", required=True)
    p.add_argument("--score-py", required=True)
    p.add_argument("--versions-dir", required=True)
    p.add_argument("--results-md", required=True)
    args = p.parse_args()

    corpus_path = Path(args.corpus)
    corpus = json.loads(corpus_path.read_text())
    clones_dir = Path(args.clones)
    workdir = Path(args.workdir)

    results = []
    pinned_changed = False

    for repo in corpus["repos"]:
        slug = repo["slug"]
        log(f"=== {slug} ===")
        git_dir = clones_dir / (slug.replace("/", "__") + ".git")
        ensure_bare_clone(repo["url"], git_dir)

        feature_sha = repo.get("feature_sha")
        if not feature_sha:
            feature_sha = resolve_feature_sha(git_dir, corpus["label_after"])
            repo["feature_sha"] = feature_sha
            pinned_changed = True
            log(f"resolved feature_sha={feature_sha}")

        checkout_dir = workdir / slug.replace("/", "__")
        if checkout_dir.exists():
            run(["git", "--git-dir", str(git_dir), "worktree", "remove", "--force", str(checkout_dir)],
                capture_output=True)
        run(["git", "--git-dir", str(git_dir), "worktree", "add", "--quiet", "--detach",
             str(checkout_dir), feature_sha])

        scores_path = workdir / f"{slug.replace('/', '__')}-scores.json"
        with open(scores_path, "w") as f:
            run([args.hotspots, "analyze", str(checkout_dir),
                 "--mode", "snapshot", "--format", "json", "--all-functions", "--force"],
                stdout=f)

        labels_path = workdir / f"{slug.replace('/', '__')}-labels.json"
        run([sys.executable, args.label_py, str(git_dir),
             "--after", corpus["label_after"], "--before", corpus["label_before"],
             "--feature-sha", feature_sha, "--out", str(labels_path)])

        score_out = run(
            [sys.executable, args.score_py, str(scores_path), str(labels_path), "--repo", slug],
            capture_output=True, text=True,
        ).stdout.strip()
        log(score_out)

        m = re.search(
            r"\|\s*ρ=([+\-0-9.—]+)\s*\|\s*P@10=([0-9.—]+)\s*\|\s*n_files=(\d+)\s*\|\s*n_bug_files=(\d+)\s*\|",
            score_out,
        )
        if not m:
            raise RuntimeError(f"could not parse score.py output: {score_out!r}")
        rho_str, p10_str, n_files, n_bug_files = m.groups()
        rho = None if rho_str == "—" else float(rho_str)
        p10 = None if p10_str == "—" else float(p10_str)

        results.append({
            "repo": slug,
            "language": repo["language"],
            "rho": rho,
            "p_at_10": p10,
            "n_files": int(n_files),
            "n_bug_files": int(n_bug_files),
        })

        run(["git", "--git-dir", str(git_dir), "worktree", "remove", "--force", str(checkout_dir)])

    if pinned_changed:
        corpus_path.write_text(json.dumps(corpus, indent=2) + "\n")
        log(f"pinned newly-resolved feature_sha values back into {corpus_path}")

    version_out = {
        "hotspots_version": args.hotspots_version,
        "run_date": args.run_date,
        "corpus_version": 1,
        "features_active": FEATURES_ACTIVE,
        "ranker": False,
        "results": results,
    }

    versions_dir = Path(args.versions_dir)
    versions_dir.mkdir(parents=True, exist_ok=True)
    out_path = versions_dir / f"v{args.hotspots_version}.json"
    out_path.write_text(json.dumps(version_out, indent=2) + "\n")
    log(f"wrote {out_path}")

    rhos = [r["rho"] for r in results if r["rho"] is not None]
    p10s = [r["p_at_10"] for r in results if r["p_at_10"] is not None]
    mean_rho = sum(rhos) / len(rhos) if rhos else float("nan")
    mean_p10 = sum(p10s) / len(p10s) if p10s else float("nan")

    feature_set = ", ".join(FEATURES_ACTIVE)
    lines = []
    lines.append(f"## Latest — v{args.hotspots_version} ({args.run_date})\n")
    lines.append("**Features:** ARS baseline (no trained ranker)  ")
    lines.append(f"**Feature set:** {feature_set}  ")
    lines.append("**Note:** adds `burst_score` (F93 sliding 30-day max/mean commit-timing "
                  "ratio) with weight 0.3; no trained-ranker feature set change.\n")
    lines.append("| Repo | Language | ρ | P@10 | n\\_files | n\\_bug\\_files |")
    lines.append("|---|---|---|---|---|---|")
    for r in results:
        rho_s = f"{r['rho']:+.3f}" if r["rho"] is not None else "—"
        p10_s = f"{r['p_at_10']:.2f}" if r["p_at_10"] is not None else "—"
        lines.append(f"| {r['repo']} | {r['language']} | {rho_s} | {p10_s} | {r['n_files']:,} | {r['n_bug_files']} |")
    lines.append(f"| **mean** | | **{mean_rho:+.3f}** | **{mean_p10:.2f}** | | |\n")
    lines.append(f"**Corpus:** {len(results)} repos · label window {corpus['label_after']} to "
                 f"{corpus['label_before']} · features from pinned pre-{corpus['label_after']} SHAs  ")
    lines.append(f"**Raw data:** [versions/v{args.hotspots_version}.json](versions/v{args.hotspots_version}.json)\n")
    lines.append("---\n")
    new_latest_block = "\n".join(lines)

    history_row = (
        f"| [v{args.hotspots_version}](versions/v{args.hotspots_version}.json) | {args.run_date} | "
        f"{mean_rho:+.3f} | {mean_p10:.2f} | off | +`burst_score` (F93 commit-timing burstiness term) |"
    )

    results_md = Path(args.results_md)
    text = results_md.read_text()

    latest_heading_re = re.compile(r"## Latest — v([0-9.]+) \(")
    m = latest_heading_re.search(text)
    same_version_rerun = bool(m and m.group(1) == args.hotspots_version)

    if same_version_rerun:
        # Re-running the same version (e.g. after an implementation-only change):
        # replace the existing "## Latest — vX.Y.Z" section in place rather than
        # appending a duplicate.
        start = text.index(f"## Latest — v{args.hotspots_version} (")
        end = text.index("\n---\n\n## ", start) + len("\n---\n\n")
        text = text[:start] + new_latest_block + "\n" + text[end:]
    else:
        # Demote the previous "## Latest —" section to a plain version heading and
        # splice the new one in above it (full per-repo detail for every version
        # still lives under versions/, so history rows link out rather than repeat).
        text = text.replace("## Latest — ", "## ", 1)
        marker = "---\n\n## "
        idx = text.find(marker)
        if idx == -1:
            raise RuntimeError("could not find insertion point in RESULTS.md")
        insert_at = idx + len("---\n\n")
        text = text[:insert_at] + new_latest_block + "\n" + text[insert_at:]

    # Add/replace the row for this version under "## History".
    history_row_re = re.compile(
        rf"\| \[v{re.escape(args.hotspots_version)}\]\(versions/v{re.escape(args.hotspots_version)}\.json\).*\|\n"
    )
    if history_row_re.search(text):
        text = history_row_re.sub(history_row + "\n", text, count=1)
    else:
        history_marker = "|---|---|---|---|---|---|\n"
        hidx = text.rfind(history_marker)
        if hidx == -1:
            raise RuntimeError("could not find History table in RESULTS.md")
        insert_row_at = hidx + len(history_marker)
        text = text[:insert_row_at] + history_row + "\n" + text[insert_row_at:]

    results_md.write_text(text)
    log(f"updated {results_md}")


if __name__ == "__main__":
    main()
