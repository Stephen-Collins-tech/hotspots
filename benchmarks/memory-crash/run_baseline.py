#!/usr/bin/env python3
"""
run_baseline.py — Memory crash benchmark for hotspots against a target repo.

Core Docker mechanics (volume management, clone verification, image build,
memory-constrained run, stats monitoring) live in DockerBenchmark so they can
be reused for other benchmarks by providing a different BenchmarkConfig.

Usage:
    python3 run_baseline.py [options]

Exit codes:
    0  — analysis completed within the memory limit (no crash)
    1  — script error (build failed, clone failed, etc.)
    2  — container OOM-killed (exit 137) — expected baseline result
    3  — container exited with non-zero, non-OOM code
"""

from __future__ import annotations

import argparse
import os
import platform
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

# ── Paths ─────────────────────────────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).parent.resolve()
PROJECT_ROOT = (SCRIPT_DIR / "../..").resolve()
RESULTS_DIR = SCRIPT_DIR / "results"


# ── Logger ────────────────────────────────────────────────────────────────────

class Logger:
    """Tees timestamped output to stdout and a results file simultaneously."""

    BAR = "═" * 60

    def __init__(self, results_file: Path) -> None:
        results_file.parent.mkdir(parents=True, exist_ok=True)
        self._file = results_file.open("a", encoding="utf-8")
        self.results_file = results_file

    def log(self, msg: str = "") -> None:
        ts = datetime.now().strftime("%H:%M:%S")
        line = f"[{ts}] {msg}"
        print(line, flush=True)
        print(line, file=self._file, flush=True)

    def raw(self, msg: str) -> None:
        """Write a line without a timestamp prefix (for streamed subprocess output)."""
        print(msg, flush=True)
        print(msg, file=self._file, flush=True)

    def section(self, title: str) -> None:
        for line in ("", self.BAR, f"  {title}", self.BAR):
            self.raw(line)

    def close(self) -> None:
        self._file.close()


# ── BenchmarkConfig ───────────────────────────────────────────────────────────

@dataclass
class BenchmarkConfig:
    """
    Describes a single benchmark target. Swap this to run against a different
    repository or with a different analysis command.

    Example — adding a second benchmark target:
        config = BenchmarkConfig(
            repo_url="https://github.com/microsoft/vscode.git",
            volume_name="hotspots-vscode-repo",
            image_name="hotspots-memory-test",
            dockerfile=SCRIPT_DIR / "Dockerfile",
            build_context=PROJECT_ROOT,
            analyze_cmd=[
                "/usr/local/bin/hotspots", "analyze", "/repo",
                "--mode", "snapshot", "--format", "json", "--no-persist",
            ],
        )
    """

    # Repository to analyze
    repo_url: str
    # Docker named volume that persists the clone between runs
    volume_name: str
    # Docker image tag for the hotspots binary
    image_name: str
    # Dockerfile path (used when building the image)
    dockerfile: Path
    # Docker build context directory
    build_context: Path
    # Command to run inside the container (passed to docker run)
    analyze_cmd: list[str]
    # Memory ceiling for the ANALYSIS container only — the clone container is
    # never memory-constrained so this does not affect git operations.
    #
    # Suggested profiles:
    #   "2g"  — small VPS / self-hosted runner (crash expected, less trace data)
    #   "4g"  — GitLab shared / CircleCI small (default — best trace coverage)
    #   "7g"  — GitHub Actions ubuntu-latest (may complete after fixes)
    #
    # Combined with --memory-swap and --memory-swappiness=0 this is a hard
    # cgroup limit: OOM-killed the moment it exceeds the ceiling.
    memory_limit: str = "4g"
    # CPU limit — 1 vCPU mirrors the lowest common denominator CI runner.
    # Prevents the container from bursting onto spare host cores and producing
    # artificially fast (or memory-light) results.
    cpu_limit: str = "1"
    # Default results label
    label: str = "baseline"


# ── MemoryMonitor ─────────────────────────────────────────────────────────────

class MemoryMonitor(threading.Thread):
    """
    Background thread that polls `docker stats` every `interval` seconds and
    logs the memory usage line. Tracks the last observed value as `peak`.
    """

    def __init__(
        self,
        container_id: str,
        logger: Logger,
        start_time: float,
        interval: float = 2.0,
    ) -> None:
        super().__init__(daemon=True)
        self._container_id = container_id
        self._logger = logger
        self._start = start_time
        self._interval = interval
        self._stop = threading.Event()
        self.peak: str = "0 B"

    def run(self) -> None:
        while not self._stop.is_set():
            result = subprocess.run(
                [
                    "docker", "stats", "--no-stream",
                    "--format", "{{.MemUsage}}",
                    self._container_id,
                ],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                mem = result.stdout.strip()
                if mem:
                    elapsed = int(time.monotonic() - self._start)
                    self._logger.raw(f"[{elapsed:5d}s] {mem}")
                    self.peak = mem
            self._stop.wait(self._interval)

    def stop(self) -> None:
        self._stop.set()


# ── DockerBenchmark ───────────────────────────────────────────────────────────

class DockerBenchmark:
    """
    Encapsulates all Docker mechanics for a memory-constrained benchmark run.

    Steps:
        1. build_image()       — build (or skip) the hotspots Docker image
        2. prepare_volume()    — create volume, probe clone state, clone if needed
        3. get_repo_stats()    — commit count + tracked file count from inside volume
        4. run_analysis()      — start container, monitor memory, stream logs
    """

    # Snapshot the parent environment once so every subprocess receives the
    # full auth context (DOCKER_HOST, HOME, PATH, etc.) regardless of how
    # the script was launched (IDE, wrapper script, restricted shell).
    _ENV = os.environ.copy()

    def __init__(self, config: BenchmarkConfig, logger: Logger) -> None:
        self.config = config
        self.log = logger

    # ── Subprocess helpers ────────────────────────────────────────────────────

    def _docker(self, *args: str, check: bool = True) -> subprocess.CompletedProcess:
        """Run a docker command, capture output, raise on failure if check=True."""
        return subprocess.run(
            ["docker", *args],
            capture_output=True,
            text=True,
            check=check,
            env=self._ENV,
        )

    def _docker_stream(self, *args: str) -> int:
        """Run a docker command and stream stdout+stderr to the logger. Returns exit code."""
        proc = subprocess.Popen(
            ["docker", *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            env=self._ENV,
        )
        assert proc.stdout is not None
        for line in proc.stdout:
            self.log.raw(line.rstrip())
        proc.wait()
        return proc.returncode

    # ── Step 1: Image build ───────────────────────────────────────────────────

    def build_image(self, skip: bool = False) -> None:
        self.log.section("STEP 1: Build Docker image")
        cfg = self.config

        if skip:
            self.log.log("--skip-build: skipping image build.")
            result = self._docker("image", "inspect", cfg.image_name, check=False)
            if result.returncode != 0:
                self.log.log(
                    f"ERROR: image '{cfg.image_name}' not found and --skip-build was set.\n"
                    "       Run without --skip-build at least once."
                )
                raise SystemExit(1)
        else:
            self.log.log(f"Building {cfg.image_name}...")
            self.log.log(f"  Context:    {cfg.build_context}")
            self.log.log(f"  Dockerfile: {cfg.dockerfile}")
            t0 = time.monotonic()
            rc = self._docker_stream(
                "build",
                "--file", str(cfg.dockerfile),
                "--tag", cfg.image_name,
                str(cfg.build_context),
            )
            if rc != 0:
                self.log.log(f"ERROR: docker build exited {rc}")
                raise SystemExit(1)
            self.log.log(f"Build completed in {time.monotonic() - t0:.0f}s")

        image_id = self._docker(
            "inspect", "--format={{.Id}}", cfg.image_name
        ).stdout.strip()[:19]
        self.log.log(f"Image: {cfg.image_name}  id={image_id}")

    # ── Step 2: Volume + clone ────────────────────────────────────────────────

    def prepare_volume(self, skip_clone: bool = False, reset: bool = False) -> None:
        self.log.section("STEP 2: Prepare repository (Docker volume)")
        cfg = self.config
        self.log.log(f"Volume: {cfg.volume_name}")

        if reset:
            self.log.log(f"--reset-repo: removing volume {cfg.volume_name}...")
            self._docker("volume", "rm", cfg.volume_name, check=False)

        # Idempotent create
        self._docker("volume", "create", cfg.volume_name)

        if skip_clone:
            self.log.log("--skip-clone: trusting volume contents as-is.")
            return

        state = self._probe_clone_state()
        self.log.log(f"Clone state: {state}")

        if state == "ok":
            self.log.log("Volume contains a valid clone — no clone needed.")
        else:
            if state == "partial":
                self.log.log("WARN: Partial or interrupted clone detected.")
                self.log.log("      Wiping volume and re-cloning from scratch...")
                self._docker("volume", "rm", cfg.volume_name)
                self._docker("volume", "create", cfg.volume_name)
            self._clone()

    def _probe_clone_state(self) -> str:
        """
        Returns one of:
            "ok"      — .git exists and HEAD resolves to a valid commit
            "partial" — .git exists but HEAD is invalid (interrupted clone)
            "missing" — no .git directory
        """
        cfg = self.config
        result = self._docker(
            "run", "--rm",
            "-v", f"{cfg.volume_name}:/repo:ro",
            "--entrypoint", "sh",
            cfg.image_name,
            "-c",
            (
                "if [ ! -d /repo/.git ]; then echo missing; "
                "elif ! git -C /repo rev-parse --verify HEAD > /dev/null 2>&1; "
                "then echo partial; "
                "else echo ok; fi"
            ),
            check=False,
        )
        return result.stdout.strip() if result.returncode == 0 else "missing"

    def _clone(self) -> None:
        cfg = self.config
        self.log.log(f"Cloning {cfg.repo_url} (full history — may take several minutes)...")
        t0 = time.monotonic()
        rc = self._docker_stream(
            "run", "--rm",
            "-v", f"{cfg.volume_name}:/repo",
            "--entrypoint", "git",
            cfg.image_name,
            # -c pack.threads=2 caps the number of threads git uses when
            # resolving and compressing the pack file during clone. Without
            # this, git defaults to one thread per logical CPU, which can
            # exhaust RAM on small machines (2–4 core) cloning a large repo
            # like expo/expo. 2 threads is sufficient for any machine size
            # while keeping clone memory bounded.
            "-c", "pack.threads=2",
            "clone", "--progress", cfg.repo_url, "/repo",
        )
        if rc != 0:
            self.log.log(f"ERROR: git clone exited {rc}")
            raise SystemExit(1)
        self.log.log(f"Clone completed in {time.monotonic() - t0:.0f}s")

    # ── Repo stats ────────────────────────────────────────────────────────────

    def get_repo_stats(self) -> dict[str, str]:
        cfg = self.config
        self.log.log("Collecting repo stats...")

        def git_query(cmd: str) -> str:
            r = self._docker(
                "run", "--rm",
                "-v", f"{cfg.volume_name}:/repo:ro",
                "--entrypoint", "sh",
                cfg.image_name,
                "-c", cmd,
                check=False,
            )
            return r.stdout.strip() if r.returncode == 0 else "unknown"

        stats = {
            "commits": git_query("git -C /repo rev-list --count HEAD 2>/dev/null"),
            "files":   git_query("git -C /repo ls-files 2>/dev/null | wc -l | tr -d ' '"),
        }
        self.log.log(f"  Total commits:  {stats['commits']}")
        self.log.log(f"  Tracked files:  {stats['files']}")
        return stats

    # ── Step 3: Run analysis ──────────────────────────────────────────────────

    def run_analysis(self, memory_limit: str, cpu_limit: str) -> tuple[int, int, str]:
        """
        Starts the analysis container under the given resource constraints,
        streams its logs, monitors memory every 2s, and returns
        (exit_code, elapsed_seconds, peak_memory_string).

        Defaults mirror GitHub Actions ubuntu-latest: 2 vCPU, 7 GB RAM.
        """
        self.log.section(
            f"STEP 3: Run analysis  [{cpu_limit} vCPU / {memory_limit} RAM]"
        )
        cfg = self.config

        cmd_str = " ".join(cfg.analyze_cmd)
        self.log.log(f"Command:  {cmd_str}")
        self.log.log(f"CPUs:     --cpus={cpu_limit}")
        self.log.log(
            f"Memory:   --memory={memory_limit} --memory-swap={memory_limit} "
            "--memory-swappiness=0  (hard ceiling, swap off, page reclaim off)"
        )
        self.log.log(f"Volume:   {cfg.volume_name} → /repo (read-only)")
        self.log.log("")
        self.log.log("Starting container...")

        container_name = f"hotspots-bench-{int(time.time())}"
        result = self._docker(
            "run",
            "--detach",
            "--name", container_name,
            # CPU constraint — matches target CI runner core count.
            # Without this the container can burst onto spare host cores,
            # making timing and memory behaviour non-representative.
            "--cpus", cpu_limit,
            # Hard memory ceiling — three flags together enforce it strictly:
            #   --memory            cgroup hard limit; OOM-killed if exceeded
            #   --memory-swap       equal to --memory → zero swap allowed
            #   --memory-swappiness=0  kernel will not reclaim anonymous pages
            #                          within the limit, so the kill fires exactly
            #                          at --memory rather than being deferred
            "--memory", memory_limit,
            "--memory-swap", memory_limit,
            "--memory-swappiness=0",
            "--oom-kill-disable=false",
            "--volume", f"{cfg.volume_name}:/repo:ro",
            cfg.image_name,
            *cfg.analyze_cmd,
        )
        container_id = result.stdout.strip()
        self.log.log(f"Container: {container_name}  ({container_id[:12]})")

        # Memory monitor (background thread)
        self.log.section("MEMORY TRACE")
        self.log.log("Sampling docker stats every 2s...")
        self.log.log("")

        t0 = time.monotonic()
        monitor = MemoryMonitor(container_id, self.log, t0)
        monitor.start()

        # Stream container logs (blocks until container exits)
        self._docker_stream("logs", "--follow", container_id)

        # Give the monitor one final poll then shut it down
        time.sleep(2.5)
        monitor.stop()
        monitor.join(timeout=5)

        elapsed = int(time.monotonic() - t0)

        # Capture exit code before removing the container record
        inspect = self._docker(
            "inspect", "--format={{.State.ExitCode}}", container_id, check=False
        )
        exit_code = int(inspect.stdout.strip()) if inspect.returncode == 0 else 137
        self._docker("rm", "-f", container_id, check=False)

        return exit_code, elapsed, monitor.peak


# ── main ──────────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Run hotspots analyze against expo/expo in a memory-constrained Docker container.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Exit codes:\n"
            "  0  analysis completed within the memory limit\n"
            "  1  script error\n"
            "  2  OOM kill (exit 137) — expected baseline result\n"
            "  3  non-zero non-OOM container exit\n"
        ),
    )
    p.add_argument(
        "--memory",
        default="4g",
        metavar="LIMIT",
        help=(
            "Memory ceiling for the analysis container (default: 4g). "
            "Swap is disabled so OOM kill fires at exactly this limit. "
            "Does not affect the git clone step. "
            "Suggested: 2g (small VPS, fast crash), "
            "4g (GitLab shared / CircleCI small, best trace), "
            "7g (GitHub Actions ubuntu-latest)."
        ),
    )
    p.add_argument(
        "--cpus",
        default="1",
        metavar="N",
        help="Number of CPUs available to the container (default: 1). "
             "Use fractional values, e.g. 0.5, 1, 2. "
             "Mirrors a constrained CI runner — prevents host core bursting.",
    )
    p.add_argument(
        "--label",
        default="baseline",
        metavar="LABEL",
        help="Prefix for the results filename, e.g. 'post-tier1' (default: baseline).",
    )
    p.add_argument(
        "--skip-clone",
        action="store_true",
        help="Trust whatever is in the Docker volume — skip the clone-state probe.",
    )
    p.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip rebuilding the Docker image (image must already exist).",
    )
    p.add_argument(
        "--reset-repo",
        action="store_true",
        help="Delete and recreate the Docker volume, forcing a full re-clone.",
    )
    p.add_argument(
        "--jobs",
        type=int,
        default=None,
        metavar="N",
        help="Worker threads for hotspots analyze (--jobs flag). "
             "Default: unset (hotspots uses all logical CPUs).",
    )
    p.add_argument(
        "--callgraph-skip-above",
        type=int,
        default=None,
        metavar="N",
        help="Skip all call graph algorithms when the repo exceeds N functions. "
             "Default: unset (always compute call graph).",
    )
    p.add_argument(
        "--per-function-touches",
        action="store_true",
        default=False,
        help="Enable per-function git log -L touch metrics. "
             "Default: disabled (uses file-level batching). "
             "Enabling this on a large cold repo will dominate CPU time.",
    )
    return p.parse_args()


def main() -> None:
    args = parse_args()

    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    results_file = RESULTS_DIR / f"{args.label}-{timestamp}.log"
    logger = Logger(results_file)

    # ── Benchmark target definition ───────────────────────────────────────────
    analyze_cmd = [
        "/usr/local/bin/hotspots", "analyze", "/repo",
        "--mode", "snapshot",
        "--format", "json",
        "--no-persist",
    ]
    if args.jobs is not None:
        analyze_cmd += ["--jobs", str(args.jobs)]
    callgraph_skip = getattr(args, "callgraph_skip_above", None)
    if callgraph_skip is not None:
        analyze_cmd += ["--callgraph-skip-above", str(callgraph_skip)]
    per_fn_touches = getattr(args, "per_function_touches", False)
    if not per_fn_touches:
        analyze_cmd += ["--no-per-function-touches"]

    config = BenchmarkConfig(
        repo_url="https://github.com/expo/expo.git",
        volume_name="hotspots-expo-repo",
        image_name="hotspots-memory-test",
        dockerfile=SCRIPT_DIR / "Dockerfile",
        build_context=PROJECT_ROOT,
        analyze_cmd=analyze_cmd,
        memory_limit=args.memory,
        cpu_limit=args.cpus,
        label=args.label,
    )

    # ── Header ────────────────────────────────────────────────────────────────
    logger.section("HOTSPOTS MEMORY CRASH BENCHMARK")
    logger.log(f"Label:          {args.label}")
    logger.log(f"Timestamp:      {timestamp}")
    logger.log(f"Results file:   {results_file}")
    logger.log(f"CPUs:           {args.cpus} vCPU")
    logger.log(f"Jobs:           {args.jobs if args.jobs is not None else 'default (all CPUs)'}")
    logger.log(f"CallgraphSkip:  {callgraph_skip if callgraph_skip is not None else 'disabled (always compute)'}")
    logger.log(f"PerFnTouches:   {'enabled' if per_fn_touches else 'disabled (file-level batching)'}")
    logger.log(f"Memory limit:   {args.memory} (hard ceiling, swap off)")
    logger.log(f"Repo:           {config.repo_url}")
    logger.log(f"Volume:         {config.volume_name}")
    docker_ver = subprocess.run(
        ["docker", "--version"], capture_output=True, text=True
    ).stdout.strip()
    logger.log(f"Docker:         {docker_ver}")
    logger.log(f"Host OS:        {platform.system()} {platform.release()}")

    if platform.system() == "Darwin":
        logger.log("")
        logger.log("  NOTE (macOS / Docker Desktop):")
        logger.log("  Memory limits are enforced inside a Linux VM that uses memory")
        logger.log("  ballooning. The container may sustain ~99% usage rather than")
        logger.log("  receiving a clean OOM kill. This differs from native Linux")
        logger.log("  (GitHub Actions, Fly.io) where cgroup limits fire decisively.")
        logger.log("  Results are still valid for relative before/after comparison.")
        logger.log("  For kill-accurate results, run on a native Linux host.")
        logger.log("")

    # ── Run benchmark ─────────────────────────────────────────────────────────
    bench = DockerBenchmark(config, logger)

    try:
        bench.build_image(skip=args.skip_build)
        bench.prepare_volume(skip_clone=args.skip_clone, reset=args.reset_repo)
        bench.get_repo_stats()
        exit_code, elapsed, peak_mem = bench.run_analysis(args.memory, args.cpus)
    except SystemExit as e:
        logger.close()
        sys.exit(e.code)
    except Exception as e:
        logger.log(f"FATAL: {e}")
        logger.close()
        sys.exit(1)

    # ── Results summary ───────────────────────────────────────────────────────
    logger.section("RESULTS")
    logger.log(f"Run duration:       {elapsed}s")
    logger.log(f"Exit code:          {exit_code}")
    logger.log(f"Peak memory (last): {peak_mem}")
    logger.log(f"Volume:             {config.volume_name} (retained for next run)")
    logger.log("")

    bar = "═" * 60

    if exit_code == 137:
        logger.log("RESULT: OOM KILL (exit 137)")
        logger.log("  The kernel OOM killer terminated the process because it")
        logger.log(f"  exceeded the {args.memory} ceiling with swap disabled.")
        logger.log("")
        logger.log("  Baseline captured. Re-run with --label post-tierN after")
        logger.log("  each fix to track improvement.")
        logger.raw(f"\n{bar}\n  BASELINE CAPTURED → {results_file}\n{bar}")
        logger.close()
        sys.exit(2)

    elif exit_code == 0:
        logger.log("RESULT: COMPLETED SUCCESSFULLY (exit 0)")
        logger.log(f"  Analysis finished within the {args.memory} limit.")
        logger.log("  Memory limit may be too generous, or a fix was applied.")
        logger.raw(f"\n{bar}\n  COMPLETED (no crash) → {results_file}\n{bar}")
        logger.close()
        sys.exit(0)

    else:
        logger.log(f"RESULT: NON-ZERO EXIT (exit {exit_code})")
        logger.log("  The process exited with a non-OOM error. See log above.")
        logger.raw(f"\n{bar}\n  ERROR (exit {exit_code}) → {results_file}\n{bar}")
        logger.close()
        sys.exit(3)


if __name__ == "__main__":
    main()
