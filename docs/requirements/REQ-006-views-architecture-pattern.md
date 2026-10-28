# REQ-006 — Views Architecture Pattern

**Source:** parent-repo design discussion, generalized from REQ-003 (explanation layer) and
REQ-005 (LLM context augmentation output)
**Status:** draft — architecture pattern, not itself a shippable feature

---

## Problem

Hotspots computes one master result per analysis run: `Snapshot` / `FunctionSnapshot` in
`hotspots-core`. Multiple consumers need different shapes of that same data:

- `AgentFunctionView` (existing) — JSON shape for agent/tool consumers.
- REQ-003's `✦` explanation phrase (shipped: `--explain` flag, PR #115 + follow-ups) — terminal
  text annotation for CRITICAL/HIGH functions, rendered from `hotspots-cli/src/cmd/analyze.rs`
  and `hotspots-core/src/phrases.rs`.
- REQ-005's LLM-context output (draft) — bounded, prompt-ready projection for hosted coding
  models.
- `hotspots-cloud`'s `summary.json` and `hotspots-research`'s scripts — external consumers
  reading the raw snapshot JSON today, with no formal view layer at all.

Without a named pattern, each new consumer either grows its own ad hoc transformation inline
(risk of silently duplicating computation) or reads raw snapshot fields directly (risk of
breaking silently when the master schema changes, since there's no contract boundary).

## What to build

A convention — not a new runtime system — for how any projection of `Snapshot`/
`FunctionSnapshot` gets built, named, and versioned. Codify what `AgentFunctionView` already
does by accident into something explicit so REQ-005 and future consumers follow it rather than
reinvent it.

## Requirements

1. **One master struct, many view structs.** `Snapshot`/`FunctionSnapshot` remains the only
   computed source of truth (churn, complexity, coupling, history signals). A view is a
   `From<&FunctionSnapshot>` (or equivalent `to_x_view()`) conversion — it may select, rename,
   bound (e.g. top-N), filter, or render fields to prose, but must never recompute a metric
   itself. Any logic that touches raw churn/complexity/coupling math belongs in `analysis.rs`,
   not in a view.
2. **Views live in one place.** New module `hotspots-core/src/views/`, one file per consumer:
   e.g. `views/agent.rs` (existing `AgentFunctionView`, moved here), `views/explain.rs`
   (REQ-003's phrase rendering), `views/llm_context.rs` (REQ-005). This makes "what views exist
   and who consumes them" answerable by listing a directory, not grepping the codebase.
3. **Each view versions independently of the master snapshot schema.** A wording change to
   `llm_context`'s prose output is not a `snapshot.json` schema change and should not force a
   major bump there. Conversely, a field removed from `Snapshot` is a breaking change to every
   view that reads it — views must be checked, not assumed compatible.
4. **Schema-change checklist gets a views entry.** The parent repo's `CLAUDE.md` "Snapshot
   schema changes" checklist currently lists `hotspots-content`, `hotspots-cloud`,
   `hotspots-research` as external consumers to update. Add: "any view in
   `hotspots-core/src/views/` that reads the changed field" as an internal consumer to check
   in the same release window.
5. **External consumers reading raw snapshot JSON are out of scope for conversion**, but new
   external integrations should be pointed at a view (once one exists for their use case)
   rather than raw snapshot fields, since views carry an explicit contract and raw fields do
   not.

## Non-goals

- Not a new serialization framework, macro, or trait beyond a plain conversion function/impl.
- Not a runtime plugin system — views are compiled-in, not dynamically registered.
- Does not require migrating `hotspots-cloud`/`hotspots-research`'s existing raw-JSON reads to
  a view in this change; that is optional follow-up, repo by repo, not a blocking requirement.
- Does not change any existing output format — `AgentFunctionView`'s move into `views/` is a
  file-location change only, not a behavior or schema change.

## Acceptance criteria

1. `hotspots-core/src/views/` exists with `agent.rs` containing the relocated
   `AgentFunctionView` and conversion logic, behavior unchanged (existing tests pass unmodified).
2. `hotspots-core/src/lib.rs` exposes `pub mod views;`.
3. Parent repo's `CLAUDE.md` schema-change checklist updated to mention `hotspots-core/src/views/`.
4. No metric computation exists inside any file under `views/` — grep for churn/complexity/
   coupling computation calls to confirm they all remain in `analysis.rs`.
5. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` all pass.

## Relationship to other REQs

- **REQ-003** (ranker explanation layer): already shipped and user-facing (`--explain`).
  Relocating its phrase-rendering logic into `views/explain.rs` under this pattern is a
  refactor of live code, not greenfield work — it must preserve `--explain`'s current CLI
  output byte-for-byte (acceptance criterion 1 covers this) and should land as its own PR,
  separate from REQ-005, so a regression in a shipped flag isn't bundled with new work.
- **REQ-005** (LLM context augmentation output): not yet shipped. Should be implemented as
  `views/llm_context.rs` from the start, once this pattern lands, rather than retrofitted.
- Implementation order: this REQ (structural convention) should land before REQ-005. The
  REQ-003 relocation can land in the same PR as this REQ or as a fast-follow — either way,
  gate it on full regression coverage of `--explain`'s existing output, not just new tests.
