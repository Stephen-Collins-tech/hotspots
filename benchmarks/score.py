#!/usr/bin/env python3
"""
Compute Spearman ρ and P@10 from hotspots JSON output and bug-commit labels.

hotspots --mode snapshot --format json --all-functions emits function-level
scores. This script aggregates to file level (max activity_risk per file),
joins with labels, and reports ρ and P@10.
"""
import argparse
import json
import math
import os
import sys
from pathlib import Path


def spearman(xs: list[float], ys: list[float]) -> float:
    n = len(xs)
    if n < 3:
        return float("nan")

    def rank(vals: list[float]) -> list[float]:
        sorted_idx = sorted(range(n), key=lambda i: vals[i])
        ranks = [0.0] * n
        i = 0
        while i < n:
            j = i
            while j < n and vals[sorted_idx[j]] == vals[sorted_idx[i]]:
                j += 1
            avg = (i + j - 1) / 2.0 + 1
            for k in range(i, j):
                ranks[sorted_idx[k]] = avg
            i = j
        return ranks

    rx, ry = rank(xs), rank(ys)
    mx = sum(rx) / n
    my = sum(ry) / n
    num = sum((rx[i] - mx) * (ry[i] - my) for i in range(n))
    dx = math.sqrt(sum((rx[i] - mx) ** 2 for i in range(n)))
    dy = math.sqrt(sum((ry[i] - my) ** 2 for i in range(n)))
    if dx == 0 or dy == 0:
        return float("nan")
    return num / (dx * dy)


def precision_at_k(scores: list[float], labels: list[int], k: int = 10) -> float:
    paired = sorted(zip(scores, labels), key=lambda x: -x[0])
    top_k = paired[:k]
    if not top_k:
        return float("nan")
    return sum(lbl for _, lbl in top_k) / len(top_k)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scores_json", help="hotspots --format json --all-functions output")
    parser.add_argument("labels_json", help="Output from label.py")
    parser.add_argument("--repo", default="", help="Repo slug for display")
    parser.add_argument("--min-bug-files", type=int, default=20,
                        help="Minimum bug-linked files to report ρ (default 20)")
    args = parser.parse_args()

    with open(args.scores_json) as f:
        hotspots_out = json.load(f)

    functions = hotspots_out.get("functions", [])

    # Aggregate function scores to file level: max(activity_risk) per file.
    # Strip absolute path prefix — labels use repo-relative paths.
    file_scores: dict[str, float] = {}
    for func in functions:
        raw_file = func.get("file", "")
        # Normalise to relative path by stripping everything up to first real path segment
        # hotspots emits absolute paths; labels use paths relative to repo root.
        # We'll strip common prefix after joining.
        score = func.get("activity_risk") or 0.0
        if raw_file not in file_scores or score > file_scores[raw_file]:
            file_scores[raw_file] = score

    with open(args.labels_json) as f:
        labels_raw = json.load(f)

    # Build label lookup by basename components to handle path prefix mismatch.
    # Labels are repo-relative; hotspots paths are absolute.
    # Strategy: find the strip depth that maximises label matches.
    label_map: dict[str, int] = {r["file"]: r["bug_linked"] for r in labels_raw}

    rel_scores: dict[str, float] = {}
    if file_scores and labels_raw:
        abs_paths = list(file_scores.keys())

        # Try stripping 1..N leading components until we get the most label hits.
        # This handles cases where commonpath goes too deep (e.g. all files share a subdir).
        best_depth = 1
        best_hits = 0
        sample = abs_paths[:200]
        for depth in range(1, 8):
            hits = sum(
                1 for ap in sample
                if str(Path(*Path(ap).parts[depth:])) in label_map
            )
            if hits > best_hits:
                best_hits = hits
                best_depth = depth

        for ap, score in file_scores.items():
            parts = Path(ap).parts
            if len(parts) > best_depth:
                rel = str(Path(*parts[best_depth:]))
            else:
                rel = ap
            rel_scores[rel] = max(rel_scores.get(rel, 0.0), score)

    # Join on relative file path
    joined: list[tuple[float, int]] = []
    for rel_path, score in rel_scores.items():
        if rel_path in label_map:
            joined.append((score, label_map[rel_path]))

    n_files = len(joined)
    n_bug_files = sum(lbl for _, lbl in joined)
    scores_list = [s for s, _ in joined]
    labels_list = [l for _, l in joined]

    if n_bug_files < args.min_bug_files:
        rho_str = "—"
        p10_str = "—"
    else:
        rho = spearman(scores_list, labels_list)
        p10 = precision_at_k(scores_list, labels_list)
        rho_str = f"{rho:+.3f}" if not math.isnan(rho) else "—"
        p10_str = f"{p10:.2f}" if not math.isnan(p10) else "—"

    print(f"| {args.repo or args.scores_json} | ρ={rho_str} | P@10={p10_str} | n_files={n_files} | n_bug_files={n_bug_files} |")


if __name__ == "__main__":
    main()
