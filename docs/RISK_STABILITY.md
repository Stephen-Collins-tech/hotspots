# Risk Score Stability & Predictability

Hotspots exists to tell teams "this is the risky code" in a way they can trust
over time. That promise only holds if the score itself behaves predictably —
the same input always yields the same output, a score doesn't drift for
reasons unrelated to the code, and when a score *does* change, the reason is
attributable. This document surveys the mechanisms already in place, the
places where predictability is weaker than it looks, and concrete next steps.

## Why this matters

A risk score that moves for the wrong reasons is worse than no score at all:
it erodes trust in CI gates (`policy.rs`), makes `hotspots diff`/`trends`
noisy, and makes engineers stop believing "Critical" means something. Two
different failure modes threaten this:

- **Non-determinism** — the same commit, analyzed twice, produces different
  numbers (randomness, iteration-order bugs, clock/thread dependence).
- **Unexplained drift** — the same function's score changes because of a
  tool upgrade, an unrelated file elsewhere in the repo, or a rolling time
  window, with no signal telling the user *why*.

## A. Existing mechanisms

**Determinism is a stated architectural invariant.** `docs/ARCHITECTURE.md`
enumerates it explicitly: no randomness/clocks/threads/async in analysis,
deterministic traversal order (files by path, functions by byte offset),
whitespace/formatting invariance, and byte-for-byte identical output for
identical input. CI runs dedicated "determinism tests" that execute analysis
twice and diff the result. Several core modules (`risk.rs`, `snapshot.rs`,
`delta.rs`, `trends.rs`, `policy.rs`, `suppression.rs`) restate the same
invariants in their own doc headers — pure functions, deterministic sort
order, no snapshot mutation.

**LRS is absolute, not relative.** The Local Risk Score is a per-function
weighted sum of log-scaled, capped structural metrics (cyclomatic complexity,
nesting depth, fan-out, non-structured exits). It does not depend on any
other function or file in the repo, so it cannot shift just because unrelated
code was added or removed elsewhere. This is a deliberate design choice
(`risk.rs`, `ARCHITECTURE.md`): "LRS is per-function and named 'Local'
deliberately."

**Population-relative signals are scoped away from the core score.**
Percentile flags (top 10/5/1%) and driver-label thresholds are intentionally
relative to the repo's own distribution (default P75) — this is documented
as a conscious trade-off, since fixed absolute thresholds would fire on
different functions in a 100-function repo versus a 100k-function one. The
important point is that this relativity is confined to labels/explanations
and does not leak into LRS or the risk bands, which use fixed absolute
cutoffs.

**Config validation rejects degenerate states.** `HotspotsConfig` uses
`deny_unknown_fields` so typos fail loudly instead of being silently
ignored, and `validate()` rejects things like all-zero LRS weights or
unordered thresholds that would collapse or corrupt scoring.

**Schema and model versioning catch stale-data misinterpretation.**
`SNAPSHOT_SCHEMA_VERSION` and `DELTA_SCHEMA_VERSION` are checked on load, and
the trained ranker's `RankerModel.model_version` rejects models trained on
an older feature set rather than silently scoring with mismatched features.

**Randomness is seeded everywhere it appears.** Isolation Forest anomaly
scoring and the RandomForest trainer both take an explicit `seed: u64`
(`isolation_forest.rs`, `trainer.rs`), with the cold-start path using a fixed
seed (`COLD_START_SEED = 42`). Given the same input rows, forest structure
and scores are reproducible run-to-run, and trained models are persisted to
JSON rather than retrained on every invocation.

**Delta/trend computation is order-independent by construction.** Functions
are matched by a stable `function_id` rather than file/line position, so
pure code moves don't register as delete+add. IDs and deltas are explicitly
sorted before serialization so JSON output order doesn't depend on
`HashMap` iteration order, and a rename-hint heuristic prevents pure renames
from being misread as "new Critical function" by policies like
`critical-introduction`.

**CI-facing gates are deterministic and auditable.** `policy.rs` evaluation
is pure and IO-free, with results sorted for stable diffs. Downgrading a
policy's severity below Block requires a mandatory `_reason` string in
config, forcing an auditable trail rather than a silent gate-weakening.
Separately, `gate.rs` is a self-check on the *ranker's* predictive validity
(P@10 against recent fix-commits) that recommends falling back to an
alternate classifier when the model itself isn't performing — a check on
whether the tool's notion of risk still matches reality for a given repo.

**The tool tracks its own ranking volatility as a feature.**
`trends.rs::compute_hotspot_stability` classifies functions as
Stable/Emerging/Volatile based on how consistently they land in the top-K
ranking across a snapshot window — stability of the *output* is treated as
a first-class signal, not an afterthought.

**Past instability has been actively corrected.** The `burst` weight
(`scoring.rs`) was validated, found to be a monotonic non-decaying ratchet —
a historical activity spike would keep a function looking "Critical"
forever — and removed from the live composite score, with the field kept
as a documented no-op for output-schema compatibility. Similarly,
`trainer.rs` documents features deliberately excluded from training
(`touch_count_30d`, `days_since_last_change`, `activity_risk`,
`convention_bug_fix_rate`) because of temporal leakage that would make the
ranker unstable or circular.

## B. Where predictability is weaker than it looks

- **No formula version surfaced in output.** Snapshots record a
  `tool_version`, but scoring weights and thresholds can change between
  releases with nothing tying that change to the snapshot. A score delta
  across a tool upgrade is currently indistinguishable from a delta caused
  by a real code change.
- **No config schema version.** Config validation is strict about unknown
  keys, but there's no `schema_version` field to migrate or flag a future
  breaking change in weight/threshold semantics.
- **Population-relative labels sit right next to an absolute score.**
  Driver/quadrant/percentile labels are *designed* to move when the rest of
  the repo changes — but users diffing output commit-to-commit may not
  realize that a driver-label flip doesn't mean the function's own risk
  changed, since it's presented alongside the (stable) LRS number.
- **Cold-start routing has no hysteresis.** The Gini-coefficient thresholds
  that decide between Formula-based and Anomaly-based ranking are hardcoded
  with no dead zone, so a repo sitting near the boundary could flip ranking
  strategy on a marginal commit.
- **The suppression gate can be noisy.** `gate.rs`'s P@10 verdict is
  computed over a rolling fix-commit window with no smoothing, so a
  CI-facing Pass/Suppressed recommendation could oscillate week to week for
  repos with sparse or bursty fix-commit history.
- **No determinism test for the trainer.** Unit tests fix RNG seeds, but
  there's no test equivalent to the analysis determinism tests that asserts
  `hotspots train`/`cold_start_rank`, run twice on identical input, produce
  a byte-identical model.
- **Rename-hint matching is a first-match heuristic**, not a stable
  tie-break — correct today, but fragile if candidate ordering ever changes
  incidentally as a side effect of an unrelated refactor.

## C. Possible next steps

1. Add a `formula_version` (or `scoring_version`) field to snapshot
   `AnalysisInfo`, bumped whenever default weights/thresholds change, so
   `hotspots diff`/`trends` can separate "the tool changed" from "the code
   changed."
2. Add a `schema_version` field to `HotspotsConfig` with the same
   deny-unknown-fields discipline already used for keys.
3. Add a configurable dead zone/hysteresis band around the cold-start
   Gini-routing thresholds, mirroring the watch/attention bands
   `policy.rs` already uses elsewhere for boundary flicker.
4. Add determinism tests for `hotspots train`/`cold_start_rank` analogous to
   the existing analysis determinism tests.
5. Document, and consider visually separating in output, which fields are
   population-relative (percentile, driver, quadrant) versus absolute (LRS,
   risk band) — so CI consumers can choose to key gates only on the stable
   subset.
6. Smooth the suppression gate's verdict (e.g. require N consecutive
   Suppressed readings, or a rolling average) instead of a single-window
   snapshot judgment.
7. Extend the audit that removed `burst_score` to other cumulative,
   full-history features (e.g. `convention_bug_fix_count`) to check for the
   same one-way-ratchet risk.
