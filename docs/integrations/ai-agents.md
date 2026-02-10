# AI Agent Examples

Example AI agent implementations and workflows using Hotspots for automated code analysis.

## Overview

Hotspots is designed for AI-assisted development with:
- 🤖 **Structured JSON output** - Machine-readable complexity metrics
- 🔧 **MCP server integration** - Direct tool access in Claude Desktop/Code
- 🎯 **Deterministic analysis** - Consistent results for AI reasoning
- ⚡ **Fast execution** - Suitable for iterative AI workflows

This guide provides practical examples of AI agents using Hotspots for code review, refactoring, and complexity monitoring.

---

## Quick Start

### Claude Desktop (Recommended)

1. **Install MCP server:**
   ```bash
   npm install -g @hotspots/mcp-server
   ```

2. **Configure Claude Desktop:**

   Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:
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

3. **Restart Claude Desktop**

4. **Test it:**
   ```
   User: Analyze the complexity of my src/ directory with Hotspots

   Claude: I'll analyze your code using Hotspots.
   [Uses hotspots_analyze tool]

   Analysis complete! Found 47 functions:
   - Critical risk: 2 functions
   - High risk: 5 functions
   - Moderate risk: 12 functions
   - Low risk: 28 functions

   The critical functions are:
   - processPayment (src/payment.ts:120) - LRS 11.2
   - validateOrder (src/orders.ts:89) - LRS 9.8

   Would you like me to help refactor these?
   ```

See [MCP Server Guide](./mcp-server.md) for complete documentation.

---

## Example Agents

### 1. Refactoring Assistant

**Purpose:** Identifies high-complexity functions and suggests refactoring strategies.

#### Implementation

```python
# refactoring_assistant.py
import json
import subprocess

class RefactoringAssistant:
    def __init__(self, threshold=8.0):
        self.threshold = threshold

    def analyze(self, path):
        """Run Hotspots analysis and return high-complexity functions"""
        result = subprocess.run(
            ['hotspots', 'analyze', path, '--format', 'json'],
            capture_output=True,
            text=True
        )

        data = json.loads(result.stdout)
        return [
            fn for fn in data['functions']
            if fn['lrs'] >= self.threshold
        ]

    def suggest_refactorings(self, function):
        """Generate refactoring suggestions based on metrics"""
        suggestions = []

        if function['metrics']['cc'] > 10:
            suggestions.append({
                'type': 'extract_functions',
                'reason': f"High cyclomatic complexity ({function['metrics']['cc']})",
                'action': 'Break down into smaller functions'
            })

        if function['metrics']['nd'] > 4:
            suggestions.append({
                'type': 'reduce_nesting',
                'reason': f"Deep nesting ({function['metrics']['nd']} levels)",
                'action': 'Use early returns or extract nested logic'
            })

        if function['metrics']['fo'] > 8:
            suggestions.append({
                'type': 'reduce_dependencies',
                'reason': f"High fan-out ({function['metrics']['fo']} calls)",
                'action': 'Consider dependency injection or facade pattern'
            })

        return suggestions

# Usage
assistant = RefactoringAssistant(threshold=8.0)
hotspots = assistant.analyze('src/')

for fn in hotspots:
    print(f"\n{fn['function_id']} (LRS: {fn['lrs']})")
    suggestions = assistant.suggest_refactorings(fn)
    for s in suggestions:
        print(f"  - {s['action']} ({s['reason']})")
```

#### Example Output

```
src/payment.ts::processPayment (LRS: 11.2)
  - Break down into smaller functions (High cyclomatic complexity (15))
  - Use early returns or extract nested logic (Deep nesting (5 levels))

src/orders.ts::validateOrder (LRS: 9.8)
  - Break down into smaller functions (High cyclomatic complexity (12))
  - Consider dependency injection or facade pattern (High fan-out (9 calls))
```

---

### 2. Code Review Bot

**Purpose:** Automated PR comments with complexity analysis and recommendations.

#### Implementation

```python
# code_review_bot.py
import json
import subprocess
import os

class CodeReviewBot:
    def __init__(self, pr_number):
        self.pr_number = pr_number

    def analyze_pr(self):
        """Run delta analysis on PR changes"""
        result = subprocess.run(
            ['hotspots', 'analyze', '.', '--mode', 'delta', '--policy', '--format', 'json'],
            capture_output=True,
            text=True
        )

        return json.loads(result.stdout)

    def generate_comment(self, delta):
        """Generate PR comment markdown"""
        violations = delta.get('policy_results', {}).get('failed', [])
        warnings = delta.get('policy_results', {}).get('warnings', [])

        comment = "# 🔍 Complexity Analysis\n\n"

        if violations:
            comment += "## ❌ Blocking Violations\n\n"
            comment += "| Function | File | LRS | Issue |\n"
            comment += "|----------|------|-----|-------|\n"

            for v in violations:
                fn_id = v['function_id']
                fn = self._find_function(delta['deltas'], fn_id)
                comment += f"| `{fn['function_id']}` | {fn['file']}:{fn['line']} | {fn['after']['lrs']} | {v['message']} |\n"

        if warnings:
            comment += "\n## ⚠️ Warnings\n\n"
            for w in warnings:
                comment += f"- {w['message']}\n"

        if not violations and not warnings:
            comment += "✅ No complexity issues detected!\n"

        return comment

    def post_comment(self, comment):
        """Post comment to GitHub PR"""
        subprocess.run([
            'gh', 'pr', 'comment', str(self.pr_number),
            '--body', comment
        ])

# Usage in GitHub Action
if __name__ == '__main__':
    pr_number = os.environ.get('GITHUB_PR_NUMBER')
    bot = CodeReviewBot(pr_number)

    delta = bot.analyze_pr()
    comment = bot.generate_comment(delta)
    bot.post_comment(comment)
```

#### GitHub Action Integration

`.github/workflows/code-review-bot.yml`:
```yaml
name: Code Review Bot

on: [pull_request]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: Stephen-Collins-tech/hotspots-action@v1
        id: hotspots
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: Run Review Bot
        env:
          GITHUB_PR_NUMBER: ${{ github.event.number }}
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: python .github/scripts/code_review_bot.py
```

---

### 3. Complexity Monitor

**Purpose:** Track complexity trends over time and alert on degradation.

#### Implementation

```python
# complexity_monitor.py
import json
import subprocess
from datetime import datetime
import sqlite3

class ComplexityMonitor:
    def __init__(self, db_path='complexity_history.db'):
        self.db_path = db_path
        self._init_db()

    def _init_db(self):
        """Initialize SQLite database for tracking history"""
        conn = sqlite3.connect(self.db_path)
        conn.execute('''
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                timestamp TEXT,
                commit_sha TEXT,
                total_functions INTEGER,
                average_lrs REAL,
                critical_count INTEGER,
                high_count INTEGER
            )
        ''')
        conn.commit()
        conn.close()

    def take_snapshot(self):
        """Analyze current codebase and store snapshot"""
        result = subprocess.run(
            ['hotspots', 'analyze', '.', '--format', 'json'],
            capture_output=True,
            text=True
        )

        data = json.loads(result.stdout)

        # Calculate aggregates
        functions = data['functions']
        total = len(functions)
        avg_lrs = sum(f['lrs'] for f in functions) / total if total > 0 else 0
        critical = len([f for f in functions if f['band'] == 'critical'])
        high = len([f for f in functions if f['band'] == 'high'])

        # Get current commit
        commit_sha = subprocess.run(
            ['git', 'rev-parse', 'HEAD'],
            capture_output=True,
            text=True
        ).stdout.strip()

        # Store snapshot
        conn = sqlite3.connect(self.db_path)
        conn.execute('''
            INSERT INTO snapshots (timestamp, commit_sha, total_functions, average_lrs, critical_count, high_count)
            VALUES (?, ?, ?, ?, ?, ?)
        ''', (datetime.now().isoformat(), commit_sha, total, avg_lrs, critical, high))
        conn.commit()
        conn.close()

        return {
            'total': total,
            'avg_lrs': avg_lrs,
            'critical': critical,
            'high': high
        }

    def check_trends(self, window=10):
        """Check if complexity is trending upward"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.execute('''
            SELECT average_lrs FROM snapshots
            ORDER BY timestamp DESC
            LIMIT ?
        ''', (window,))

        lrs_values = [row[0] for row in cursor.fetchall()]
        conn.close()

        if len(lrs_values) < 2:
            return None

        # Simple trend: compare recent average to older average
        recent_avg = sum(lrs_values[:3]) / 3
        older_avg = sum(lrs_values[-3:]) / 3

        return {
            'trend': 'up' if recent_avg > older_avg else 'down',
            'delta': recent_avg - older_avg,
            'recent_avg': recent_avg,
            'older_avg': older_avg
        }

# Usage in CI
monitor = ComplexityMonitor()
current = monitor.take_snapshot()
trend = monitor.check_trends(window=10)

if trend and trend['trend'] == 'up' and trend['delta'] > 0.5:
    print(f"⚠️ Complexity trending upward! Recent avg LRS: {trend['recent_avg']:.2f} (was {trend['older_avg']:.2f})")
else:
    print(f"✅ Complexity stable or improving. Current avg LRS: {current['avg_lrs']:.2f}")
```

#### Scheduled Monitoring

`.github/workflows/complexity-monitor.yml`:
```yaml
name: Complexity Monitor

on:
  schedule:
    - cron: '0 0 * * *'  # Daily at midnight
  push:
    branches: [main]

jobs:
  monitor:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: Stephen-Collins-tech/hotspots-action@v1

      - name: Run Complexity Monitor
        run: python scripts/complexity_monitor.py

      - name: Upload History
        uses: actions/upload-artifact@v4
        with:
          name: complexity-history
          path: complexity_history.db
```

---

### 4. AI-Guided Refactoring (LLM)

**Purpose:** Use LLM (Claude, GPT, etc.) to generate refactoring suggestions.

#### Implementation

```python
# ai_refactoring.py
import json
import subprocess
import anthropic

class AIRefactoringAgent:
    def __init__(self, api_key):
        self.client = anthropic.Anthropic(api_key=api_key)

    def analyze_function(self, file_path, function_name):
        """Get complexity metrics for a specific function"""
        result = subprocess.run(
            ['hotspots', 'analyze', file_path, '--format', 'json'],
            capture_output=True,
            text=True
        )

        data = json.loads(result.stdout)
        functions = [f for f in data['functions'] if f['function_id'].endswith(function_name)]

        return functions[0] if functions else None

    def get_source_code(self, file_path, line_start, line_end):
        """Extract source code for function"""
        with open(file_path, 'r') as f:
            lines = f.readlines()
            return ''.join(lines[line_start-1:line_end])

    def suggest_refactoring(self, file_path, function_name):
        """Generate refactoring suggestions using Claude"""
        fn = self.analyze_function(file_path, function_name)
        if not fn:
            return "Function not found"

        # Estimate function end line (simplified)
        source = self.get_source_code(file_path, fn['line'], fn['line'] + 50)

        prompt = f"""
Analyze this function's complexity and suggest refactorings:

**Function:** {function_name}
**File:** {file_path}:{fn['line']}
**Complexity Metrics:**
- Cyclomatic Complexity (CC): {fn['metrics']['cc']}
- Nesting Depth (ND): {fn['metrics']['nd']}
- Fan-Out (FO): {fn['metrics']['fo']}
- Non-Structured Exits (NS): {fn['metrics']['ns']}
- Leverage Risk Score (LRS): {fn['lrs']}
- Risk Band: {fn['band']}

**Source Code:**
```typescript
{source}
```

Provide specific refactoring suggestions to reduce complexity. Focus on:
1. Reducing cyclomatic complexity (break down complex conditionals)
2. Reducing nesting depth (use early returns, extract methods)
3. Reducing fan-out (minimize dependencies)

For each suggestion, show before/after code snippets.
"""

        response = self.client.messages.create(
            model="claude-sonnet-4-5",
            max_tokens=4096,
            messages=[{"role": "user", "content": prompt}]
        )

        return response.content[0].text

# Usage
agent = AIRefactoringAgent(api_key="your-api-key")
suggestions = agent.suggest_refactoring("src/api.ts", "processRequest")
print(suggestions)
```

#### Example Output

```markdown
## Refactoring Suggestions for processRequest

### 1. Extract Validation Logic (Reduces CC by 4)

**Before:**
```typescript
function processRequest(req) {
    if (!req.user) throw new Error("No user");
    if (!req.user.id) throw new Error("No user ID");
    if (!req.data) throw new Error("No data");
    // ... more logic
}
```

**After:**
```typescript
function validateRequest(req) {
    if (!req.user) throw new Error("No user");
    if (!req.user.id) throw new Error("No user ID");
    if (!req.data) throw new Error("No data");
}

function processRequest(req) {
    validateRequest(req);
    // ... rest of logic
}
```

### 2. Use Early Returns (Reduces ND by 2)

**Before:**
```typescript
if (condition) {
    if (anotherCondition) {
        // nested logic
    }
}
```

**After:**
```typescript
if (!condition) return;
if (!anotherCondition) return;
// logic at same level
```

**Expected Improvement:** LRS 11.2 → ~7.5 (moderate risk)
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

- [MCP Server Guide](./mcp-server.md) - Claude Desktop integration
- [CI/CD Integration](../guide/ci-integration.md) - Automate in pipelines
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
