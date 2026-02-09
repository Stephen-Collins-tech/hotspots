# Synthetic Harness Research - Current State Analysis

## Executive Summary

This document analyzes Hotspots's current architecture to determine exactly what's needed to build a synthetic codebase harness that exercises the snapshot and delta system without requiring any changes to Hotspots itself.

**Key Finding:** The harness can be built entirely as external tooling that creates git repos and runs Hotspots CLI commands. No code changes needed.

---

## 1. Current State Constraints

### What Hotspots Already Does

**Snapshot System (`hotspots-core/src/snapshot.rs`):**
- Creates immutable snapshots from `GitContext` + `Vec<FunctionRiskReport>`
- Persists to `.hotspots/snapshots/<commit_sha>.json`
- Updates `.hotspots/index.json` atomically
- Function ID format: `<relative_file_path>::<symbol>` (e.g., `src/module_01.ts::f_01`)
- Deterministic serialization (byte-for-byte identical)
- Never overwrites existing snapshots (idempotent if identical)

**Delta System (`hotspots-core/src/delta.rs`):**
- Computes parent-relative deltas using `parents[0]` only
- Matches functions by `function_id` (file moves = delete + add)
- Handles baseline case (no parent = all functions marked `new`)
- Computes numeric deltas (cc, nd, fo, ns, lrs) and band transitions
- Status: `New`, `Deleted`, `Modified`, `Unchanged`

**Git Integration (`hotspots-core/src/git.rs`):**
- Extracts git context: SHA, parents, timestamp, branch
- Uses `git_at(repo_path, args)` for path-aware git commands
- Handles detached HEAD, shallow clones, multiple parents
- Thread-safe for parallel execution

**CLI (`hotspots-cli/src/main.rs`):**
- `hotspots analyze <path> --mode snapshot --format json` - Creates and persists snapshot
- `hotspots analyze <path> --mode delta --format json` - Computes and emits delta
- Automatically finds repo root (searches up directory tree)
- Mainline mode: persists snapshots
- PR mode: computes delta vs merge-base, doesn't persist

**Analysis (`hotspots-core/src/lib.rs`):**
- `analyze(path, options)` - Analyzes TypeScript files, returns `Vec<FunctionRiskReport>`
- Collects `.ts` files recursively (excludes `.d.ts`)
- Deterministic file ordering
- Per-function metrics: CC, ND, FO, NS, LRS, band

### What We're NOT Building

- New metrics
- Semantic analysis
- Function identity tracking across renames
- Intent annotations
- Visualization tools

---

## 2. Minimal Synthetic Harness Architecture

### Repo Structure (v0)

```
synthetic-repo/
  .gitignore          # Must include .hotspots/
  package.json        # Optional, for TypeScript config
  tsconfig.json       # Optional, for TypeScript config
  src/
    module_01.ts      # 10-20 baseline functions
    module_02.ts      # 10-20 baseline functions
    module_03.ts      # 10-20 baseline functions
```

### Baseline Function Template

```typescript
export function f_01(x: number): number {
  return x + 1;
}
```

**Expected Metrics:**
- CC = 1
- ND = 0
- FO = 0
- NS = 0
- LRS ≈ 1.0 (Low band)

### Git Setup Requirements

From `git_history_tests.rs`, we need:

1. **Initialize git repo:**
   ```bash
   git init
   git config user.name "Test User"
   git config user.email "test@example.com"
   ```

2. **Create `.gitignore`:**
   ```
   .hotspots/
   ```

3. **Commit pattern:**
   ```bash
   git add .
   git commit -m "message"
   git rev-parse HEAD  # Get SHA
   ```

---

## 3. Mutation Patterns (One Per Commit)

### Pattern 1: Add CC (Branching)

**Target:** `src/module_02.ts::f_07` (hotspot function)

**Change:**
```typescript
// Before
export function f_07(x: number): number {
  return x + 1;
}

// After
export function f_07(x: number): number {
  if (x > 10) {
    return x * 2;
  }
  return x + 1;
}
```

**Expected:**
- CC: 1 → 2 (+1)
- ND: 0 → 0 (unchanged)
- FO: 0 → 0 (unchanged)
- NS: 0 → 0 (unchanged)
- LRS: increases (R_cc increases from log2(2) to log2(3))
- Delta status: `Modified`
- Delta: `{cc: 1, nd: 0, fo: 0, ns: 0, lrs: +delta}`

### Pattern 2: Add ND (Nesting)

**Target:** `src/module_02.ts::f_07`

**Change:**
```typescript
// Before
export function f_07(x: number): number {
  if (x > 10) {
    return x * 2;
  }
  return x + 1;
}

// After
export function f_07(x: number): number {
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}
```

**Expected:**
- CC: 2 → 2 (unchanged)
- ND: 0 → 1 (+1)
- FO: 0 → 0 (unchanged)
- NS: 0 → 0 (unchanged)
- LRS: increases (R_nd increases from 0 to 1, weight 0.8)
- Delta status: `Modified`
- Delta: `{cc: 0, nd: 1, fo: 0, ns: 0, lrs: +0.8}`

### Pattern 3: Add FO (Fan-Out)

**Target:** `src/module_02.ts::f_07`

**Change:**
```typescript
// Before
export function f_07(x: number): number {
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}

// After
export function f_07(x: number): number {
  helperA();
  helperB();
  helperC();
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}

// Add helper functions to same file
function helperA(): void { }
function helperB(): void { }
function helperC(): void { }
```

**Expected:**
- CC: 2 → 2 (unchanged)
- ND: 1 → 1 (unchanged)
- FO: 0 → 3 (+3)
- NS: 0 → 0 (unchanged)
- LRS: increases (R_fo increases from 0 to log2(4) = 2.0, weight 0.6, so +1.2)
- Delta status: `Modified`
- Delta: `{cc: 0, nd: 0, fo: 3, ns: 0, lrs: +1.2}`

### Pattern 4: Add NS (Non-Structured Exits)

**Target:** `src/module_02.ts::f_07`

**Change:**
```typescript
// Before
export function f_07(x: number): number {
  helperA();
  helperB();
  helperC();
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}

// After
export function f_07(x: number): number {
  if (x < 0) return 0;  // Early return
  if (x === 0) throw new Error("zero");  // Throw
  helperA();
  helperB();
  helperC();
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}
```

**Expected:**
- CC: 2 → 4 (+2, from two new conditionals)
- ND: 1 → 1 (unchanged, but could decrease if we refactor)
- FO: 3 → 3 (unchanged)
- NS: 0 → 2 (+2, early return + throw)
- LRS: increases (R_cc increases, R_ns increases)
- Delta status: `Modified`
- Delta: `{cc: 2, nd: 0, fo: 0, ns: 2, lrs: +delta}`

### Pattern 5: Refactor (Negative Delta)

**Target:** `src/module_02.ts::f_07`

**Change:**
```typescript
// Before (deeply nested)
export function f_07(x: number): number {
  if (x < 0) return 0;
  if (x === 0) throw new Error("zero");
  helperA();
  helperB();
  helperC();
  if (x > 0) {
    if (x > 10) {
      return x * 2;
    }
  }
  return x + 1;
}

// After (guard clauses, reduced nesting)
export function f_07(x: number): number {
  if (x < 0) return 0;
  if (x === 0) throw new Error("zero");
  if (x <= 10) return x + 1;
  
  helperA();
  helperB();
  helperC();
  return x * 2;
}
```

**Expected:**
- CC: 4 → 4 (unchanged, same decision points)
- ND: 1 → 0 (-1, removed nested if)
- FO: 3 → 3 (unchanged)
- NS: 2 → 2 (unchanged, still has early returns)
- LRS: decreases (R_nd decreases from 1 to 0, weight 0.8, so -0.8)
- Delta status: `Modified`
- Delta: `{cc: 0, nd: -1, fo: 0, ns: 0, lrs: -0.8}`
- **Band transition possible:** If LRS crosses threshold

---

## 4. Artifacts the Harness Must Emit

### A. Git History

Real git commits with:
- Deterministic commit messages
- Sequential commits (no merges initially)
- Real SHAs (not fixed, but predictable from content)

### B. Hotspots Outputs

**Snapshots:**
```
.hotspots/
  snapshots/
    <sha_v0>.json
    <sha_v1>.json
    <sha_v2>.json
    ...
    <sha_v12>.json
  index.json
```

**Deltas (computed, not persisted):**
```
out/
  v1.delta.json  # Computed from v0 → v1
  v2.delta.json  # Computed from v1 → v2
  ...
  v12.delta.json # Computed from v11 → v12
```

### C. Manifest File

```json
{
  "repo_name": "synthetic-repo-v0",
  "hotspot": "src/module_02.ts::f_07",
  "baseline_commit": "<sha_v0>",
  "commits": [
    {
      "sha": "<sha_v1>",
      "message": "add CC to hotspot",
      "mutation": "add_cc",
      "target": "src/module_02.ts::f_07"
    },
    {
      "sha": "<sha_v2>",
      "message": "add CC to hotspot",
      "mutation": "add_cc",
      "target": "src/module_02.ts::f_07"
    },
    {
      "sha": "<sha_v3>",
      "message": "add ND to hotspot",
      "mutation": "add_nd",
      "target": "src/module_02.ts::f_07"
    }
  ],
  "expected_transitions": {
    "v4": "Moderate",  # When LRS crosses 3.0
    "v9": "High"       # When LRS crosses 6.0
  }
}
```

**Purpose:** For validation, not used by Hotspots.

---

## 5. How to Run Hotspots on Synthetic Repo

**Note:** Manual iteration is preferred for Step 1. This forces explicit inspection of each snapshot/delta and catches surprises early.

### Step-by-Step Process (Manual)

**For each commit:**

1. **Checkout commit:**
   ```bash
   cd synthetic-repo
   git checkout <sha>
   ```

2. **Create snapshot:**
   ```bash
   hotspots analyze src/ --mode snapshot --format json > out/v<num>.snapshot.json
   ```
   - This persists to `.hotspots/snapshots/<sha>.json`
   - Updates `.hotspots/index.json`
   - **Inspect the snapshot** to verify metrics are as expected

3. **Compute delta:**
   ```bash
   hotspots analyze src/ --mode delta --format json > out/v<num>.delta.json
   ```
   - This loads parent snapshot from `.hotspots/snapshots/<parent_sha>.json`
   - Computes delta and emits JSON
   - **Inspect the delta** to verify changes match expectations

**Why manual?** For 8-10 commits, manual iteration is trivial and forces validation at each step. Automation can come later once signal is proven.

### Automation Script (Future)

```bash
#!/bin/bash
# generate_synthetic.sh

REPO_DIR="synthetic-repo"
OUT_DIR="out"

mkdir -p "$OUT_DIR"

# Get all commit SHAs in order
COMMITS=($(git -C "$REPO_DIR" log --reverse --format="%H"))

for i in "${!COMMITS[@]}"; do
  SHA="${COMMITS[$i]}"
  VERSION="v$i"
  
  # Checkout commit
  git -C "$REPO_DIR" checkout "$SHA"
  
  # Create snapshot
  hotspots analyze "$REPO_DIR/src" --mode snapshot --format json > "$OUT_DIR/$VERSION.snapshot.json"
  
  # Compute delta
  hotspots analyze "$REPO_DIR/src" --mode delta --format json > "$OUT_DIR/$VERSION.delta.json"
done
```

---

## 6. Validation Checks

### Metric Isolation

**Check:** CC-only commits should only change CC
```bash
# Parse v1.delta.json
# Find hotspot function delta
# Assert: delta.cc != 0, delta.nd == 0, delta.fo == 0, delta.ns == 0
```

### Locality

**Check:** One function degrades, others stay flat
```bash
# Parse v1.snapshot.json and v0.snapshot.json
# Compare all functions
# Assert: Only hotspot function changed
```

### Trend Semantics

**Check:** LRS increases monotonically (unless refactored)
```bash
# Parse all snapshots
# Extract hotspot function LRS for each commit
# Assert: LRS[v0] <= LRS[v1] <= ... <= LRS[v8] (before refactor)
# Assert: LRS[v8] > LRS[v9] (after refactor)
```

### Band Correctness

**Check:** Band transitions occur at expected LRS thresholds
```bash
# Parse all snapshots
# Extract hotspot function band for each commit
# Assert: band transitions match expected_transitions in manifest
# Assert: Low → Moderate at LRS >= 3.0
# Assert: Moderate → High at LRS >= 6.0
# Assert: High → Critical at LRS >= 9.0
```

### Delta Correctness

**Check:** Deltas match snapshot differences
```bash
# For each commit i:
#   Load v<i>.snapshot.json
#   Load v<i-1>.snapshot.json
#   Manually compute expected delta
#   Load v<i>.delta.json
#   Assert: computed delta matches emitted delta
```

---

## 7. Implementation Approach

### Phase 1: Manual Creation (Recommended First Step)

1. Create `synthetic-repo/` directory
2. Initialize git repo
3. Create baseline TypeScript files (3 modules, 10-20 functions each)
4. Commit baseline (v0)
5. Apply mutations one-by-one, committing after each
6. Run Hotspots on each commit manually
7. Validate outputs

**Goal:** Prove the concept works end-to-end.

### Phase 2: Scripted Generation

1. Create Rust/Python script to:
   - Generate baseline TypeScript files
   - Apply mutations programmatically
   - Run git commands
   - Run Hotspots CLI
   - Validate outputs

**Goal:** Automate the process for regression testing.

### Phase 3: Test Integration

1. Add to `hotspots-core/tests/`:
   - `synthetic_harness_tests.rs`
   - Uses `create_temp_git_repo()` pattern from `git_history_tests.rs`
   - Generates synthetic repo in temp directory
   - Runs Hotspots on each commit
   - Validates invariants

**Goal:** CI integration for continuous validation.

---

## 8. What to Defer

**Do NOT build yet:**
- Randomized generation
- Large repo scaling (1000+ functions)
- Branch and merge simulations
- DSLs for mutation specification
- Config-driven mutation engines
- Visualization tooling
- Multi-hotspot scenarios

**Why:** Focus on clarity first. Prove the signal is correct before scaling.

---

## 9. Key Implementation Details

### Function ID Stability

**Critical:** Function IDs must be stable across commits for delta matching.

**Format:** `<relative_file_path>::<symbol>`

**Example:**
- `src/module_02.ts::f_07` - Stable if function name doesn't change
- If function is renamed: `f_07` → `f_07_refactored`, this is treated as delete + add

**Implication:** Synthetic harness should keep function names stable unless testing rename scenarios.

### Snapshot Persistence

**Location:** `.hotspots/snapshots/<sha>.json`

**Atomic writes:** Uses temp file + rename pattern (from `snapshot::atomic_write`)

**Idempotency:** If snapshot already exists and is identical, operation succeeds silently.

**Implication:** Harness can safely re-run Hotspots on same commit.

### Delta Computation

**Parent selection:** Uses `parents[0]` only (first parent)

**Baseline handling:** If parent snapshot doesn't exist, all functions marked `new`, `baseline: true`

**Implication:** Harness must ensure parent snapshots exist before computing deltas (or test baseline case explicitly).

### Git Context Extraction

**Uses:** `git::extract_git_context_at(repo_path)` for thread-safe, path-aware extraction

**Extracts:**
- `head_sha` - Current commit SHA
- `parent_shas` - All parent SHAs (for merge commits)
- `timestamp` - Commit timestamp
- `branch` - Branch name (None for detached HEAD)

**Implication:** Harness can run in parallel test environments safely.

---

## 10. Next Steps

### Immediate (Manual)

1. Create `synthetic-repo/` directory structure
2. Write baseline TypeScript files (3 modules, 10-20 functions each)
3. Initialize git repo
4. Commit baseline
5. Apply 6-8 mutations (one per commit)
6. Run Hotspots on each commit
7. Validate outputs manually

### Short-term (Scripted)

1. Write script to generate baseline files
2. Write script to apply mutations
3. Write script to run Hotspots and collect outputs
4. Write validation script

### Long-term (Test Integration)

1. Add `synthetic_harness_tests.rs`
2. Integrate into CI
3. Use for regression testing

---

## 11. Example Commit Sequence

```
v0: Baseline
  - 3 modules, 15 functions each
  - All functions: CC=1, ND=0, FO=0, NS=0, LRS≈1.0

v1: Add CC to hotspot (f_07)
  - CC: 1 → 2
  - LRS: 1.0 → ~2.0

v2: Add CC to hotspot (f_07)
  - CC: 2 → 3
  - LRS: ~2.0 → ~2.58

v3: Add ND to hotspot (f_07)
  - ND: 0 → 1
  - LRS: ~2.58 → ~3.38 (crosses Moderate threshold)

v4: Add FO to hotspot (f_07)
  - FO: 0 → 3
  - LRS: ~3.38 → ~4.58

v5: Add NS to hotspot (f_07)
  - NS: 0 → 2
  - LRS: ~4.58 → ~5.98

v6: Add CC to non-hotspot (f_01)
  - f_01: CC: 1 → 2
  - f_07: unchanged
  - Repo median increases slightly

v7: Add ND to non-hotspot (f_01)
  - f_01: ND: 0 → 1
  - f_07: unchanged

v8: Refactor hotspot (f_07)
  - ND: 1 → 0 (reduced nesting)
  - LRS: ~5.98 → ~5.18 (decreases)

v9: Add CC to hotspot (f_07)
  - CC: 3 → 4
  - LRS: ~5.18 → ~6.18 (crosses High threshold)

v10: Add FO to hotspot (f_07)
  - FO: 3 → 6
  - LRS: ~6.18 → ~7.38

v11: Noop commit (whitespace only)
  - All metrics unchanged
  - Delta: all functions `Unchanged`

v12: Refactor repo-wide (extract helpers)
  - Multiple functions change
  - Some LRS decreases, some increases
```

---

## 12. Validation Invariants

### Snapshot Invariants

1. **Immutability:** Snapshot file never changes after creation
2. **Determinism:** Identical code produces identical snapshot JSON
3. **Function ID format:** All function_ids match `<path>::<symbol>` pattern
4. **Sorting:** Functions sorted by function_id (ASCII lexical)

### Delta Invariants

1. **Baseline:** First commit has `baseline: true`, all functions `New`
2. **Parent matching:** Delta uses `parents[0]` only
3. **Function matching:** Functions matched by `function_id`, not file/line
4. **Status correctness:** Status reflects actual changes (New/Deleted/Modified/Unchanged)
5. **Delta math:** Numeric deltas match `after - before`
6. **Band transitions:** Band transitions only when LRS crosses thresholds

### Trend Invariants

1. **Monotonicity:** LRS increases unless refactored (negative delta)
2. **Locality:** Changes to one function don't affect others
3. **Isolation:** Metric changes are isolated (CC-only commits only change CC)

---

## Conclusion

The synthetic harness can be built entirely as external tooling that:
1. Creates git repos with TypeScript files
2. Applies mutations via git commits
3. Runs Hotspots CLI commands
4. Validates outputs

**No changes to Hotspots core are required.** The harness exercises the existing snapshot and delta system through the public CLI interface.

The recommended approach is to start manually, then script it, then integrate into tests.
