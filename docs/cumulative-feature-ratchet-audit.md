# Audit: cumulative/full-history features and one-way-ratchet risk

Tracks issue #172, which extends the audit that led to removing `burst_score`
from the live composite score (`hotspots-research/docs/burst-score-non-decaying-issue.md`,
shipped as `docs/promotion-briefs/burst-score-remove-from-live-score.md`, PR #126,
v1.33.2) to three other cumulative/full-history features named in the issue:
`convention_bug_fix_count`, `directed_coupling`, `total_churn`.

## Methodology (same definition used for burst_score)

A feature has **one-way-ratchet risk** when both of these hold:

1. **Cumulative / full-history**: the value is monotonically non-decreasing
   over the life of the file (a sum or max taken over the entire commit
   history, with no trailing window, decay, or ratio normalization that lets
   it fall back down).
2. **Feeds the live, continuously-updated composite score** — i.e. it moves
   `activity_risk`/the CRITICAL-HIGH-etc. tier shown on every `hotspots
   analyze` run — as opposed to only appearing in a one-time cold-start
   bootstrap or an offline training/eval context.

Only a feature that is both cumulative *and* live-scored can trap a file at
an artificially high tier forever, the way `burst_score` did. A cumulative
feature that never reaches the live score has a materially different risk
profile (same shape as the "historical/eval context" carve-out kept for
`burst_score` in its own writeup).

## Where the live composite score lives

`hotspots-core/src/scoring.rs::compute_activity_risk` is the default live
formula. Confirmed by direct inspection: it does **not** reference
`convention_bug_fix_count`, `directed_coupling`, or `total_churn` at all.
Its inputs are `churn`, `touch_count_30d`, `days_since_last_change`,
`fan_in`, `scc`, `depth`, `neighbor_churn` — plus `burst`, whose weight is
permanently `0.0` per the prior fix.

However, there is a **second, opt-in path** that also writes into the same
`activity_risk` field: `hotspots-cli/src/cmd/analyze.rs::apply_trained_ranker`.
If `.hotspots/ranker.json` exists (produced by the user running `hotspots
train`), every `hotspots analyze` run **overwrites** `activity_risk` with
`hotspots_core::trainer::score(&model, func)`, using the 10-feature vector
built by `trainer::extract_features` — which includes `total_churn` (index 6),
`directed_coupling` (index 8), and `convention_bug_fix_count` (index 9). This
is not a one-time cold-start bootstrap: it re-extracts fresh feature values
and re-scores on *every* subsequent `analyze` invocation for the life of the
repo, using whatever `ranker.json` is on disk. This is the crux of the audit
below.

## Findings

### `total_churn` — **ratchet risk: yes (opt-in trained-ranker path only)**

- Computed in `hotspots-core/src/trainer.rs` from `func.churn` as
  `lines_added + lines_deleted`; the doc comment at
  `hotspots-core/src/trainer.rs:11` calls it explicitly a "lifetime lines
  added + deleted (non-windowed structural signal)" — i.e. cumulative and
  monotonically non-decreasing by construction, same shape as `burst_score`.
- **Not** referenced in `scoring.rs`'s default composite formula.
- **Is** one of the 10 features scored by the trained ranker
  (`trainer::extract_features` index 6, `trainer::FEATURE_NAMES[6]`), and the
  trained ranker's output overwrites the live `activity_risk`/tier on every
  `analyze` run once a repo has `.hotspots/ranker.json` (see above).
- Also used in `hotspots-core/src/phrases.rs` for `--explain` phrase text
  (percentile rank within the current repo, not an absolute cumulative
  value) — this is descriptive text only, not the score, so it is out of
  scope for ratchet risk.
- **Verdict:** genuine ratchet risk exists, but only through the optional
  trained-ranker path, not the default score. See "Why no fix was made"
  below for why this is not a small, clear-cut change.

### `convention_bug_fix_count` — **ratchet risk: yes (opt-in trained-ranker path only)**

- Computed in `hotspots-core/src/snapshot.rs::populate_convention_bug_fix_count`
  as a full-history count of fix-keyword commits per file; the doc comment at
  `hotspots-core/src/trainer.rs:14` calls it "full-history count of
  fix-keyword commits per file (F54)" — cumulative and monotonically
  non-decreasing, same shape as `burst_score`.
- **Not** referenced in `scoring.rs`'s default composite formula.
- **Is** one of the 10 trained-ranker features (`extract_features` index 9),
  subject to the same `apply_trained_ranker` overwrite path as `total_churn`.
- **Verdict:** same as `total_churn` — real risk, but confined to the opt-in
  trained-ranker path.

### `directed_coupling` — **ratchet risk: no**

- Computed in `hotspots-core/src/coupling.rs::compute_directed_coupling` as
  `weighted_co_change_score / appearance_count` — an **average**, not a raw
  cumulative sum, so it is not monotonically non-decreasing the way
  `burst_score`/`total_churn`/`convention_bug_fix_count` are: new co-changes
  with low-scoring partners pull the average down, and the divisor grows
  with every appearance regardless of partner score.
- The repo already has a self-correcting design for the case that matters
  most: `compute_directed_coupling_for_repo` computes Jaccard label
  stability (`compute_jaccard_stability`) and automatically switches to a
  365-day trailing window (`DC_WINDOW_365D`) whenever stability is low
  (`jaccard < DC_JACCARD_THRESHOLD`), i.e. whenever defect-prone files rotate
  over time rather than staying fixed. This is analogous to the `fix_density`
  precedent (F100) cited in the issue — a ratio/adaptive-window feature, not
  a naive full-history accumulator.
- Also appears in the trained-ranker feature set (index 8) and in
  `phrases.rs` explain text, same as the other two, but its own computation
  is not a one-way ratchet regardless of which path uses it.
- **Verdict:** not a ratchet by construction; no action needed.

## Summary table

| Feature | Cumulative / full-history? | Feeds default live score (`scoring.rs`)? | Feeds live score via trained ranker (`apply_trained_ranker`)? | Ratchet risk |
|---|---|---|---|---|
| `total_churn` | Yes — lifetime sum, non-windowed | No | Yes | **Yes**, opt-in path only |
| `convention_bug_fix_count` | Yes — full-history count | No | Yes | **Yes**, opt-in path only |
| `directed_coupling` | No — ratio, and self-adapts to a trailing window when file rankings are unstable | No | Yes | No |

## Why no code fix was made on this branch

`burst_score`'s fix was small and clear-cut: delete one term from one
formula (`scoring.rs`), leaving `history_signals.rs`'s computation and
`trainer::cold_start_features` untouched. The equivalent fix here would not
be that small:

- `total_churn` and `convention_bug_fix_count` are two of the 10
  index-stable features (`model_version = 5`, per `trainer.rs`'s module doc)
  that `hotspots train` fits a RandomForest/Tweedie model against. Removing
  either changes the trained feature vector shape, which means bumping
  `model_version`, invalidating every `.hotspots/ranker.json` a user has
  already trained, and touching `trainer.rs`'s `FEATURE_NAMES`,
  `extract_features`, `score`, and every call site/test enumerated in the
  grep above (12+ files by the `directed_coupling`/`total_churn`/
  `convention_bug_fix_count` occurrence counts alone).
- Unlike the additive linear formula in `scoring.rs`, the trained ranker can
  be a decision tree (`linfa_trees::DecisionTreeParams`) or a Tweedie
  regressor, so whether a cumulative feature actually produces a
  never-decreasing score contribution depends on the fitted model's
  structure and sign, not on the feature alone — this needs its own
  train-time analysis (e.g. checking learned feature importances/monotonic
  direction across trained repos) rather than a mechanical "remove the term"
  edit.
- The trained-ranker path is opt-in (`hotspots train`) and not the default
  scoring behavior, so the blast radius and urgency differ from the
  `burst_score` case, which affected every repo's default score
  unconditionally.

Per CLAUDE.md's guidance for promotion briefs ("if a criterion cannot be met,
note the blocker in the tracker row and ask — do not silently work around
it") and issue #172's own instruction to prefer filing a follow-up rather
than scope-creep when a fix is non-trivial: **this audit documents the
finding but implements no code fix.** A follow-up issue should decide
whether to (a) exclude `total_churn` and `convention_bug_fix_count` from the
trained-ranker feature set (bumping `model_version`), (b) replace them with
windowed/ratio variants analogous to `directed_coupling`'s adaptive window,
or (c) leave them as-is with documentation noting the trained-ranker path can
ratchet for files with an old burst of fixes/churn, since — unlike the
default score — a user must explicitly opt in via `hotspots train` to hit
this path.

## Cross-repo note

This is a CLI-only audit. Per the task scope, `../hotspots-research`'s
promotion tracker was not modified — the `burst_score`-specific case is
already tracked there as promoted (`burst-score-remove-from-live-score.md`,
PR #126). A broader "cumulative/full-history feature audit" finding is not
yet tracked in that repo; if a follow-up issue is filed for the
trained-ranker fix described above, it should be logged there per the
`hotspots-research` promotion-tracker workflow at that time.
