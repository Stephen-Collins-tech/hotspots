# hotspots Benchmark

This directory contains the reproducible benchmark that validates hotspots' ability to
identify files that will have bugs. Every number in [RESULTS.md](RESULTS.md) can be
reproduced by anyone with `git`, `python3`, and the `hotspots` binary.

---

## What we are measuring

hotspots ranks every function in a codebase by risk. The question this benchmark answers is:

> **Do the files hotspots ranks highest actually contain bugs?**

We answer it by comparing hotspots' rankings against a ground-truth set of files that had
real bug-fix commits in the year following the analysis date.

---

## How the scores are calculated

### Step 1 — Fix the analysis point in time

hotspots is run against each repo as it existed at a pinned commit SHA — the last commit
before 2024-01-01. This means hotspots only sees git history up to that point: churn,
ownership, complexity, coupling. Nothing from 2024 onward.

The SHAs are fixed in [`corpus.json`](corpus.json). Running the benchmark in 2027 gives
the same feature inputs as running it in 2024.

### Step 2 — Collect ground-truth bug labels (2024 only)

We walk every commit in the repo between **2024-01-01 and 2025-01-01** and flag commits
whose message matches a bug-fix keyword pattern:

```
fix, fixes, fixed, fixing, bug, patch, defect, regression, broken, hotfix
```

Any source file touched by a matching commit is labelled **bug-linked** (`1`). All other
source files are labelled clean (`0`).

This approach is known as **[commit classification by keyword](https://en.wikipedia.org/wiki/Bug_tracking_system#Commit_messages)**
and is the standard method used in empirical software engineering research for labelling
bug-fix commits without access to an issue tracker. It is a conservative label — it only
captures bugs explicitly called out in the commit message. It undercounts real bugs, which
means our scores are a lower bound.

### Step 3 — Aggregate hotspots scores to file level

`hotspots analyze` scores individual functions, not files. We aggregate to the file level
by taking the **maximum `activity_risk` score** across all functions in each file. A file
is as risky as its riskiest function.

### Step 4 — Compute Spearman ρ

We rank all files in the repo by their hotspots score (highest = most risky) and by their
bug-linked label. **[Spearman ρ](https://en.wikipedia.org/wiki/Spearman%27s_rank_correlation_coefficient)**
(rho) measures how well these two rankings agree.

- **ρ = +1.0** — perfect: every bug-linked file is ranked above every clean file
- **ρ = 0.0** — random: hotspots rankings carry no information about bug location
- **ρ = -1.0** — inverted: hotspots is systematically wrong

In practice, no tool achieves +1.0 on real codebases. A ρ of +0.3 to +0.5 on mature,
widely-reviewed projects is meaningful signal. Spearman ρ is preferred over
[Pearson correlation](https://en.wikipedia.org/wiki/Pearson_correlation_coefficient)
here because we care about ranking order, not the magnitude of scores.

### Step 5 — Compute P@10

**[Precision at K](https://en.wikipedia.org/wiki/Evaluation_measures_(information_retrieval)#Precision_at_K)
(P@10)** is the fraction of the top 10 files hotspots flagged that are genuinely bug-linked.

- **P@10 = 1.00** — all 10 of the files hotspots ranked highest had real bugs in 2024
- **P@10 = 0.50** — 5 of the top 10 had real bugs
- **P@10 = 0.00** — none of the top 10 had real bugs

P@10 is a practical metric: it answers "if a developer looks at the top 10 files hotspots
flags, how often will they find something real?" It is noisier than ρ at low bug densities,
so we report it alongside ρ rather than instead of it.

---

## Why these repos

The 7 benchmark repos were chosen to be **recognisable** and **hard**:

| Repo | Language | Why it is hard |
|---|---|---|
| `facebook/react` | JavaScript | Large, many contributors, distributed ownership |
| `golang/go` | Go | Compiler and runtime — very high code quality bar |
| `git/git` | C | Extremely mature, every line reviewed by experts |
| `redis/redis` | C | Tight, focused codebase; small team |
| `curl/curl` | C | Security-critical networking code |
| `microsoft/vscode` | TypeScript | 160k commits, massive contributor base |
| `django/django` | Python | 35k commits, stable mature framework |

These are not repos where hotspots is expected to win easily. Any tool can find bugs in
a prototype. The question is whether it works on code that has been maintained and reviewed
for a decade or more.

---

## What the scores mean in plain English

**curl/curl — ρ=+0.476, P@10=1.00**  
Every single one of the 10 files hotspots flagged as highest-risk had a real bug in 2024.
hotspots identified the right files in curl's C networking code without any knowledge of
what bugs would appear — purely from historical patterns.

**redis/redis — ρ=+0.476, P@10=0.70**  
Strong rank correlation across a focused C database codebase. 7 of the top 10 files were
bug-linked.

**git/git — ρ=+0.340, P@10=0.50**  
Solid signal on one of the most carefully maintained C codebases in existence.

**django/django — ρ=+0.293, P@10=0.70**  
Good correlation on a mature Python web framework. 7 of the top 10 bug-linked.

**facebook/react — ρ=+0.352, P@10=0.50**  
Meaningful signal on a large JavaScript UI framework with hundreds of contributors.

**microsoft/vscode — ρ=+0.251, P@10=0.40**  
Positive correlation across 28,000 functions and 160k commits of TypeScript.

**golang/go — ρ=+0.265, P@10=0.00**  
Positive rank correlation but P@10=0.00 — the top 10 files by score were not the bug-linked
ones in 2024. hotspots identifies risky areas but the highest-ranked files were not where
the 2024 bugs landed. This is the honest hard case: a language runtime with extremely
distributed bug patterns.

---

## What hotspots does NOT do

- It does not read source code for semantic meaning
- It does not know which functions are called in production
- It does not have access to test results or CI history
- It does not use any machine learning model in this baseline (v1.25.3) — all scores
  come from the activity risk formula derived from git history alone

The scores above are the **floor**. As hotspots adds a trained
[random forest ranker](https://en.wikipedia.org/wiki/Random_forest) (planned), scores
will improve. Each release that changes ranking or scoring triggers a new benchmark run,
appended to [RESULTS.md](RESULTS.md) under the new version number.

---

## How to reproduce

**Requirements:** `git`, `python3` (3.9+), `scipy`, `hotspots` binary

```bash
git clone https://github.com/your-org/hotspots
cd hotspots
./benchmarks/run.sh $(which hotspots)
```

The script will:
1. Clone each benchmark repo (or skip if already cloned)
2. Check out the pinned feature SHA for each
3. Run `hotspots analyze` to produce function-level scores
4. Walk the 2024 commit history to generate bug-commit labels
5. Compute ρ and P@10
6. Write `benchmarks/versions/vX.Y.Z.json` and print a RESULTS.md block

Total runtime: 30–60 minutes depending on hardware. Clones require ~3 GB of disk space.

**Do not** run this in CI — the clone sizes and analysis time make it impractical.

---

## How results are versioned

Every hotspots release that changes ranking or scoring produces:

- **`benchmarks/versions/vX.Y.Z.json`** — machine-readable results: version, date, features
  active, ranker on/off, per-repo ρ and P@10. Written once, never edited.
- **A new block in `RESULTS.md`** — human-readable table appended below prior versions.
  Prior blocks are never edited.

The `features_active` list in each JSON file documents exactly which signals were used.
When a new feature ships, it appears in that list and the corresponding ρ change shows
what it contributed.

---

## Corpus design and label protocol

Full specification: [`corpus.json`](corpus.json) and [REQ-004](../docs/requirements/REQ-004-public-benchmarks.md).

- **Feature cutoff:** 2024-01-01 (pinned SHA per repo)
- **Label window:** 2024-01-01 to 2025-01-01 (fixed — does not grow over time)
- **Label method:** bug-fix keyword match on commit messages ([SZZ-style](https://en.wikipedia.org/wiki/SZZ_algorithm) commit classification without the blame-tracking step)
- **File-level aggregation:** max(activity\_risk) across functions per file
- **Suppression threshold:** repos with fewer than 20 bug-linked files report `—`
- **Temporal holdout:** features and labels come from non-overlapping time periods — a standard technique in [defect prediction research](https://en.wikipedia.org/wiki/Software_defect_prediction) to prevent [data leakage](https://en.wikipedia.org/wiki/Leakage_(machine_learning))
