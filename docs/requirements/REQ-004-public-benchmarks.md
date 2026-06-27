# REQ-004 — Public Benchmark Corpus and Baseline Scores

**Status:** ready to implement  
**Purpose:** Establish a fixed, reproducible benchmark that publicly demonstrates hotspots accuracy on real bug prediction, and tracks how each improvement raises that score.

---

## Problem

hotspots has no public proof of accuracy. Users and evaluators must take the tool's claims on faith. A published benchmark — fixed corpus, pinned SHAs, known ground truth, versioned scores — makes every claim verifiable and reproducible by anyone.

This is the public equivalent of the internal temporal holdout protocol used in hotspots-research (F54). Same design, same label logic, written as instructions anyone can follow.

## What to build

1. A fixed 8-repo benchmark corpus with pinned commit SHAs and a fixed label window
2. A `benchmarks/run.sh` eval script that runs `hotspots analyze` and measures ρ
3. A `benchmarks/RESULTS.md` table published and updated with each release

The benchmark must be runnable by anyone with git, python3, and the `hotspots` binary. No API keys. No proprietary data. No internal tooling.

---

## Corpus

These 7 repos are the fixed benchmark set. Do not add or remove repos without incrementing `corpus.json` version.

| Repo | Language | Domain | Feature SHA (last commit before 2024-01-01) |
|---|---|---|---|
| `facebook/react` | JavaScript | UI framework | `bf859705b55a8ccaedbed8546cd4d9c6c003bf62` |
| `golang/go` | Go | Compiler/runtime | `b25f5558c69140deb652337afaab5c1186cd0ff1` |
| `git/git` | C | Version control | `2232a88ab6bfbe41faf73f85912937e20bf8b4ee` |
| `redis/redis` | C | Database | `9d0158bf89265daa96e1711478102147117f6b14` |
| `curl/curl` | C | Networking | `373d34494c7fc1fd0928e846d32c5d970a985d09` |
| `microsoft/vscode` | TypeScript | Editor | `df8db3a75a49b85c9636530a3557cdbc639f7bdc` |
| `django/django` | Python | Web framework | `c72001644fa794b82fa88a7d2ecc20197b01b6f2` |

**Why these repos:** Every developer recognises all 7. Five of them represent the hardest class of software to predict — mature, widely-reviewed codebases where hotspots has to work hard. django and vscode anchor the table as the baseline "where it works" cases. This is an honest benchmark, not cherry-picked wins.

**Supported languages:** TypeScript, JavaScript, Go, C, Python. Ruby is not currently supported by hotspots — `rails/rails` was excluded on this basis. Ruby support is tracked as a future language addition.

**Why pinned SHAs:** hotspots has no `--as-of` flag. Pinning the feature SHA means anyone who clones the repo and checks out this commit gets identical feature inputs regardless of when they run the benchmark. The SHA is the last commit before 2024-01-01 for each repo.

---

## Ground truth: bug-commit labels

**Label window: `[2024-01-01, 2025-01-01]` — fixed, not open-ended.**

Using a fixed one-year window means reruns are deterministic. An open cutoff would let label counts grow over time and make historical scores incomparable.

**Label protocol:**
1. Clone the repo (any recent commit is fine for label generation — labels come from history after the feature SHA, not from the checkout state)
2. Walk all commits with `author_date >= 2024-01-01 AND author_date < 2025-01-01`
3. Flag a commit as a bug fix if its message matches the fix-keyword pattern: `\b(fix(es|ed)?|bug|patch|defect|regression|broken)\b` (case-insensitive) — same regex as `convention_bug_fix_count` in the ranker
4. For each flagged commit, record every changed `.py`, `.ts`, `.js`, `.rb`, `.go`, `.c` file path
5. A file scores `1` (bug-linked) if it appears in ≥1 flagged commit; `0` otherwise

**Feature inputs:** Run `hotspots analyze` against the repo checked out at the pinned SHA — this gives features derived from history up to that point only.

**File-level aggregation:** `hotspots analyze --format json` emits function-level scores. Aggregate to file level by taking `max(activity_risk)` across all functions in each file. This is the file's score for ρ computation.

---

## Metrics

**Primary: Spearman ρ** — rank correlation between file-level hotspots scores and bug-linked labels. Computed over all files present in both the score output and the label set.

**Secondary: P@10** — precision at top 10 files. Reported alongside ρ but not used for version comparisons — too noisy at low base rates.

**Per-repo only.** Do not aggregate to a single number. The distribution across easy and hard repos is the claim. A tool that scores +0.32 on django and +0.00 on react should report both, not their mean.

**Suppress if sparse:** If a repo yields `n_bug_files < 20` in the label window, report `—` for that repo. Do not publish a ρ on fewer than 20 positive examples.

---

## Eval script: `benchmarks/run.sh`

```bash
#!/usr/bin/env bash
# Reproduce the hotspots benchmark.
# Usage: ./benchmarks/run.sh [/path/to/hotspots/binary]
# Requires: git, python3 (stdlib only), hotspots binary
set -euo pipefail

BINARY=${1:-hotspots}
RESULTS_DIR="benchmarks/results"
CORPUS="benchmarks/corpus.json"
mkdir -p "$RESULTS_DIR" benchmarks/clones

python3 - <<'EOF'
import json, subprocess, sys, pathlib

corpus  = json.loads(pathlib.Path("benchmarks/corpus.json").read_text())
binary  = sys.argv[1] if len(sys.argv) > 1 else "hotspots"

for repo in corpus["repos"]:
    slug   = repo["slug"].replace("/", "__")
    url    = repo["url"]
    sha    = repo["feature_sha"]
    clone  = pathlib.Path(f"benchmarks/clones/{slug}")
    out    = pathlib.Path(f"benchmarks/results/{slug}")
    out.mkdir(parents=True, exist_ok=True)

    # Clone (bare) if not already present
    if not clone.exists():
        subprocess.run(["git", "clone", "--bare", url, str(clone)], check=True)

    # Create a non-bare worktree checked out at the feature SHA
    worktree = pathlib.Path(f"benchmarks/clones/{slug}-worktree")
    if not worktree.exists():
        subprocess.run(
            ["git", "--git-dir", str(clone), "worktree", "add",
             str(worktree), sha],
            check=True,
        )

    # Run hotspots analyze at the pinned feature SHA
    scores_path = out / "scores.json"
    with scores_path.open("w") as f:
        subprocess.run(
            [binary, "analyze", str(worktree), "--mode", "snapshot",
             "--format", "json", "--all-functions"],
            stdout=f, check=True,
        )

    # Generate bug-commit labels from the bare clone (label window: 2024 only)
    labels_path = out / "labels.json"
    subprocess.run(
        ["python3", "benchmarks/label.py", str(clone),
         "--after", corpus["label_window_start"],
         "--before", corpus["label_window_end"],
         "--out", str(labels_path)],
        check=True,
    )

    # Compute ρ and P@10
    subprocess.run(
        ["python3", "benchmarks/score.py",
         str(scores_path), str(labels_path), "--repo", repo["slug"]],
        check=True,
    )
EOF
```

`benchmarks/label.py` — walks `git log` on the bare clone for the label window; emits one JSON object per file: `{"file": "src/foo.py", "bug_linked": 1}`.

`benchmarks/score.py` — joins scores and labels on file path; aggregates function scores to file level with `max(activity_risk)`; prints a RESULTS.md-formatted row with ρ and P@10.

---

## corpus.json

```json
{
  "version": 1,
  "label_window_start": "2024-01-01",
  "label_window_end": "2025-01-01",
  "label_protocol": "bug-commit-keywords-v1",
  "repos": [
    {
      "slug": "facebook/react",
      "url": "https://github.com/facebook/react",
      "language": "JavaScript",
      "feature_sha": "bf859705b55a8ccaedbed8546cd4d9c6c003bf62"
    },
    {
      "slug": "golang/go",
      "url": "https://github.com/golang/go",
      "language": "Go",
      "feature_sha": "b25f5558c69140deb652337afaab5c1186cd0ff1"
    },
    {
      "slug": "git/git",
      "url": "https://github.com/git/git",
      "language": "C",
      "feature_sha": "2232a88ab6bfbe41faf73f85912937e20bf8b4ee"
    },
    {
      "slug": "redis/redis",
      "url": "https://github.com/redis/redis",
      "language": "C",
      "feature_sha": "9d0158bf89265daa96e1711478102147117f6b14"
    },
    {
      "slug": "curl/curl",
      "url": "https://github.com/curl/curl",
      "language": "C",
      "feature_sha": "373d34494c7fc1fd0928e846d32c5d970a985d09"
    },
    {
      "slug": "microsoft/vscode",
      "url": "https://github.com/microsoft/vscode",
      "language": "TypeScript",
      "feature_sha": "df8db3a75a49b85c9636530a3557cdbc639f7bdc"
    },
    {
      "slug": "django/django",
      "url": "https://github.com/django/django",
      "language": "Python",
      "feature_sha": "c72001644fa794b82fa88a7d2ecc20197b01b6f2"
    }
  ]
}
```

---

## Versioned results

Every hotspots release that changes ranking or scoring — new feature, formula change, new ranker model — triggers a benchmark run before the release tag is cut. Results are recorded in two places:

**`benchmarks/versions/vX.Y.Z.json`** — machine-readable raw results for that version. One file per release, never edited after creation. Enables automated diffing between versions.

**`benchmarks/RESULTS.md`** — human-readable append-only table. Each release appends one block. Prior blocks are never edited.

### What triggers a run

Run the benchmark before tagging any release that changes:
- The ranker feature set (`FEATURE_NAMES`)
- The activity risk formula
- A trained `ranker.json` model update
- Any scoring weight or threshold

Do not re-run for releases that only change output formatting, CLI flags, or docs.

### `benchmarks/versions/vX.Y.Z.json` format

`run.sh` writes this automatically. The `hotspots_version` field is captured from `hotspots --version`.

```json
{
  "hotspots_version": "1.25.3",
  "run_date": "2026-06-27",
  "corpus_version": 1,
  "features_active": ["lrs", "cc", "nd", "loc", "fo", "fan_in", "total_churn", "authors_90d", "directed_coupling"],
  "ranker": false,
  "results": [
    {
      "repo": "django/django",
      "language": "Python",
      "rho": 0.293,
      "p_at_10": 0.70,
      "n_files": 756,
      "n_bug_files": 233
    }
  ]
}
```

`ranker: true` when a trained `ranker.json` was present during the run. `features_active` lists the feature names in order — changes here signal a model version bump.

### `benchmarks/RESULTS.md` block format

```markdown
## v1.25.3 — 2026-06-27

**Features:** ARS baseline (no trained ranker) · features: lrs, cc, nd, loc, fo, fan_in, total_churn, authors_90d, directed_coupling

| Repo | Language | ρ | P@10 | n_files | n_bug_files |
|---|---|---|---|---|---|
| facebook/react | JavaScript | | | | |
| golang/go | Go | | | | |
| git/git | C | | | | |
| redis/redis | C | | | | |
| curl/curl | C | | | | |
| microsoft/vscode | TypeScript | | | | |
| django/django | Python | | | | |
```

`—` in the ρ column means `n_bug_files < 20`. The **Features** line documents exactly what was active so readers can correlate score changes to specific improvements.

---

## Files to create

| Path | Description |
|---|---|
| `benchmarks/README.md` | What the benchmark is, how to run it, how to interpret the numbers, release process |
| `benchmarks/run.sh` | Main eval driver — captures `hotspots --version`, writes versioned JSON + RESULTS.md row |
| `benchmarks/label.py` | Bug-commit label generator (stdlib only) |
| `benchmarks/score.py` | ρ + P@10 computation (stdlib + scipy) |
| `benchmarks/corpus.json` | Machine-readable corpus manifest with pinned SHAs |
| `benchmarks/RESULTS.md` | Human-readable results table (append-only) |
| `benchmarks/versions/` | One `vX.Y.Z.json` per scored release (machine-readable, never edited) |

Do not add benchmarks to CI. Full repo clones + analysis takes 30–60 minutes. Document this in `benchmarks/README.md`.

---

## Acceptance criteria

1. `benchmarks/run.sh` completes end-to-end on a fresh machine with no steps beyond `git clone` + binary install.
2. Running `run.sh` twice produces byte-identical label files (deterministic).
3. ρ for django falls within ±0.05 of +0.293; ρ for vscode falls within ±0.05 of internal research figure — verifies label protocol is consistent across runs.
4. `benchmarks/corpus.json` passes `python3 -m json.tool` and contains all 7 repos with `feature_sha` set.
5. `benchmarks/RESULTS.md` and `benchmarks/versions/v{current}.json` are both populated before any public release that cites benchmark numbers.
6. `benchmarks/versions/vX.Y.Z.json` contains `hotspots_version`, `run_date`, `corpus_version`, `features_active`, `ranker`, and `results` array.
7. Two consecutive version JSON files can be diffed on `rho` per repo to show improvement or regression.
8. No API key, token, or proprietary tooling is required at any point.

---

## Do not

- Do not run benchmarks in CI.
- Do not aggregate per-repo ρ to a single headline number.
- Do not add or remove repos from the corpus without bumping `corpus.json` version and re-running all rows.
- Do not use OSV/CVE labels — they require API keys and ecosystem-specific package mappings.
- Do not extend the label window beyond 2025-01-01 for the v1 corpus — open-ended windows make historical comparisons meaningless.
- Do not edit a `benchmarks/versions/vX.Y.Z.json` after it is written — it is the permanent record for that release.
- Do not run the benchmark for releases that only change formatting, flags, or docs — only scoring changes warrant a new run.

---

## Supporting evidence

| Finding | Relevance |
|---|---|
| F54 — Bug-Commits Temporal Holdout | Label protocol source; django ρ=+0.318, vscode ρ=+0.232, react CIRCULAR, golang WEAK |
| F24 — Score Collapse on Mature Repos | Explains why rails and golang are in the hard tier |
| F22 — XGBoost vs Hotspots Formula | Establishes ARS as the baseline to beat |
| F67 — OSV Signal Eval | Corroborates activity signals as leading predictors; C repos as hardest class |
