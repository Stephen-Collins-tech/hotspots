# Output & Infrastructure Improvements

**Motivation:** Several output features are referenced in the CLI or architecture docs
but not yet implemented: HTML output for `hotspots trends`, snapshot compaction (levels
1 and 2), and better rename tracking in delta. These are self-contained improvements
that don't require new analysis features.

---

## OT-1: HTML Output for `hotspots trends`

**Current state:** `hotspots trends . --format html` returns a hard error:
`"HTML/JSONL format is not supported for trends analysis"`.

**Summary:** Add an interactive HTML report for trend analysis, parallel to the existing
HTML snapshot report. Should visualize risk velocity over time and highlight stable
hotspots.

**Proposed output sections:**
1. **Risk Velocity chart** — sparkline or table of LRS over snapshots for top-K functions
2. **Hotspot Stability table** — sorted by stability score (stable → volatile)
3. **Refactor Effectiveness table** — functions with detected LRS drops

**CLI surface:**
```
hotspots trends . --format html --output trends-report.html
```

**Tasks:**
- [ ] **OT-1a (design):** Define the HTML template structure. Decide: extend the
  existing `report.html` Handlebars/Tera template, or generate from a separate
  trends-specific template.
- [ ] **OT-1b:** Implement `print_trends_html_output()` in `hotspots-cli/src/main.rs`.
  Write to `--output` path (default `.hotspots/trends-report.html`).
- [ ] **OT-1c:** Remove or update the `anyhow::bail!` error guard for HTML format in
  `handle_trends()`.
- [ ] **OT-1d:** Add a `--output` flag to `hotspots trends` (currently only `--format`,
  `--window`, `--top` are exposed).
- [ ] **OT-1e:** Document the new format in `docs/reference/cli.md` under the
  `hotspots trends` section.

**Effort:** Medium. Template work + wiring. JSON output is already correct; this is
presentation only.
**Risk:** Low. Additive; existing `json`/`text` modes unchanged.

---

## OT-2: Snapshot Compaction (Levels 1 and 2)

**Current state:** `hotspots compact --level 1` and `--level 2` set the compaction
level in the index metadata but immediately return an error: `"compaction to level N
is not yet implemented"`. Level 0 (full snapshots, current) is the only working mode.

**Summary:**
- **Level 1 (delta-only):** Keep only the delta between consecutive snapshots instead
  of full snapshots. Reduces storage ~5-10× for large repos with many snapshots.
- **Level 2 (band-transitions-only):** Keep only snapshots where a function changes
  risk band. Maximum compression; loses fine-grained LRS history.

**Design note:** Compaction requires `hotspots trends` and `hotspots analyze --mode delta`
to reconstruct full snapshots from deltas on demand. This is a significant scope
increase. Level 1 and 2 are only useful for repos with 1000+ snapshots.

**Tasks:**
- [ ] **OT-2a (design):** Document the on-disk format for delta-only snapshots.
  Define reconstruction algorithm: `full_snapshot(N) = full_snapshot(base) + Σ deltas`.
  Identify the read path that must change (`Snapshot::from_json` callers).
- [ ] **OT-2b:** Implement level 1 compaction: on `hotspots compact --level 1`,
  convert existing full snapshots to delta format keeping one base every N commits.
- [ ] **OT-2c:** Update `hotspots trends` and delta mode to reconstruct full snapshots
  from deltas when the index indicates compaction level > 0.
- [ ] **OT-2d:** Implement level 2 as a further compaction of level 1.

**Effort:** High. Requires format change + read-path reconstruction. Not recommended
until there is user demand from large repos.
**Risk:** Medium. Touches the snapshot read path which is core to delta and trends.

---

## OT-3: Rename-Aware Delta Matching

**Current state:** Delta matching uses two heuristics in `delta.rs`:
1. Same function name, different file → rename hint
2. Same file, line number within ±10 lines → rename hint

There is no content-hash or signature-based matching. Renamed functions show as
delete + add, losing the LRS continuity signal.

**Summary:** Add a content-hash (or metric-signature) fallback for matching deleted
and added functions that aren't caught by heuristics. A function with identical
metrics `(cc, nd, fo, ns, loc)` and similar name is very likely the same function.

**Proposed matching priority:**
1. Exact `function_id` match (current — unchanged)
2. Same metrics signature + Levenshtein name distance < 3 (new heuristic)
3. Same metrics + same file (new heuristic for intra-file renames)
4. Current line-proximity heuristic (unchanged)

**Tasks:**
- [ ] **OT-3a (research):** Measure false-positive rate of existing heuristics on
  this repo's git history. Quantify how often renames produce spurious delete+add pairs.
- [ ] **OT-3b:** Implement the metrics-signature matching pass in `delta.rs` after
  exact ID matching and before the line-proximity heuristic.
- [ ] **OT-3c:** Expose `rename_hint` in JSON delta output (it is currently computed
  but may not be serialized — verify and add if absent).
- [ ] **OT-3d:** Add a golden test for a commit that renames a function and verify
  the delta shows `rename_hint` rather than a delete+add pair.

**Effort:** Medium. Heuristics are straightforward; main risk is false positives.
**Risk:** Low-Medium. Additive; existing delete+add behavior is preserved as fallback.

---

## Ordering / Dependencies

```
OT-1 (trends HTML)     — no dependencies, start now; highest user-visible value
OT-3 (rename delta)    — OT-3a research first; can proceed in parallel with OT-1
OT-2 (compaction)      — defer until large-repo user demand; highest implementation risk
```

---

**Created:** 2026-02-19
