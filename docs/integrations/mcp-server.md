# AI Integration Guide

Hotspots is designed with AI-assisted development in mind. This guide shows how to integrate Hotspots into AI workflows for code review, refactoring, and complexity-aware code generation.

## Why AI-First?

Hotspots provides **structured, machine-readable complexity metrics** that AI assistants can use to:

- **Review code changes** for complexity regressions before commits
- **Guide refactoring** by identifying high-risk functions and validating improvements
- **Generate better code** by checking complexity during generation and iterating
- **Enforce quality gates** in automated workflows

The JSON output format, deterministic analysis, and MCP integration make Hotspots a natural fit for AI-assisted development.

---

## Quick Start: Claude Integration

### Claude Desktop (MCP Server)

> **Coming Soon** — A native MCP server (`@hotspots/mcp-server`) is planned for a future release.
> It will allow Claude Desktop to call `hotspots_analyze` as a tool directly.
> Track progress: [GitHub Issues](https://github.com/Stephen-Collins-tech/hotspots/issues).

### Claude Code

When using Claude Code, you can invoke Hotspots directly via bash commands:

```bash
# Analyze current changes
hotspots analyze . --mode delta --format json

# Check specific files
hotspots analyze src/api.ts --format json
```

Claude Code can parse the JSON output and provide insights about complexity.

---

## Common AI Workflows

### 1. Pre-Commit Code Review

**Goal:** Catch complexity regressions before code is committed

**Workflow:**

```bash
# 1. Analyze changes vs parent commit
hotspots analyze . --mode delta --policies --format json > delta.json

# 2. AI reviews delta.json and provides feedback
# 3. Fix issues if needed
# 4. Re-analyze to verify improvements
# 5. Commit when complexity is acceptable
```

**AI Prompt Template:**

```
Review this complexity delta and identify any concerns:

[Paste delta.json content]

Focus on:
- New critical functions
- Functions with LRS increases > 1.0
- Band transitions to higher risk
- Policy violations

Suggest specific refactorings if needed.
```

**Example AI Response:**

```
⚠️  Found 2 complexity concerns:

1. handleRequest (src/api.ts:88)
   - Status: modified
   - LRS increased: 4.8 → 6.2 (+1.4)
   - Band transition: moderate → high
   - Violation: Excessive Risk Regression

   Recommendation: Extract validation logic into separate function

2. processData (src/utils.ts:42)
   - Status: new
   - LRS: 9.8 (critical)
   - Violation: Critical Introduction

   Recommendation: Break into smaller functions by stage (parse, transform, validate)
```

### 2. Refactoring Loop

**Goal:** Iteratively reduce complexity with AI assistance

**Workflow:**

```bash
# 1. Identify high-complexity functions
hotspots analyze . --min-lrs 9.0 --format json > targets.json

# 2. AI analyzes targets.json and suggests refactorings
# 3. Apply refactorings
# 4. Re-analyze to measure improvement
hotspots analyze . --mode delta --format json > improvement.json

# 5. Repeat until complexity is acceptable
```

**AI Prompt Template:**

```
I need to refactor these high-complexity functions:

[Paste targets.json content]

For each function:
1. Analyze why complexity is high (CC, ND, FO, NS)
2. Suggest specific refactoring strategies
3. Show example code for the refactoring

Prioritize functions with highest LRS first.
```

**Measuring Success:**

Look for negative deltas in improvement.json:
- `delta.lrs < 0` (complexity reduced)
- `band_transition.to` is lower risk
- `metrics.cc`, `metrics.nd` decreased

### 3. Complexity-Aware Code Generation

**Goal:** Generate code that meets complexity constraints from the start

**Workflow:**

```bash
# 1. AI generates code
# 2. Analyze generated code
hotspots analyze src/generated.ts --format json > analysis.json

# 3. If LRS > threshold, AI regenerates with simpler approach
# 4. Repeat until complexity is acceptable
```

**AI Prompt Template:**

```
Generate a TypeScript function that [description].

Constraints:
- LRS must be < 6.0 (moderate complexity or lower)
- Prefer multiple small functions over one large function
- Avoid deep nesting (ND ≤ 2)

After generating, I'll run:
hotspots analyze src/new-feature.ts --format json

And share the results. Iterate if LRS > 6.0.
```

**Iterative Example:**

```
Iteration 1: AI generates monolithic function → LRS 8.5 (high)
Iteration 2: AI splits into 3 functions → LRS 4.2, 3.8, 2.9 (all moderate/low) ✅
```

### 4. Automated PR Review

**Goal:** AI comments on PRs with complexity feedback

**Workflow (GitHub Actions):**

```yaml
name: AI Complexity Review

on: pull_request

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Analyze complexity
        run: |
          hotspots analyze . --mode delta --policies --format json > delta.json

      - name: AI Review
        run: |
          # Send delta.json to AI API (GPT-4, Claude, etc.)
          # AI generates review comment
          # Post comment to PR via GitHub API
```

See [examples/ai-agents/pr-reviewer.ts](../examples/ai-agents/pr-reviewer.ts) for reference implementation.

---

## AI Assistant Integration Examples

### Claude Code

Claude Code can run Hotspots CLI commands directly. Just ask:

```
"Run hotspots analyze . --mode delta --format json and explain the results"
"Check if my recent changes increased complexity"
"Find the most complex functions in src/"
```

Claude Code will execute `hotspots analyze ...` via bash, parse the JSON, and provide insights.

### GPT-4 / ChatGPT (API)

**Approach:** Run Hotspots locally, send JSON to GPT-4 API

```typescript
import OpenAI from 'openai';
import { execa } from 'execa';

const openai = new OpenAI();

// Run hotspots
const { stdout } = await execa('hotspots', [
  'analyze', '.', '--mode', 'delta', '--format', 'json'
]);

const analysis = JSON.parse(stdout);

// Send to GPT-4
const response = await openai.chat.completions.create({
  model: 'gpt-4',
  messages: [
    {
      role: 'system',
      content: 'You are a code review assistant. Analyze complexity metrics and suggest refactorings.'
    },
    {
      role: 'user',
      content: `Review this complexity analysis:\n\n${JSON.stringify(analysis, null, 2)}`
    }
  ]
});

console.log(response.choices[0].message.content);
```

### Cursor / Windsurf (Agentic IDEs)

**Approach:** Add Hotspots to your workflow via terminal commands

**Example `.cursorrules` file:**

```
When reviewing code changes:
1. Run: hotspots analyze . --mode delta --format json
2. Check for policy violations in policy_results.failed
3. Highlight any functions with LRS > 6.0
4. Suggest refactorings for critical functions

When generating new code:
1. After generation, run: hotspots analyze <file> --format json
2. If any function has LRS > 9.0, refactor immediately
3. Aim for all functions < 6.0 (moderate or lower)
```

**Usage in Cursor:**

```
User: Add a new API endpoint for user registration

Cursor: [Generates code in src/api/users.ts]

Cursor: Let me check the complexity...
[Runs: hotspots analyze src/api/users.ts --format json]

Cursor: The registerUser function has LRS 7.8 (high complexity).
I'll refactor it to extract validation and database logic.

[Refactors code]

Cursor: After refactoring:
- registerUser: LRS 3.2 (moderate)
- validateRegistration: LRS 2.1 (low)
- createUserRecord: LRS 2.8 (low)

All functions are now within acceptable complexity ranges.
```

### GitHub Copilot

**Approach:** Use Copilot Chat with manual Hotspots commands

```
User: @workspace Check complexity of my recent changes

GitHub Copilot: I'll analyze the complexity using Hotspots.

[User runs: hotspots analyze . --mode delta --format json]
[User pastes output]

GitHub Copilot: I see 3 functions with increased complexity:
1. handleSubmit in Form.tsx - LRS increased by 1.2
2. validateInput - New function with LRS 8.5 (high)
3. processFormData - Modified, now LRS 6.8

Let me suggest refactorings...
```

---

## JSON Output Reference

Hotspots produces structured JSON suitable for AI consumption. See [json-schema.md](json-schema.md) for complete documentation.

### Key Fields for AI Analysis

**Function-level metrics:**

```json
{
  "function_id": "src/api.ts::handleRequest",
  "file": "src/api.ts",
  "line": 88,
  "metrics": {
    "cc": 15,  // Decision points (branching)
    "nd": 4,   // Nesting depth
    "fo": 8,   // Function calls made
    "ns": 3    // Early returns/throws
  },
  "lrs": 11.2,
  "band": "critical"
}
```

**Delta analysis (changes):**

```json
{
  "function_id": "src/api.ts::handleRequest",
  "status": "modified",
  "before": { "lrs": 4.8, "band": "moderate" },
  "after": { "lrs": 6.2, "band": "high" },
  "delta": {
    "cc": 2,    // Added 2 decision points
    "nd": 1,    // Increased nesting by 1 level
    "lrs": 1.4  // LRS increased by 1.4
  },
  "band_transition": {
    "from": "moderate",
    "to": "high"
  }
}
```

**Policy violations:**

```json
{
  "policy_results": {
    "failed": [
      {
        "id": "critical-introduction",
        "severity": "blocking",
        "function_id": "src/api.ts::handleRequest",
        "message": "Function entered critical risk band (LRS: 9.2)"
      }
    ]
  }
}
```

---

## Best Practices

### 1. Use Deterministic Analysis

Hotspots produces **byte-for-byte identical output** for identical input. This is critical for:

- **Caching:** Cache analysis results by file hash to avoid re-analysis
- **Reproducibility:** Same code always produces same metrics
- **Testing:** Golden file tests validate AI workflows

**Example caching strategy:**

```typescript
import crypto from 'crypto';

function getCacheKey(filePath: string, content: string): string {
  const hash = crypto.createHash('sha256').update(content).digest('hex');
  return `hotspots:${filePath}:${hash}`;
}

async function analyzeCached(filePath: string): Promise<HotspotsOutput> {
  const content = await fs.readFile(filePath, 'utf-8');
  const cacheKey = getCacheKey(filePath, content);

  // Check cache first
  const cached = await cache.get(cacheKey);
  if (cached) return JSON.parse(cached);

  // Run analysis
  const { stdout } = await execa('hotspots', ['analyze', filePath, '--format', 'json']);
  const output = JSON.parse(stdout);

  // Cache result
  await cache.set(cacheKey, JSON.stringify(output));
  return output;
}
```

### 2. Incremental Analysis (Delta Mode)

For large codebases, use **delta mode** to analyze only changed functions:

```bash
# Analyze only what changed vs parent commit
hotspots analyze . --mode delta --format json
```

**Benefits:**
- Faster analysis (10-100x for large codebases)
- Focused feedback (only changed functions)
- Lower AI token usage (smaller JSON payload)

**When to use:**
- Pre-commit hooks
- PR reviews
- Continuous development workflows

**When NOT to use:**
- Initial codebase audit (use snapshot mode)
- Historical analysis (use snapshot mode with specific commits)

### 3. Rate Limiting and Batching

When integrating with AI APIs, batch analyze requests:

```typescript
// BAD: Analyze each file separately
for (const file of files) {
  await analyzeAndReview(file);  // Many API calls
}

// GOOD: Analyze all files at once
const analysis = await analyzeDirectory('src/');
await reviewAll(analysis);  // Single API call
```

**Hotspots is fast:**
- ~1000 functions/second on modern hardware
- Analyze entire medium-sized codebase in < 1 second
- No need to parallelize or batch Hotspots itself

### 4. Feedback Loops

Structure AI workflows as feedback loops:

```
1. Generate/modify code
2. Analyze complexity
3. If complexity too high:
   a. AI suggests refactoring
   b. Apply refactoring
   c. Go to step 2
4. Accept code
```

**Example implementation:**

```typescript
async function generateWithComplexityConstraint(
  prompt: string,
  maxLRS: number = 6.0,
  maxIterations: number = 3
): Promise<string> {
  for (let i = 0; i < maxIterations; i++) {
    const code = await ai.generate(prompt);

    await fs.writeFile('temp.ts', code);
    const analysis = await analyze('temp.ts');

    const maxFunctionLRS = Math.max(...analysis.functions.map(f => f.lrs));

    if (maxFunctionLRS <= maxLRS) {
      return code;  // Success!
    }

    // Provide feedback for next iteration
    prompt += `\n\nPrevious attempt had LRS ${maxFunctionLRS.toFixed(1)} (too high).
    Break into smaller functions. Target: LRS < ${maxLRS}`;
  }

  throw new Error(`Could not generate code within complexity constraint after ${maxIterations} attempts`);
}
```

### 5. Context-Aware Prompts

Provide Hotspots metrics in AI prompts for better refactoring suggestions:

```typescript
function buildRefactoringPrompt(func: FunctionReport): string {
  return `
Refactor this function to reduce complexity:

File: ${func.file}:${func.line}
Function: ${func.function_id.split('::')[1]}
Current LRS: ${func.lrs.toFixed(1)} (${func.band})

Metrics breakdown:
- Cyclomatic Complexity (CC): ${func.metrics.cc} (decision points)
- Nesting Depth (ND): ${func.metrics.nd} (max nesting level)
- Fan-Out (FO): ${func.metrics.fo} (functions called)
- Non-Structured Exits (NS): ${func.metrics.ns} (early returns/throws)

${func.metrics.nd > 3 ? '⚠️  High nesting - consider early returns or extraction' : ''}
${func.metrics.cc > 10 ? '⚠️  High branching - consider strategy pattern or lookup table' : ''}
${func.metrics.fo > 8 ? '⚠️  High fan-out - consider facade or coordinator pattern' : ''}

Target: LRS < 6.0 (moderate complexity)

Provide:
1. Specific refactoring strategy
2. Refactored code
3. Expected metric improvements
`;
}
```

---

## Troubleshooting

### "hotspots binary not found in PATH"

**Problem:** AI workflow or script can't find the hotspots binary.

**Solution:** Add hotspots to PATH:

```bash
export PATH="/usr/local/bin:$PATH"
```

Or specify the full path in your script:

```bash
/usr/local/bin/hotspots analyze . --format json
```

### "Failed to parse JSON output"

**Problem:** Hotspots output is not valid JSON

**Solution:**

Ensure you're using `--format json`:

```bash
hotspots analyze . --format json  # ✅ Valid JSON
hotspots analyze .                 # ❌ Text format
```

Check for errors in output:

```bash
hotspots analyze . --format json 2>&1 | jq .
```

### AI suggestions don't improve metrics

**Problem:** AI refactoring doesn't reduce LRS as expected

**Diagnosis:**

Re-analyze after refactoring and check delta:

```bash
hotspots analyze . --mode delta --format json
```

Look for:
- Which metrics changed (cc, nd, fo, ns)
- Whether LRS actually decreased
- Band transitions

**Common issues:**

1. **Refactoring increased FO (fan-out):** Extracted functions are counted as calls
   - Solution: This is okay! Multiple small functions > one large function

2. **LRS decreased but still high:** Need more aggressive refactoring
   - Solution: Continue iterating with tighter constraints

3. **Metrics unchanged:** Refactoring was cosmetic, not structural
   - Solution: Focus on reducing decision points, nesting, and early exits

### Large JSON payloads for AI APIs

**Problem:** Full snapshot JSON is too large for AI context window

**Solution:**

1. **Use delta mode** (only changed functions)
2. **Filter by risk band:**
   ```bash
   hotspots analyze . --min-lrs 6.0 --format json  # Only high/critical
   ```
3. **Extract only needed fields:**
   ```typescript
   const summary = {
     critical: output.functions.filter(f => f.band === 'critical').length,
     high: output.functions.filter(f => f.band === 'high').length,
     top10: getHighestRiskFunctions(output.functions, 10)
   };
   ```

---

## Advanced Workflows

### Pre-Commit Hook with AI Review

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash

# Analyze changes
hotspots analyze . --mode delta --policies --format json > /tmp/delta.json

# Check for blocking violations
if jq -e '.policy_results.failed | length > 0' /tmp/delta.json > /dev/null; then
  echo "❌ Complexity violations detected"

  # Optional: Send to AI for review and suggestions
  # curl -X POST https://api.openai.com/v1/chat/completions \
  #   -d "$(build_review_request /tmp/delta.json)"

  exit 1
fi

echo "✅ Complexity check passed"
```

### Continuous Refactoring Agent

Build an agent that continuously monitors and refactors high-complexity code:

```typescript
async function continuousRefactor() {
  while (true) {
    // Find critical functions
    const { stdout } = await execa('hotspots', [
      'analyze', '.', '--min-lrs', '9.0', '--format', 'json'
    ]);

    const output: HotspotsOutput = JSON.parse(stdout);
    const critical = filterByRiskBand(output.functions, 'critical');

    if (critical.length === 0) {
      console.log('✅ No critical functions');
      break;
    }

    // Refactor highest-risk function
    const target = getHighestRiskFunctions(critical, 1)[0];
    const refactored = await ai.refactor(target);

    // Apply and verify
    await applyRefactoring(target, refactored);

    // Wait before next iteration
    await sleep(5000);
  }
}
```

### Multi-Repository Analysis

Analyze complexity across multiple repositories:

```bash
for repo in project-a project-b project-c; do
  cd $repo
  hotspots analyze . --format json > ../analysis/${repo}.json
  cd ..
done

# Aggregate and send to AI for cross-repo insights
```

---

## Example Prompts

### Code Review

```
Review this complexity analysis for my pull request:

[paste delta.json]

Identify:
1. Any blocking violations (critical introductions, excessive regressions)
2. Functions that should be refactored before merge
3. Overall code quality trend (improving/degrading)
```

### Refactoring Planning

```
I need to reduce complexity in these functions:

[paste functions from analysis.json where lrs > 9.0]

For each function:
1. Explain why complexity is high (which metrics contribute most)
2. Suggest specific refactoring techniques
3. Estimate expected LRS after refactoring
```

### Architecture Review

```
Analyze the complexity distribution of my codebase:

[paste full snapshot.json]

Provide insights on:
1. Overall health (% critical, high, moderate, low)
2. Hotspot files (files with most high-risk functions)
3. Recommended refactoring priorities
4. Architectural improvements to reduce complexity
```

---

## See Also

- [JSON Schema Documentation](../reference/json-schema.md) - Complete JSON format reference
- [Agent Examples](./ai-agents.md) - Example AI agent implementations
- [GitHub Action Guide](../guide/github-action.md) - CI/CD integration
- [CLI Reference](../reference/cli.md) - All commands and flags
