# JSON Schema & Output Format

Hotspots outputs structured JSON that can be consumed by CI/CD pipelines, analysis tools, and AI assistants. This document describes the output format and provides integration examples.

## Overview

Hotspots produces JSON output in two modes:

- **Snapshot Mode** (`hotspots analyze --json`): Complete analysis of all functions in the codebase
- **Delta Mode** (`hotspots analyze --delta --json`): Analysis of changed functions since the last commit

Both modes use the same JSON schema with consistent structure.

## Schema Files

JSON Schema definitions are available in the `schemas/` directory:

- **`hotspots-output.schema.json`**: Complete output schema (main entry point)
- **`function-report.schema.json`**: Individual function analysis
- **`metrics.schema.json`**: Raw complexity metrics (CC, ND, FO, NS)
- **`policy-result.schema.json`**: Policy violation/warning format

All schemas follow JSON Schema Draft 07 specification.

## Output Structure

```typescript
{
  schema_version: 1,
  commit: {
    sha: "abc123...",           // Git commit SHA (40 chars)
    parents: ["def456..."],     // Parent commit SHAs
    timestamp: 1234567890,      // Unix timestamp
    branch: "main"              // Current branch (optional)
  },
  analysis: {
    scope: "full" | "delta",    // Analysis mode
    tool_version: "1.0.0"       // Hotspots version
  },
  functions: [
    {
      function_id: "/path/to/file.ts::functionName",
      file: "/absolute/path/to/file.ts",
      line: 42,
      metrics: {
        cc: 8,    // Cyclomatic Complexity
        nd: 2,    // Nesting Depth
        fo: 4,    // Fan-Out
        ns: 2     // Non-Structured exits
      },
      lrs: 7.2,   // Logarithmic Risk Score
      band: "high",  // Risk band: low | moderate | high | critical
      suppression_reason: "Legacy code, refactor planned"  // Optional
    }
  ],
  aggregates: {  // Optional (when --aggregates used)
    files: [...],
    directories: [...]
  },
  policy_results: {  // Optional (when --policy used)
    failed: [...],    // Blocking failures
    warnings: [...]   // Non-blocking warnings
  }
}
```

## Metrics Explained

### CC - Cyclomatic Complexity
Number of linearly independent paths through the code (decision points + 1). Counts `if`, `while`, `for`, `switch`, `||`, `&&`, `?:`, etc.

**Example:**
```typescript
function simple() {
  return 42;  // CC = 1 (one path)
}

function withBranch(x) {
  if (x > 0) {     // +1 decision point
    return x;
  }
  return -x;       // CC = 2 (two paths)
}
```

### ND - Nesting Depth
Maximum level of nested control structures. Deeply nested code is harder to understand and maintain.

**Example:**
```typescript
function nested(x) {
  if (x > 0) {           // depth 1
    if (x < 100) {       // depth 2
      if (x % 2 === 0) { // depth 3 (ND = 3)
        return x;
      }
    }
  }
  return 0;
}
```

### FO - Fan-Out
Number of distinct functions or methods called. High fan-out indicates many dependencies.

**Example:**
```typescript
function highFanOut() {
  validate();     // callee 1
  transform();    // callee 2
  save();         // callee 3
  notify();       // callee 4
  // FO = 4
}
```

### NS - Non-Structured Exits
Number of early returns, throws, breaks, and continues. Multiple exit points increase complexity.

**Example:**
```typescript
function multipleExits(x) {
  if (x < 0) return null;        // NS +1
  if (x === 0) throw new Error(); // NS +1
  if (x > 100) return x;         // NS +1
  return x * 2;                   // Normal exit (not counted)
  // NS = 3
}
```

### LRS - Logarithmic Risk Score
Composite metric combining all raw metrics with logarithmic scaling:

```
LRS = ln(CC + 1) + ln(ND + 1) + ln(FO + 1) + ln(NS + 1)
```

Higher scores indicate higher complexity and maintenance risk.

## Risk Bands

Functions are classified into risk bands based on LRS:

| Band       | LRS Range  | Description                    |
|------------|------------|--------------------------------|
| Low        | < 3.0      | Simple, easy to maintain       |
| Moderate   | 3.0 - 6.0  | Moderate complexity, acceptable |
| High       | 6.0 - 9.0  | Complex, consider refactoring  |
| Critical   | ≥ 9.0      | Very complex, refactor recommended |

## TypeScript Integration

### Using @hotspots/types Package

```bash
npm install @hotspots/types
```

```typescript
import type { HotspotsOutput, FunctionReport } from '@hotspots/types';
import {
  filterByRiskBand,
  getHighestRiskFunctions,
  policyPassed
} from '@hotspots/types';

// Parse Hotspots output
const output: HotspotsOutput = JSON.parse(
  await fs.readFile('hotspots-output.json', 'utf-8')
);

// Get high-risk functions
const highRisk = filterByRiskBand(output.functions, 'high');
const critical = filterByRiskBand(output.functions, 'critical');

console.log(`Found ${highRisk.length} high-risk functions`);
console.log(`Found ${critical.length} critical functions`);

// Get top 10 most complex
const top10 = getHighestRiskFunctions(output.functions, 10);
top10.forEach(func => {
  console.log(`${func.function_id} - LRS: ${func.lrs}`);
});

// Check policy results
if (output.policy_results && !policyPassed(output.policy_results)) {
  console.error('Policy check failed!');
  output.policy_results.failed.forEach(failure => {
    console.error(`  ${failure.id}: ${failure.message}`);
  });
  process.exit(1);
}
```

### Manual Schema Validation (TypeScript)

```bash
npm install ajv ajv-formats
```

```typescript
import Ajv from 'ajv';
import addFormats from 'ajv-formats';
import * as fs from 'fs';

const ajv = new Ajv();
addFormats(ajv);

// Load schema
const schema = JSON.parse(
  fs.readFileSync('schemas/hotspots-output.schema.json', 'utf-8')
);

const validate = ajv.compile(schema);

// Validate output
const output = JSON.parse(
  fs.readFileSync('hotspots-output.json', 'utf-8')
);

if (!validate(output)) {
  console.error('Invalid Hotspots output:', validate.errors);
  process.exit(1);
}

console.log('✓ Output is valid');
```

## Python Integration

### Using jsonschema

```bash
pip install jsonschema
```

```python
import json
from jsonschema import validate, ValidationError

# Load schema
with open('schemas/hotspots-output.schema.json') as f:
    schema = json.load(f)

# Load and validate output
with open('hotspots-output.json') as f:
    output = json.load(f)

try:
    validate(instance=output, schema=schema)
    print('✓ Output is valid')
except ValidationError as e:
    print(f'Invalid output: {e.message}')
    exit(1)

# Analyze results
high_risk = [
    func for func in output['functions']
    if func['band'] in ['high', 'critical']
]

print(f'Found {len(high_risk)} high-risk functions')

# Sort by LRS
top_10 = sorted(
    output['functions'],
    key=lambda f: f['lrs'],
    reverse=True
)[:10]

for func in top_10:
    print(f"{func['function_id']} - LRS: {func['lrs']}")

# Check policy results
if 'policy_results' in output:
    if output['policy_results']['failed']:
        print('Policy check failed!')
        for failure in output['policy_results']['failed']:
            print(f"  {failure['id']}: {failure['message']}")
        exit(1)
```

## Go Integration

### Using gojsonschema

```bash
go get github.com/xeipuuv/gojsonschema
```

```go
package main

import (
    "encoding/json"
    "fmt"
    "io/ioutil"
    "os"
    "sort"

    "github.com/xeipuuv/gojsonschema"
)

type HotspotsOutput struct {
    SchemaVersion int              `json:"schema_version"`
    Commit        CommitInfo       `json:"commit"`
    Analysis      AnalysisInfo     `json:"analysis"`
    Functions     []FunctionReport `json:"functions"`
    PolicyResults *PolicyResults   `json:"policy_results,omitempty"`
}

type CommitInfo struct {
    SHA       string   `json:"sha"`
    Parents   []string `json:"parents"`
    Timestamp int64    `json:"timestamp"`
    Branch    *string  `json:"branch,omitempty"`
}

type AnalysisInfo struct {
    Scope       string `json:"scope"`
    ToolVersion string `json:"tool_version"`
}

type FunctionReport struct {
    FunctionID  string  `json:"function_id"`
    File        string  `json:"file"`
    Line        int     `json:"line"`
    Metrics     Metrics `json:"metrics"`
    LRS         float64 `json:"lrs"`
    Band        string  `json:"band"`
}

type Metrics struct {
    CC int `json:"cc"`
    ND int `json:"nd"`
    FO int `json:"fo"`
    NS int `json:"ns"`
}

type PolicyResults struct {
    Failed   []PolicyResult `json:"failed"`
    Warnings []PolicyResult `json:"warnings"`
}

type PolicyResult struct {
    ID       string `json:"id"`
    Severity string `json:"severity"`
    Message  string `json:"message"`
}

func main() {
    // Validate against schema
    schemaLoader := gojsonschema.NewReferenceLoader("file://schemas/hotspots-output.schema.json")
    documentLoader := gojsonschema.NewReferenceLoader("file://hotspots-output.json")

    result, err := gojsonschema.Validate(schemaLoader, documentLoader)
    if err != nil {
        panic(err)
    }

    if !result.Valid() {
        fmt.Println("Schema validation failed:")
        for _, desc := range result.Errors() {
            fmt.Printf("- %s\n", desc)
        }
        os.Exit(1)
    }

    // Parse output
    data, _ := ioutil.ReadFile("hotspots-output.json")
    var output HotspotsOutput
    json.Unmarshal(data, &output)

    // Get high-risk functions
    var highRisk []FunctionReport
    for _, fn := range output.Functions {
        if fn.Band == "high" || fn.Band == "critical" {
            highRisk = append(highRisk, fn)
        }
    }

    fmt.Printf("Found %d high-risk functions\n", len(highRisk))

    // Sort by LRS
    sort.Slice(output.Functions, func(i, j int) bool {
        return output.Functions[i].LRS > output.Functions[j].LRS
    })

    fmt.Println("\nTop 10 most complex functions:")
    for i := 0; i < 10 && i < len(output.Functions); i++ {
        fn := output.Functions[i]
        fmt.Printf("%s - LRS: %.2f\n", fn.FunctionID, fn.LRS)
    }

    // Check policy results
    if output.PolicyResults != nil && len(output.PolicyResults.Failed) > 0 {
        fmt.Println("\nPolicy check failed!")
        for _, failure := range output.PolicyResults.Failed {
            fmt.Printf("  %s: %s\n", failure.ID, failure.Message)
        }
        os.Exit(1)
    }
}
```

## Rust Integration

### Using serde and jsonschema

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
jsonschema = "0.17"
```

```rust
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
struct HotspotsOutput {
    schema_version: u32,
    commit: CommitInfo,
    analysis: AnalysisInfo,
    functions: Vec<FunctionReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_results: Option<PolicyResults>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommitInfo {
    sha: String,
    parents: Vec<String>,
    timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AnalysisInfo {
    scope: String,
    tool_version: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct FunctionReport {
    function_id: String,
    file: String,
    line: u32,
    metrics: Metrics,
    lrs: f64,
    band: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Metrics {
    cc: u32,
    nd: u32,
    fo: u32,
    ns: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyResults {
    failed: Vec<PolicyResult>,
    warnings: Vec<PolicyResult>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyResult {
    id: String,
    severity: String,
    message: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load and validate schema
    let schema_json = fs::read_to_string("schemas/hotspots-output.schema.json")?;
    let schema = serde_json::from_str(&schema_json)?;
    let compiled = jsonschema::JSONSchema::compile(&schema)?;

    // Load output
    let output_json = fs::read_to_string("hotspots-output.json")?;
    let output_value: serde_json::Value = serde_json::from_str(&output_json)?;

    // Validate
    if let Err(errors) = compiled.validate(&output_value) {
        eprintln!("Schema validation failed:");
        for error in errors {
            eprintln!("  {}", error);
        }
        std::process::exit(1);
    }

    // Parse into struct
    let output: HotspotsOutput = serde_json::from_str(&output_json)?;

    // Get high-risk functions
    let high_risk: Vec<_> = output.functions
        .iter()
        .filter(|f| f.band == "high" || f.band == "critical")
        .collect();

    println!("Found {} high-risk functions", high_risk.len());

    // Sort by LRS
    let mut sorted = output.functions.clone();
    sorted.sort_by(|a, b| b.lrs.partial_cmp(&a.lrs).unwrap());

    println!("\nTop 10 most complex functions:");
    for func in sorted.iter().take(10) {
        println!("{} - LRS: {:.2}", func.function_id, func.lrs);
    }

    // Check policy results
    if let Some(policy) = &output.policy_results {
        if !policy.failed.is_empty() {
            eprintln!("\nPolicy check failed!");
            for failure in &policy.failed {
                eprintln!("  {}: {}", failure.id, failure.message);
            }
            std::process::exit(1);
        }
    }

    Ok(())
}
```

## CI/CD Integration Patterns

### GitHub Actions

```yaml
- name: Run Hotspots Analysis
  run: hotspots analyze --json > hotspots-output.json

- name: Validate Output
  run: |
    npm install -g ajv-cli
    ajv validate -s schemas/hotspots-output.schema.json -d hotspots-output.json

- name: Check for High-Risk Functions
  run: |
    node -e "
    const output = require('./hotspots-output.json');
    const highRisk = output.functions.filter(f =>
      f.band === 'high' || f.band === 'critical'
    );
    if (highRisk.length > 0) {
      console.error(\`Found \${highRisk.length} high-risk functions\`);
      process.exit(1);
    }
    "
```

### GitLab CI

```yaml
hotspots:
  script:
    - hotspots analyze --json > hotspots-output.json
    - python3 scripts/validate_output.py
  artifacts:
    reports:
      codequality: hotspots-output.json
```

## AI Assistant Integration

AI assistants can use Hotspots output to:

1. **Code Review**: Identify complex functions that need attention
2. **Refactoring Suggestions**: Target high-LRS functions for simplification
3. **Test Prioritization**: Focus testing on high-complexity areas
4. **Documentation**: Generate complexity reports and visualizations

### Example: Claude with MCP

```typescript
// In MCP tool definition
tools: [
  {
    name: "analyze_complexity",
    description: "Analyze code complexity using Hotspots",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string" }
      }
    },
    async handler({ path }) {
      const output = await runHotspots(path);
      const highRisk = output.functions.filter(f =>
        f.band === 'high' || f.band === 'critical'
      );
      return { highRisk, summary: generateSummary(output) };
    }
  }
]
```

## Schema Versioning

The `schema_version` field tracks schema compatibility:

- **Version 1** (current): Initial stable schema
- Future versions will increment for breaking changes
- Tools should check `schema_version` and handle accordingly

## Additional Resources

- [JSON Schema Specification](https://json-schema.org/)
- [Hotspots GitHub Repository](https://github.com/yourusername/hotspots)
- [@hotspots/types npm package](https://www.npmjs.com/package/@hotspots/types)
- [Complexity Metrics Research](docs/metrics-research.md)
