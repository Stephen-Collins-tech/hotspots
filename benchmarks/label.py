#!/usr/bin/env python3
"""
Generate bug-commit file labels from a bare git clone.

Walks commits in [--after, --before) and flags files touched by bug-fix
commits (matched by fix-keyword regex). Emits one JSON line per file.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

FIX_RE = re.compile(
    r"\b(fix(es|ed|ing)?|bug|patch|defect|regression|broken|hotfix)\b",
    re.IGNORECASE,
)

SOURCE_EXTS = {
    ".py", ".ts", ".tsx", ".js", ".jsx", ".rb", ".go",
    ".c", ".h", ".cc", ".cpp", ".rs", ".java", ".cs",
}


def iter_bug_files(git_dir: str, after: str, before: str) -> dict[str, int]:
    """Return {file_path: 1} for files touched in bug-fix commits in the window."""
    cmd = [
        "git", "--git-dir", git_dir,
        "log", "--format=%H%x00%s", "--name-only",
        f"--after={after}", f"--before={before}",
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, check=True)
    bug_files: dict[str, int] = {}
    current_is_fix = False
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        if "\x00" in line:
            sha, subject = line.split("\x00", 1)
            current_is_fix = bool(FIX_RE.search(subject))
        elif current_is_fix:
            ext = Path(line).suffix.lower()
            if ext in SOURCE_EXTS:
                bug_files[line] = 1
    return bug_files


def all_files(git_dir: str, sha: str) -> list[str]:
    """List all source files present at the given commit SHA."""
    cmd = ["git", "--git-dir", git_dir, "ls-tree", "-r", "--name-only", sha]
    result = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return [
        line.strip()
        for line in result.stdout.splitlines()
        if Path(line.strip()).suffix.lower() in SOURCE_EXTS
    ]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("git_dir", help="Path to bare git clone")
    parser.add_argument("--after", required=True, help="Label window start (exclusive), e.g. 2024-01-01")
    parser.add_argument("--before", required=True, help="Label window end (exclusive), e.g. 2025-01-01")
    parser.add_argument("--feature-sha", required=True, help="SHA at which features were computed (for file list)")
    parser.add_argument("--out", required=True, help="Output JSON file path")
    args = parser.parse_args()

    bug_files = iter_bug_files(args.git_dir, args.after, args.before)
    files = all_files(args.git_dir, args.feature_sha)

    records = [
        {"file": f, "bug_linked": bug_files.get(f, 0)}
        for f in files
    ]

    n_bug = sum(r["bug_linked"] for r in records)
    print(
        f"  {args.git_dir}: {len(records)} files, {n_bug} bug-linked "
        f"({100*n_bug/max(len(records),1):.1f}%)",
        file=sys.stderr,
    )

    Path(args.out).write_text(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()
