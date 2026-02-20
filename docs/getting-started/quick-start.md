# Quick Start

**Find your first hotspot in 5 minutes.**

This guide walks you through your first complexity analysis and shows you exactly which code needs attention.

---

## Prerequisites

✅ Hotspots installed - [Install now](./installation.md) (2 minutes)
✅ A git repository with code (TypeScript, JavaScript, Go, Python, Rust, or Java)

**Ready?** Let's find your hotspots.

---

## Step 1: Find Your Hotspots

Navigate to your project and run:

```bash
cd your-project
hotspots analyze src/
```

**What this does:** Analyzes all supported files in `src/` and shows you the functions with highest complexity.

**Output example:**

```
Hotspots Analysis Results
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

CRITICAL (LRS ≥ 9.0) - Refactor NOW
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/auth/validateUser.ts:142  validateUser
  LRS: 12.4  CC: 15  ND: 4  FO: 8  NS: 3

src/api/billing.ts:89  processPlanUpgrade
  LRS: 10.1  CC: 12  ND: 3  FO: 6  NS: 4

HIGH (6.0 ≤ LRS < 9.0) - Watch Closely
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/db/migrations.ts:203  applySchema
  LRS: 8.1  CC: 10  ND: 2  FO: 5  NS: 2
```

### Understanding the Output

**LRS (Local Risk Score):** The overall complexity/risk score
- **Critical (≥ 9.0):** Refactor now. These cause incidents.
- **High (6.0-9.0):** Refactor when you touch them.
- **Moderate (3.0-6.0):** Monitor. Block increases.
- **Low (< 3.0):** Safe. Don't overthink these.

**Metrics breakdown:**
- **CC (Cyclomatic Complexity):** Number of decision points
- **ND (Nesting Depth):** Maximum nesting level
- **FO (Fan-Out):** Number of function calls
- **NS (Non-Structured):** Early returns, breaks, throws

---

## Step 2: Focus on Critical Functions

Filter to show only critical functions:

```bash
hotspots analyze src/ --min-lrs 9.0
```

**What you get:** Only functions with LRS ≥ 9.0 - your top priorities for refactoring.

**Pro tip:** Start with the worst offender. Refactor one critical function per sprint.

---

## Step 3: Get Machine-Readable Output

Export results as JSON for tooling or AI:

```bash
hotspots analyze src/ --format json > hotspots.json
```

**What you can do with JSON:**
- Feed to Claude/Cursor/Copilot for refactoring suggestions
- Build dashboards and charts
- Track metrics over time
- Integrate with other tools

**Example JSON output:**
```json
{
  "schema_version": 2,
  "generated_at": "2026-02-20T12:00:00Z",
  "summary": {
    "total_functions": 47,
    "by_band": { "critical": 2, "high": 5, "moderate": 18, "low": 22 }
  },
  "functions": [
    {
      "file": "src/auth/validateUser.ts",
      "name": "validateUser",
      "line": 142,
      "lrs": 12.4,
      "band": "critical",
      "driver": "high_complexity",
      "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 }
    }
  ]
}
```

---

## Step 4: Track Changes Over Time

Create a baseline snapshot:

```bash
hotspots analyze src/ --mode snapshot
```

**What this does:** Saves current complexity state to `.hotspots/snapshots/<commit-sha>.json`

**Why it matters:** You can now track complexity changes commit-to-commit.

---

## Step 5: Compare With Baseline (Delta Mode)

After making changes and committing:

```bash
git commit -am "refactor validateUser"
hotspots analyze src/ --mode delta
```

**What this shows:**

```
Delta Analysis: main...HEAD
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

IMPROVED Functions ✅
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/auth/validateUser.ts:142  validateUser
  LRS: 12.4 → 6.2 (-6.2, -50%)  ← Nice work!

REGRESSED Functions ❌
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/api/billing.ts:89  processPlanUpgrade
  LRS: 10.1 → 11.3 (+1.2, +12%)  ← Needs attention

NEW Functions (added in this change)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/auth/helpers.ts:15  checkPermissions
  LRS: 3.2 (moderate)
```

**What you learn:**
- ✅ Your refactoring reduced `validateUser` complexity by 50%
- ❌ `processPlanUpgrade` got more complex - needs review
- ℹ️ New function `checkPermissions` is moderate complexity - acceptable

---

## Step 6: Enforce Quality in CI/CD

Block risky changes before they merge:

```bash
hotspots analyze src/ --mode delta --policy
```

**What this does:**
- ✅ Exit code 0 if no policy violations → CI passes
- ❌ Exit code 1 if policies fail → CI fails, prevents merge

**Built-in policies:**

| Policy | Severity | Trigger |
|--------|----------|---------|
| **Critical Introduction** | Blocking | New function with LRS ≥ 9.0 |
| **Excessive Regression** | Blocking | Function LRS increases by ≥1.0 |
| **Watch Threshold** | Warning | Function approaching moderate (2.5-3.0) |
| **Attention Threshold** | Warning | Function approaching high (5.5-6.0) |
| **Rapid Growth** | Warning | Function complexity +50% or more |

**Example output when policies fail:**

```
Policy Evaluation Results
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

❌ BLOCKING VIOLATIONS (CI will fail)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[critical-introduction] src/api/billing.ts:89 processPlanUpgrade
  NEW function with critical complexity (LRS 10.1)
  Refactor before merging

⚠️  WARNINGS (informational)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[attention-threshold] src/auth/session.ts:45 refreshToken
  Approaching high threshold (LRS 5.8)
  Consider simplifying before it becomes problematic

RESULT: FAILED (1 blocking violation)
```

---

## Step 7: Generate Shareable Reports

Create an HTML report for your team:

```bash
hotspots analyze src/ --mode snapshot --format html
```

**What you get:** Interactive HTML report at `.hotspots/report.html` with:
- Sortable, filterable function table with driver badges
- Triage section with recommended actions per function
- Trend charts (band counts, activity risk, top-1% share) when history exists
- Shareable with stakeholders

**Open it:**
```bash
open .hotspots/report.html  # macOS
xdg-open .hotspots/report.html  # Linux
start .hotspots/report.html  # Windows
```

---

## Common Workflows

### 🎯 Find Top 10 Hotspots

```bash
hotspots analyze src/ --top 10
```

**Use case:** Weekly refactoring meetings - "What should we tackle this sprint?"

### 🔍 Analyze Single File

```bash
hotspots analyze src/auth/validateUser.ts
```

**Use case:** Before touching a file - "Is this function a landmine?"

### 📊 Export for Dashboards

```bash
hotspots analyze src/ --format json | jq '.[] | select(.lrs > 9)' > critical-functions.json
```

**Use case:** Track critical function count in Grafana/DataDog

### 🤖 Feed to AI for Refactoring

```bash
hotspots analyze src/ --format json --min-lrs 9.0 | pbcopy
# Paste into Claude: "These are my critical functions. Suggest refactoring strategies."
```

**Use case:** AI-assisted refactoring with context

---

## Next Steps

### ✅ You just learned:
- How to find your hotspots
- How to interpret LRS and metrics
- How to track changes over time
- How to enforce quality in CI/CD

### 🚀 What's next:

**For Solo Developers:**
1. Add `hotspots analyze src/` to your workflow
2. Refactor one hotspot per week
3. Track your progress with delta mode

**For Teams:**
1. [Set up GitHub Action](../guide/github-action.md) - Block risky code at merge time
2. [Configure policies](../guide/usage.md#policy-engine) - Customize thresholds for your team
3. [Add to CI/CD](../guide/ci-integration.md) - Jenkins, GitLab CI, CircleCI

**For AI Users:**
1. [AI Integration Guide](../integrations/mcp-server.md) - Claude Code and AI workflow examples
2. [Agent Examples](../integrations/ai-agents.md) - Python/TypeScript automation scripts

---

## Troubleshooting

### "No functions found"

**Problem:** Hotspots found no files to analyze.

**Solution:** Check file extensions and paths:
```bash
# Verify files exist
ls src/**/*.ts

# Try explicit path
hotspots analyze src/components/Button.tsx
```

### "Git repository required"

**Problem:** Delta mode needs git history.

**Solution:** Initialize git or use snapshot mode:
```bash
git init
# or
hotspots analyze src/ --mode snapshot  # No git required
```

### "Permission denied"

**Problem:** Hotspots binary not executable.

**Solution:**
```bash
chmod +x /usr/local/bin/hotspots
```

---

## Learn More

- **[CLI Reference](../reference/cli.md)** - All commands and options
- **[Configuration](../guide/configuration.md)** - Customize thresholds and filters
- **[Metrics Deep-Dive](../reference/metrics.md)** - How LRS is calculated
- **[Policy Engine](../guide/usage.md#policy-engine)** - Advanced policy configuration
- **[Language Support](../reference/language-support.md)** - TypeScript, JS, Go, Python, Rust, Java

---

**Need help?** [Open an issue](https://github.com/Stephen-Collins-tech/hotspots/issues) or [start a discussion](https://github.com/Stephen-Collins-tech/hotspots/discussions).

**Ready for CI/CD?** [Set up GitHub Action](../guide/github-action.md) in 5 minutes.
