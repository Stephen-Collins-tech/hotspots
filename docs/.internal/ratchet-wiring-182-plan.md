# Plan: #182 — trained-ranker ratchet risk + convention_bug_fix_count wiring gap

https://github.com/Stephen-Collins-tech/hotspots/issues/182

Companion doc for issue #210, worked in parallel on a separate worktree, lives
at `hotspots.worktrees/depth-score-210/docs/.internal/depth-score-210-plan.md`
(branch `research/depth-score-210`). See "Cross-issue risk notes" at the
bottom.

## Coupling check

`hotspots coordinate --files hotspots-core/src/scoring.rs,hotspots-core/src/trainer.rs`
→ coupling_ratio 0.5 (below 0.7 serialize threshold), `recommendation: parallel_safe`,
shared ownership (2 authors, same set, on both files). Hidden dependencies worth
checking before merge: `hotspots-cli/src/cmd/train.rs`,
`hotspots-core/tests/trainer_tests.rs`, `hotspots-core/src/isolation_forest.rs`,
`hotspots-core/src/models.rs`, `hotspots-core/src/sarif.rs`,
`hotspots-core/src/config.rs`, `hotspots-core/src/snapshot.rs`, `benchmarks/*`,
golden test fixtures under `tests/golden/`.

## Sequencing

**Issue #210 should land first**, ahead of this one. This issue's likely code
change (if ratchet is confirmed) is larger and centered on `trainer.rs`'s
feature vector + call sites across ~12 files, vs. #210's small,
self-contained `scoring.rs`-only edit. Rebase this branch onto main after
#210 merges if #210 lands a `scoring.rs` diff.

## Key fact

Raw monotonicity of `total_churn`/`convention_bug_fix_count` isn't sufficient
evidence of a ratchet in the *trained* path — it depends on whether the fitted
model (Ridge vs. RandomForest) actually propagates it. Ridge
(`ModelClass::Ridge`, linear combination, `trainer.rs:1226-1245`) is the
deterministic case: a positive coefficient guarantees monotonic non-decrease.
RandomForest's ensemble + per-tree feature subsampling could discount or
reverse it — unconfirmed either way.

## Plan — Part 1: empirical ratchet test

1. Build a partial-dependence/monotonicity probe against real trained models
   from the existing `benchmarks/` corpus (not synthetic repos). For each
   repo: `hotspots train` → load via `RankerModel::load`.
2. Sample real function feature vectors; for each, hold every feature fixed
   except `total_churn` (feature index 6) or `convention_bug_fix_count`
   (index 9); sweep that one feature across its own observed percentile grid
   (not extrapolated out-of-distribution); evaluate `trainer::score`.
3. Record per repo/feature/model-class: monotonicity fraction (does score
   ever decrease across the sweep), saturation point (Ridge clamped to [0,1]
   will plateau — a *bounded* ratchet, same max-tier-forever problem as
   burst_score, just capped), split frequency (`SerializedTree.feature_indices`),
   and Ridge coefficient sign/magnitude directly (`trainer.rs:988-993`) as the
   deterministic ground-truth case.
4. Real-world spot check: replicate burst_score brief's acceptance-criterion-6
   pattern — find a real function with high historical churn/fix-count but
   low recent activity, confirm whether `apply_trained_ranker`'s output stays
   elevated despite quiescence.
5. Harness lives in `hotspots-core/tests/` integration test or a `benchmarks/`
   script — one-time diagnostic, not shipped code.
6. Decision rule: if RF is materially non-monotonic for most sampled
   vectors/repos, scope the fix narrower (Ridge-regime only). If RF also
   shows a dominant monotonic trend (>90% never decrease), treat both model
   classes the same.

## Plan — Part 2: wiring gap (documentation, ship regardless of Part 1)

Verdict: **intentional-by-precedent, not an oversight, but under-documented.**
`docs/.internal/cumulative-feature-ratchet-audit.md` already scoped a
`scoring.rs` fix as pending this exact empirical confirmation. META-08
(`hotspots-research/docs/meta/RESEARCH-STATE.md:174-188`) says "if confirmed,
weight convention_fix_count more heavily" — the "if confirmed" plus the
absence of a promotion brief means this was never promoted to an
implementable change. Per the project's research-sync rule, no `scoring.rs`
edit should happen without a promotion brief.

Action: add a documentation note (ratchet audit doc or `docs/REFERENCE.md`)
stating explicitly that `convention_bug_fix_count`'s absence from the direct
formula is intentional pending Part 1, cross-referencing META-08. Ship this
regardless of Part 1's outcome — low risk, resolves the "is this a bug"
confusion immediately.

If/when promoted later, the right design (avoiding the burst_score mistake):
a windowed/decayed variant (e.g. `convention_bug_fix_count_365d`, reusing
`TOUCH_WINDOW_DAYS`), following the `directed_coupling` self-correcting
pattern — not the raw cumulative count. Needs its own `F##` validation before
promotion; out of scope for a quick `scoring.rs` edit.

## Plan — Part 3: fix design, if Part 1 confirms ratchet

1. **`model_version` bump** 5→6 (`trainer.rs:516`, `:574`). `RankerModel::load`'s
   existing version gate (`trainer.rs:1334-1346`) handles `ranker.json`
   invalidation automatically — no separate migration path needed.
2. **Feature vector change**: `FEATURE_NAMES`/`extract_features`
   (`trainer.rs:88-120`) are index-stable and shared by train+inference — any
   index change must touch both plus every call site (12+ files per the
   original ratchet audit): `hotspots-cli/src/cmd/train.rs`,
   `hotspots-cli/src/cmd/analyze.rs` (`populate_explanations`,
   `analyze.rs:1540-1547`), `hotspots-core/src/phrases.rs`,
   `hotspots-core/tests/trainer_tests.rs`. Old trees can't be reused with a
   new feature count — this is why `model_version` invalidation (not
   patching) is correct.
3. **Preferred shape**: push the windowed/decayed variant down to the
   *feature computation* layer (`snapshot.rs::populate_convention_bug_fix_count`,
   `trainer.rs`'s `total_churn` computation) rather than only in the
   trained-ranker's consumption — keeps `scoring.rs`'s potential future use
   and `trainer.rs`'s use pointed at the same non-ratcheting field. If
   windowing needs its own research validation first (likely), the minimal
   interim fix mirrors burst_score's exact precedent: drop the two features
   from `extract_features` entirely (bump `model_version`), leave
   `cold_start_features` (already excludes them) and raw `FunctionSnapshot`
   storage (used by `phrases.rs` for `--explain`) untouched.
4. **`--explain` consumer carve-out**: `analyze.rs::populate_explanations` and
   `phrases.rs` use *percentile rank* of `total_churn`, not absolute value —
   already non-ratcheting, confirmed by the original audit. Do not touch this
   consumer.
5. Document the breaking change (every existing `ranker.json` becomes
   unusable until re-trained) in CHANGELOG/release notes.

## Open questions to resolve before build

- [ ] Run the partial-dependence probe before deciding fix scope — don't
      assume RF behaves like Ridge.
- [ ] `grep -n "dependency_depth\|depth" hotspots-core/src/trainer.rs` —
      confirm whether trainer.rs already has depth-adjacent feature columns,
      relevant to both this issue and #210's "keep the field" carve-out.
- [ ] Confirm model regime (`regime_verdict`) per benchmark repo before
      interpreting Part 1 results — Ridge vs. RandomForest split matters for
      the decision rule.

## Cross-issue risk notes (shared with #210)

- Neither plan touches `ScoringWeights`/`ActivityRiskInput`'s field list, so
  serialization-format risk from the `coordinate` coupling signal stays low
  regardless of ordering.
- Grep `trainer_tests.rs` and golden fixtures before merging to confirm no
  shared golden-output assertions broke.
- If #210 resolves to branch A (zero depth_score) and this issue's trainer
  work independently touches `dependency_depth`-derived features, both PRs
  will diff `trainer.rs` — confirmed low risk today since #210 doesn't
  currently plan to touch `trainer.rs`, but re-check at build time against
  #210's current state.
