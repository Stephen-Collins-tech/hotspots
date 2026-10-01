# Plan: #210 — depth_score has no research backing

https://github.com/Stephen-Collins-tech/hotspots/issues/210

Companion doc for issue #182, worked in parallel on a separate worktree, lives
at `hotspots.worktrees/ratchet-wiring-182/docs/.internal/ratchet-wiring-182-plan.md`
(branch `fix/ratchet-wiring-182`). See "Cross-issue risk notes" at the bottom.

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

**This issue (#210) should land first**, ahead of #182. #210's likely code
change (if any) is a small, self-contained edit to `compute_activity_risk` in
`scoring.rs` only. #182's likely code change is larger and centered on
`trainer.rs`'s feature vector + call sites across ~12 files. Landing #210 first
avoids #182 rebasing around an unrelated `scoring.rs` diff. If #210 resolves to
"add a citation, no code change" (branch B below), sequencing no longer
matters.

## Key fact

F160's own pre-registered design doc already flagged this gap by name ("Do not
claim the missing CLI terms — churn lines, depth, neighbor_churn — were
tested"). This is closing a gap F160 called out, not proposing a new signal —
so it does not trip META-02's "no new ranking-signal proposals" freeze.

## Plan

1. **Add `dependency_depth` to the research holdout data.** It's not there
   yet. In `hotspots-research/scripts/features/extract_training_pairs.py`,
   next to `fan_in`/`scc_size` extraction (~lines 443-444, 473-474), add
   `depth = cg.get("dependency_depth")` and a `"dependency_depth"` column.
   Add as a separate column, not folded into `ars` — keeps F160's
   reproducibility check (0-mismatch verification) undisturbed.
2. **Regenerate `holdout.jsonl`** for F160's repo set. Check
   `data/repos/<slug>/` for cached raw snapshot JSON (should already have
   `callgraph.dependency_depth` from original snapshot runs) before re-running
   `hotspots analyze` — avoid unnecessary re-clone/re-snapshot.
3. **Clone F160's exact methodology** into a new pre-registered doc at
   `hotspots-research/docs/research/depth-score-decomposition.md` (template:
   `docs/research/ars-formula-decomposition.md`). Same repo gate
   (n_holdout ≥ 200, positive rate in [0.05, 0.95]), same metrics (Spearman
   rho, P@10% vs `composite_label`), same non-inferiority margins
   (d_rho > −0.02, d_P@10 > −0.03, 2000-resample bootstrap, BH-corrected).
   Test against the **current production baseline** (depth_score on top of
   already-zeroed fan_in/scc/burst), not F160's old 5-term proxy.
   New eval script: `hotspots-research/scripts/eval/eval_depth_score_decomposition.py`
   (clone of `eval_ars_formula_decomposition.py`).
4. **Corroborate with a live CLI ablation**: `benchmarks/run.sh` once with
   `weights.depth = 0.1` (current default), once zeroed, writing to
   `benchmarks/versions/vX.Y.Z-depth-ablation.json` (don't overwrite
   `v1.30.0.json`). Spot-check only — the pre-registered study is the real
   gate.
5. **Write the finding**: `hotspots-research/docs/findings/F<next>-depth-score-decomposition.md`,
   `metas: [META-02]`, `Precursors: F160`.

## Decision branches

- **A. Non-inferior to drop** (likely, given fan_in/scc's precedent as
  structurally similar terms): zero `depth_score` in `scoring.rs`'s
  `compute_activity_risk` exactly like fan_in/scc/burst — same pattern,
  comment citing the new finding ID and numbers. Keep
  `ActivityRiskInput.dependency_depth` and `ScoringWeights.depth` fields
  (other consumers still need them — verify via
  `grep -n dependency_depth hotspots-core/src/trainer.rs` before writing the
  promotion brief, so it names exact line numbers). `RiskFactors.depth`
  should report `0.0`, matching `.fan_in`/`.cyclic_dependency`/`.burst`.
  Update tests asserting on `factors.depth` (search `scoring.rs` tests for
  `dependency_depth: Some(9)`) and `docs/REFERENCE.md`.
- **B. Depth survives the bar**: no code change to the computation. Add the
  missing citation comment above the `depth_score` block (style: match the
  churn/touch/recency/neighbor_churn comments citing META-02), pointing at
  the new finding ID and headline number.

Write the promotion brief only after the study runs:
`hotspots-research/docs/promotion-briefs/depth-score-<outcome>.md` (template:
`burst-score-remove-from-live-score.md`).

## Open questions to resolve before build

- [ ] Confirm raw per-function snapshot JSON for F160's repo set still has
      cached `callgraph.dependency_depth` — avoids a costly re-snapshot.
- [ ] Confirm `dependency_depth` non-null coverage/density in the holdout data
      before running the study (expected dense, unlike fan_in's 16%/scc's 1%,
      but unmeasured).
- [ ] `grep -n dependency_depth hotspots-core/src/trainer.rs` — confirm exact
      trainer touchpoints so branch A's "keep the field" carve-out is backed
      by real line numbers, not assumption.

## Cross-issue risk notes (shared with #182)

- Neither plan touches `ScoringWeights`/`ActivityRiskInput`'s field list (only
  values/comments in branch A), so serialization-format risk from the
  `coordinate` coupling signal stays low regardless of ordering.
- Grep `trainer_tests.rs` and golden fixtures before merging to confirm no
  shared golden-output assertions broke.
- If this issue resolves to branch A (zero depth_score) and #182's trainer
  work independently touches `dependency_depth`-derived features, both PRs
  will diff `trainer.rs` — confirmed low risk today since this plan doesn't
  currently touch `trainer.rs`, but re-check at build time against #182's
  current state.
