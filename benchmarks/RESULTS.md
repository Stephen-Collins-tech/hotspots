# Benchmark Results

Scores are Spearman ρ (rank correlation between hotspots risk scores and bug-commit labels).
P@10 is precision at the top 10 ranked files. `—` means fewer than 20 bug-linked files in
the label window — too sparse to report reliably.

See [README.md](README.md) for corpus design, label protocol, and how to reproduce these numbers.

Each block is written once when a release ships and never edited. The versioned raw JSON lives
in [`versions/`](versions/).

---

## v1.25.3 — 2026-06-27

**Features:** ARS baseline (no trained ranker)  
**Feature set:** lrs, cc, nd, loc, fo, fan_in, total_churn, authors_90d, directed_coupling

| Repo | Language | ρ | P@10 | n\_files | n\_bug\_files |
|---|---|---|---|---|---|
| facebook/react | JavaScript | +0.352 | 0.50 | 862 | 45 |
| golang/go | Go | +0.265 | 0.00 | 6,116 | 276 |
| git/git | C | +0.340 | 0.50 | 633 | 244 |
| redis/redis | C | +0.476 | 0.70 | 162 | 46 |
| curl/curl | C | +0.476 | **1.00** | 632 | 216 |
| microsoft/vscode | TypeScript | +0.251 | 0.40 | 2,245 | 642 |
| django/django | Python | +0.293 | 0.70 | 756 | 233 |
| **mean** | | **+0.350** | **0.54** | | |

**Corpus:** 7 repos · label window 2024-01-01 to 2025-01-01 · features from pinned pre-2024 SHAs  
**Raw data:** [versions/v1.25.3.json](versions/v1.25.3.json)
