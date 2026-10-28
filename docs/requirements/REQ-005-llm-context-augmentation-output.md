# REQ-005 — LLM Context Augmentation Output

**Source:** parent-repo design discussion, "Hotspots as a Context Layer for Hosted Coding Models"
**Status:** draft — requirements only, not yet scoped for implementation

---

## Problem

When a hosted coding model (Claude, etc.) is asked to review or modify code, it has no signal
about which files/functions in the repo are historically defect-prone. Hotspots already computes
this signal locally and deterministically. Today there is no output mode shaped for a model to
consume as prompt context — callers would have to hand-parse `snapshot.json` or CLI text output
and reformat it themselves.

This requirement covers exactly one integration pattern: **pre-request context enrichment** —
emitting hotspots' risk signal in a form meant to be inserted into a model's prompt before it
reasons about the code. It does not cover context-window budgeting, request routing, or
post-hoc validation of model output (see Non-goals).

## What to build

A new output mode on the existing scoring/report pipeline — e.g. a `--format llm-context` value
(exact flag/shape TBD, see Open questions) — that renders the current risk computation as a
block suitable for direct inclusion in an LLM prompt.

This is a formatter over existing output, not a new analysis pass. It must not introduce a new
scoring algorithm, a new metrics collector, or a parallel code path to `snapshot.json`.

## Requirements

1. **Reuses the existing scoring pipeline.** No duplicate computation of churn, complexity,
   coupling, or history signals — this mode only reshapes values already produced for
   `snapshot.json` / `AgentFunctionView`.
2. **Feature-level detail, not a bare score.** Each flagged file/function includes the risk
   score *and* the contributing signals in short form (churn, complexity trend, coupling,
   past-bug density, recency). REQ-003's explanation layer (`--explain`, shipped) already
   computes exactly this as `FunctionSnapshot.explanation` / `AgentFunctionView.explanation`
   via `phrases::top_phrases()` — this mode should consume that existing field directly
   rather than deriving its own phrasing.
3. **Bounded output.** Caller can limit how many files/functions are included (top-N by score,
   or a threshold flag) so this can be dropped into a prompt without exceeding a token budget.
   Unbounded output (every function in the repo) must not be the only option.
4. **Deterministic.** Same repo state at the same commit produces byte-identical output
   (aside from any timestamp field, which must be isolable/omittable).
5. **Versioned, documented schema.** Any new JSON shape added for this mode goes through the
   same schema-change checklist as `snapshot.json`/`aggregates.*` (tracked in the parent
   `hotspots-repos/CLAUDE.md`) so `hotspots-cloud`, `hotspots-research`, and any future adapter
   don't silently break.
6. **No required dependency on a private sibling.** Works standalone in the public `hotspots`
   repo; makes no calls to `hotspots-cloud`, `hotspots-research`, or `hotspots-analytics`.

## Non-functional requirements

- No new runtime dependency (no HTTP client, no model SDK, no network call of any kind).
  The command's job ends at producing text/JSON on stdout.
- Same performance budget as existing `hotspots analyze`/`hotspots report` — this is a
  formatting layer, not a new pass over the repo.
- The output is the stable contract. Anything that wires this into a specific model or tool
  (an MCP server, a Claude Code hook, a CI step) is a thin adapter maintained outside this
  repo's scope — this repo does not ship such an adapter.

## Non-goals (explicitly out of scope for this requirement)

- Context-window token budgeting or automatic truncation/summarization strategy — a caller
  concern, not this mode's.
- Request routing or triage logic (deciding which model tier to call based on risk) — not a
  hotspots responsibility.
- Post-hoc re-scoring/validation of a model's proposed diff — a separate requirement if pursued.
- Any MCP server, HTTP server, or long-running process. This is a CLI output mode only.
- Any fine-tuning, self-hosted inference, or learned re-ranker.

## Open questions (resolve before scoping implementation)

1. **Output shape:** reuse `snapshot.json`'s existing per-file/per-function fields verbatim
   (cheapest, most consistent with existing consumers), or a new purpose-built schema optimized
   for prompt-readability (prose phrases, similar to REQ-003's `✦` explanation line, instead of
   raw numbers)? Recommend starting from the former and only diverging if raw JSON reads poorly
   as LLM context in practice.
2. **Flag surface:** new `--format` value, a new subcommand, or a flag on existing `analyze`/
   `report` commands? Should follow whatever precedent `--explain` (REQ-003) sets once that
   ships.
3. **Scope unit:** per-file, per-function, or both? REQ-003's explanation layer is per-function;
   this may want a per-file rollup for prompt compactness.
4. **Relationship to REQ-003:** REQ-003 is already shipped (`--explain`, PR #115 + follow-ups),
   not a future dependency. This mode should require `--explain` (or compute percentiles
   itself if `--explain` wasn't passed) and reuse `explanation` directly rather than inventing
   separate phrasing.

## Acceptance criteria (draft — revisit once scoping questions above are answered)

1. Default output of existing commands is unchanged (byte-identical) when this mode's flag is
   not passed.
2. New mode's output is deterministic across repeated runs on the same commit.
3. No network call, no subprocess, no LLM invocation at any point in this mode.
4. New JSON fields (if any) are documented in the schema-change checklist location named in
   `hotspots-repos/CLAUDE.md`.
5. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` all pass.
