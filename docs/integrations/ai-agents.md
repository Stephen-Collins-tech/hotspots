# AI Agent Examples

Example AI agent implementations and workflows using Hotspots for automated code analysis.

## Overview

Hotspots produces structured JSON output, runs deterministically, and completes in seconds — making it a natural fit for automated code review and refactoring workflows.

This guide covers practical patterns for agents using Hotspots: CI enforcement, pre-commit hooks, and LLM-assisted refactoring.

---

## Quick Start

### Claude Code (Recommended)

Claude Code can invoke Hotspots CLI commands directly in your project:

```bash
# Analyze current changes
hotspots analyze . --mode delta --format json

# Full snapshot with agent-optimized output
hotspots analyze . --mode snapshot --all-functions --format json
```

Ask Claude Code: *"Run hotspots analyze and show me which functions to refactor first."*

### Running Agents via CLI

For standalone scripts and CI agents, use the CLI directly:

```bash
# Get structured JSON for any agent to parse
hotspots analyze src/ --format json > analysis.json

# Delta analysis for PR review agents
hotspots analyze . --mode delta --policy --format json > delta.json
```

---

## Common patterns

### Get critical functions from any script

```python
import json, subprocess

result = subprocess.run(
    ['hotspots', 'analyze', 'src/', '--format', 'json'],
    capture_output=True, text=True
)
data = json.loads(result.stdout)
critical = [f for f in data['functions'] if f['band'] == 'critical']
```

### Pass hotspot context to an LLM

```python
import anthropic, json, subprocess

result = subprocess.run(
    ['hotspots', 'analyze', 'src/', '--min-lrs', '9.0', '--format', 'json'],
    capture_output=True, text=True
)
hotspots = json.loads(result.stdout)['functions']

client = anthropic.Anthropic()
response = client.messages.create(
    model="claude-sonnet-4-6",
    max_tokens=4096,
    messages=[{
        "role": "user",
        "content": f"These are my critical functions. Suggest specific refactoring strategies:\n\n{json.dumps(hotspots, indent=2)}"
    }]
)
print(response.content[0].text)
```

### PR delta check

```python
import json, subprocess, sys

result = subprocess.run(
    ['hotspots', 'analyze', '.', '--mode', 'delta', '--policy', '--format', 'json'],
    capture_output=True, text=True
)
delta = json.loads(result.stdout)
violations = delta.get('policy_results', {}).get('failed', [])

if violations:
    for v in violations:
        print(f"FAIL: {v['message']}")
    sys.exit(1)
```

---

## Common Workflows

### Pre-Commit Hook

Analyze changes before commit:

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Analyze staged changes
hotspots analyze . --mode delta --policy --format json > /tmp/hotspots-delta.json

# Check for violations
violations=$(jq '.policy_results.failed | length' /tmp/hotspots-delta.json)

if [ "$violations" -gt 0 ]; then
    echo "❌ Complexity violations detected!"
    jq -r '.policy_results.failed[] | "  - \(.message)"' /tmp/hotspots-delta.json
    echo ""
    echo "Run 'hotspots analyze . --mode delta --policy' for details"
    exit 1
fi

echo "✅ No complexity violations"
```

### Continuous Monitoring

Track complexity in CI:

```yaml
# .github/workflows/complexity.yml
name: Complexity Tracking

on: [push, pull_request]

jobs:
  track:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: Stephen-Collins-tech/hotspots-action@v1
        id: hotspots

      - name: Upload Results
        uses: actions/upload-artifact@v4
        with:
          name: complexity-report-${{ github.sha }}
          path: ${{ steps.hotspots.outputs.json-output }}
```

### AI Code Review

Integrate with AI code review service:

```python
# ai_code_review.py
def review_with_ai(delta_json):
    """Send delta to AI for review"""
    violations = delta_json['policy_results']['failed']

    if not violations:
        return "✅ No issues found"

    prompt = f"""
Review these complexity violations and provide guidance:

{json.dumps(violations, indent=2)}

For each violation:
1. Explain why it's problematic
2. Suggest specific refactoring
3. Estimate effort (S/M/L)
"""

    # Call AI service (Claude, GPT, etc.)
    return ai_service.complete(prompt)
```

---

## Best Practices

### 1. Set Appropriate Thresholds

Don't start too strict - iterate:

```json
{
  "thresholds": {
    "moderate": 5.0,   // Start conservative
    "high": 8.0,
    "critical": 10.0
  }
}
```

### 2. Focus on High-Risk Functions

Prioritize refactoring high-LRS functions:

```python
# Focus on top 10 highest-risk functions
hotspots = sorted(functions, key=lambda f: f['lrs'], reverse=True)[:10]
```

### 3. Measure Improvement

Track before/after metrics:

```python
before = analyze_function(source_before)
after = analyze_function(source_after)

improvement = before['lrs'] - after['lrs']
print(f"LRS improved by {improvement:.1f} ({improvement/before['lrs']*100:.0f}%)")
```

### 4. Automate Workflows

Use GitHub Actions, GitLab CI, or pre-commit hooks.

---

## Related Documentation

- [CI/CD Guide](../guide/ci-cd.md) - Automate in pipelines
- [Output Formats](../guide/output-formats.md) - JSON schema for parsing
- [CLI Reference](../reference/cli.md) - Command-line usage

---

## Example Repository

See [examples/ai-agents/](https://github.com/Stephen-Collins-tech/hotspots/tree/main/examples/ai-agents) for:
- Complete agent implementations
- GitHub Action workflows
- Pre-commit hook examples
- AI integration scripts

---

**Build your own AI agents with Hotspots!** 🤖
