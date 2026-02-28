# Code Pattern Reference

This document is the single source of truth for the code patterns that Hotspots detects. Each entry defines what the pattern means, which signals trigger it, what the industry calls it, and why it matters for maintenance risk.

Patterns are **informational labels** — they do not affect the LRS score or risk band. A function can carry multiple patterns simultaneously.

---

## How patterns work

Patterns are derived from the same signals Hotspots already measures: cyclomatic complexity (CC), nesting depth (ND), fan-out (FO), non-structured exits (NS), lines of code (LOC), and — in snapshot mode — call graph data (fan-in, SCC membership, dependency depth) and git data (churn, touch frequency, recency).

Two tiers exist based on what data is available:

| Tier | Available in | Requires |
|---|---|---|
| **Tier 1 — Structural** | All modes | CC, ND, FO, NS, LOC only |
| **Tier 2 — Enriched** | Snapshot mode only | Call graph + git history |

Tier 2 patterns are a superset: a function can carry both structural and enriched patterns at once.

Some Tier 2 patterns are **derived** — they are pure conjunctions of other named patterns and reuse those patterns' thresholds directly rather than defining independent thresholds. This is noted in each entry. All other patterns are **primitive**.

---

## Metric definitions

All thresholds reference the following metrics. Definitions are locked here to ensure consistent implementation and testing.

**`LOC`** — physical lines of code for the function body, excluding blank lines and comment-only lines.

**`CC`** — cyclomatic complexity (McCabe 1976): number of linearly independent paths, equivalent to the number of binary decision points + 1.

**`ND`** — maximum nesting depth within the function body: the deepest level of nested control structures (if/else, loops, try/catch, match arms, closures).

**`FO`** — fan-out: number of distinct functions called directly within the function body, excluding calls to the standard library or external dependencies unless they cross a module boundary.

**`NS`** — non-structured exits: count of early returns, thrown exceptions, and multi-level breaks or continues within the function body.

**`fan_in`** — number of distinct functions in the repo that contain a direct call to this function. Computed from the snapshot call graph. External callers (outside the repo) are not counted.

**`scc_size`** — size of the strongly connected component containing this function in the call graph. A value of 1 means the function is not in a cycle. Requires snapshot mode.

**`churn_lines`** — cumulative sum of lines added plus lines deleted touching this function across all commits in the git history, or within a configurable window (default: lifetime). Renames are followed. Merge commits are excluded. Whitespace-only changes are excluded by default.

**`touch_frequency`** — number of distinct commits touching this function. Default window: lifetime. A configurable rolling window (e.g. last 90 days) is a planned extension.

**`days_since_last_change`** — calendar days between today and the most recent commit that modified at least one non-whitespace line in this function body.

**`neighbor_churn`** — sum of `churn_lines` for all direct callees (outgoing call graph edges, 1-hop only). Excludes stdlib and external dependencies. If a callee has no git history entry (e.g. it was added and never changed), it contributes 0.

---

## Default thresholds and configuration

All thresholds listed in this document are **defaults**. They are deliberately conservative: it is better to surface a genuine pattern and have a developer dismiss it than to miss a real structural problem.

Thresholds can be tuned per project via the `patterns` section of `.hotspotsrc.json` (planned — not yet implemented). When implemented, each threshold will support two forms:

- **Absolute**: `LOC >= 80` — fixed value, language- and repo-size-agnostic
- **Percentile**: `LOC >= p95` — computed within a configurable scope (repo, language, module) — planned extension

The absolute form is used for all defaults today. Percentile-based thresholds are a planned extension and will not require a schema change when added.

---

## Exclusions and guards

The following exclusions apply by default to reduce noise. They can be overridden in `.hotspotsrc.json` (planned):

- **Generated files** — files matching common generated-code patterns (e.g. `*.pb.go`, `*_generated.*`, files containing a `// Code generated` header) are excluded from pattern detection.
- **Vendored directories** — `vendor/`, `node_modules/`, and similar third-party trees are excluded.
- **Test files** — files matching language-standard test patterns (e.g. `*_test.go`, `*.spec.ts`, `test_*.py`) are excluded. This avoids false positives on test helpers and fixtures.
- **Path globs** — arbitrary glob patterns can be added to `.hotspotsrc.json` to exclude generated, legacy, or scaffolding code.

Note: `middle_man` and `neighbor_risk` are particularly sensitive to entrypoint functions (web handlers, CLI commands, dispatchers) that are expected to have high fan-in and fan-out by design. Until explicit entrypoint exclusion is implemented, these patterns may fire on legitimate dispatch code and should be reviewed with that context in mind.

---

## Output contract

Patterns appear in all output modes under a stable schema:

```
patterns: string[]
```

An optional extended form is available with `--explain-patterns` (planned):

```
pattern_details?: {
  id: string,
  tier: 1 | 2,
  kind: "primitive" | "derived",
  triggered_by: { metric: string, op: string, value: number, threshold: number }[]
}[]
```

In tabular output, patterns appear as a comma-separated list in a `patterns` column. Ordering within the list is deterministic: Tier 1 patterns before Tier 2, then alphabetical within each tier.

---

## Tier 1 — Structural Patterns

These fire from static metrics alone and are available in every output mode.

---

### `complex_branching`

**Kind:** Primitive
**Industry names:** Complex Method, Arrow Anti-Pattern, Spaghetti Logic
**Source:** McCabe cyclomatic complexity (1976); Fowler *Refactoring* (long conditional chains)

**Signals:** High CC **and** high ND

**Definition:**
High cyclomatic complexity (many independent paths) compounded by deep nesting (those paths are hard to visually parse). Either dimension alone is tolerable; together they produce functions that are genuinely difficult to reason about because the branching structure must be held in working memory while navigating multiple nesting levels.

**Why it matters:**
CC directly predicts defect density and test case count. Deep nesting amplifies this by making the logic structure opaque. Functions with both properties consistently show higher bug rates in empirical studies.

**Default thresholds:** CC ≥ 10 **and** ND ≥ 4

---

### `deeply_nested`

**Kind:** Primitive
**Industry names:** Arrow Anti-Pattern, Pyramid of Doom
**Source:** Common style guidance across most languages; prominent in JavaScript community as "callback hell"

**Signals:** High ND, regardless of CC

**Definition:**
Excessive nesting depth even in the absence of high branch count. This typically appears as guard clauses that were never inverted, nested callbacks, or deeply nested loops. Readability degrades non-linearly with depth: at ND ≥ 5, the outermost context is genuinely difficult to hold in mind while reading inner code.

**Why it matters:**
Beyond readability, deep nesting is a sign that early-exit refactoring opportunities have been missed. It correlates with higher defect rates independently of CC because the number of implicit assumptions grows with depth.

**Default thresholds:** ND ≥ 5

---

### `exit_heavy`

**Kind:** Primitive
**Industry names:** Multiple Return Points (contested — guard-clause style rehabilitates small counts)
**Source:** Structured programming tradition

**Signals:** High NS

**Definition:**
A function with many non-structured exit points: early returns, thrown exceptions, and break or continue statements that exit multiple levels. A small number of guard clauses at the top of a function is good style and will not trigger this pattern. The concern is exits *scattered throughout* the body, which make it hard to reason about the complete set of outcomes and to instrument uniformly (e.g. adding logging on all exit paths).

What NS counts is language-specific. See the NS metric definition above. If your language's NS metric has additional detail (e.g. whether `break` inside a `match` arm counts), that definition takes precedence here.

**Why it matters:**
Each exit point is a place where a caller's assumption about post-conditions can be violated. Many scattered exits also increase the number of paths that must be independently tested, compounding the effect of high CC.

**Default thresholds:** NS ≥ 5

---

### `god_function`

**Kind:** Primitive
**Industry names:** God Method, Brain Method, Long Method
**Source:** Fowler *Refactoring* (Long Method); Tornhill *Software Design X-Rays* (Brain Method)

**Signals:** High LOC **and** high FO

**Definition:**
A function that does too much *and* orchestrates too many other concerns. The size (LOC) indicates a Single Responsibility violation; the fan-out (FO) shows it is reaching into many other modules to do so. The combination is worse than either alone: a large function that is also deeply entangled is expensive to read, test, and change. This pattern often co-occurs with Feature Envy (Fowler), where a function accesses another module's data more than its own, but the two are not equivalent — `god_function` is defined by size and breadth of coupling, not by data access patterns.

**Why it matters:**
God functions are the most common root cause of defect clusters. They are hard to unit test (too many paths, too many dependencies), hard to review (too much context), and attract further additions because "it already does everything."

**Default thresholds:** LOC ≥ 60 **and** FO ≥ 10

---

### `long_function`

**Kind:** Primitive
**Industry names:** Long Method
**Source:** Fowler *Refactoring* — one of the original code smells

**Signals:** High LOC alone

**Definition:**
A physically large function, irrespective of branching or coupling. Length alone is a proxy for Single Responsibility violations: functions that are hard to name concisely are usually doing more than one thing. LOC also directly predicts review time and the likelihood that a reviewer misses a subtle bug.

Note: `long_function` will frequently co-occur with `god_function`. `god_function` is the stronger signal when both fire, but `long_function` alone is still meaningful — a large function with low fan-out may be a sequential pipeline that is easy to follow but should still be decomposed.

**Why it matters:**
Short functions are easier to name, test, review, and reuse. Function length is one of the strongest simple predictors of bug probability per function in empirical research.

**Default thresholds:** LOC ≥ 80

---

## Tier 2 — Enriched Patterns

These require snapshot mode (`--mode snapshot`) because they combine structural metrics with call graph topology and git history.

---

### `churn_magnet`

**Kind:** Primitive
**Industry names:** Hotspot (Tornhill), Change-Prone Complex Method
**Source:** Tornhill *Your Code as a Crime Scene* — the core thesis: complexity × churn = maintenance risk

**Signals:** High churn_lines **and** high CC

**Definition:**
The canonical hotspot: a function that is both structurally complex and frequently changed. Complexity means each change is expensive and error-prone; churn means those expensive changes happen often. This is the primary signal for maintenance debt accumulation.

**Why it matters:**
Tornhill's empirical research across many large codebases shows that the intersection of high complexity and high churn predicts defect density far better than either dimension alone. A complex function that never changes is legacy, but manageable. A complex function that changes constantly is actively generating defects.

**Default thresholds:** churn_lines ≥ 200 **and** CC ≥ 8

---

### `cyclic_hub`

**Kind:** Primitive
**Industry names:** Inappropriate Intimacy, Cyclic Dependency, Circular Coupling
**Source:** Fowler *Refactoring* (Inappropriate Intimacy); Martin — Acyclic Dependencies Principle

**Signals:** scc_size > 1 **and** high fan_in

**Definition:**
A function caught in a cyclic dependency (its SCC contains more than one node) that is also heavily depended upon from outside the cycle. This is the hardest structural problem to refactor: breaking the cycle requires decoupling functions that mutually depend on each other, and the high fan-in means many external callers are also affected by any interface change.

**Why it matters:**
Cyclic dependencies prevent independent deployment, make testing harder (circular mocking), and are the primary obstacle to modularisation. When a cyclic function is also a hub, breaking it requires coordinated changes across multiple files. Detecting it early gives teams a chance to break cycles before the fan-in grows further.

**Default thresholds:** scc_size ≥ 2 **and** fan_in ≥ 6

---

### `hub_function`

**Kind:** Primitive
**Industry names:** Knowledge Concentration, Central Module
**Source:** Tornhill *Your Code as a Crime Scene* (knowledge concentration); object-oriented coupling literature (hub coupling)

**Signals:** High fan_in **and** high CC

**Definition:**
A function that many callers depend on and that is itself structurally complex. Fan-in measures how much of the codebase relies on this function; CC measures how hard it is to understand and change. The combination creates a structural bottleneck: any bug here has wide blast radius, and changing it requires understanding complex logic under pressure from many dependent callers.

**Why it matters:**
Hub functions with high complexity are the highest-leverage refactoring targets in a codebase. Simplifying them reduces risk for all their callers simultaneously. They are also the most likely locus of defensive code that accumulates over time as callers make conflicting demands.

**Default thresholds:** fan_in ≥ 10 **and** CC ≥ 8

---

### `middle_man`

**Kind:** Primitive
**Industry names:** Middle Man, Dispatcher Anti-Pattern, God Router
**Source:** Fowler *Refactoring* (Middle Man)

**Signals:** High fan_in **and** high FO **and** low CC

**Definition:**
A function that many things call, which itself calls many other things, but contains little logic of its own. It is a routing layer that has grown beyond its original purpose. Low CC distinguishes this from `hub_function`: a middle man is structurally simple inside but topologically central. It may be acceptable as a legitimate dispatcher, but at scale it represents unnecessary coupling — all callers are coupled to a routing function they do not actually need.

See the note in [Exclusions and guards](#exclusions-and-guards) about entrypoint functions, which can trigger this pattern legitimately.

**Why it matters:**
Middle men add indirection without abstraction. They make the call graph harder to navigate, increase coupling, and become a maintenance problem when routing logic starts to grow. Large middle men are a sign that the dependency inversion principle has been bypassed.

**Default thresholds:** fan_in ≥ 8 **and** FO ≥ 8 **and** CC ≤ 4

---

### `neighbor_risk`

**Kind:** Primitive
**Industry names:** Instability by Proximity, Dependency Contamination
**Source:** New pattern — not in standard literature. Justified by Hotspots' unique ability to measure churn in the call graph neighbourhood, a signal no traditional static analysis tool provides.

**Signals:** High neighbor_churn **and** high FO

**Definition:**
A function whose own code is stable, but whose dependencies are churning heavily. Even if this function never changes, it is at risk from the instability of the code it calls. A function with many callees, most of which are actively being changed, is one dependency update away from needing to adapt.

See the note in [Exclusions and guards](#exclusions-and-guards) about entrypoint-style functions that may have high FO by design.

**Why it matters:**
Standard hotspot analysis focuses on a function's own churn. Hotspots' call graph data allows detection of *indirect* instability — functions that are stable islands in unstable neighbourhoods. These are the functions most likely to appear stable in reviews but break unexpectedly after a sprint of changes in adjacent code. Flagging them allows teams to add integration tests before the instability propagates.

**Default thresholds:** neighbor_churn ≥ 400 **and** FO ≥ 8

---

### `shotgun_target`

**Kind:** Primitive
**Industry names:** Shotgun Surgery target, High-Impact Churn, Ripple Effect Source
**Source:** Fowler *Refactoring* — Shotgun Surgery describes code where a single change requires many scattered edits; the *target* is the function receiving those changes under pressure from many callers

**Signals:** High fan_in **and** high churn_lines

**Definition:**
A function that is both heavily depended upon and frequently changed. Each change here has the potential to break any of its callers, and the frequency of change means risk materialises regularly. This is distinct from `hub_function` (which focuses on complexity as the amplifier) and `churn_magnet` (which focuses on CC × churn): here the risk is breadth of impact × change frequency, regardless of the function's internal complexity.

**Why it matters:**
High fan-in with high churn is the signature of a function that has been modified without its interface being stabilised. It suggests the abstraction boundary is wrong — callers are depending on implementation details that keep shifting. Stabilising the interface (or breaking the function apart) is the primary remediation.

**Default thresholds:** fan_in ≥ 8 **and** churn_lines ≥ 150

---

### `stale_complex`

**Kind:** Primitive
**Industry names:** Untouchable Code, Fear Code, Frozen Complexity
**Source:** Tornhill *Software Design X-Rays* — the "fear" metric: complex code that nobody dares change

**Signals:** High CC **and** high LOC **and** very low touch_frequency (days_since_last_change above threshold)

**Definition:**
A complex, large function that has not been changed in an extended period — not because it is stable and well-understood, but because it is complex enough to be feared. This is the *inverse* of `churn_magnet`: rather than accumulating churn, it accumulates entropy through avoidance. Teams work around it, adding adapter layers rather than modifying the core logic.

"Not changed" means no commit has modified at least one non-whitespace line in the function body within the threshold window. Renames are followed. Formatting-only commits (whitespace-only diffs) do not reset the clock.

**Why it matters:**
Stale complex functions are a hidden liability. They appear stable in metrics but represent a significant risk when they *do* need to change — under urgency, with no recent institutional knowledge. Identifying them proactively allows teams to schedule incremental familiarisation and decomposition before a crisis forces a rushed change.

**Default thresholds:** CC ≥ 10 **and** LOC ≥ 60 **and** days_since_last_change ≥ 180

---

### `volatile_god`

**Kind:** Derived from `god_function` && `churn_magnet`
**Industry names:** Volatile God Method, Churning Monolith
**Source:** Composite of Fowler (God Method) + Tornhill (hotspot). Not separately named in the literature but represents the intersection of both seminal concerns.

**Signals:** All conditions for `god_function` fire **and** churn_lines meets the `churn_magnet` threshold

**Definition:**
A god function that is also frequently changed: the worst-case combination of structural debt and maintenance pressure. The size and coupling of a god function make every change expensive; the frequency of change means those costs are incurred repeatedly. Because this pattern is derived, its thresholds are exactly those of the two primitives it combines — there are no independent thresholds to keep in sync.

**Why it matters:**
A god function that is rarely touched is legacy debt — expensive to pay down but not actively causing new problems. A god function that changes regularly is *active* debt: it is generating defects, slowing every sprint that touches it, and training developers to avoid it. `volatile_god` is the highest-priority pattern for targeted refactoring investment.

**Derived thresholds (inherited):** LOC ≥ 60 **and** FO ≥ 10 **and** churn_lines ≥ 200 **and** CC ≥ 8

---

## Pattern combinations and escalation

Some patterns frequently co-occur and together signal a higher-priority concern than either alone. `volatile_god` is already a named derived pattern covering the most common escalation; the combinations below are observed co-occurrences that are not yet named patterns but are worth noting in triage.

| Combination | Escalated meaning |
|---|---|
| `hub_function` + `cyclic_hub` | Architecture-level coupling problem; cannot be safely refactored at the function level alone — cycle must be broken first |
| `complex_branching` + `exit_heavy` | Control flow is non-linear in two compounding ways; test coverage is likely incomplete — prioritise coverage before any refactor |
| `middle_man` + `churn_magnet` | A routing layer that keeps absorbing logic — the routing function is growing into the thing it was meant to dispatch away from |
| `stale_complex` + `hub_function` | Feared bottleneck — high blast radius if it needs to change, and it hasn't been touched in a long time; schedule familiarisation proactively |
| `shotgun_target` + `churn_magnet` | Interface instability compounded by internal complexity — the function is changing a lot and breaking callers; interface stabilisation is urgent |

---

## Engineering requirements

This section describes what the pattern engine must implement. It is intended to drive the implementation plan directly.

### Pattern engine

- Input: a `FunctionMetrics` record (Tier 1 fields) plus optional `EnrichedMetrics` (Tier 2 fields; absent outside snapshot mode)
- Output: `Vec<PatternId>` with deterministic ordering (Tier 1 before Tier 2, then alphabetical within each tier)
- Derived patterns are computed by checking whether component primitive patterns fired, not by re-evaluating raw metric conditions — this prevents threshold drift between a primitive and its derived pattern
- The engine must be pure and stateless: same inputs always produce same outputs

### Threshold registry

- Central map: `PatternId -> ThresholdSet`
- All defaults embedded in code; config overrides layered on top
- Even if `.hotspotsrc.json` config is not yet implemented, the internal type should accommodate a `ThresholdOverride` layer so the config path is a thin addition, not a refactor

### Test matrix

For each primitive pattern:
- Just below threshold (must not fire)
- Exactly at threshold (must fire)
- Above threshold (must fire)
- Opposite end of any compound condition at threshold, other metric below (must not fire)

For derived patterns:
- Each component primitive below threshold (must not fire)
- All component primitives at threshold (must fire)

Golden tests:
- At least one synthetic function triggering 4–5 patterns simultaneously, with expected output verified
- Snapshot-only tests using a small synthetic call graph + git history fixture covering all Tier 2 patterns

### Raw vs normalised metrics

All thresholds in this document operate on **raw metrics** — no normalisation, no z-scores, no per-language adjustment. This is intentional for v1. Percentile-based thresholds are a planned extension and are explicitly accommodated in the threshold registry design (see above). Deciding to normalise later will require adding a normalisation layer, not changing the pattern definitions.

---

## Sources and further reading

- Martin Fowler — *Refactoring: Improving the Design of Existing Code* (1999, 2nd ed. 2018)
- Adam Tornhill — *Your Code as a Crime Scene* (2015)
- Adam Tornhill — *Software Design X-Rays* (2018)
- Thomas J. McCabe — "A Complexity Measure" (IEEE Transactions on Software Engineering, 1976)
- Robert C. Martin — *Clean Code* (2008) and the Acyclic Dependencies Principle
- Empirical studies on CC and defect density: Gill & Kemerer (1991), Basili et al. (1996)
