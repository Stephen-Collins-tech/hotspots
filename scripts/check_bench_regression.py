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

CI runners are shared and drift between back-to-back phases, so CI measures
base and head twice, interleaved, and passes both pairs with --pair: a
benchmark only fails if it regressed in every pair. --min-base-ns excludes
sub-microsecond benchmarks, whose jitter exceeds any sensible threshold, from
gating (they are still reported).

Usage:
    cargo bench -p hotspots-core -- --save-baseline base   # on main / merge-base
    cargo bench -p hotspots-core -- --save-baseline pr      # on PR head
    python3 scripts/check_bench_regression.py --base base --pr pr
    # or, for repeated measurements:
    python3 scripts/check_bench_regression.py --pair base1:head1 --pair base2:head2 --min-base-ns 1000

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


def compare(
    base: dict[str, MeanEstimate], pr: dict[str, MeanEstimate], threshold: float, min_base_ns: float
) -> dict[str, tuple[float, float, float, bool, bool]]:
    """Per-bench (base_ns, pr_ns, pct_change, cis_overlap, is_regression) for one base/pr pair."""
    results = {}
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
        # Benchmarks below the noise floor are reported but never gate: at
        # ~100ns a shared CI runner's jitter exceeds any threshold.
        gated = base_ns >= min_base_ns
        is_regression = gated and pct_change > threshold and not cis_overlap
        results[bench_id] = (base_ns, pr_ns, pct_change, cis_overlap, is_regression)
    return results


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--criterion-dir", default="target/criterion")
    p.add_argument("--base", help="baseline name for the comparison point (e.g. main)")
    p.add_argument("--pr", help="baseline name for the candidate (e.g. the PR head run)")
    p.add_argument(
        "--pair",
        action="append",
        default=[],
        metavar="BASE:PR",
        help="a base:pr baseline pair; repeatable. With several pairs (independent "
        "back-to-back measurements), a benchmark only fails the gate if it regressed "
        "in every pair, so one runner-drift episode can't fail a PR by itself",
    )
    p.add_argument("--threshold", type=float, default=10.0, help="percent regression that fails the gate")
    p.add_argument(
        "--min-base-ns",
        type=float,
        default=0.0,
        help="benchmarks whose base mean is below this many nanoseconds are reported but never fail the gate",
    )
    args = p.parse_args()

    pairs = [tuple(spec.split(":", 1)) for spec in args.pair]
    if args.base and args.pr:
        pairs.insert(0, (args.base, args.pr))
    if not pairs or any(len(pair) != 2 or not all(pair) for pair in pairs):
        print("error: pass --base/--pr or one or more --pair BASE:PR", file=sys.stderr)
        return 2

    criterion_dir = Path(args.criterion_dir)
    per_pair = []
    for base_name, pr_name in pairs:
        base = find_estimates(criterion_dir, base_name)
        pr = find_estimates(criterion_dir, pr_name)
        if not base:
            print(f"error: no benchmarks found for baseline '{base_name}' under {criterion_dir}", file=sys.stderr)
            return 2
        if not pr:
            print(f"error: no benchmarks found for baseline '{pr_name}' under {criterion_dir}", file=sys.stderr)
            return 2
        for bench_id in sorted(set(base) - set(pr)):
            print(f"warning: bench '{bench_id}' present in '{base_name}' but missing from '{pr_name}'", file=sys.stderr)
        for bench_id in sorted(set(pr) - set(base)):
            print(
                f"warning: bench '{bench_id}' present in '{pr_name}' but missing from '{base_name}' "
                "(new benchmark, nothing to compare)",
                file=sys.stderr,
            )
        per_pair.append(compare(base, pr, args.threshold, args.min_base_ns))

    bench_ids = sorted(set.intersection(*(set(r) for r in per_pair)))
    regressions = []
    print(f"{'bench':<30} {'base (ms)':>12} {'pr (ms)':>12} {'change':>9}")
    for bench_id in bench_ids:
        base_ns, pr_ns, pct_change, cis_overlap, _ = per_pair[0][bench_id]
        regressed_in = [r[bench_id][4] for r in per_pair]
        if all(regressed_in):
            flag = " <-- REGRESSION"
            if len(per_pair) > 1:
                flag += f" (reproduced in all {len(per_pair)} runs)"
            regressions.append((bench_id, pct_change))
        elif any(regressed_in):
            flag = f" <-- not reproduced ({sum(regressed_in)}/{len(per_pair)} runs, not flagged)"
        elif pct_change > args.threshold and cis_overlap:
            flag = " <-- noisy (CIs overlap, not flagged)"
        elif pct_change > args.threshold and base_ns < args.min_base_ns:
            flag = " <-- below noise floor (not flagged)"
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
