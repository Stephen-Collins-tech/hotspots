# Output Formats

Hotspots supports three output formats: JSON (machine-readable), HTML (interactive reports), and Text (human-readable terminal output).

## Format Overview

| Format | Use Case | Features |
|--------|----------|----------|
| **JSON** | CI/CD, tooling, AI agents | Structured, versioned schema, machine-parseable |
| **HTML** | Reports, dashboards, sharing | Interactive, charts, filterable, standalone |
| **Text** | Terminal, quick inspection | Color-coded, compact, human-friendly |

---

## JSON Format

Machine-readable structured output for programmatic consumption.

### Basic Usage

```bash
# Snapshot mode JSON
hotspots analyze src/ --format json

# Delta mode JSON
hotspots analyze src/ --mode delta --format json

# With policy evaluation
hotspots analyze src/ --mode delta --policy --format json
```

### Output Structure

```json
{
  "schema_version": 2,
  "commit": {
    "sha": "abc123def456...",
    "parents": ["def456..."],
    "timestamp": 1704067200,
    "branch": "main"
  },
  "analysis": {
    "scope": "full",
    "tool_version": "1.0.0"
  },
  "functions": [
    {
      "function_id": "/path/to/file.ts::functionName",
      "file": "/absolute/path/to/file.ts",
      "line": 42,
      "metrics": {
        "cc": 8,
        "nd": 2,
        "fo": 4,
        "ns": 2
      },
      "lrs": 7.2,
      "band": "high",
      "patterns": ["complex_branching", "god_function"],
      "pattern_details": [
        {
          "id": "complex_branching",
          "tier": 1,
          "kind": "primitive",
          "triggered_by": [
            { "metric": "cc", "op": ">=", "value": 8, "threshold": 10 },
            { "metric": "nd", "op": ">=", "value": 2, "threshold": 4 }
          ]
        }
      ]
    }
  ],
  "aggregates": {
    "total_functions": 150,
    "by_band": {
      "low": 80,
      "moderate": 45,
      "high": 20,
      "critical": 5
    },
    "average_lrs": 4.3
  },
  "policy_results": {
    "failed": [],
    "warnings": [
      {
        "id": "watch-threshold",
        "level": "info",
        "function_id": "/path/to/file.ts::someFunction",
        "message": "Function approaching moderate threshold",
        "metadata": {
          "current_lrs": 2.8,
          "threshold": 3.0
        }
      }
    ]
  }
}
```

### Schema Documentation

Complete JSON schema definitions available:

- **`hotspots-output.schema.json`** - Full output structure
- **`function-report.schema.json`** - Function analysis format
- **`metrics.schema.json`** - Raw metrics (CC, ND, FO, NS)
- **`policy-result.schema.json`** - Policy violations/warnings

See the [JSON Schema Reference section](#json-schema-reference) below for complete documentation.

### Fields Reference

#### Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | number | Schema version (currently `2` for snapshot/delta output) |
| `commit` | object | Git commit metadata |
| `analysis` | object | Analysis metadata |
| `functions` | array | Function analysis results |
| `aggregates` | object | Optional aggregated statistics |
| `policy_results` | object | Optional policy evaluation results |

#### Commit Metadata

| Field | Type | Description |
|-------|------|-------------|
| `sha` | string | Git commit SHA (40 chars) |
| `parents` | string[] | Parent commit SHAs |
| `timestamp` | number | Unix timestamp |
| `branch` | string | Branch name (optional) |

#### Function Report

| Field | Type | Description |
|-------|------|-------------|
| `function_id` | string | Unique identifier: `file::functionName` |
| `file` | string | Absolute file path |
| `line` | number | Line number where function starts |
| `metrics` | object | Raw complexity metrics |
| `lrs` | number | Leverage Risk Score |
| `band` | string | Risk band: `low`, `moderate`, `high`, `critical` |
| `suppression_reason` | string | Optional suppression comment |
| `patterns` | string[] | Code patterns detected (e.g. `"god_function"`, `"complex_branching"`). Omitted when empty. |
| `pattern_details` | object[] | Per-pattern trigger details. Present only when `--explain-patterns` is set. |

**`pattern_details` entry:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Pattern identifier (e.g. `"complex_branching"`) |
| `tier` | number | `1` = structural (always available), `2` = enriched (snapshot mode only) |
| `kind` | string | `"primitive"` or `"derived"` |
| `triggered_by` | object[] | Each condition that caused the pattern to fire: `metric`, `op`, `value`, `threshold` |

#### Metrics Object

| Field | Type | Description |
|-------|------|-------------|
| `cc` | number | Cyclomatic Complexity |
| `nd` | number | Nesting Depth |
| `fo` | number | Fan-Out (function calls) |
| `ns` | number | Non-Structured exits |

### TypeScript Integration

```bash
npm install @hotspots/types
```

```typescript
import type { HotspotsOutput, FunctionReport } from '@hotspots/types';
import { filterByRiskBand, getHighestRiskFunctions } from '@hotspots/types';

// Parse output
const output: HotspotsOutput = JSON.parse(jsonOutput);

// Filter high-risk functions
const highRisk = filterByRiskBand(output.functions, 'high');

// Get top 10 most complex
const top10 = getHighestRiskFunctions(output.functions, 10);

// Type-safe access
output.functions.forEach((fn: FunctionReport) => {
  console.log(`${fn.function_id}: LRS ${fn.lrs} (${fn.band})`);
});
```

### Python Integration

```python
import json

# Parse output
with open('hotspots-output.json') as f:
    output = json.load(f)

# Filter critical functions
critical = [
    fn for fn in output['functions']
    if fn['band'] == 'critical'
]

# Calculate stats
total_functions = len(output['functions'])
avg_lrs = sum(fn['lrs'] for fn in output['functions']) / total_functions

print(f"Total functions: {total_functions}")
print(f"Average LRS: {avg_lrs:.2f}")
print(f"Critical functions: {len(critical)}")
```

### jq Examples

```bash
# Extract high-risk functions
jq '.functions[] | select(.band == "high" or .band == "critical")' output.json

# Count by risk band
jq '.aggregates.by_band' output.json

# Top 10 by LRS
jq '.functions | sort_by(.lrs) | reverse | .[0:10]' output.json

# Average LRS
jq '[.functions[].lrs] | add / length' output.json

# Functions with policy violations
jq '.policy_results.failed[] | .function_id' output.json

# Functions carrying any pattern
jq '.functions[] | select(.patterns | length > 0) | {id: .function_id, patterns}' output.json

# Count functions per pattern
jq '[.functions[].patterns[]?] | group_by(.) | map({pattern: .[0], count: length})' output.json

# Functions with god_function pattern
jq '.functions[] | select(.patterns[]? == "god_function") | .function_id' output.json
```

---

## HTML Format

Interactive HTML reports with charts and visualizations.

### Basic Usage

```bash
# Snapshot mode HTML
hotspots analyze src/ --mode snapshot --format html

# Delta mode HTML
hotspots analyze src/ --mode delta --format html

# Custom output path
hotspots analyze src/ --mode snapshot --format html --output reports/complexity.html
```

**Default output:** `.hotspots/report.html`

### Report Features

**Overview Dashboard:**
- Total functions analyzed
- Risk band distribution (pie chart)
- Average LRS
- Policy violations summary

**Pattern Breakdown Panel** (shown above the function table when any patterns are detected):
- Frequency chips for each detected pattern, sorted by count descending
- One chip per pattern: count, name, and short description
- Per-pattern color coding — warm reds/ambers for Tier 1 structural patterns, cool blues/purples for Tier 2 enriched patterns, dark crimson for `volatile_god`
- Dark mode support

**Function Table:**
- Sortable by LRS, CC, ND, FO, NS
- Filterable by risk band and driver label
- Searchable by function name
- Color-coded risk levels and driver badges
- Pattern column: colored pill badges per detected pattern
- Action column: per-function refactoring recommendation (driver × quadrant)

**Charts (snapshot mode):**
- Risk band distribution (donut chart)
- Historical trend charts: stacked band count, activity risk line, top-1% share line
  (requires ≥2 prior snapshots; up to 30 history points; hover for per-bar detail)

**Delta Mode Additions:**
- Before/after comparison
- Complexity changes (Δ LRS)
- New functions highlighted
- Removed functions shown
- Policy violation details

### Opening Reports

```bash
# Generate and open in browser
hotspots analyze src/ --mode snapshot --format html
open .hotspots/report.html  # macOS
xdg-open .hotspots/report.html  # Linux
start .hotspots/report.html  # Windows
```

### CI/CD Artifacts

**GitHub Actions:**
```yaml
- uses: Stephen-Collins-tech/hotspots-action@v1
  id: hotspots
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: hotspots-report
    path: ${{ steps.hotspots.outputs.report-path }}
    retention-days: 30
```

**GitLab CI:**
```yaml
artifacts:
  paths:
    - .hotspots/report.html
  expire_in: 1 week
```

### Sharing Reports

HTML reports are self-contained (embedded CSS/JS):

```bash
# Email report
echo "See attached complexity report" | mail -a .hotspots/report.html team@example.com

# Upload to S3
aws s3 cp .hotspots/report.html s3://my-bucket/reports/complexity-$(date +%Y%m%d).html

# Serve with HTTP server
python3 -m http.server 8000 --directory .hotspots
# Open http://localhost:8000/report.html
```

---

## Text Format

Human-readable terminal output with color coding.

### Basic Usage

```bash
# Default text output
hotspots analyze src/

# Explicit text format
hotspots analyze src/ --format text

# Delta mode with policy
hotspots analyze src/ --mode delta --policy --format text

# Show pattern trigger details inline
hotspots analyze src/ --explain-patterns
hotspots analyze src/ --mode snapshot --format text --explain --explain-patterns
```

### Snapshot Mode Output

```
Hotspots Analysis
================================================================================

Functions by Risk Band:

Critical (LRS ≥ 9.0):
processComplexOrder             /src/orders.ts:142        LRS 10.2  CC 15  ND 4  FO 8  NS 3
handlePaymentFlow               /src/payments.ts:89       LRS 9.5   CC 12  ND 3  FO 6  NS 4

High (6.0 ≤ LRS < 9.0):
validateUserInput               /src/validation.ts:23     LRS 7.8   CC 10  ND 2  FO 5  NS 2
generateReport                  /src/reports.ts:156       LRS 6.5   CC 8   ND 3  FO 4  NS 1

Moderate (3.0 ≤ LRS < 6.0):
formatDate                      /src/utils.ts:45          LRS 4.2   CC 5   ND 1  FO 2  NS 1

Summary:
  Total functions: 150
  Critical: 2
  High: 2
  Moderate: 45
  Low: 101
  Average LRS: 3.2
```

### Snapshot Mode with `--explain`

The `--explain` flag adds per-function detail: driver label, recommended action, and — for
`composite`-labeled functions — the top near-miss dimensions with their percentile ranks.

```
hotspots analyze src/ --mode snapshot --format text --explain
```

```
processComplexOrder             /src/orders.ts:142
   LRS: 10.2 | Band: critical | Driver: cc
   CC: 15, ND: 4, FO: 8, NS: 3
   Action: Reduce branching; extract sub-functions

handlePaymentFlow               /src/payments.ts:89
   LRS: 9.5 | Band: critical | Driver: composite
   CC: 12, ND: 3, FO: 6, NS: 4
   Action: Multiple complexity dimensions — address the highest first
   Near-threshold: fan_out (P78), cc (P72), nd (P61)

validateUserInput               /src/validation.ts:23
   LRS: 7.8 | Band: high | Driver: nd
   CC: 10, ND: 2, FO: 5, NS: 2
   Action: Reduce nesting depth; early returns help
```

`Near-threshold` appears only for `composite` functions and lists up to 3 dimensions at
or above the 40th percentile (i.e., above median across all analysed functions), sorted
by percentile rank descending. This makes multi-factor functions interpretable without
changing how the driver label is computed.

### Delta Mode Output

```
Delta Analysis
================================================================================

Changed Functions:

processOrder                    /src/orders.ts:42
  Before: LRS 6.5 (high)         CC 8   ND 2  FO 4  NS 1
  After:  LRS 7.2 (high)         CC 10  ND 2  FO 5  NS 2
  Change: +0.7 LRS (+10.8%)      ⚠️  Regression

validateInput                   /src/validation.ts:15
  Before: LRS 5.2 (moderate)     CC 6   ND 2  FO 3  NS 1
  After:  LRS 4.8 (moderate)     CC 5   ND 2  FO 3  NS 1
  Change: -0.4 LRS (-7.7%)       ✅ Improvement

New Functions:
handleNewFeature                /src/features.ts:89       LRS 3.2 (moderate)

Removed Functions:
deprecatedFunction              /src/legacy.ts:123        LRS 8.5 (high)

Summary:
  Changed: 2
  New: 1
  Removed: 1
  Net ΔLRS: +0.3
```

### Delta Mode with Policy Output

```
Policy Evaluation Results
================================================================================

Policy failures:
- no-regressions: /src/orders.ts::processOrder

Violating functions:
Function                                 Before       After        ΔLRS       Policy
----------------------------------------------------------------------------------------------
processOrder                             high         high         +0.70      no-regressions

Watch Level (approaching moderate threshold):
Function                                 Current LRS  Band
----------------------------------------------------------------
formatCurrency                           2.8          low

Attention Level (approaching high threshold):
Function                                 Current LRS  Band
----------------------------------------------------------------
validateEmail                            5.7          moderate

Summary:
  Blocking failures: 1
  Watch warnings: 1
  Attention warnings: 1
```

### Color Coding

Terminal output uses ANSI colors:

- **Critical (red):** LRS ≥ 9.0
- **High (yellow):** 6.0 ≤ LRS < 9.0
- **Moderate (blue):** 3.0 ≤ LRS < 6.0
- **Low (green):** LRS < 3.0

**Disable colors:**
```bash
NO_COLOR=1 hotspots analyze src/ --format text
```

---

## Format Comparison

### When to Use Each Format

**JSON:**
- ✅ CI/CD pipelines
- ✅ Tooling integration
- ✅ AI agent consumption
- ✅ Data analysis
- ✅ Long-term storage
- ❌ Human inspection (use HTML/Text)

**HTML:**
- ✅ Sharing reports
- ✅ Dashboards
- ✅ Historical tracking
- ✅ Presentations
- ✅ Non-technical stakeholders
- ❌ Programmatic parsing (use JSON)

**Text:**
- ✅ Terminal inspection
- ✅ Quick checks
- ✅ Git hooks
- ✅ Local development
- ❌ Automation (use JSON)
- ❌ Visual analysis (use HTML)

---

## Output Redirection

### Save to File

```bash
# JSON
hotspots analyze src/ --format json > analysis.json

# Text
hotspots analyze src/ --format text > analysis.txt

# HTML (use --output instead)
hotspots analyze src/ --mode snapshot --format html --output report.html
```

### Pipe to Tools

```bash
# Parse with jq
hotspots analyze src/ --format json | jq '.functions[] | select(.band == "critical")'

# Filter with grep
hotspots analyze src/ --format text | grep "Critical"

# Count lines
hotspots analyze src/ --format text | wc -l
```

---

## Related Documentation

- [CLI Reference](../reference/cli.md) - Command-line options
- [CI/CD & GitHub Action](./ci-cd.md) - Using output in pipelines
- [Configuration](./configuration.md) - Filter and threshold options

---

## JSON Schema Reference

Hotspots produces versioned JSON output. The `schema_version` field indicates the format.

### Schema Versions

| Version | Scope | Added fields |
|---------|-------|--------------|
| **v2** (current) | Snapshot and delta output | `driver`, `driver_detail`, `patterns`, `pattern_details`, enriched `aggregates` (file_risk, co_change, modules) |
| **v3** | Agent-optimized (`--all-functions`) | `fire`/`debt`/`watch`/`ok` quadrant buckets, per-function `action` text |
| **v1** | Delta output (legacy constant) | — |

Always check `schema_version` before consuming output in tooling.

### Driver Labels

Each function includes an optional `driver` string identifying the primary source of risk:

| Label | Condition | Recommended action |
|-------|-----------|-------------------|
| `cyclic_dep` | Function is in a dependency cycle (SCC size > 1) | Break the cycle before adding more callers |
| `high_complexity` | CC above the Pth percentile | Schedule a refactor; extract sub-functions |
| `high_churn_low_cc` | touch_count above Pth percentile and CC below (100-P)th | Add regression tests before next change |
| `high_fanout_churning` | fan_out above Pth percentile and touch above 50th | Extract an interface boundary |
| `deep_nesting` | ND above the Pth percentile | Flatten with early returns or guard clauses |
| `high_fanin_complex` | fan_in above Pth percentile and CC above 50th | Extract and stabilize; wide blast radius |
| `composite` | None of the above specific drivers | Monitor complexity trends |

Thresholds are percentile-relative (default P=75, configurable via `driver_threshold_percentile`). `cyclic_dep` is the sole absolute check.

### `driver_detail` — Near-miss context for composite functions

When a function receives the `composite` label, `driver_detail` lists the top dimensions (up to 3) that came closest to firing a specific label, with their percentile rank. Example: `"cc (P72), nd (P68)"` means CC is at the 72nd percentile and ND at the 68th — notable but below the P75 threshold. Only dimensions above the 40th percentile are included.

`driver_detail` is omitted from JSON when null (forward-compatible).

### Aggregates (Snapshot Mode)

Snapshot output includes an `aggregates` object with three arrays:

#### `aggregates.file_risk` — File-Level Risk

Each entry covers one source file, ranked by `file_risk_score` descending.

```typescript
{
  file: "src/api.ts",
  function_count: 12,
  loc: 340,
  max_cc: 14,
  avg_cc: 6.8,
  critical_count: 2,
  file_churn: 180,
  file_risk_score: 8.3   // max_cc×0.4 + avg_cc×0.3 + log2(fn_count+1)×0.2 + churn×0.1
}
```

#### `aggregates.co_change` — Co-Change Coupling

Pairs of files that frequently change together in the same commit. High coupling with no static dependency = hidden implicit coupling.

```typescript
{
  file_a: "hotspots-cli/src/main.rs",
  file_b: "hotspots-core/src/aggregates.rs",
  co_change_count: 14,
  coupling_ratio: 0.78,
  has_static_dep: false,
  risk: "high"   // "high" | "moderate" | "expected" | "low"
}
```

`risk: "expected"` means a static import exists between the files — the co-change is explained. Default window: 90 days; minimum count: 3.

#### `aggregates.modules` — Module Instability

Robert Martin's instability metric at the directory level: `instability = efferent / (afferent + efferent)`.

```typescript
{
  module: "hotspots-core/src",
  file_count: 12,
  function_count: 409,
  avg_complexity: 3.2,
  afferent: 8,       // external modules depending on this one
  efferent: 3,       // external modules this one depends on
  instability: 0.27, // near 0 = risky to change; near 1 = safe
  module_risk: "high"
}
```

### Schema Files

JSON Schema definitions are available in the `schemas/` directory:

- `hotspots-output.schema.json` — Complete output schema
- `function-report.schema.json` — Individual function analysis
- `metrics.schema.json` — Raw metrics (CC, ND, FO, NS)
- `policy-result.schema.json` — Policy violation/warning format

All schemas follow JSON Schema Draft 07.

---

## Examples

### Extract Critical Functions (JSON)

```bash
hotspots analyze src/ --format json | \
  jq -r '.functions[] | select(.band == "critical") | "\(.function_id): \(.lrs)"'
```

### Generate Report Card (Text)

```bash
hotspots analyze src/ --format text | \
  grep -A 10 "Summary:"
```

### Track Complexity Over Time (JSON)

```bash
# Daily snapshots
hotspots analyze src/ --mode snapshot --format json > "reports/$(date +%Y%m%d).json"

# Compare last 7 days
for file in reports/*.json; do
  avg_lrs=$(jq '.aggregates.average_lrs' "$file")
  echo "$(basename $file .json): $avg_lrs"
done
```

### Custom HTML Dashboard

Embed HTML report in iframe:

```html
<!DOCTYPE html>
<html>
<head>
  <title>Complexity Dashboard</title>
</head>
<body>
  <h1>Latest Complexity Report</h1>
  <iframe src=".hotspots/report.html" width="100%" height="800px"></iframe>
</body>
</html>
```

---

**Need more examples?** Check out [examples/output-formats/](https://github.com/Stephen-Collins-tech/hotspots/tree/main/examples/output-formats).

**Using in CI/CD?** See [CI/CD & GitHub Action](./ci-cd.md) for pipeline integration examples.
