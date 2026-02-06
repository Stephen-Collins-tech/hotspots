# @hotspots/types

TypeScript types and JSON Schema definitions for Hotspots complexity analysis output.

## Installation

```bash
npm install @hotspots/types
```

## Usage

```typescript
import type { HotspotsOutput, FunctionReport, PolicyResult } from '@hotspots/types';
import { filterByRiskBand, getHighestRiskFunctions } from '@hotspots/types';

// Parse Hotspots JSON output
const output: HotspotsOutput = JSON.parse(hotspotsJsonOutput);

// Filter functions by risk level
const highRiskFunctions = filterByRiskBand(output.functions, 'high');

// Get top 10 most complex functions
const top10 = getHighestRiskFunctions(output.functions, 10);

// Check if policy passed
if (output.policy_results && !policyPassed(output.policy_results)) {
  console.error('Policy check failed!');
  process.exit(1);
}
```

## Type Definitions

### Core Types

- **`HotspotsOutput`**: Complete analysis output from Hotspots
- **`FunctionReport`**: Complexity analysis for a single function
- **`Metrics`**: Raw complexity metrics (CC, ND, FO, NS)
- **`PolicyResult`**: A single policy violation or warning
- **`PolicyResults`**: Collection of policy check results

### Type Aliases

- **`RiskBand`**: `"low" | "moderate" | "high" | "critical"`
- **`Severity`**: `"error" | "warning" | "info"`
- **`PolicyId`**: Union of all policy identifiers

### Helper Functions

- **`filterByRiskBand(functions, band)`**: Filter functions by risk level
- **`filterBySeverity(results, severity)`**: Filter policy results by severity
- **`getHighestRiskFunctions(functions, n)`**: Get N functions with highest LRS
- **`getFunctionsAboveThreshold(functions, threshold)`**: Get functions exceeding LRS threshold
- **`policyPassed(results)`**: Check if policy check passed

### Type Guards

- **`isHotspotsOutput(obj)`**: Check if object is valid HotspotsOutput
- **`isFunctionReport(obj)`**: Check if object is valid FunctionReport
- **`isPolicyResult(obj)`**: Check if object is valid PolicyResult

## Metrics Explained

### CC - Cyclomatic Complexity
Number of linearly independent paths through the code (decision points + 1). Higher values indicate more branching logic.

### ND - Nesting Depth
Maximum level of nested control structures (if/while/for/try). Deeply nested code is harder to understand.

### FO - Fan-Out
Number of distinct functions or methods called. High fan-out suggests many dependencies.

### NS - Non-Structured Exits
Number of early returns, throws, breaks, and continues. Multiple exit points increase complexity.

### LRS - Logarithmic Risk Score
Composite metric combining all raw metrics with logarithmic scaling. Higher scores indicate higher complexity and maintenance risk.

## Risk Bands

- **Low** (LRS < 3.0): Simple, easy to maintain
- **Moderate** (3.0 ≤ LRS < 6.0): Moderate complexity, acceptable
- **High** (6.0 ≤ LRS < 9.0): Complex, consider refactoring
- **Critical** (LRS ≥ 9.0): Very complex, refactor recommended

## JSON Schema

JSON Schema definitions are available in the main Hotspots repository at `/schemas/`:

- `hotspots-output.schema.json`: Complete output schema
- `function-report.schema.json`: Function analysis schema
- `metrics.schema.json`: Metrics schema
- `policy-result.schema.json`: Policy result schema

## License

MIT
