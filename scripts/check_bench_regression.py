#!/usr/bin/env python3
"""
check_bench_regression.py — hotspots#267 part 4 (CI regression gate)

Compares two Criterion baselines saved under target/criterion/ and fails
(non-zero exit) if any benchmark's mean wall-clock time regressed by more
than --threshold (default 10%) between them.

Usage:
    cargo bench -p hotspots-core -- --save-baseline base   # on main / merge-base
    cargo bench -p hotspots-core -- --save-baseline pr      # on PR head
    python3 scripts/check_bench_regression.py --base base --pr pr

Each Criterion bench ID (e.g. parse/rust/large) gets its own directory under
target/criterion/<id>/<baseline-name>/estimates.json, with the point estimate
in nanoseconds at estimates["mean"]["point_estimate"].
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def find_estimates(criterion_dir: Path, baseline: str) -> dict[str, float]:
    """Maps bench ID (e.g. "parse/rust/large") -> mean point estimate (ns)."""
    results: dict[str, float] = {}
    for estimates_path in criterion_dir.glob(f"**/{baseline}/estimates.json"):
        # Path shape: target/criterion/<group>/<...>/<baseline>/estimates.json
        bench_id = str(estimates_path.relative_to(criterion_dir).parent.parent)
        try:
            data = json.loads(estimates_path.read_text())
            results[bench_id] = data["mean"]["point_estimate"]
        except (json.JSONDecodeError, KeyError) as e:
            print(f"warning: could not read {estimates_path}: {e}", file=sys.stderr)
    return results


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--criterion-dir", default="target/criterion")
    p.add_argument("--base", required=True, help="baseline name for the comparison point (e.g. main)")
    p.add_argument("--pr", required=True, help="baseline name for the candidate (e.g. the PR head run)")
    p.add_argument("--threshold", type=float, default=10.0, help="percent regression that fails the gate")
    args = p.parse_args()

    criterion_dir = Path(args.criterion_dir)
    base = find_estimates(criterion_dir, args.base)
    pr = find_estimates(criterion_dir, args.pr)

    if not base:
        print(f"error: no benchmarks found for baseline '{args.base}' under {criterion_dir}", file=sys.stderr)
        return 2
    if not pr:
        print(f"error: no benchmarks found for baseline '{args.pr}' under {criterion_dir}", file=sys.stderr)
        return 2

    rows = []
    regressions = []
    for bench_id in sorted(set(base) & set(pr)):
        base_ns = base[bench_id]
        pr_ns = pr[bench_id]
        pct_change = (pr_ns - base_ns) / base_ns * 100.0
        rows.append((bench_id, base_ns, pr_ns, pct_change))
        if pct_change > args.threshold:
            regressions.append((bench_id, pct_change))

    missing_in_pr = set(base) - set(pr)
    missing_in_base = set(pr) - set(base)
    for bench_id in sorted(missing_in_pr):
        print(f"warning: bench '{bench_id}' present in '{args.base}' but missing from '{args.pr}'", file=sys.stderr)
    for bench_id in sorted(missing_in_base):
        print(f"warning: bench '{bench_id}' present in '{args.pr}' but missing from '{args.base}' (new benchmark, nothing to compare)", file=sys.stderr)

    print(f"{'bench':<30} {'base (ms)':>12} {'pr (ms)':>12} {'change':>9}")
    for bench_id, base_ns, pr_ns, pct_change in rows:
        flag = " <-- REGRESSION" if pct_change > args.threshold else ""
        print(f"{bench_id:<30} {base_ns / 1e6:>12.4f} {pr_ns / 1e6:>12.4f} {pct_change:>+8.2f}%{flag}")

    if regressions:
        print(f"\nFAIL: {len(regressions)} benchmark(s) regressed more than {args.threshold}%:", file=sys.stderr)
        for bench_id, pct_change in regressions:
            print(f"  {bench_id}: {pct_change:+.2f}%", file=sys.stderr)
        return 1

    print(f"\nOK: no benchmark regressed more than {args.threshold}%")
    return 0


if __name__ == "__main__":
    sys.exit(main())
