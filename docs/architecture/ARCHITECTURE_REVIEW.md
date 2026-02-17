# Architecture Review Findings

**Date:** 2026-02-16
**Reviewer:** Claude (claude-sonnet-4-5)
**Status:** Findings only — no plan yet
**Scope:** Review of `ARCHITECTURE.md` against actual codebase behavior

---

## Overview

This document records findings from a review of `ARCHITECTURE.md` against the actual codebase. It is
distinct from `IMPROVEMENTS.md` (which focuses on performance and extensibility). These findings are
primarily about **scoring accuracy**, **correctness**, and **documentation gaps** — places where the
architecture doc either understates limitations or omits known constraints.

---

## Findings

### F-1: Touch Metrics Are File-Level, Not Function-Level

**Severity:** Medium — affects scoring accuracy
**Location:** `hotspots-core/src/git.rs`, `snapshot.rs` enrichment pipeline

**Description:**
Touch count (commits in last 30 days) and days-since-last-change are computed at the **file** level
and then attributed uniformly to every function in that file. A file with 50 functions where only
one has been touched in the last month will report `touch_count_30d = N` for all 50 functions.

The architecture doc acknowledges this in a single line ("Computed at: File level") but does not
flag it as a limitation or note how it affects score interpretation.

**Impact:**
- `touch_factor` in `activity_risk` is a noisy signal for large files with many functions
- Functions in "hot" files are over-penalized relative to their actual activity
- Especially misleading for large utility files or files with mixed-stability functions

**What the doc should say:**
Explicitly call out that touch metrics are file-granularity approximations, and that files with many
functions will distribute the same touch count across all of them regardless of which function
actually changed.

---

### F-2: Fan-Out Is Double-Penalized in the Scoring Formula

**Severity:** Low-Medium — affects score calibration
**Location:** `hotspots-core/src/risk.rs`, `hotspots-core/src/scoring.rs`

**Description:**
Fan-out enters the score in two distinct ways:

1. `R_fo = min(log2(FO + 1), 6.0)` feeds into **LRS** (via `w_fo * R_fo`)
2. LRS feeds directly into `activity_risk` with weight 1.0
3. `fan_in_factor = min(fan_in / 5, 10.0)` also feeds into `activity_risk`

Fan-in and fan-out are different metrics, so (3) is not literally double-counting. However, because
high-fan-out functions tend to also have high fan-in (functions that call many things tend to be
called by many things), there is a systematic correlation that amplifies scores for highly-connected
functions beyond what either metric alone would suggest.

The architecture doc describes these as independent additive factors without noting the correlation.

**Impact:**
- Hub functions (high fan-in AND high fan-out) are disproportionately penalized
- Score interpretation is harder without knowing this interaction

**What the doc should say:**
Note that fan-in and fan-out are correlated in typical codebases and that connected hub functions
will tend to score higher than the formula implies in isolation.

---

### F-3: Name-Based Call Graph Resolution Has Significant Accuracy Limits

**Severity:** Medium — affects call graph metric accuracy
**Location:** `hotspots-core/src/callgraph.rs`

**Description:**
Call graph edges are built by matching callee names extracted from ASTs against known function names
using simple rules: prefer same-file match, fall back to first match globally. This approach cannot
resolve:

- **Method dispatch through interfaces/traits** — calling `trait.method()` where the concrete type
  is unknown
- **Higher-order functions** — functions passed as arguments and called indirectly
- **Closures** — anonymous functions called through variables
- **Dynamic dispatch** — virtual methods, function pointers, reflection

For Go/Java/Python specifically, where interface-based dispatch is idiomatic, a significant
fraction of call edges may be unresolved or incorrectly resolved.

The architecture doc says "Resolve calls — Match callee names to function IDs" and describes the
preference rules without noting these limitations.

**Impact:**
- PageRank, betweenness centrality, fan-in, and neighbor_churn are all derived from an incomplete
  call graph
- Functions that are primary callee targets via interfaces may appear to have lower fan-in than they
  actually do
- "Architectural hub" detection may miss actual hubs that are only reached via interfaces

**What the doc should say:**
Explicitly characterize the call graph as a "best-effort static approximation" that works well for
direct calls but systematically misses dynamic/interface dispatch. Quantify the expected coverage
where possible (e.g., "Go codebases using idiomatic interface patterns may have 20-40% unresolved
call edges").

---

### F-4: Tree-Sitter Re-Parse Per Function Is O(n × m)

**Severity:** Medium — main performance bottleneck for Go/Java/Python
**Location:** `hotspots-core/src/language/{go,java,python}/cfg_builder.rs`

**Description:**
The Go, Java, and Python CFG builders re-parse the full source file with tree-sitter for every
function in that file. A file with 30 functions is parsed 30 times. This is O(n × m) where n =
number of functions in the file and m = file size, rather than O(m) for parsing once and O(n) for
CFG construction.

The architecture doc explains _why_ the source is stored (tree-sitter node lifetimes) and notes
"re-parse when needed" as the design choice, but frames this as a rational trade-off rather than
a known bottleneck.

**Impact:**
- For large Go/Java/Python files (e.g. 1000+ line files with 30+ functions), CFG building
  dominates analysis time
- Contrast: ECMAScript and Rust parse once (owned ASTs), so the problem is language-specific
- The `IMPROVEMENTS.md` "Incremental Analysis" section would partly address this but does not
  identify it as the root cause

**What the doc should say:**
Flag the re-parse pattern as a known performance issue specific to tree-sitter languages, and note
that the fix (parse once, traverse AST to find the target function node) is straightforward but
requires careful handling of tree-sitter lifetimes.

---

### F-5: `function_id` Is Path-Dependent — Refactoring Commits Lose History

**Severity:** Medium — affects delta accuracy for refactoring
**Location:** `hotspots-core/src/delta.rs`, `hotspots-core/src/snapshot.rs`

**Description:**
`function_id` is `file_path::function_name`. Any of these operations reset the ID and cause a
delete+add in delta output:

- Renaming a file
- Moving a file to a different directory
- Renaming a function

This is documented for file moves ("Treated as delete + add") but not for function renames, and
neither case is framed as a limitation — just as a design choice.

**Impact:**
- Refactoring commits (which frequently rename/move things) produce delta noise: functions appear
  as deleted and re-added with no history
- Trend analysis and policy evaluation see new functions where old ones existed
- Teams that rename functions to improve clarity get penalized by policies that flag "New critical
  functions"
- The suppression system cannot easily suppress renames since the ID has changed

**What the doc should say:**
Explicitly document that `function_id` stability depends on stable file paths and function names.
Refactoring commits that rename or move functions will appear as delete+add pairs. Consider whether
a content-hash or signature-based ID would better serve the delta use case.

---

### F-6: Schema Migration Strategy Is Undefined

**Severity:** Low-Medium — operational risk
**Location:** `hotspots-core/src/snapshot.rs`

**Description:**
Snapshots carry a `schema_version: u32` field described as enabling "forward compatibility." The
architecture doc says: "Schema versioning — snapshots carry version numbers for forward
compatibility." But nowhere is the actual migration behavior defined:

- What happens when a new version of `hotspots` reads an old-schema snapshot?
- Is it silently ignored? Does it fail with an error? Is it automatically migrated?
- What happens to the index if it contains mixed schema versions?

**Impact:**
- Users who upgrade hotspots may find existing snapshots unreadable or silently dropped
- The index could reference snapshots that the new version cannot parse
- No migration guide exists

**What the doc should say:**
Document the schema migration policy explicitly: which schema versions are supported, what happens
on mismatch, and whether migration is automatic or manual.

---

### F-7: PR Context Detection Is CI-Only

**Severity:** Low — documentation gap
**Location:** `hotspots-core/src/git.rs` (`detect_pr_context`)

**Description:**
PR detection relies on CI environment variables (`GITHUB_BASE_REF`, `CI`, `PULL_REQUEST`, etc.).
Running `hotspots analyze --mode snapshot` locally on a PR branch will be detected as mainline and
will persist a snapshot for that commit, which may not be desired.

The architecture doc mentions "Detect PR context (best-effort, CI env vars only)" but the
"best-effort" qualifier is easy to overlook and the local-branch implication is not spelled out.

**Impact:**
- Local development on PR branches generates snapshots indexed as mainline
- Could pollute the snapshot history with PR commits if developer runs snapshot mode locally

**What the doc should say:**
Explicitly note that `--mode snapshot` run locally on any branch (including feature branches) will
persist a snapshot. PR detection only suppresses persistence in CI environments that set standard
PR environment variables.

---

### F-8: Trend Analysis Module Is Undocumented

**Severity:** Low — documentation gap
**Location:** `hotspots-core/src/trends.rs`, `hotspots-cli/src/main.rs` (`Commands::Trends`)

**Description:**
The `trends` subcommand (`hotspots trends`) is implemented and exposed in the CLI but does not
appear anywhere in `ARCHITECTURE.md`. The module computes risk velocity and hotspot stability from
snapshot history.

**Impact:**
- Users and contributors have no architecture-level documentation for this feature
- The scoring formula, window size semantics, and output format are undocumented architecturally

**What the doc should say:**
Add a section covering the trends system: what it computes, how it reads the snapshot index, what
"risk velocity" and "hotspot stability" mean, and how the window parameter works.

---

## Summary Table

| ID  | Finding                                  | Severity    | Type                    |
|-----|------------------------------------------|-------------|-------------------------|
| F-1 | Touch metrics are file-level             | Medium      | Scoring accuracy        |
| F-2 | Fan-out double-penalized                 | Low-Medium  | Score calibration       |
| F-3 | Name-based call graph accuracy limits    | Medium      | Call graph correctness  |
| F-4 | Tree-sitter re-parse is O(n×m)           | Medium      | Performance bottleneck  |
| F-5 | function_id is path-dependent            | Medium      | Delta accuracy          |
| F-6 | Schema migration strategy undefined      | Low-Medium  | Operational risk        |
| F-7 | PR detection is CI-only                  | Low         | Documentation gap       |
| F-8 | Trends module undocumented               | Low         | Documentation gap       |

---

**Status:** Findings documented. Plan to address not yet determined.
**See also:** [`IMPROVEMENTS.md`](./IMPROVEMENTS.md) for performance and extensibility proposals.
