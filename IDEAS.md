# Hotspots — Ideas & Future Directions

Captured after v1.9.0 (2026-03-24).

---

## Prioritized by Bang for Buck

### Tier 1 — High Value, Low Effort

**SARIF output format**
- Add `OutputFormat::Sarif` variant alongside Text/Json/Html/Jsonl
- SARIF is the standard for GitHub code scanning — enables native GitHub Actions integration with zero user config
- Effort: ~1–2 days (new output format, well-specified schema, no core changes)

**Pre-commit hook template generation**
- `hotspots init --hooks` writes a ready-to-use pre-commit config snippet
- Effort: ~half a day (mostly docs/template string)

**Kotlin support**
- Java CFG builder is 669 lines; Kotlin is structurally similar (JVM, tree-sitter grammar available)
- Effort: ~2–3 days (new parser + cfg_builder, update FunctionBody enum + all match arms)
- High reach: Kotlin is now the default Android language

---

### Tier 2 — High Value, Moderate Effort

**File-level aggregation in output**
- Roll up function scores to file level (max CC, avg LRS, total touches)
- `OutputLevel::File` already exists as an enum variant — it may be partially stubbed
- Effort: ~2–3 days
- Makes reports actionable for large codebases where function-level noise is overwhelming

**Configurable thresholds per language**
- Config file (already supported) gains language-specific CC/LRS cutoffs
- e.g., Python tends to have higher CC for idiomatic code than Go
- Effort: ~2–3 days (config schema + per-language filter pass)

**Historical trend charts in HTML report**
- html.rs is already 2820 lines with a full scatter plot
- Add a time-series chart using existing snapshot/trend data
- Effort: ~3–4 days (JS chart rendering, data serialization, HTML template work)

---

### Tier 3 — Moderate Value, Higher Effort

**C/C++ support**
- Tree-sitter grammar exists but C/C++ is complex: macros, headers, no clear function boundary convention
- Effort: ~1–2 weeks (parser + cfg_builder + significant edge-case handling)
- High corpus size but requires robust handling to avoid noisy results

**Cross-function call graph risk propagation**
- `callgraph.rs` (789 lines) already computes fan-in/fan-out and PageRank
- Would propagate risk scores up the call graph so callers of high-CC functions inherit risk
- Effort: ~3–5 days (scoring.rs + risk.rs changes, callgraph integration, golden test updates)

**Rust/Python `match`/`match-case` CFG accuracy**
- Rust CFG builder is 325 lines, Python 596 — match arms are partially handled
- Getting edge cases right (guards, binding patterns, exhaustiveness) is fiddly
- Effort: ~3–5 days per language

**Ruby support**
- No existing patterns to reuse (unlike Kotlin/Java)
- Effort: ~1 week

---

### Tier 4 — Lower Priority / Large Effort

**VS Code extension**
- Separate project, different tech stack (TypeScript extension API)
- Requires language server protocol or subprocess integration
- Effort: ~2–4 weeks

**async/await control flow modeling**
- Affects JS/TS, Python, Rust — each has different semantics
- Would significantly improve CC accuracy for modern codebases
- Effort: ~1–2 weeks per language, high correctness risk

**Exception propagation edges in CFG**
- Needs an exception overlay graph across the whole call graph
- High complexity, moderate CC accuracy gain
- Effort: ~2–3 weeks

---

## Raw Ideas (Unprioritized)

- Swift support (mobile)
- PHP support
- Dead code detection (CFG nodes unreachable from entry — infrastructure already exists)
