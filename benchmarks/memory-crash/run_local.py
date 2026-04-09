#!/usr/bin/env python3
"""
run_local.py — Run the hotspots memory benchmark locally using Docker.

Docker enforces --memory limits via cgroup v2 on the Linux kernel inside
Docker Desktop's VM (macOS) or natively on Linux. When a process exceeds
the ceiling the OOM killer sends SIGKILL → exit 137.

CPU and memory are sampled every second via `docker stats` and plotted
after the run (requires matplotlib; falls back to CSV if unavailable).

Usage:
    python3 run_local.py [options]
    python3 run_local.py --memory 2g --skip-clone
    python3 run_local.py --skip-build --skip-clone --memory 4g

Exit codes:
    0  — analysis completed within the memory limit
    1  — script error
    2  — OOM kill (exit 137) — expected baseline result
    3  — non-zero non-OOM exit
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
PROJECT_ROOT = (SCRIPT_DIR / "../..").resolve()
EXPO_DIR     = SCRIPT_DIR / "repos" / "expo"
RESULTS_DIR  = SCRIPT_DIR / "results"
IMAGE_NAME   = "hotspots-bench"
REPO_URL     = "https://github.com/expo/expo.git"

BAR = "═" * 60


# ── Helpers ───────────────────────────────────────────────────────────────────

def log(msg: str = "") -> None:
    ts = datetime.now().strftime("%H:%M:%S")
    print(f"[{ts}] {msg}", flush=True)


def section(title: str) -> None:
    for line in ("", BAR, f"  {title}", BAR):
        print(line, flush=True)


def parse_memory(s: str) -> int:
    """Convert '4g' / '512m' to megabytes."""
    s = s.strip().lower()
    if s.endswith("g"):
        return int(s[:-1]) * 1024
    if s.endswith("m"):
        return int(s[:-1])
    raise ValueError(f"Unrecognised memory format: {s!r}  (use e.g. 2g, 4g, 512m)")


def parse_docker_size_mb(s: str) -> float:
    """Parse Docker size strings like '1.5GiB', '512MiB', '123kB' → MB."""
    s = s.strip()
    for suffix, factor in [
        ("GiB", 1024.0), ("MiB", 1.0), ("KiB", 1.0 / 1024),
        ("GB",  1000.0), ("MB",  1.0), ("kB",  1.0 / 1000),
        ("B",   1.0 / (1024 * 1024)),
    ]:
        if s.endswith(suffix):
            return float(s[: -len(suffix)]) * factor
    return 0.0


# ── Stats sampler ─────────────────────────────────────────────────────────────

@dataclass
class Sample:
    elapsed_s: float
    mem_mb:    float
    cpu_pct:   float


@dataclass
class WatchdogConfig:
    """Kill the container if utilization stays above threshold for sustain_s seconds."""
    mem_pct:   float = 85.0   # % of memory_limit_mb; 0 = disabled
    cpu_pct:   float = 0.0    # absolute CPU%; 0 = disabled
    sustain_s: float = 10.0   # seconds the condition must hold
    trigger_pct: float = 0.9  # fraction of samples in window that must exceed threshold


@dataclass
class Sampler:
    container_id:    str
    memory_limit_mb: float
    watchdog:        WatchdogConfig = field(default_factory=WatchdogConfig)
    interval_s:      float = 1.0
    samples:         list[Sample] = field(default_factory=list)
    killed_reason:   str = ""          # set if watchdog triggered
    _stop:           threading.Event = field(default_factory=threading.Event)
    _thread:         threading.Thread | None = None

    def start(self) -> None:
        self._t0 = time.monotonic()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=10)

    @property
    def watchdog_triggered(self) -> bool:
        return bool(self.killed_reason)

    def _run(self) -> None:
        while not self._stop.is_set():
            result = subprocess.run(
                [
                    "docker", "stats",
                    "--no-stream",
                    "--format", "{{json .}}",
                    self.container_id,
                ],
                capture_output=True, text=True, check=False,
            )
            if result.returncode == 0 and result.stdout.strip():
                for line in result.stdout.strip().splitlines():
                    try:
                        data = json.loads(line)
                        mem_mb  = parse_docker_size_mb(data["MemUsage"].split("/")[0])
                        cpu_pct = float(data["CPUPerc"].rstrip("%"))
                        elapsed = time.monotonic() - self._t0
                        self.samples.append(Sample(elapsed, mem_mb, cpu_pct))
                    except (KeyError, ValueError, json.JSONDecodeError):
                        pass
                self._check_watchdog()
            # docker stats --no-stream takes ~1s; skip extra sleep unless faster
            if self.interval_s > 1.0:
                time.sleep(self.interval_s - 1.0)

    def _check_watchdog(self) -> None:
        if not self.samples:
            return
        cfg = self.watchdog
        now = self.samples[-1].elapsed_s
        window = [s for s in self.samples if s.elapsed_s >= now - cfg.sustain_s]
        # Need enough samples to cover the sustain window (at least 3 or half the window)
        if len(window) < max(3, cfg.sustain_s / 2):
            return

        def _fraction_above(values: list[float], threshold: float) -> float:
            return sum(1 for v in values if v >= threshold) / len(values)

        reason = ""
        if cfg.mem_pct > 0:
            threshold_mb = self.memory_limit_mb * cfg.mem_pct / 100
            frac = _fraction_above([s.mem_mb for s in window], threshold_mb)
            if frac >= cfg.trigger_pct:
                peak = max(s.mem_mb for s in window)
                reason = (
                    f"memory ≥ {cfg.mem_pct:.0f}% of limit "
                    f"({threshold_mb:.0f} MB) for {cfg.sustain_s:.0f}s "
                    f"(peak {peak:.0f} MB, {frac*100:.0f}% of samples)"
                )
        if not reason and cfg.cpu_pct > 0:
            frac = _fraction_above([s.cpu_pct for s in window], cfg.cpu_pct)
            if frac >= cfg.trigger_pct:
                reason = (
                    f"CPU ≥ {cfg.cpu_pct:.0f}% for {cfg.sustain_s:.0f}s "
                    f"({frac*100:.0f}% of samples)"
                )

        if reason:
            self.killed_reason = reason
            log(f"WATCHDOG: killing container — {reason}")
            result = subprocess.run(
                ["docker", "kill", self.container_id],
                capture_output=True, text=True, check=False,
            )
            if result.returncode != 0:
                log(f"WATCHDOG: docker kill failed: {result.stderr.strip()}")
            self._stop.set()


# ── Plot / CSV ────────────────────────────────────────────────────────────────

def save_results(
    samples: list[Sample],
    memory_limit: str,
    memory_mb: int,
    exit_code: int,
    oom_killed: bool,
    watchdog_reason: str,
    label: str,
    timestamp: str,
) -> None:
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    stem = RESULTS_DIR / f"{label}-{timestamp}"

    # Always write CSV
    csv_path = stem.with_suffix(".csv")
    with csv_path.open("w") as f:
        f.write("elapsed_s,mem_mb,cpu_pct\n")
        for s in samples:
            f.write(f"{s.elapsed_s:.2f},{s.mem_mb:.1f},{s.cpu_pct:.2f}\n")
    log(f"Samples:  {csv_path}  ({len(samples)} points)")

    if not samples:
        log("WARN: no samples collected — skipping plot")
        return

    if not HAS_MATPLOTLIB:
        log("WARN: matplotlib not installed — skipping plot (pip install matplotlib)")
        return

    if watchdog_reason:
        result_str = f"WATCHDOG KILL"
    elif oom_killed:
        result_str = "OOM KILL"
    else:
        result_str = "OK" if exit_code == 0 else f"exit {exit_code}"

    times = [s.elapsed_s for s in samples]
    mem   = [s.mem_mb    for s in samples]
    cpu   = [s.cpu_pct   for s in samples]
    end_t = times[-1]

    fig, (ax_mem, ax_cpu) = plt.subplots(2, 1, figsize=(10, 6), sharex=True)
    fig.suptitle(
        f"hotspots analyze expo/expo — {memory_limit} limit — {result_str}",
        fontsize=13, fontweight="bold",
    )

    # Memory plot
    ax_mem.plot(times, mem, color="#e74c3c", linewidth=1.5, label="RSS")
    ax_mem.axhline(memory_mb, color="#e74c3c", linestyle="--", linewidth=1,
                   alpha=0.5, label=f"limit ({memory_limit})")
    if oom_killed:
        ax_mem.axvline(end_t, color="#c0392b", linestyle=":", linewidth=1.5,
                       label="OOM kill")
    elif watchdog_reason:
        ax_mem.axvline(end_t, color="#e67e22", linestyle=":", linewidth=1.5,
                       label="watchdog kill")
    ax_mem.set_ylabel("Memory (MB)")
    ax_mem.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: f"{x:.0f}"))
    ax_mem.legend(fontsize=8, loc="upper left")
    ax_mem.grid(True, alpha=0.3)

    # CPU plot
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
    log(f"Plot:     {png_path}")


# ── Steps ─────────────────────────────────────────────────────────────────────

def build_image(skip: bool) -> None:
    section("STEP 1: Build Docker image")
    if skip:
        log(f"--skip-build: using existing image '{IMAGE_NAME}'")
        return

    dockerfile = SCRIPT_DIR / "Dockerfile"
    log(f"Building '{IMAGE_NAME}' (context: {PROJECT_ROOT})...")
    t0 = time.monotonic()
    result = subprocess.run(
        [
            "docker", "build",
            "-t", IMAGE_NAME,
            "-f", str(dockerfile),
            str(PROJECT_ROOT),
        ],
        check=False,
    )
    if result.returncode != 0:
        log(f"ERROR: docker build failed (exit {result.returncode})")
        sys.exit(1)
    log(f"Build completed in {time.monotonic() - t0:.0f}s")


def ensure_clone(skip: bool) -> None:
    section("STEP 2: Prepare expo/expo repository")
    if skip:
        log(f"--skip-clone: trusting {EXPO_DIR}")
        return
    if (EXPO_DIR / ".git").exists():
        log(f"Clone exists: {EXPO_DIR}")
        return
    EXPO_DIR.parent.mkdir(parents=True, exist_ok=True)
    log(f"Cloning {REPO_URL} → {EXPO_DIR}")
    log(f"  Full history ~3-4 GB — this takes a while.")
    t0 = time.monotonic()
    result = subprocess.run(
        ["git", "-c", "pack.threads=2", "clone", "--progress", REPO_URL, str(EXPO_DIR)],
        check=False,
    )
    if result.returncode != 0:
        log(f"ERROR: git clone failed (exit {result.returncode})")
        sys.exit(1)
    log(f"Clone completed in {time.monotonic() - t0:.0f}s")


def _wait_for_cidfile(path: Path, timeout: float = 15.0) -> str | None:
    """Poll until Docker writes the container ID to the cidfile."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            cid = path.read_text().strip()
            if cid:
                return cid
        except FileNotFoundError:
            pass
        time.sleep(0.1)
    return None


def run_analysis(
    memory: str,
    memory_mb: int,
    cpus: str,
    jobs: int | None,
    callgraph_skip_above: int | None,
    watchdog_cfg: WatchdogConfig,
) -> tuple[int, bool, str, list[Sample]]:
    """
    Run the analysis container, streaming its output directly to the terminal.

    Stats are sampled in a background thread via `docker stats`. The container
    ID is obtained via --cidfile so we can start sampling without --detach.
    Returns (exit_code, oom_killed, watchdog_reason, samples).
    """
    jobs_label = f", {jobs} threads" if jobs is not None else ""
    skip_label = f", skip-callgraph>{callgraph_skip_above}" if callgraph_skip_above is not None else ""
    section(f"STEP 3: Run analysis  [{cpus} CPU / {memory} RAM{jobs_label}{skip_label}]")
    log(f"Memory limit: {memory}  (hard cgroup ceiling — no swap)")
    if watchdog_cfg.mem_pct > 0:
        thresh_mb = memory_mb * watchdog_cfg.mem_pct / 100
        log(f"Watchdog mem: kill if ≥ {watchdog_cfg.mem_pct:.0f}% ({thresh_mb:.0f} MB) "
            f"for {watchdog_cfg.sustain_s:.0f}s")
    if watchdog_cfg.cpu_pct > 0:
        log(f"Watchdog CPU: kill if ≥ {watchdog_cfg.cpu_pct:.0f}% "
            f"for {watchdog_cfg.sustain_s:.0f}s")
    log(f"Repo:         {EXPO_DIR}")
    log("")

    cidfile = Path(tempfile.mktemp(suffix=".cid"))
    sampler: Sampler | None = None

    try:
        env_args = []
        if jobs is not None:
            env_args += ["-e", f"BENCH_JOBS={jobs}"]
        if callgraph_skip_above is not None:
            env_args += ["-e", f"BENCH_CALLGRAPH_SKIP_ABOVE={callgraph_skip_above}"]

        proc = subprocess.Popen(
            [
                "docker", "run",
                "--cidfile", str(cidfile),
                f"--memory={memory}",
                f"--memory-swap={memory}",   # == memory → zero swap
                f"--cpus={cpus}",
                *env_args,
                "-v", f"{EXPO_DIR}:/repo",
                IMAGE_NAME,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )

        # Background thread: wait for cidfile, then start stats + watchdog
        def _start_sampler() -> None:
            nonlocal sampler
            cid = _wait_for_cidfile(cidfile)
            if not cid:
                log("WARN: timed out waiting for container ID — stats unavailable")
                return
            log(f"Container: {cid[:12]}  (sampling every ~1s)")
            sampler = Sampler(
                container_id=cid,
                memory_limit_mb=float(memory_mb),
                watchdog=watchdog_cfg,
            )
            sampler.start()

        sampler_thread = threading.Thread(target=_start_sampler, daemon=True)
        sampler_thread.start()

        # Stream container output directly to the terminal
        assert proc.stdout
        for line in proc.stdout:
            print(line, end="", flush=True)
        proc.wait()

        sampler_thread.join(timeout=15)
        if sampler:
            sampler.stop()

        exit_code = proc.returncode

        # OOM detection: container is stopped but not yet removed
        oom_killed = False
        container_id = cidfile.read_text().strip() if cidfile.exists() else None
        if container_id and (sampler is None or not sampler.watchdog_triggered):
            inspect = subprocess.run(
                ["docker", "inspect", container_id,
                 "--format", "{{.State.OOMKilled}}"],
                capture_output=True, text=True, check=False,
            )
            if inspect.stdout.strip().lower() == "true":
                oom_killed = True
                exit_code = 137

        # Clean up
        if container_id:
            subprocess.run(
                ["docker", "rm", container_id],
                capture_output=True, check=False,
            )

    finally:
        cidfile.unlink(missing_ok=True)

    return (
        exit_code,
        oom_killed,
        sampler.killed_reason if sampler else "",
        sampler.samples if sampler else [],
    )


# ── CLI ───────────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Run hotspots memory benchmark locally via Docker.",
        epilog=(
            "Exit codes:\n"
            "  0  analysis completed within the memory limit\n"
            "  1  script error\n"
            "  2  OOM kill (exit 137) — expected baseline result\n"
            "  3  non-zero non-OOM exit\n"
            "  4  watchdog kill — sustained high CPU or memory\n"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--memory", default="4g", metavar="LIMIT",
                   help="Memory ceiling (default: 4g). Hard cgroup limit, no swap.")
    p.add_argument("--cpus", default="1", metavar="N",
                   help="CPU count for docker run (default: 1).")
    p.add_argument("--label", default="bench", metavar="LABEL",
                   help="Results filename prefix (default: bench).")
    p.add_argument("--jobs", type=int, default=None, metavar="N",
                   help="Worker threads for hotspots analyze (passed as BENCH_JOBS env var). "
                        "Default: unset (hotspots uses all logical CPUs).")
    p.add_argument("--callgraph-skip-above", type=int, default=None, metavar="N",
                   help="Skip all call graph algorithms when the repo exceeds N functions "
                        "(passed as BENCH_CALLGRAPH_SKIP_ABOVE env var). "
                        "Default: unset (no skip). Use to isolate analysis CPU from graph CPU.")
    p.add_argument("--skip-build", action="store_true",
                   help="Skip docker build, use existing image.")
    p.add_argument("--skip-clone", action="store_true",
                   help="Skip clone check, trust existing repos/expo.")

    wd = p.add_argument_group("watchdog — kill container on sustained high utilization")
    wd.add_argument("--watchdog-mem", type=float, default=85.0, metavar="PCT",
                    help="Kill if memory ≥ PCT%% of limit for --watchdog-secs (default: 85). "
                         "0 to disable.")
    wd.add_argument("--watchdog-cpu", type=float, default=0.0, metavar="PCT",
                    help="Kill if CPU ≥ PCT%% for --watchdog-secs (default: 0 = disabled).")
    wd.add_argument("--watchdog-secs", type=float, default=10.0, metavar="S",
                    help="Sustain window in seconds (default: 10).")

    return p.parse_args()


def main() -> None:
    args = parse_args()

    try:
        memory_mb = parse_memory(args.memory)
    except ValueError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)

    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")

    section("HOTSPOTS MEMORY BENCHMARK — LOCAL DOCKER")
    log(f"Memory:  {args.memory}  ({memory_mb} MB hard ceiling)")
    log(f"CPUs:    {args.cpus}")
    log(f"Jobs:    {args.jobs if args.jobs is not None else 'default (all CPUs)'}")
    skip = getattr(args, "callgraph_skip_above", None)
    log(f"CallgraphSkipAbove: {skip if skip is not None else 'disabled (always compute)'}")
    log(f"Repo:    expo/expo  ({EXPO_DIR})")
    log(f"Image:   {IMAGE_NAME}")

    watchdog_cfg = WatchdogConfig(
        mem_pct=args.watchdog_mem,
        cpu_pct=args.watchdog_cpu,
        sustain_s=args.watchdog_secs,
    )

    build_image(args.skip_build)
    ensure_clone(args.skip_clone)
    exit_code, oom_killed, watchdog_reason, samples = run_analysis(
        args.memory, memory_mb, args.cpus, args.jobs,
        getattr(args, "callgraph_skip_above", None),
        watchdog_cfg,
    )

    section("RESULT")
    save_results(samples, args.memory, memory_mb, exit_code, oom_killed,
                 watchdog_reason, args.label, timestamp)

    if watchdog_reason:
        log(f"WATCHDOG KILL — {watchdog_reason}")
        log(f"  Container was killed before natural completion.")
        print(BAR)
        sys.exit(4)
    elif oom_killed or exit_code == 137:
        log("OOM KILL (exit 137)")
        log(f"  Killed at the {args.memory} ceiling.")
        log(f"  Fix a tier, rebuild, and re-run:")
        log(f"  uv run python run_local.py --skip-clone --memory {args.memory}")
        print(BAR)
        sys.exit(2)
    elif exit_code == 0:
        log("COMPLETED SUCCESSFULLY (exit 0)")
        print(BAR)
        sys.exit(0)
    else:
        log(f"NON-ZERO EXIT (exit {exit_code})")
        print(BAR)
        sys.exit(3)


if __name__ == "__main__":
    main()
