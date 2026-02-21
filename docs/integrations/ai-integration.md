# AI Integration

Hotspots is designed with AI-assisted development in mind. Its structured JSON output, deterministic analysis, and fast execution make it a natural fit for AI code review and refactoring workflows.

## Claude Code / CLI Workflows

Claude Code can invoke Hotspots CLI commands directly in your project — no setup required.

### Analyze current changes

```bash
# Ask Claude Code to run this and explain results
hotspots analyze . --mode delta --format json

# Or with agent-optimized output (quadrant buckets + action text)
hotspots analyze . --mode snapshot --all-functions --format json
```

**Example prompts for Claude Code:**
- *"Run hotspots analyze and show me which functions to refactor first."*
- *"Check if my recent changes increased complexity."*
- *"Find the most complex functions in src/ and suggest refactorings."*

Claude Code will execute the command, parse the JSON, and provide actionable insights.

---

## Common AI Workflows

### 1. Pre-Commit Code Review

Catch complexity regressions before code is committed.

```bash
# Analyze changes vs parent commit
hotspots analyze . --mode delta --policy --format json > delta.json

# Review delta.json with an AI assistant, then fix and re-analyze
```

**AI prompt template:**
```
Review this complexity delta and identify concerns:

[paste delta.json content]

Focus on:
- New critical functions (LRS ≥ 9.0)
- Functions where LRS increased > 1.0
- Band transitions to higher risk
- Policy violations

Suggest specific refactorings for each issue.
```

### 2. Refactoring Loop

Iteratively reduce complexity with AI assistance.

```bash
# 1. Identify high-complexity targets
hotspots analyze . --min-lrs 9.0 --format json > targets.json

# 2. AI analyzes and suggests refactorings
# 3. Apply refactorings
# 4. Measure improvement
hotspots analyze . --mode delta --format json > improvement.json

# 5. Repeat until complexity is acceptable
```

**Measuring success:** Look for `delta.lrs < 0`, lower `band_transition.to`, and decreased `metrics.cc` / `metrics.nd`.

### 3. Complexity-Aware Code Generation

Generate code that meets complexity constraints from the start.

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

**Iterative example:**
```
Iteration 1: Monolithic function → LRS 8.5 (high)
Iteration 2: Split into 3 functions → LRS 4.2, 3.8, 2.9 ✅
```

### 4. Automated PR Review (GitHub Actions)

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
        run: hotspots analyze . --mode delta --policy --format json > delta.json

      - name: AI Review
        run: |
          # Send delta.json to AI API and post comment to PR
          # See examples/ai-agents/ for reference implementation
```

---

## MCP Server

> **Coming Soon** — A native MCP server (`@hotspots/mcp-server`) is planned for a future release. It will allow Claude Desktop to call `hotspots_analyze` as a tool directly, without any manual command execution.
>
> Track progress: [GitHub Issues](https://github.com/Stephen-Collins-tech/hotspots/issues)

Once available, Claude Desktop configuration will look like:
```json
{
  "mcpServers": {
    "hotspots": {
      "command": "npx",
      "args": ["@hotspots/mcp-server"]
    }
  }
}
```

---

## Agent Examples

### Refactoring Assistant (Python)

```python
import json
import subprocess

class RefactoringAssistant:
    def __init__(self, threshold=8.0):
        self.threshold = threshold

    def analyze(self, path):
        result = subprocess.run(
            ['hotspots', 'analyze', path, '--format', 'json'],
            capture_output=True, text=True
        )
        data = json.loads(result.stdout)
        return [fn for fn in data['functions'] if fn['lrs'] >= self.threshold]

    def suggest_refactorings(self, function):
        suggestions = []
        if function['metrics']['cc'] > 10:
            suggestions.append('High CC — extract sub-functions to reduce branching')
        if function['metrics']['nd'] > 4:
            suggestions.append('Deep nesting — use early returns or guard clauses')
        if function['metrics']['fo'] > 8:
            suggestions.append('High fan-out — consider a facade or coordinator pattern')
        return suggestions

assistant = RefactoringAssistant(threshold=8.0)
targets = assistant.analyze('src/')

for fn in targets:
    print(f"\n{fn['function_id']} (LRS: {fn['lrs']:.1f}, band: {fn['band']})")
    for s in assistant.suggest_refactorings(fn):
        print(f"  - {s}")
```

### AI-Guided Refactoring (TypeScript / Claude API)

```typescript
import Anthropic from '@anthropic-ai/sdk';
import { execa } from 'execa';

const client = new Anthropic();

async function suggestRefactoring(filePath: string, functionName: string) {
  const { stdout } = await execa('hotspots', ['analyze', filePath, '--format', 'json']);
  const output = JSON.parse(stdout);
  const fn = output.functions.find((f: any) => f.function_id.endsWith(functionName));

  if (!fn) return 'Function not found';

  const response = await client.messages.create({
    model: 'claude-sonnet-4-6',
    max_tokens: 2048,
    messages: [{
      role: 'user',
      content: `Refactor this function to reduce complexity:

Function: ${fn.function_id}
LRS: ${fn.lrs} (${fn.band})
CC: ${fn.metrics.cc}, ND: ${fn.metrics.nd}, FO: ${fn.metrics.fo}, NS: ${fn.metrics.ns}

Target: LRS < 6.0. Provide specific refactoring with before/after code.`
    }]
  });

  return response.content[0].type === 'text' ? response.content[0].text : '';
}
```

### Pre-Commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

hotspots analyze . --mode delta --policy --format json > /tmp/delta.json

violations=$(jq '.policy_results.failed | length' /tmp/delta.json)

if [ "$violations" -gt 0 ]; then
    echo "Complexity violations detected:"
    jq -r '.policy_results.failed[] | "  - \(.message)"' /tmp/delta.json
    exit 1
fi

echo "Complexity check passed"
```

---

## JSON Output for AI Consumption

Hotspots produces structured JSON suitable for AI consumption. Key fields:

```json
{
  "function_id": "src/api.ts::handleRequest",
  "file": "src/api.ts",
  "line": 88,
  "metrics": {
    "cc": 15,
    "nd": 4,
    "fo": 8,
    "ns": 3
  },
  "lrs": 11.2,
  "band": "critical",
  "driver": "high_complexity",
  "quadrant": "fire"
}
```

**Tips for AI workflows:**
- Use `--mode delta` to reduce payload size (only changed functions)
- Use `--min-lrs 6.0` to focus on high/critical functions
- Use `--all-functions` for the agent-optimized v3 schema with quadrant buckets (`fire`/`debt`/`watch`/`ok`) and per-function `action` text

See [Output Formats](../guide/output-formats) for complete JSON schema documentation.

---

## Best Practices

1. **Use deterministic analysis** — Hotspots produces byte-for-byte identical output for identical input. Cache results by file hash.
2. **Prefer delta mode** — Faster and lower AI token usage; only changed functions sent.
3. **Batch analysis** — Analyze entire directory at once rather than file-by-file.
4. **Close the feedback loop** — Re-analyze after refactoring to verify improvement.
5. **Include metric context in prompts** — Specify which metrics (CC, ND, FO, NS) are high for targeted suggestions.
