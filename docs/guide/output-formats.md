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
  "schema_version": 1,
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
      "band": "high"
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

See [JSON Schema Reference](../reference/json-schema.md) for complete documentation.

### Fields Reference

#### Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | number | Schema version (currently `1`) |
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

**Function Table:**
- Sortable by LRS, CC, ND, FO, NS
- Filterable by risk band
- Searchable by function name
- Color-coded risk levels

**Charts:**
- Risk band distribution (pie chart)
- LRS histogram
- Metrics scatter plots
- Trend charts (delta mode)

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

- [JSON Schema Reference](../reference/json-schema.md) - Complete schema documentation
- [CLI Reference](../reference/cli.md) - Command-line options
- [CI/CD Integration](./ci-integration.md) - Using output in pipelines
- [Configuration](./configuration.md) - Filter and threshold options

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
