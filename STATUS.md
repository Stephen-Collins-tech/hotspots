# hotspots — Work in Progress

Current version: **1.37.0**
Last updated: 2026-09-09

This document tracks what is ready to implement, what is in flight, and known gaps
against the project's own process rules. It is the working companion to
[`docs/requirements/`](docs/requirements/), [`benchmarks/RESULTS.md`](benchmarks/RESULTS.md),
[`TASKS.md`](TASKS.md) (active promotion-brief handoffs from `../hotspots-research`),
and open GitHub issues for day-to-day tracking — this file had gone unmaintained since
the v1.25.3 baseline (2026-06-27) through eleven releases; the sections below are a
2026-09-09 correction pass, not a claim that this file is kept current release-to-release
going forward. Prefer `CHANGELOG.md`, `TASKS.md`, and GitHub issues for anything more
recent than this correction.

---

## REQ-001 through REQ-004 — resolved

The four requirements this file originally tracked as "ready to implement" have all
been resolved, three shipped and one rejected after implementation:

- **REQ-001 (history depth tier annotation) — rejected, not shipped.** Implemented as
  hotspots PR #133, but the finding behind it (F10, `hotspots-research`) was
  re-triaged after implementation and found not to survive function-granularity
  testing — the confidence-tier premise (`history_depth` predicting ranker
  reliability) did not hold once measured correctly. PR #133 was closed 2026-09-06,
  not merged. See `hotspots-research/docs/findings/10-history-depth-and-signal-quality.md`
  and the F10 row in `hotspots-research/docs/promotion-tracker.md`. Do not
  re-attempt this without first re-reading that closure — the rejection is about the
  underlying signal, not the implementation.
- **REQ-002 (`convention_bug_fix_count` as 10th ranker feature) — shipped.** Landed
  v1.26.0 (`convention_bug_fix_count` collected) with the trained-ranker feature
  activation following; see `docs/requirements/REQ-002-convention-bug-fix-feature.md`
  and the F54 row in `hotspots-research/docs/promotion-tracker.md` (`promoted`).
- **REQ-003 (ranker explanation layer, `✦` phrases) — shipped.** `--explain` flag +
  `phrases.rs`, hotspots PR #115 (commit `7579672`). See TASKS.md's "done" entry.
- **REQ-004 (public benchmark corpus) — shipped.** `benchmarks/run.sh` and
  `benchmarks/corpus.json` were the two missing pieces this file called out; both
  were built as part of the F93 task (see TASKS.md), which also re-ran the full
  7-repo benchmark (`benchmarks/versions/v1.30.0.json`).

---

## Known gap: the benchmark has not been re-run since v1.30.0

STATUS.md's own rule below (kept from the original version of this file, still the
right rule) has not been followed for the last seven releases:

> Every future release that changes ranking or scoring must run the benchmark and
> append a new block to `RESULTS.md` before the release tag is cut.

`benchmarks/RESULTS.md`'s most recent entry is **v1.30.0** (2026-07-12). Since then,
at least one release changed the live scoring formula in a way the benchmark rule
was written to catch: **v1.33.2** removed the `burst_score` term from
`compute_activity_risk` entirely (`burst-score-remove-from-live-score`,
hotspots PR #126) — a real change to what `activity_risk` computes for every repo,
never benchmarked. Other since-v1.30.0 changes with plausible ranking impact:
the F62/F63 cold-start Gini routing (v1.33.0), the cold-start Gini dead zone
(v1.37.0), and the suppression-gate smoothing (v1.37.0) — none of these have a
`benchmarks/versions/v1.3x.x.json` entry either.

This means `benchmarks/RESULTS.md`'s numbers do not reflect what v1.37.0 actually
computes. Re-running the 7-repo benchmark and appending a current block is the
single most out-of-date piece of process debt this file knows about as of this
correction pass — flagged here rather than silently run, since it's a
multi-repo-clone, non-trivial task that deserves its own scoped pass rather than
being folded into a docs-audit commit.

---

## Language support gaps

hotspots currently supports: TypeScript, JavaScript, Go, Java, Python, Rust, Vue, C#, C.

**Ruby is not supported.** `rails/rails` was excluded from the benchmark corpus on this
basis. Adding Ruby support (tree-sitter-ruby grammar) would unlock a significant class of
well-known repos. Tracked as a future addition — not in scope for any current REQ.
