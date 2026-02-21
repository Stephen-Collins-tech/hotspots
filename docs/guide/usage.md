# Usage & Workflows

## Basic Usage

### Point-in-Time Analysis

**Analyze a file or directory:**

```bash
# Text output (default)
hotspots analyze src/

# JSON output
hotspots analyze src/ --format json

# Analyze specific file
hotspots analyze src/api.ts
```

**Filter results:**

```bash
# Show only top 10 most complex functions
hotspots analyze src/ --top 10

# Show only functions with LRS >= 5.0
hotspots analyze src/ --min-lrs 5.0

# Combine filters
hotspots analyze src/ --top 10 --min-lrs 5.0 --format json
```

**Example output (text):**
```
LRS     File              Line  Function
11.2    src/api.ts        88    handleRequest
9.8     src/db/migrate.ts 41    runMigration
7.5     src/utils.ts      15    processData
```

---

## Language-Specific Examples

### ECMAScript (TypeScript/JavaScript/React)

```bash
# Analyze TypeScript files
hotspots analyze src/api.ts

# Analyze entire TypeScript project
hotspots analyze src/ --format json

# Analyze React components
hotspots analyze src/components/ --format json
```

**Supported files:** `.ts`, `.tsx`, `.js`, `.jsx`, `.mts`, `.cts`, `.mjs`, `.cjs`

### Go

```bash
# Analyze single Go file
hotspots analyze main.go

# Analyze Go package
hotspots analyze pkg/handlers/

# Analyze entire Go project
hotspots analyze . --format json
```

**Go-specific metrics:**
- **Defer statements** count as Non-Structured Exits (NS)
- **Goroutines** (`go` statements) count as Fan-Out (FO)
- **Select statements** - each case counts toward Cyclomatic Complexity (CC)
- **Type switches** - each case counts toward CC

**Example Go analysis:**

```bash
$ hotspots analyze server.go --format json
```

```json
[
  {
    "file": "server.go",
    "function": "HandleRequest",
    "line": 45,
    "metrics": {
      "cc": 8,
      "nd": 3,
      "fo": 6,
      "ns": 2
    },
    "risk": {
      "r_cc": 3.17,
      "r_nd": 3.0,
      "r_fo": 2.81,
      "r_ns": 2.0
    },
    "lrs": 9.24,
    "band": "critical"
  }
]
```

**Understanding Go metrics in this example:**
- **CC=8:** Base complexity + switch cases + boolean operators
- **ND=3:** Three levels of nesting (e.g., `for` → `if` → `switch`)
- **FO=6:** 4 function calls + 1 goroutine + 1 defer
- **NS=2:** 1 early return + 1 defer statement
- **LRS=9.24:** Composite risk score → **critical** band

### Java

```bash
# Analyze single Java file
hotspots analyze src/Main.java

# Analyze Java package
hotspots analyze src/com/example/

# Analyze entire Java project
hotspots analyze src/ --format json

# Analyze Spring Boot application
hotspots analyze src/main/java/com/example/
```

**Supported files:** `.java`

**Java-specific metrics:**
- **Lambda expressions** - control flow inside lambdas counted inline with parent method
- **Stream operations** - don't inflate CC, but lambdas inside streams do count
- **Try-with-resources** - resource declarations add 0 CC, only catch clauses count
- **Switch expressions (Java 14+)** - treated same as traditional switch statements
- **Synchronized blocks** - each adds +1 to CC (critical section)
- **Anonymous inner classes** - methods analyzed as separate functions

**Example Java analysis:**

```bash
$ hotspots analyze UserService.java --format json
```

```json
[
  {
    "file": "UserService.java",
    "function": "processUsers",
    "line": 23,
    "language": "Java",
    "metrics": {
      "cc": 7,
      "nd": 2,
      "fo": 4,
      "ns": 3
    },
    "risk": {
      "r_cc": 3.0,
      "r_nd": 2.0,
      "r_fo": 2.32,
      "r_ns": 3.0
    },
    "lrs": 8.82,
    "band": "critical"
  }
]
```

**Understanding Java metrics in this example:**
- **CC=7:** Base complexity + if statements + try/catch clauses + boolean operators
- **ND=2:** Two levels of nesting (e.g., `try` → `for`)
- **FO=4:** 4 method calls (including constructors)
- **NS=3:** 2 returns + 1 throw statement
- **LRS=8.82:** Critical risk band - consider refactoring

**Common Java patterns and their metrics:**

```java
// Simple method with early return
public int getValue(String key) {
    if (key == null) {     // CC +1
        return -1;          // NS +1
    }
    return map.get(key);    // FO +1, NS +1
}
// CC=2, ND=1, FO=1, NS=2

// Try-with-resources
public String readFile(String path) {
    try (BufferedReader br = new BufferedReader(new FileReader(path))) {
        return br.readLine();   // FO +1, NS +1
    } catch (IOException e) {   // CC +1
        return "";               // NS +1
    }
}
// CC=2, ND=0, FO=1, NS=2

// Stream with filter
public List<User> getActiveUsers(List<User> users) {
    return users.stream()
        .filter(u -> u.isActive())   // CC +0 (stream doesn't inflate)
        .collect(Collectors.toList()); // FO +1
}
// CC=1, ND=0, FO=1, NS=1
```

### Python

```bash
# Analyze single Python file
hotspots analyze app.py

# Analyze Python package
hotspots analyze src/handlers/

# Analyze entire Python project
hotspots analyze . --format json
```

**Python-specific metrics:**
- **Comprehensions with filters** count toward Cyclomatic Complexity (CC)
- **Context managers** (`with` statements) count toward Nesting Depth (ND) but NOT CC
- **Exception handlers** - each `except` clause counts toward CC
- **Match statements** (Python 3.10+) - each `case` counts toward CC
- **Boolean operators** (`and`, `or`) each count toward CC

**Example Python analysis:**

```bash
$ hotspots analyze api.py --format json
```

```json
[
  {
    "file": "api.py",
    "function": "process_request",
    "line": 12,
    "language": "Python",
    "metrics": {
      "cc": 7,
      "nd": 3,
      "fo": 5,
      "ns": 3
    },
    "risk": {
      "r_cc": 3.01,
      "r_nd": 3.0,
      "r_fo": 2.58,
      "r_ns": 2.58
    },
    "lrs": 8.64,
    "band": "critical"
  }
]
```

**Understanding Python metrics in this example:**
- **CC=7:** Base complexity + if statements + boolean operators + except clauses + comprehension filters
- **ND=3:** Three levels of nesting (e.g., `if` → `with` → `try`)
- **FO=5:** 5 unique function calls
- **NS=3:** 2 early returns + 1 raise statement
- **LRS=8.64:** Composite risk score → **critical** band

**Python design decisions:**
- **Context managers don't inflate CC:** `with open(file) as f:` is resource management, not branching
- **Filtered comprehensions count:** `[x for x in items if x > 0]` has a conditional decision (the `if`)
- **Each except is a branch:** `try/except ValueError/except KeyError` has 2 decision points

---

## Git History Tracking

### Prerequisites

Hotspots must be run from within a git repository for snapshot/delta modes.

### Creating Snapshots

**Create a snapshot for the current commit:**

```bash
# In a git repository
cd my-repo
hotspots analyze . --mode snapshot --format json
```

This will:
- Analyze all TypeScript files in the repository
- Create a snapshot with commit metadata (SHA, parents, timestamp, branch)
- Persist to `.hotspots/snapshots/<commit_sha>.json`
- Update `.hotspots/index.json`

**What gets stored:**
- All functions with their metrics (CC, ND, FO, NS, LRS, band)
- Commit information (SHA, parents, timestamp, branch)
- Function IDs (`<file_path>::<symbol>`)

### Computing Deltas

**Compare current state vs parent commit:**

```bash
hotspots analyze . --mode delta --format json
```

This will:
- Load the parent snapshot (from `parents[0]`)
- Compare functions by `function_id`
- Show what changed: new, deleted, modified, unchanged
- Display metric deltas and band transitions

**Delta output shows:**
- Function status (new/deleted/modified/unchanged)
- Before/after metrics and LRS
- Numeric deltas (cc, nd, fo, ns, lrs)
- Band transitions (e.g., "moderate" → "high")

**Example delta output:**
```json
{
  "schema_version": 1,
  "commit": {
    "sha": "abc123",
    "parent": "def456"
  },
  "baseline": false,
  "deltas": [
    {
      "function_id": "src/api.ts::handleRequest",
      "status": "modified",
      "before": {
        "metrics": {"cc": 5, "nd": 2, "fo": 3, "ns": 1},
        "lrs": 4.8,
        "band": "moderate"
      },
      "after": {
        "metrics": {"cc": 7, "nd": 3, "fo": 3, "ns": 1},
        "lrs": 6.2,
        "band": "high"
      },
      "delta": {
        "cc": 2,
        "nd": 1,
        "fo": 0,
        "ns": 0,
        "lrs": 1.4
      },
      "band_transition": {
        "from": "moderate",
        "to": "high"
      }
    }
  ]
}
```

---

## Higher-Level Analysis

In addition to per-function output, snapshot mode offers three higher-level views accessible
with `--level` or `--explain`.

### File-Level Risk View (`--level file`)

Aggregate per-function data up to the file level and see a ranked file table:

```bash
# Ranked file risk table (requires --mode snapshot --format text)
hotspots analyze . --mode snapshot --format text --level file

# Limit to top 20 files
hotspots analyze . --mode snapshot --format text --level file --top 20
```

Columns: `#`, `file`, `fns`, `loc`, `max_cc`, `avg_cc`, `critical`, `churn`, `file_risk`.

A file with 40 functions averaging cc=12 is a maintenance liability even if no single
function individually tops the per-function list. The composite `file_risk_score` captures this:

```
file_risk = max_cc × 0.4 + avg_cc × 0.3 + log2(fn_count + 1) × 0.2 + churn_factor × 0.1
```

### Module Instability View (`--level module`)

See Robert Martin's instability metric at the directory level:

```bash
hotspots analyze . --mode snapshot --format text --level module
```

Columns: `#`, `module`, `files`, `fns`, `avg_cc`, `afferent`, `efferent`, `instability`, `risk`.

- **Instability near 0.0** — everything depends on this module; risky to change
- **Instability near 1.0** — depends on others but nothing depends on it; safe to change
- **`module_risk = high`** — when `instability < 0.3` AND `avg_complexity > 10`

The interesting hotspots are high-complexity modules with low instability (hard to change
AND everything depends on them).

### Per-Function Explanations (`--explain`)

See a human-readable breakdown of each function's risk score including individual metric
contributions, activity signals, and a co-change coupling section:

```bash
hotspots analyze . --mode snapshot --format text --explain
hotspots analyze . --mode snapshot --format text --explain --top 10
```

The co-change section at the bottom shows pairs of files that frequently change together
in the same commit. High co-change with no static dependency = hidden implicit coupling.
This signal is mined from the last 90 days of git history.

### Snapshot Without Persisting (`--no-persist`)

Run analysis in snapshot mode without writing to disk — useful for one-off inspection:

```bash
hotspots analyze . --mode snapshot --no-persist --format json | jq .aggregates.file_risk
```

### Regenerating a Snapshot (`--force`)

Snapshots are immutable by default. Use `--force` if you need to regenerate one:

```bash
hotspots analyze . --mode snapshot --force
```

### Precise Per-Function Touch Metrics (`--per-function-touches`)

By default, touch metrics are file-level. For more accurate per-function activity signals:

```bash
hotspots analyze . --mode snapshot --per-function-touches
```

**Warning:** Approximately 50× slower. Only use when precise per-function touch counts are needed.

---

## Common Workflows

### Daily Development

```bash
# 1. Check current complexity
hotspots analyze . --format text

# 2. Before making changes, create snapshot
hotspots analyze . --mode snapshot --format json

# 3. Make your changes...

# 4. See what changed
hotspots analyze . --mode delta --format json

# 5. Commit changes
git commit -m "Add feature"

# 6. Create snapshot for new commit
hotspots analyze . --mode snapshot --format json
```

### CI/CD Integration

**Mainline branch (persist snapshots):**

```yaml
# .github/workflows/complexity.yml
- name: Track complexity
  run: |
    hotspots analyze . --mode snapshot --format json
```

**PR branch (compare vs merge-base, don't persist):**

```yaml
# Automatically detected in PR context
- name: Check complexity changes
  run: |
    hotspots analyze . --mode delta --format json > delta.json
    # Parse delta.json and fail if critical functions degraded
```

Hotspots automatically detects PR context via `GITHUB_EVENT_NAME` and `GITHUB_REF` environment variables.

### Refactoring Validation

```bash
# Before refactoring
hotspots analyze . --mode snapshot --format json > before.json

# Make refactoring changes...

# After refactoring
hotspots analyze . --mode snapshot --format json > after.json

# See the improvement
hotspots analyze . --mode delta --format json
```

Look for:
- Negative deltas (CC, ND, LRS decreased)
- Band transitions to lower risk (e.g., "high" → "moderate")
- Overall LRS reduction

---

## History Management

### Prune Unreachable Snapshots

After force-pushes or branch deletions, clean up orphaned snapshots:

```bash
# Dry-run: see what would be pruned
hotspots prune --unreachable --dry-run

# Prune unreachable snapshots older than 30 days
hotspots prune --unreachable --older-than 30

# Prune all unreachable snapshots
hotspots prune --unreachable
```

**Safety:** Only prunes snapshots unreachable from `refs/heads/*` (local branches). Never prunes reachable snapshots.

### Set Compaction Level

```bash
# Set compaction level (currently only Level 0 is implemented)
hotspots compact --level 0
```

**Note:** Levels 1-2 are metadata placeholders for future implementation.

---

## Output Formats

### Text Format (Default)

```
LRS     File              Line  Function
11.2    src/api.ts        88    handleRequest
9.8     src/db/migrate.ts 41    runMigration
```

### JSON Format

```bash
hotspots analyze src/ --format json
```

Outputs structured JSON with all metrics, risk components, LRS, and band.

---

## Risk Bands

Functions are classified into risk bands based on LRS:

| Band      | Range      | Interpretation                          |
|-----------|------------|-----------------------------------------|
| Low       | LRS < 3    | Simple, maintainable functions          |
| Moderate  | 3 ≤ LRS < 6| Moderate complexity, review recommended |
| High      | 6 ≤ LRS < 9| High complexity, refactor recommended   |
| Critical  | LRS ≥ 9    | Very high complexity, urgent refactor   |

---

## Examples

### Find Most Complex Functions

```bash
hotspots analyze src/ --top 5 --format text
```

### Find Functions Needing Refactoring

```bash
hotspots analyze src/ --min-lrs 9.0 --format json
```

### Track Complexity Over Time

```bash
# On every commit (e.g., in pre-commit hook or CI)
hotspots analyze . --mode snapshot --format json
```

Then use deltas to see trends:
```bash
hotspots analyze . --mode delta --format json
```

### Compare Two Commits

```bash
# Checkout first commit
git checkout <sha1>
hotspots analyze . --mode snapshot --format json > commit1.json

# Checkout second commit
git checkout <sha2>
hotspots analyze . --mode snapshot --format json > commit2.json

# Compare manually or use delta mode
hotspots analyze . --mode delta --format json
```

---

## Troubleshooting

### "Path does not exist"

Make sure you're pointing to a valid file or directory:
```bash
hotspots analyze ./src  # Correct
hotspots analyze src    # Also correct (relative path)
```

### "failed to extract git context"

Snapshot/delta modes require a git repository:
```bash
# Make sure you're in a git repo
cd my-git-repo
hotspots analyze . --mode snapshot
```

### "snapshot already exists and differs"

Snapshots are immutable by default. This error means a snapshot already exists for the
current commit but its content differs from the freshly-computed result.

Repeated `analyze` runs on the same commit should be idempotent. If you see this error,
common causes are:
- A config change between runs that altered scores
- A tool version upgrade that changed metric computation

To regenerate the snapshot intentionally:
```bash
hotspots analyze . --mode snapshot --force
```

### No output in delta mode

If delta shows no changes:
- Check that parent snapshot exists (should be in `.hotspots/snapshots/`)
- Verify you're comparing against the correct parent (uses `parents[0]`)
- First commit will show `baseline: true` with all functions marked `new`

---

## Development Mode

For development, use the `dev` script:

```bash
# Run without building
./dev analyze src/

# Equivalent to: cargo run -- analyze src/
```

---

## Configuration

### Configuration File

Hotspots supports project-specific configuration via config files. Create one of the following:

- `.hotspotsrc.json` (recommended)
- `hotspots.config.json`
- `package.json` with a `"hotspots"` key

**Example `.hotspotsrc.json`:**

```json
{
  "include": ["src/**/*.ts", "lib/**/*.ts"],
  "exclude": ["**/*.test.ts", "**/*.spec.ts"],
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "weights": {
    "cc": 1.0,
    "nd": 1.0,
    "fo": 0.5,
    "ns": 1.0
  },
  "warnings": {
    "watch": {
      "min": 4.0,
      "max": 6.0
    },
    "attention": {
      "min": 6.0,
      "max": 9.0
    },
    "rapid_growth_percent": 50.0
  }
}
```

**Configuration options:**

- `include`: Glob patterns for files to analyze (default: all TypeScript/JavaScript files)
- `exclude`: Glob patterns for files to exclude (default: test files, node_modules, dist, build)
- `thresholds`: Custom risk band thresholds (moderate, high, critical)
- `weights`: Custom weights for metrics (cc, nd, fo, ns) - values 0.0-10.0
- `warnings`: Proactive warning thresholds (see Policy Engine section)

**Validate configuration:**

```bash
hotspots config validate
```

**View resolved configuration:**

```bash
hotspots config show
```

**CLI flags override config:**

```bash
# Config file has --min-lrs 3.0, this overrides to 5.0
hotspots analyze . --min-lrs 5.0
```

---

## Policy Engine

The policy engine evaluates complexity regressions and enforces quality gates in CI/CD.

### Running Policy Checks

```bash
# Analyze with policy evaluation
hotspots analyze . --mode delta --policy --format json
```

Output includes a `policy` section with failures and warnings.

### Built-in Policies

**Blocking Policies (cause non-zero exit code):**

1. **Critical Introduction** - Triggers when a function becomes Critical
   - New functions introduced as Critical
   - Existing functions crossing into Critical band

2. **Excessive Risk Regression** - Triggers when LRS increases by ≥1.0
   - Modified functions only
   - Threshold: +1.0 LRS (fixed)

**Warning Policies (informational only):**

3. **Watch Threshold** - Functions entering the "watch zone"
   - Range: `watch_min` to `watch_max` (default: 4.0-6.0)
   - Proactive alert before functions become high-risk

4. **Attention Threshold** - Functions entering the "attention zone"
   - Range: `attention_min` to `attention_max` (default: 6.0-9.0)
   - Alerts for functions approaching critical complexity

5. **Rapid Growth** - Functions with high percentage LRS increase
   - Threshold: `rapid_growth_percent` (default: 50%)
   - Detects sudden complexity spikes

6. **Suppression Missing Reason** - Suppressions without documentation
   - Warns when `// hotspots-ignore:` has no reason
   - Encourages documenting why functions are suppressed

7. **Net Repo Regression** - Overall repository complexity increase
   - Sum of all function LRS scores increased
   - Warning only (allows controlled growth)

**Example policy output:**

```json
{
  "policy": {
    "failed": [
      {
        "id": "critical-introduction",
        "severity": "blocking",
        "function_id": "src/api.ts::handleRequest",
        "message": "Function src/api.ts::handleRequest introduced as Critical"
      }
    ],
    "warnings": [
      {
        "id": "watch-threshold",
        "severity": "warning",
        "function_id": "src/db.ts::query",
        "message": "Function src/db.ts::query entered watch threshold range (LRS: 4.5)"
      },
      {
        "id": "net-repo-regression",
        "severity": "warning",
        "message": "Repository total LRS increased by 3.20",
        "metadata": {
          "total_delta": 3.20
        }
      }
    ]
  }
}
```

### Configuring Warning Thresholds

Customize warning ranges in your config file:

```json
{
  "warnings": {
    "watch": {
      "min": 5.0,
      "max": 7.0
    },
    "attention": {
      "min": 7.0,
      "max": 10.0
    },
    "rapid_growth_percent": 75.0
  }
}
```

---

## Suppressing Policy Violations

Use suppression comments to exclude specific functions from policy checks while keeping them in reports.

### Suppression Syntax

Place a comment **immediately before** the function:

```typescript
// hotspots-ignore: legacy code, refactor planned for Q2 2026
function complexLegacyParser(input: string) {
  // High complexity code...
}
```

**Rules:**
- Comment must be on the line immediately before the function
- Format: `// hotspots-ignore: reason`
- Reason is required (warning if missing)
- Blank lines break the suppression

### What Suppressions Do

**Excluded from:**
- Critical Introduction policy
- Excessive Risk Regression policy
- Watch/Attention/Rapid Growth warnings

**Included in:**
- Analysis reports (visible with `suppression_reason` field)
- Net Repo Regression (repo-level metric includes all functions)
- HTML reports

**Example suppressed function in JSON:**

```json
{
  "file": "src/legacy.ts",
  "function": "oldParser",
  "line": 42,
  "lrs": 12.5,
  "band": "critical",
  "suppression_reason": "legacy code, refactor planned for Q2 2026"
}
```

### Suppression Validation

Functions suppressed without a reason will trigger a warning:

```typescript
// hotspots-ignore:
function foo() { }  // ⚠️  Warning: suppressed without reason
```

**Best practices:**
- Document WHY the function is suppressed
- Include planned action (e.g., "refactor in Q2")
- Require code review for new suppressions
- Periodically audit suppressed functions

**When to suppress:**
- Complex algorithms with well-established tests
- Legacy code pending migration
- Generated code
- Intentionally complex code (e.g., state machines)

**When NOT to suppress:**
- New code that should be refactored
- "I'll fix it later" without a concrete plan
- Code that could be simplified

---

## HTML Reports

Generate interactive HTML reports for better visualization:

```bash
hotspots analyze . --mode snapshot --format html
```

**HTML report features:**
- Interactive sorting by any column
- Filter by risk band and driver label
- Search by function name
- Color-coded risk bands and driver badges
- **Action column** in triage table: per-function refactoring recommendation (driver × quadrant)
- **Trend charts** (snapshot mode, requires ≥2 prior snapshots):
  - Stacked bar chart: band-count distribution over time (up to 30 snapshots)
  - Line charts: activity risk and top-1% concentration over time
  - Hover tooltip on band chart with per-band counts
- Responsive design
- Self-contained (no external dependencies)

**Delta mode HTML:**

```bash
hotspots analyze . --mode delta --format html > delta-report.html
```

Shows function changes with:
- Status badges (new/modified/deleted/unchanged)
- Before/after metrics
- Band transitions
- Policy violations highlighted

**Open in browser:**

```bash
hotspots analyze . --mode snapshot --format html
open .hotspots/report.html  # macOS
xdg-open .hotspots/report.html  # Linux
start .hotspots/report.html  # Windows
```

---

## See Also

- [Metrics & LRS](../reference/metrics.md) - Local Risk Score details
