# hotspots — Work in Progress

Current version: **1.25.3**  
Last updated: 2026-06-27

This document tracks what is ready to implement, what is in flight, and what the next
release milestone looks like. It is the working companion to
[`docs/requirements/`](docs/requirements/) and [`benchmarks/RESULTS.md`](benchmarks/RESULTS.md).

---

## Baseline established

Benchmark run completed 2026-06-27 against v1.25.3 (ARS formula, no trained ranker).
Full results: [`benchmarks/RESULTS.md`](benchmarks/RESULTS.md) · raw: [`benchmarks/versions/v1.25.3.json`](benchmarks/versions/v1.25.3.json)

| Repo | Language | ρ | P@10 |
|---|---|---|---|
| curl/curl | C | +0.476 | **1.00** |
| redis/redis | C | +0.476 | 0.70 |
| facebook/react | JavaScript | +0.352 | 0.50 |
| git/git | C | +0.340 | 0.50 |
| django/django | Python | +0.293 | 0.70 |
| golang/go | Go | +0.265 | 0.00 |
| microsoft/vscode | TypeScript | +0.251 | 0.40 |
| **mean** | | **+0.350** | **0.54** |

Every future release that changes ranking or scoring must run the benchmark and append a
new block to `RESULTS.md` before the release tag is cut.

---

## Ready to implement

Three requirements docs are written and fully specified. Implement in this order —
REQ-001 and REQ-002 are independent; REQ-003 depends on neither but benefits from
REQ-002's feature names being stable first.

### REQ-001 — History depth tier annotation
**Spec:** [`docs/requirements/REQ-001-history-depth-tier.md`](docs/requirements/REQ-001-history-depth-tier.md)  
**What it does:** Annotates each function with a `history_depth` tier (`sparse` / `moderate` / `rich` / `very_rich`) derived from lifetime churn. Output annotation only — does not affect scoring. Shown in `--explain` output.  
**Files to change:** `snapshot.rs`, `analysis.rs`, `explain.rs`, `aggregates.rs`  
**Effort:** Small — new enum + one match expression + populate in one analysis pass.

### REQ-002 — convention_bug_fix_count as 10th ranker feature
**Spec:** [`docs/requirements/REQ-002-convention-bug-fix-feature.md`](docs/requirements/REQ-002-convention-bug-fix-feature.md)  
**What it does:** Adds `convention_bug_fix_count` to `FEATURE_NAMES` in `trainer.rs`, making it a 10th input to the trained ranker. Field already exists on `FunctionSnapshot`. Model version bumped.  
**Files to change:** `trainer.rs` only.  
**Effort:** Minimal — two array literals, one `as f64` cast, one version bump, one test update.  
**Benchmark trigger:** Yes — re-run after this ships and append v1.26.x results.

### REQ-003 — Ranker explanation layer (✦ phrases)
**Spec:** [`docs/requirements/REQ-003-ranker-explanation-layer.md`](docs/requirements/REQ-003-ranker-explanation-layer.md)  
**What it does:** When `--explain` is passed, renders a `✦` line below each CRITICAL and HIGH function with a plain-English phrase naming the 1–3 most elevated signals. Deterministic phrase-table lookup — no LLM, no network.  
**Files to change:** new `phrases.rs`, `lib.rs`, `snapshot.rs`, `analysis.rs`, `explain.rs`, `aggregates.rs`  
**Effort:** Medium — new module + percentile computation pass + phrase table.

### REQ-004 — Public benchmark corpus
**Spec:** [`docs/requirements/REQ-004-public-benchmarks.md`](docs/requirements/REQ-004-public-benchmarks.md)  
**What it does:** Adds `benchmarks/` to the public repo — `corpus.json`, `run.sh`, `label.py`, `score.py`, `RESULTS.md`, `versions/`. Already partially complete (see below).  
**Status:** Scripts and first results are written. Remaining work: wire `run.sh` to auto-detect `hotspots --version` and write the versioned JSON automatically; write `corpus.json`; finalise `README.md`.  

---

## Benchmark infrastructure — current state

Already written and working:

| File | Status |
|---|---|
| `benchmarks/label.py` | Done — generates bug-commit labels from bare clone |
| `benchmarks/score.py` | Done — computes ρ and P@10, handles path normalisation |
| `benchmarks/RESULTS.md` | Done — v1.25.3 baseline populated |
| `benchmarks/versions/v1.25.3.json` | Done — first versioned result on record |
| `benchmarks/README.md` | Done — full explanation with Wikipedia links |

Still needed:

| File | What to do |
|---|---|
| `benchmarks/run.sh` | Wire together label.py + hotspots analyze + score.py; auto-detect version; write versioned JSON |
| `benchmarks/corpus.json` | Write the 7-repo manifest with pinned SHAs (content is in REQ-004) |

---

## Next release checklist (v1.26.x)

- [ ] Implement REQ-002 (`convention_bug_fix_count` feature)
- [ ] Implement REQ-001 (history depth tier)
- [ ] Re-train ranker with new 10-feature set
- [ ] Run benchmark → append `benchmarks/versions/v1.26.x.json` + RESULTS.md block
- [ ] Implement REQ-003 (`--explain` phrase layer) — can ship in same release or follow-on

---

## Language support gaps

hotspots currently supports: TypeScript, JavaScript, Go, Java, Python, Rust, Vue, C#, C.

**Ruby is not supported.** `rails/rails` was excluded from the benchmark corpus on this
basis. Adding Ruby support (tree-sitter-ruby grammar) would unlock a significant class of
well-known repos. Tracked as a future addition — not in scope for any current REQ.
