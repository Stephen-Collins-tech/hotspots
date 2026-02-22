from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import List, Optional


class Runner:
    def __init__(self, repo_root: Path):
        self.repo_root = repo_root.resolve()
        self.bin = self.repo_root / "target" / "release" / "hotspots"

    def ensure_built(self) -> None:
        if not self.bin.exists():
            subprocess.run(["cargo", "build", "--release"], check=True, cwd=self.repo_root)

    def run(self, args: List[str], cwd: Path, out: Optional[Path] = None) -> subprocess.CompletedProcess:
        cmd = [str(self.bin)] + args
        if out is not None:
            with open(out, "w") as f:
                proc = subprocess.run(
                    cmd,
                    cwd=cwd,
                    stdout=f,
                    stderr=subprocess.PIPE,
                    text=True,
                    check=False,
                )
                if proc.stderr:
                    # Keep stderr out of JSON files
                    print(proc.stderr)
                return proc
        return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=False)

    def git(self, args: List[str], cwd: Path) -> None:
        subprocess.run(["git"] + args, cwd=cwd, check=True, text=True)


def init_test_repo(base: Path) -> Path:
    repo = base / "e2e-repo"
    if repo.exists():
        subprocess.run(["rm", "-rf", str(repo)])
    repo.mkdir(parents=True, exist_ok=True)

    subprocess.run(["git", "init"], cwd=repo, check=True)
    subprocess.run(["git", "config", "user.name", "CI"], cwd=repo, check=True)
    subprocess.run(["git", "config", "user.email", "ci@example.com"], cwd=repo, check=True)
    (repo / ".gitignore").write_text(".hotspots/\n")
    subprocess.run(["git", "add", ".gitignore"], cwd=repo, check=True)
    subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True)
    return repo


def write_ts(repo: Path, content: str) -> None:
    src = repo / "src"
    src.mkdir(exist_ok=True)
    (src / "main.ts").write_text(content)
    subprocess.run(["git", "add", "src/main.ts"], cwd=repo, check=True)
    subprocess.run(["git", "commit", "-m", "update"], cwd=repo, check=True)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())

