# Pending Tasks — Promotion Brief Handoffs

Briefs are written by Claude in `../hotspots-research` and are complete implementation
specs. Read the brief in full before writing any code.

---

## Task: `--explain` opt-in flag (ranker score + explanation layer)

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f55-jepa-ranker-explanation.md`

Implement the promotion brief above in this repo. Create a new branch first
(`feat/explain-flag`), implement exactly what's specified (the `--explain` flag,
`phrases.rs` module, phrase table), run `cargo test` in `hotspots-core` and
`hotspots-cli`, and report back — do not push or open a PR.

**Status:** done — branch `feat/explain-flag`, commit a6a4bcd, 423 tests pass

---

## Task: F61 regime-based model class selection

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f61-regime-model-selection.md`

Implement the promotion brief above in this repo. Create a new branch first
(`feat/f61-regime-model-selection`), implement exactly what's specified (Ridge via
`linfa-linear`, Q3 depth-2 screener, auto model-class selection in `hotspots train`),
run `cargo test` in `hotspots-core` and `hotspots-cli`, and report back — do not push or
open a PR.

**Status:** done — branch `feat/f61-regime-model-selection`, PR #114

---

## Task: F63 signal-porting prerequisite (blocks cold-start routing)

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f63-signal-porting-prerequisite.md`

Adds `commit_count`, `author_count`, `author_entropy`, `burst_score`, `isolation_rate`,
`age_days`, `last_touch_days` to `FunctionSnapshot` via a single git-log pass (not the
existing per-file-subprocess pattern — see brief for why). Must ship before the next
task below.

**Status:** done — PR #121 (branch `feat/history-signals-porting`), merged

---

## Task: F62/F63 Gini-gated cold-start routing

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f62-f63-cold-start-routing.md`

**Depends on:** F63 signal-porting prerequisite above — do not start until that one is done.

Adds `hotspots rank --cold-start` using Gini-gated routing (formula path vs. hand-rolled
streaming IsolationForest). IsolationForest design pre-validated against sklearn in
Python (`scripts/poc/validate_streaming_isolation_forest.py` in the research repo,
mean ρ gap −0.014) — see the brief's Research Artifacts section before implementing.

**Status:** done — PR #123 (branch `feat/f62-f63-cold-start-routing`), merged, released
v1.33.0. `hotspots analyze --cold-start` ships with Gini-gated routing
(`ColdStartRoute::Formula`/`Anomaly`/`UniformPrior`) in `trainer.rs`.

---

## Task: F98 low-label-density gate for cold-start routing

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f98-lowlabel-coldstart-gate.md`

**Depends on:** F62/F63 Gini-gated cold-start routing above — shipped, so this was
unblocked (hotspots#118 closed).

Adds a second, independent gate to `cold_start_rank()`: repos with fix-commit history
but a low positive rate (0.05–0.30, function-level) route to the same `IsolationForest`
anomaly path as the Gini-low branch, evaluated before it. Always uses function-level
(blame-based) labels via `collect_fix_functions`, ignoring `cfg.blame_labels`, to avoid
the file-level pos_rate inflation the brief calls out for spark/envoy-shaped repos.

**Status:** implemented — branch `feat/f98-lowlabel-coldstart-gate`, not yet pushed/PR'd.
`cold_start_rank()` signature extended to `(snapshot, repo_root, cfg)`; `handle_cold_start`
in `hotspots-cli/src/cmd/analyze.rs` updated accordingly. 4 new integration tests in
`hotspots-core/tests/trainer_tests.rs` cover all acceptance criteria. Full workspace
`cargo test` (457 tests), `cargo fmt --check`, `cargo clippy -D warnings` all clean.

---

## Task: F93 OSV-weighted activity-risk formula

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/f93-osv-weighted-formula.md`

Implement the promotion brief above in this repo. Create a new branch first
(`feat/f93-osv-weighted-formula`), implement exactly what's specified (add a `burst`
weight to `ScoringWeights` and a `burst_score` term to `compute_activity_risk` in
`scoring.rs`), run `cargo test`, and report back — do not push or open a PR.

**Status:** done — branch `feat/f93-osv-weighted-formula`, all 4 acceptance criteria met:
`burst: f64` weight (default 0.3) in `ScoringWeights`, `burst_score: Option<f64>` in
`ActivityRiskInput`/`RiskFactors`, `Snapshot::populate_burst_score()` (sliding 30-day
max/mean commit-timing ratio) in `snapshot.rs`, new test in `scoring.rs`, `cargo test`
passing. Also rebuilt missing `benchmarks/run.sh` + `benchmarks/corpus.json` (referenced
by README but absent from the repo) and re-ran the 7-repo benchmark: mean ρ +0.350 →
+0.386, all 7 repos improved (`benchmarks/versions/v1.30.0.json`,
`benchmarks/RESULTS.md`). Tracker row updated to `promoted`.

---

## Task: Remove `burst_score` from live activity-risk score

**Brief:** `/Users/stephencollins/projects/stephencollins.tech-repos/hotspots-research/docs/promotion-briefs/burst-score-remove-from-live-score.md`

`burst_score` is computed over a file's entire commit history and is effectively
monotonic non-decreasing, so including it in `compute_activity_risk` made
`activity_risk` a one-way ratchet — a single historical burst could permanently
keep a file at CRITICAL regardless of current state. Removes the burst term from
`compute_activity_risk`'s sum; `RiskFactors.burst` is now always `0.0`.
`ScoringWeights.burst`, `ActivityRiskInput.burst_score`, and the `burst_score`
computation in `history_signals.rs`/`snapshot.rs` are untouched — kept in place for
a future trailing-window replacement, per the brief's "Do not" section.

**Status:** done — branch `fix/burst-score-remove-from-live-score`, PR #126,
`cargo test` passing (452 tests). Tracker row `burst-score-remove-from-live` still
needs to flip from `pending` to `promoted` once merged.
