#!/usr/bin/env python3
"""
check_bench_regression.py — hotspots#267 part 4 (CI regression gate)

Compares two Criterion baselines saved under target/criterion/ and fails
(non-zero exit) if any benchmark's mean wall-clock time regressed by more
than --threshold (default 10%) between them AND the two runs' 95% confidence
intervals for the mean don't overlap. The confidence-interval check guards
against exactly the failure mode a point-estimate-only comparison hits on CI
runners: two noisy runs of a sub-microsecond benchmark (e.g. cfg/typescript/
small, ~100ns) can show a >10% point-estimate delta purely from measurement
noise, while their confidence intervals still overlap heavily — that's not a
regression, it's the same distribution sampled twice. A real regression
shifts the whole distribution, which shows up as non-overlapping intervals.

Usage:
    cargo bench -p hotspots-core -- --save-baseline base   # on main / merge-base
    cargo bench -p hotspots-core -- --save-baseline pr      # on PR head
    python3 scripts/check_bench_regression.py --base base --pr pr

Each Criterion bench ID (e.g. parse/rust/large) gets its own directory under
target/criterion/<id>/<baseline-name>/estimates.json, with the mean's point
estimate and 95% confidence interval in nanoseconds at
estimates["mean"]["point_estimate"] and
estimates["mean"]["confidence_interval"]["lower_bound"/"upper_bound"].
"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class MeanEstimate:
    point_estimate: float
    lower_bound: float
    upper_bound: float


def find_estimates(criterion_dir: Path, baseline: str) -> dict[str, MeanEstimate]:
    """Maps bench ID (e.g. "parse/rust/large") -> mean estimate (ns)."""
    results: dict[str, MeanEstimate] = {}
    for estimates_path in criterion_dir.glob(f"**/{baseline}/estimates.json"):
        # Path shape: target/criterion/<group>/<...>/<baseline>/estimates.json
        bench_id = str(estimates_path.relative_to(criterion_dir).parent.parent)
        try:
            data = json.loads(estimates_path.read_text())
            mean = data["mean"]
            ci = mean["confidence_interval"]
            results[bench_id] = MeanEstimate(
                point_estimate=mean["point_estimate"],
                lower_bound=ci["lower_bound"],
                upper_bound=ci["upper_bound"],
            )
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
        base_est = base[bench_id]
        pr_est = pr[bench_id]
        base_ns = base_est.point_estimate
        pr_ns = pr_est.point_estimate
        pct_change = (pr_ns - base_ns) / base_ns * 100.0
        # Intervals overlap unless the base's upper bound sits entirely below
        # the PR's lower bound — i.e. even the most pessimistic base reading
        # was still faster than the most optimistic PR reading.
        cis_overlap = base_est.upper_bound >= pr_est.lower_bound
        is_regression = pct_change > args.threshold and not cis_overlap
        rows.append((bench_id, base_ns, pr_ns, pct_change, cis_overlap))
        if is_regression:
            regressions.append((bench_id, pct_change))

    missing_in_pr = set(base) - set(pr)
    missing_in_base = set(pr) - set(base)
    for bench_id in sorted(missing_in_pr):
        print(f"warning: bench '{bench_id}' present in '{args.base}' but missing from '{args.pr}'", file=sys.stderr)
    for bench_id in sorted(missing_in_base):
        print(f"warning: bench '{bench_id}' present in '{args.pr}' but missing from '{args.base}' (new benchmark, nothing to compare)", file=sys.stderr)

    print(f"{'bench':<30} {'base (ms)':>12} {'pr (ms)':>12} {'change':>9}")
    for bench_id, base_ns, pr_ns, pct_change, cis_overlap in rows:
        if pct_change > args.threshold and cis_overlap:
            flag = " <-- noisy (CIs overlap, not flagged)"
        elif pct_change > args.threshold:
            flag = " <-- REGRESSION"
        else:
            flag = ""
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
