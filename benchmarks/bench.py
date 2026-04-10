#!/usr/bin/env python3
"""
bench.py — hotspots benchmark runner

Subcommands:
  stress   Memory/CPU stress test under Docker resource limits.
           Answers: "does it crash or complete within N MB?"
           Produces CSV + PNG plots of memory and CPU over time.

  show     Run analysis with no resource constraints, capture JSON output,
           and display the top hotspots in a readable table.
           Answers: "what does hotspots actually find in this repo?"

Usage:
    uv run python bench.py stress [options]
    uv run python bench.py show   [options]
    uv run python bench.py stress --help
    uv run python bench.py show   --help
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.ticker as ticker
    HAS_MATPLOTLIB = True
except ImportError:
    HAS_MATPLOTLIB = False

SCRIPT_DIR   = Path(__file__).parent.resolve()
PROJECT_ROOT = (SCRIPT_DIR / "..").resolve()
REPOS_DIR    = SCRIPT_DIR / "memory-crash" / "repos"
RESULTS_DIR  = SCRIPT_DIR / "memory-crash" / "results"
DOCKERFILE   = SCRIPT_DIR / "memory-crash" / "Dockerfile"
IMAGE_NAME   = "hotspots-bench"
BAR          = "═" * 64

# Supported benchmark repositories.
REPOS: dict[str, dict] = {
    "expo": {
        "url":  "https://github.com/expo/expo.git",
        "desc": "expo/expo — large React Native monorepo (~51k functions)",
    },
    "react": {
        "url":  "https://github.com/facebook/react.git",
        "desc": "facebook/react — medium JS/TS repo (~3k functions)",
    },
    "kubernetes": {
        "url":              "https://github.com/kubernetes/kubernetes.git",
        "desc":             "kubernetes/kubernetes — very large Go monorepo (~140k functions)",
        "callgraph_warn":   50_000,  # warn if callgraph not skipped above this
    },
}


# ── Logging ───────────────────────────────────────────────────────────────────

def log(msg: str = "") -> None:
    ts = datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


def section(title: str) -> None:
    print(f"\n{BAR}\n  {title}\n{BAR}", flush=True)


# ── Docker helpers ────────────────────────────────────────────────────────────

def docker(*args: str, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["docker", *args],
        capture_output=capture,
        text=True,
        check=check,
    )


def build_image(skip: bool) -> None:
    section("Build Docker image")
    if skip:
        log(f"--skip-build: using existing '{IMAGE_NAME}'")
        return
    log(f"Building '{IMAGE_NAME}' from {DOCKERFILE} (context: {PROJECT_ROOT})…")
    t0 = time.monotonic()
    result = subprocess.run(
        ["docker", "build", "-t", IMAGE_NAME, "-f", str(DOCKERFILE), str(PROJECT_ROOT)],
        check=False,
    )
    if result.returncode != 0:
        log(f"ERROR: docker build failed (exit {result.returncode})")
        sys.exit(1)
    log(f"Build completed in {time.monotonic() - t0:.0f}s")


def repo_dir(repo_name: str) -> Path:
    return REPOS_DIR / repo_name


def ensure_clone(repo_name: str, skip: bool) -> Path:
    meta    = REPOS[repo_name]
    url     = meta["url"]
    dest    = repo_dir(repo_name)
    section(f"Prepare {repo_name} repository")
    if skip:
        log(f"--skip-clone: trusting {dest}")
        return dest
    if (dest / ".git").exists():
        log(f"Clone exists: {dest}")
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    log(f"Cloning {url} → {dest}")
    t0 = time.monotonic()
    result = subprocess.run(
        ["git", "-c", "pack.threads=2", "clone", "--progress", url, str(dest)],
        check=False,
    )
    if result.returncode != 0:
        log(f"ERROR: git clone failed (exit {result.returncode})")
        sys.exit(1)
    log(f"Clone completed in {time.monotonic() - t0:.0f}s")
    return dest


def build_env_args(jobs: int | None, callgraph_skip: int | None, touch: str) -> list[str]:
    args: list[str] = []
    if jobs is not None:
        args += ["-e", f"BENCH_JOBS={jobs}"]
    if callgraph_skip is not None:
        args += ["-e", f"BENCH_CALLGRAPH_SKIP_ABOVE={callgraph_skip}"]
    if touch != "skip":
        args += ["-e", f"BENCH_TOUCH={touch}"]
    return args


# ── Stats sampler (stress subcommand) ─────────────────────────────────────────

@dataclass
class Sample:
    elapsed_s: float
    mem_mb:    float
    cpu_pct:   float


def _parse_docker_size_mb(s: str) -> float:
    s = s.strip()
    for suffix, factor in [
        ("GiB", 1024.0), ("MiB", 1.0), ("KiB", 1.0 / 1024),
        ("GB",  1000.0), ("MB",  1.0), ("kB",  1.0 / 1000),
        ("B",   1.0 / (1024 * 1024)),
    ]:
        if s.endswith(suffix):
            return float(s[: -len(suffix)]) * factor
    return 0.0


def _parse_memory_mb(s: str) -> int:
    s = s.strip().lower()
    if s.endswith("g"):
        return int(s[:-1]) * 1024
    if s.endswith("m"):
        return int(s[:-1])
    raise ValueError(f"Unrecognised memory format: {s!r}  (use e.g. 2g, 4g, 512m)")


@dataclass
class Sampler:
    container_id:    str
    memory_limit_mb: float
    watchdog_mem:    float = 85.0   # % of limit; 0 = disabled
    watchdog_cpu:    float = 0.0    # absolute CPU%; 0 = disabled
    watchdog_secs:   float = 10.0
    samples:         list[Sample] = field(default_factory=list)
    killed_reason:   str = ""
    _stop:           threading.Event = field(default_factory=threading.Event)
    _thread:         threading.Thread | None = None
    _t0:             float = field(default_factory=time.monotonic)

    def start(self) -> None:
        self._t0 = time.monotonic()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=10)

    def _run(self) -> None:
        while not self._stop.is_set():
            result = subprocess.run(
                ["docker", "stats", "--no-stream", "--format", "{{json .}}", self.container_id],
                capture_output=True, text=True, check=False,
            )
            if result.returncode == 0:
                for line in result.stdout.strip().splitlines():
                    try:
                        data = json.loads(line)
                        mem_mb  = _parse_docker_size_mb(data["MemUsage"].split("/")[0])
                        cpu_pct = float(data["CPUPerc"].rstrip("%"))
                        self.samples.append(Sample(time.monotonic() - self._t0, mem_mb, cpu_pct))
                    except (KeyError, ValueError, json.JSONDecodeError):
                        pass
                self._check_watchdog()

    def _check_watchdog(self) -> None:
        if not self.samples or self.killed_reason:
            return
        now  = self.samples[-1].elapsed_s
        win  = [s for s in self.samples if s.elapsed_s >= now - self.watchdog_secs]
        if len(win) < max(3, self.watchdog_secs / 2):
            return
        reason = ""
        if self.watchdog_mem > 0:
            thresh = self.memory_limit_mb * self.watchdog_mem / 100
            frac   = sum(1 for s in win if s.mem_mb >= thresh) / len(win)
            if frac >= 0.9:
                reason = f"memory ≥ {self.watchdog_mem:.0f}% for {self.watchdog_secs:.0f}s"
        if not reason and self.watchdog_cpu > 0:
            frac = sum(1 for s in win if s.cpu_pct >= self.watchdog_cpu) / len(win)
            if frac >= 0.9:
                reason = f"CPU ≥ {self.watchdog_cpu:.0f}% for {self.watchdog_secs:.0f}s"
        if reason:
            self.killed_reason = reason
            log(f"WATCHDOG: killing container — {reason}")
            subprocess.run(["docker", "kill", self.container_id], capture_output=True, check=False)
            self._stop.set()


# ── stress subcommand ─────────────────────────────────────────────────────────

def cmd_stress(args: argparse.Namespace) -> None:
    try:
        memory_mb = _parse_memory_mb(args.memory)
    except ValueError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)

    label = args.label or f"stress-{args.repo}"
    section("HOTSPOTS STRESS BENCHMARK")
    log(f"Repo:       {REPOS[args.repo]['desc']}")
    log(f"Memory:     {args.memory}  ({memory_mb} MB hard ceiling, no swap)")
    log(f"CPUs:       {args.cpus}")
    log(f"Jobs:       {args.jobs if args.jobs is not None else 'default (all CPUs)'}")
    log(f"Callgraph:  {'skip above ' + str(args.callgraph_skip_above) if args.callgraph_skip_above else 'always compute'}")
    log(f"Touch:      {args.touch}")

    warn_threshold = REPOS[args.repo].get("callgraph_warn")
    if warn_threshold and args.callgraph_skip_above is None:
        log(f"WARN: {args.repo} has ~{warn_threshold:,}+ functions — call graph will be very slow.")
        log(f"      Consider: --callgraph-skip-above {warn_threshold}")

    build_image(args.skip_build)
    local_repo = ensure_clone(args.repo, args.skip_clone)

    env_args = build_env_args(args.jobs, args.callgraph_skip_above, args.touch)
    cidfile  = Path(tempfile.mktemp(suffix=".cid"))
    sampler: Sampler | None = None

    section(f"Run analysis  [{args.cpus} CPU / {args.memory} RAM]")

    try:
        proc = subprocess.Popen(
            [
                "docker", "run",
                "--cidfile", str(cidfile),
                f"--memory={args.memory}",
                f"--memory-swap={args.memory}",
                f"--cpus={args.cpus}",
                *env_args,
                "-v", f"{local_repo}:/repo",
                IMAGE_NAME,
            ],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        )

        def _start_sampler() -> None:
            nonlocal sampler
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                try:
                    cid = cidfile.read_text().strip()
                    if cid:
                        break
                except FileNotFoundError:
                    pass
                time.sleep(0.1)
            else:
                return
            log(f"Container: {cid[:12]}  (sampling every ~1s)")
            sampler = Sampler(
                container_id=cid,
                memory_limit_mb=float(memory_mb),
                watchdog_mem=args.watchdog_mem,
                watchdog_cpu=args.watchdog_cpu,
                watchdog_secs=args.watchdog_secs,
            )
            sampler.start()

        sampler_thread = threading.Thread(target=_start_sampler, daemon=True)
        sampler_thread.start()

        assert proc.stdout
        for line in proc.stdout:
            print(line, end="", flush=True)
        proc.wait()
        sampler_thread.join(timeout=15)
        if sampler:
            sampler.stop()

        exit_code = proc.returncode
        oom_killed = False
        container_id = cidfile.read_text().strip() if cidfile.exists() else None
        if container_id and (not sampler or not sampler.killed_reason):
            r = subprocess.run(
                ["docker", "inspect", container_id, "--format", "{{.State.OOMKilled}}"],
                capture_output=True, text=True, check=False,
            )
            if r.stdout.strip().lower() == "true":
                oom_killed = True
                exit_code  = 137
        if container_id:
            subprocess.run(["docker", "rm", container_id], capture_output=True, check=False)
    finally:
        cidfile.unlink(missing_ok=True)

    samples       = sampler.samples if sampler else []
    watchdog_reason = sampler.killed_reason if sampler else ""
    _save_stress_results(samples, args.memory, memory_mb, exit_code, oom_killed,
                         watchdog_reason, label)

    section("RESULT")
    if watchdog_reason:
        log(f"WATCHDOG KILL — {watchdog_reason}")
        sys.exit(4)
    elif oom_killed or exit_code == 137:
        log(f"OOM KILL (exit 137) — killed at the {args.memory} ceiling")
        sys.exit(2)
    elif exit_code == 0:
        log("COMPLETED SUCCESSFULLY (exit 0)")
    else:
        log(f"NON-ZERO EXIT (exit {exit_code})")
        sys.exit(3)


def _save_stress_results(
    samples: list[Sample], memory_limit: str, memory_mb: int,
    exit_code: int, oom_killed: bool, watchdog_reason: str, label: str,
) -> None:
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    ts   = datetime.now().strftime("%Y%m%d-%H%M%S")
    stem = RESULTS_DIR / f"{label}-{ts}"

    csv_path = stem.with_suffix(".csv")
    with csv_path.open("w") as f:
        f.write("elapsed_s,mem_mb,cpu_pct\n")
        for s in samples:
            f.write(f"{s.elapsed_s:.2f},{s.mem_mb:.1f},{s.cpu_pct:.2f}\n")
    log(f"CSV:  {csv_path}  ({len(samples)} samples)")

    if not samples or not HAS_MATPLOTLIB:
        if not HAS_MATPLOTLIB:
            log("WARN: matplotlib not installed — skipping plot")
        return

    if watchdog_reason:
        result_str = "WATCHDOG KILL"
    elif oom_killed:
        result_str = "OOM KILL"
    else:
        result_str = "OK" if exit_code == 0 else f"exit {exit_code}"

    times = [s.elapsed_s for s in samples]
    mem   = [s.mem_mb    for s in samples]
    cpu   = [s.cpu_pct   for s in samples]

    fig, (ax_mem, ax_cpu) = plt.subplots(2, 1, figsize=(10, 6), sharex=True)
    fig.suptitle(f"hotspots analyze expo/expo — {memory_limit} limit — {result_str}",
                 fontsize=13, fontweight="bold")

    ax_mem.plot(times, mem, color="#e74c3c", linewidth=1.5, label="RSS")
    ax_mem.axhline(memory_mb, color="#e74c3c", linestyle="--", linewidth=1,
                   alpha=0.5, label=f"limit ({memory_limit})")
    end_t = times[-1]
    if oom_killed:
        ax_mem.axvline(end_t, color="#c0392b", linestyle=":", linewidth=1.5, label="OOM kill")
    elif watchdog_reason:
        ax_mem.axvline(end_t, color="#e67e22", linestyle=":", linewidth=1.5, label="watchdog")
    ax_mem.set_ylabel("Memory (MB)")
    ax_mem.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: f"{x:.0f}"))
    ax_mem.legend(fontsize=8, loc="upper left")
    ax_mem.grid(True, alpha=0.3)

    ax_cpu.plot(times, cpu, color="#3498db", linewidth=1.5)
    if watchdog_reason:
        ax_cpu.axvline(end_t, color="#e67e22", linestyle=":", linewidth=1.5)
    ax_cpu.set_ylabel("CPU (%)")
    ax_cpu.set_xlabel("Elapsed (s)")
    ax_cpu.set_ylim(bottom=0)
    ax_cpu.grid(True, alpha=0.3)

    plt.tight_layout()
    png_path = stem.with_suffix(".png")
    fig.savefig(png_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    log(f"Plot: {png_path}")


# ── Artifact filter ───────────────────────────────────────────────────────────

# Path segments that identify compiled/vendored artifacts rather than source.
_ARTIFACT_SEGMENTS = frozenset([
    "/cjs/",    # JS CommonJS build output
    "/umd/",    # JS UMD build output
    "/esm/",    # JS ESM build output
    "/dist/",   # generic build output
    "/build/",  # generic build output
    "/vendor/", # Go vendored dependencies
])
_ARTIFACT_SUFFIXES = (
    ".min.js",
    ".development.js",
    ".production.js",
    ".pb.go",           # Go protobuf generated
    "_generated.go",    # Go generated (controller-gen, mockgen, etc.)
)
# Filename prefixes that indicate generated Go files
_ARTIFACT_PREFIXES_GO = ("zz_generated.",)


def _is_artifact(function_id: str) -> bool:
    """Return True if the function lives in a compiled, vendored, or generated file."""
    path = function_id.split("::")[0] if "::" in function_id else function_id
    for seg in _ARTIFACT_SEGMENTS:
        if seg in path:
            return True
    if path.endswith(_ARTIFACT_SUFFIXES):
        return True
    filename = path.rsplit("/", 1)[-1]
    return any(filename.startswith(p) for p in _ARTIFACT_PREFIXES_GO)


# ── show subcommand ───────────────────────────────────────────────────────────

def cmd_show(args: argparse.Namespace) -> None:
    section("HOTSPOTS SHOWCASE")
    log(f"Repo:       {REPOS[args.repo]['desc']}")
    log(f"Jobs:       {args.jobs if args.jobs is not None else 'default (all CPUs)'}")
    log(f"Callgraph:  {'skip above ' + str(args.callgraph_skip_above) if args.callgraph_skip_above else 'always compute'}")
    log(f"Touch:      {args.touch}")
    log(f"Top N:      {args.top}")

    # Warn if a large repo is about to run without a callgraph skip threshold.
    warn_threshold = REPOS[args.repo].get("callgraph_warn")
    if warn_threshold and args.callgraph_skip_above is None:
        log(f"WARN: {args.repo} has ~{warn_threshold:,}+ functions — call graph will be very slow.")
        log(f"      Consider: --callgraph-skip-above {warn_threshold}")

    build_image(args.skip_build)
    local_repo = ensure_clone(args.repo, args.skip_clone)

    env_args = build_env_args(args.jobs, args.callgraph_skip_above, args.touch)

    section("Running analysis…")
    t0 = time.monotonic()
    result = subprocess.run(
        [
            "docker", "run", "--rm",
            f"--cpus={args.cpus}",
            "--memory=8g",
            *env_args,
            "-e", "BENCH_OUTPUT=all-functions",
            "-v", f"{local_repo}:/repo",
            IMAGE_NAME,
        ],
        capture_output=True, text=True, check=False,
    )
    elapsed = time.monotonic() - t0

    if result.returncode != 0:
        log(f"ERROR: analysis failed (exit {result.returncode})")
        print(result.stderr[-2000:] if result.stderr else "(no stderr)", flush=True)
        sys.exit(1)

    log(f"Analysis completed in {elapsed:.1f}s")

    # Parse JSON snapshot
    try:
        snapshot = json.loads(result.stdout)
    except json.JSONDecodeError as e:
        log(f"ERROR: failed to parse JSON output: {e}")
        print(result.stdout[:500], flush=True)
        sys.exit(1)

    functions = snapshot.get("functions", [])
    commit    = snapshot.get("commit", {})
    summary   = snapshot.get("summary", {})

    # Sort by activity_risk if present, else lrs
    functions.sort(
        key=lambda f: f.get("activity_risk") or f.get("lrs") or 0.0,
        reverse=True,
    )

    # Filter compiled/vendored artifacts from display (not from analysis).
    # These are real files hotspots correctly scored, just not useful to show.
    if not args.show_artifacts:
        display_funcs = [f for f in functions if not _is_artifact(f.get("function_id", ""))]
        filtered = len(functions) - len(display_funcs)
    else:
        display_funcs = functions
        filtered = 0

    top_funcs = display_funcs[: args.top]

    _print_showcase(top_funcs, display_funcs, commit, summary, elapsed, args, filtered)

    if args.save:
        RESULTS_DIR.mkdir(parents=True, exist_ok=True)
        ts   = datetime.now().strftime("%Y%m%d-%H%M%S")
        path = RESULTS_DIR / f"show-{args.repo}-{ts}.json"
        path.write_text(result.stdout)
        log(f"\nFull snapshot saved: {path}")


def _print_showcase(
    top_funcs: list[dict],
    all_funcs: list[dict],
    commit: dict,
    summary: dict,
    elapsed: float,
    args: argparse.Namespace,
    filtered: int = 0,
) -> None:
    sha    = commit.get("sha", "unknown")[:12]
    n      = len(all_funcs)
    cg     = summary.get("call_graph") or {}
    by_band = summary.get("by_band", {})

    def band_count(name: str) -> int:
        return (by_band.get(name) or {}).get("count", 0)

    critical = band_count("critical")
    high     = band_count("high")
    moderate = band_count("moderate")

    print(f"\n{'═' * 64}", flush=True)
    print(f"  hotspots · {args.repo} @ {sha}", flush=True)
    print(f"{'═' * 64}", flush=True)
    print(f"  {n:,} functions analyzed in {elapsed:.1f}s", flush=True)
    if filtered:
        print(f"  ({filtered} build artifacts hidden — use --show-artifacts to include)", flush=True)
    if cg:
        edges    = cg.get("total_edges", 0)
        avg_fi   = cg.get("avg_fan_in", 0.0)
        scc      = cg.get("scc_count", 0)
        print(f"  call graph: {edges:,} edges  avg fan-in {avg_fi:.2f}  {scc} SCCs", flush=True)
    print(f"  risk bands: {critical} critical  {high} high  {moderate} moderate", flush=True)
    print(f"{'─' * 64}", flush=True)

    BAND_COLOR = {
        "critical": "\033[91m",  # bright red
        "high":     "\033[33m",  # yellow
        "moderate": "\033[93m",  # bright yellow
        "low":      "\033[32m",  # green
    }
    RESET = "\033[0m"

    header = f"  {'#':>3}  {'Score':>6}  {'Band':<9}  {'Patterns':<30}  Function"
    print(header, flush=True)
    print(f"  {'─'*3}  {'─'*6}  {'─'*9}  {'─'*30}  {'─'*40}", flush=True)

    for i, fn in enumerate(top_funcs, 1):
        score   = fn.get("activity_risk") or fn.get("lrs") or 0.0
        band    = fn.get("band", "low")
        fid     = fn.get("function_id", "?")
        patterns = fn.get("patterns") or []
        pat_str = ", ".join(patterns[:3])
        if len(patterns) > 3:
            pat_str += f" +{len(patterns)-3}"

        # Shorten function_id: strip common prefix, keep last path segment + symbol
        parts = fid.split("::")
        if len(parts) >= 2:
            path_parts = parts[0].split("/")
            short_path = "/".join(path_parts[-3:]) if len(path_parts) >= 3 else parts[0]
            short_fid  = f"{short_path}::{parts[-1]}"
        else:
            short_fid = fid

        color = BAND_COLOR.get(band, "")
        score_str = f"{score:>6.2f}"
        band_str  = f"{color}{band:<9}{RESET}"
        pat_col   = f"{pat_str:<30}"
        fn_col    = short_fid[:60]

        print(f"  {i:>3}  {score_str}  {band_str}  {pat_col}  {fn_col}", flush=True)

    print(f"{'═' * 64}\n", flush=True)


# ── CLI ───────────────────────────────────────────────────────────────────────

def _add_common_args(p: argparse.ArgumentParser) -> None:
    repo_choices = list(REPOS.keys())
    repo_help    = "  |  ".join(f"{k}: {v['desc']}" for k, v in REPOS.items())
    p.add_argument("--repo", choices=repo_choices, default="expo", metavar="REPO",
                   help=f"Repository to benchmark (default: expo). Choices: {repo_help}")
    p.add_argument("--jobs", type=int, default=None, metavar="N",
                   help="Worker threads for hotspots analyze (default: all CPUs).")
    p.add_argument("--callgraph-skip-above", type=int, default=None, metavar="N",
                   help="Skip call graph algorithms above N functions.")
    p.add_argument("--touch", choices=["skip", "file", "per-function"], default="skip",
                   metavar="MODE",
                   help="Touch metrics: skip (default), file, or per-function.")
    p.add_argument("--cpus", default="2", metavar="N",
                   help="Docker --cpus (default: 2).")
    p.add_argument("--skip-build", action="store_true",
                   help="Skip docker build, use existing image.")
    p.add_argument("--skip-clone", action="store_true",
                   help="Skip clone check, trust existing repos/<name>.")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="hotspots benchmark runner",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    # ── stress ────────────────────────────────────────────────────────────────
    s = sub.add_parser("stress", help="Memory/CPU stress test under Docker resource limits.")
    _add_common_args(s)
    s.add_argument("--memory", default="4g", metavar="LIMIT",
                   help="Hard memory ceiling (default: 4g).")
    s.add_argument("--label", default=None, metavar="LABEL",
                   help="Results filename prefix (default: stress-<repo>).")
    wd = s.add_argument_group("watchdog")
    wd.add_argument("--watchdog-mem", type=float, default=85.0, metavar="PCT",
                    help="Kill if memory ≥ PCT%% of limit for --watchdog-secs (default: 85; 0=off).")
    wd.add_argument("--watchdog-cpu", type=float, default=0.0, metavar="PCT",
                    help="Kill if CPU ≥ PCT%% for --watchdog-secs (default: 0=off).")
    wd.add_argument("--watchdog-secs", type=float, default=10.0, metavar="S",
                    help="Sustain window in seconds (default: 10).")

    # ── show ──────────────────────────────────────────────────────────────────
    sh = sub.add_parser("show", help="Run analysis and display top hotspots.")
    _add_common_args(sh)
    sh.add_argument("--top", type=int, default=20, metavar="N",
                    help="Number of top functions to display (default: 20).")
    sh.add_argument("--show-artifacts", action="store_true",
                    help="Include compiled/vendored artifacts in results (hidden by default).")
    sh.add_argument("--save", action="store_true",
                    help="Save full JSON snapshot to results/.")

    return p.parse_args()


def main() -> None:
    args = parse_args()
    if args.cmd == "stress":
        cmd_stress(args)
    elif args.cmd == "show":
        cmd_show(args)


if __name__ == "__main__":
    main()
