# Agent-Optimized JSON Output

**Motivation:** The current JSON output is a flat dump of all functions — useful for tooling
and visualization, but poorly shaped for AI agent consumption. Agents have context windows,
need pre-triaged output, and work best when the JSON tells them what to do rather than what
to compute. The HTML report already has the right mental model (triage quadrants, driver
badges, hidden coupling). This document specifies a JSON shape that mirrors that model and
makes hotspots usable as an agent skill.

**Principle:** Agents don't need all 542 functions. They need the decision-relevant subset,
pre-classified, with action text included. The full function list is opt-in.

---

## The Problem with Current JSON

Current shape:

```json
{
  "schema_version": 2,
  "commit": { ... },
  "summary": { ... },
  "functions": [ /* all 542, flat */ ],
  "aggregates": {
    "file_risk": [ ... ],
    "co_change": [ ... ],
    "modules": [ ... ]
  }
}
```

Problems:
- 542 functions is too much context for most agent tasks
- No pre-triage — agent must derive its own ranking and grouping
- `driver` label present but `action` text is not — agent must re-derive recommendations
- `co_change` mixes expected pairs and hidden coupling — agent must filter
- Absolute file paths — agents working in repo context need relative
- Field names differ from HTML data attributes (`activity_risk` vs `data-activity`) —
  two schemas for the same model

---

## Target Shape

```json
{
  "schema_version": 3,
  "commit": {
    "sha": "2d4001f",
    "branch": "feature/dimensions",
    "timestamp": 1739923200
  },
  "triage": {
    "fire":  { "count": 4,  "top": [ /* top N functions, high complexity + high churn  */ ] },
    "debt":  { "count": 16, "top": [ /* top N functions, high complexity + low churn   */ ] },
    "watch": { "count": 3,  "top": [ /* top N functions, low complexity + high churn   */ ] },
    "ok":    { "count": 519 }
  },
  "co_change": {
    "hidden_coupling": [ /* pairs with has_static_dep=false only */ ],
    "total_pairs": 155,
    "hidden_count": 151
  },
  "file_risk":  [ /* top 10 by file_risk_score */ ],
  "modules":    [ /* all, already small */ ],
  "summary":    { /* unchanged */ }
}
```

`functions` array omitted by default. Available via `--all-functions` flag.

---

## Function Shape (in triage.*.top)

```json
{
  "function": "handle_mode_output",
  "file": "hotspots-cli/src/main.rs",
  "line": 630,
  "band": "critical",
  "quadrant": "fire",
  "driver": "high_complexity",
  "action": "Stable debt: schedule a refactor. Extract sub-functions to reduce CC.",
  "lrs": 12.88,
  "activity_risk": 14.21,
  "metrics": { "cc": 15, "nd": 3, "fo": 13 },
  "touches_30d": 11,
  "days_since_changed": 1
}
```

Key decisions:
- `action` is a first-class field — agent can use it directly without re-deriving
- `file` is relative to repo root, not absolute
- `quadrant` is explicit on each function — no need to re-derive from context
- Metrics kept to the three that drive classification (cc, nd, fo) — full metrics in
  `--all-functions` output

---

## Co-Change Shape

```json
{
  "hidden_coupling": [
    {
      "file_a": "hotspots-cli/src/main.rs",
      "file_b": "hotspots-core/src/lib.rs",
      "co_change_count": 5,
      "coupling_ratio": 1.0,
      "risk": "high"
    }
  ],
  "total_pairs": 155,
  "hidden_count": 151
}
```

`expected` pairs (has_static_dep=true) are omitted from the default output — they are not
actionable for an agent. Available in `--all-functions` output if needed.

---

## Quadrant Classification

Mirrors the HTML triage section. Classification uses the same percentile-relative thresholds
as the driver detection (SQ-3), so quadrant and driver are always consistent.

| quadrant | condition |
|---|---|
| `fire`  | band=critical or high AND touches_30d > touch_p50 |
| `debt`  | band=critical or high AND touches_30d ≤ touch_p50 |
| `watch` | band=moderate or low AND touches_30d > touch_p75  |
| `ok`    | everything else                                   |

The HTML currently derives quadrants independently. This spec makes the quadrant a computed
field on each `FunctionSnapshot` during enrichment, ensuring HTML and JSON always agree.

---

## Field Name Alignment (HTML ↔ JSON)

| HTML data attribute | Current JSON field | Target JSON field |
|---|---|---|
| `data-activity`  | `activity_risk`       | `activity_risk` (unchanged) |
| `data-driver`    | `driver`              | `driver` (unchanged) |
| `data-band`      | (derived from lrs)    | `band` (add to snapshot) |
| `data-recency`   | `days_since_changed`  | `days_since_changed` (rename) |
| `data-touches`   | `touch_count_30d`     | `touches_30d` (rename for brevity) |
| `data-fanin`     | `callgraph.fan_in`    | `fan_in` (flatten) |
| `data-lrs`       | `lrs`                 | `lrs` (unchanged) |
| `data-cc`        | `metrics.cc`          | `metrics.cc` (unchanged) |
| `data-nd`        | `metrics.nd`          | `metrics.nd` (unchanged) |
| (none)           | (none)                | `quadrant` (new) |
| (none)           | (none)                | `action` (new, from SQ-2) |

---

## CLI Surface

```
# Default: triage view (agent-optimized)
hotspots analyze . --mode snapshot --format json

# Full function list (tooling/visualization)
hotspots analyze . --mode snapshot --format json --all-functions

# Top N per quadrant (default: 5, configurable)
hotspots analyze . --mode snapshot --format json --top 10
```

`--top N` controls the size of each `triage.*.top` array, not a global filter.
`--all-functions` appends the full `functions` array to the output.

---

## Tasks

### AJ-1: Quadrant classification on FunctionSnapshot

- [ ] **AJ-1a:** Add `quadrant: Option<String>` field to `FunctionSnapshot` in `snapshot.rs`.
  Values: `"fire"`, `"debt"`, `"watch"`, `"ok"`. `None` before enrichment.
- [ ] **AJ-1b:** Add `compute_quadrants()` to the snapshot enricher. Uses same percentile
  thresholds as SQ-3. Must run after `compute_activity_risk()` and `populate_driver_labels()`.
- [ ] **AJ-1c:** Update HTML report to derive quadrant from `function.quadrant` field rather
  than computing independently. Ensures HTML and JSON always agree.
- [ ] **AJ-1d:** Add `band: String` as a flat field on `FunctionSnapshot` (derived from LRS
  thresholds). Currently only derivable by the consumer. Make it explicit.

**Effort:** Low. Additive fields, no existing logic changes except HTML read path.
**Risk:** Low. HTML behaviour unchanged; quadrant computation extracted not replaced.

---

### AJ-2: Agent-optimized JSON structure

- [ ] **AJ-2a:** Add `TriageView` struct to `aggregates.rs`:
  `fire`, `debt`, `watch` each contain `count: usize` and `top: Vec<AgentFunctionView>`.
  `ok` contains only `count`.
- [ ] **AJ-2b:** Add `AgentFunctionView` struct — the slim function shape for triage output.
  Fields: `function`, `file` (relative), `line`, `band`, `quadrant`, `driver`, `action`,
  `lrs`, `activity_risk`, `metrics` (cc/nd/fo only), `touches_30d`, `days_since_changed`.
- [ ] **AJ-2c:** Add `compute_triage_view(functions, top_n)` to `aggregates.rs`. Groups
  functions by quadrant, sorts each group by `activity_risk` descending, takes top N.
- [ ] **AJ-2d:** Update snapshot JSON output: replace top-level `functions` array with
  `triage` object as default. Add `--all-functions` flag to restore full array.
- [ ] **AJ-2e:** Update `co_change` in JSON output: split into `hidden_coupling` (has_static_dep=false)
  and move expected pairs out of default output. Add `hidden_count` and `total_pairs` fields.

**Effort:** Medium. New structs, new output path, flag plumbing.
**Risk:** Medium. Breaking change to default JSON shape — consumers of current flat
`functions` array will need to use `--all-functions`. Document in CHANGELOG.

---

### AJ-3: Field name alignment

- [ ] **AJ-3a:** Rename `touch_count_30d` → `touches_30d` on `AgentFunctionView` (slim shape
  only — keep original name on `FunctionSnapshot` for backward compat).
- [ ] **AJ-3b:** Add `action` field to `AgentFunctionView` — populated from `driving_dimension()`
  action text. Agents can use this string directly.
- [ ] **AJ-3c:** Flatten `callgraph.fan_in` → `fan_in` on `AgentFunctionView`.
- [ ] **AJ-3d:** Update HTML `data-*` attributes to match the target field names where they
  currently differ. HTML and JSON should use the same vocabulary.

**Effort:** Low. Mostly renaming in the slim view; no changes to `FunctionSnapshot`.
**Risk:** Low. `AgentFunctionView` is new; `FunctionSnapshot` field names unchanged.

---

### AJ-4: Documentation and schema

- [ ] **AJ-4a:** Update `docs/reference/json-schema.md` with the v3 schema.
- [ ] **AJ-4b:** Add `docs/guide/agent-skill.md` — how to use hotspots as an agent skill,
  what each triage quadrant means, how to act on co-change hidden coupling.
- [ ] **AJ-4c:** Add entry to CHANGELOG for the v3 JSON breaking change.

**Effort:** Low.
**Risk:** Low.

---

## Ordering

```
AJ-1 (quadrant on snapshot)   — prerequisite for AJ-2 and HTML alignment
AJ-2 (triage JSON structure)  — blocked by AJ-1
AJ-3 (field alignment)        — can run in parallel with AJ-2
AJ-4 (docs)                   — after AJ-2 and AJ-3
```

Start with AJ-1a/1b (quadrant computation) — lowest risk, unblocks everything else.

---

## What this unlocks

An agent skill that calls:

```
hotspots analyze . --mode snapshot --format json
```

Gets back a response it can act on immediately:

- `triage.fire.top` → "refactor these this sprint"
- `triage.debt.top` → "schedule these for next quarter"
- `triage.watch.top` → "add tests before next change"
- `co_change.hidden_coupling` → "these files are secretly coupled — investigate"

No post-processing. No ranking. No filtering. The JSON is the recommendation.

---

**Created:** 2026-02-20
