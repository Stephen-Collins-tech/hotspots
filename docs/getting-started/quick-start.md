# Quick Start

**Find your first hotspot in 5 minutes.**

## Prerequisites

- Hotspots installed — [Install now](./installation.md)
- A git repository with code (TypeScript, JavaScript, Go, Python, Rust, or Java)

---

## Step 1: Find Your Hotspots

Navigate to your project and run:

```bash
cd your-project
hotspots analyze src/
```

**Output example:**

```
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
- **High (6.0–9.0):** Refactor when you touch them.
- **Moderate (3.0–6.0):** Monitor. Block increases.
- **Low (< 3.0):** Safe. Don't overthink these.

**Metrics:** CC = decision points, ND = nesting depth, FO = function calls, NS = early exits/throws.

---

## Step 2: Focus on Critical Functions

```bash
hotspots analyze src/ --min-lrs 9.0
```

Start with the worst offender. Refactor one critical function per sprint.

---

## Step 3: Get Machine-Readable Output

Export results as JSON for tooling or AI:

```bash
hotspots analyze src/ --format json > hotspots.json
```

```json
{
  "schema_version": 2,
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

## Step 4: Track Changes Over Time (Snapshot Mode)

```bash
hotspots analyze src/ --mode snapshot
```

This saves current complexity to `.hotspots/snapshots/<commit-sha>.json`. You can now track complexity changes commit-to-commit.

---

## Step 5: Compare With Baseline (Delta Mode)

```bash
git commit -am "refactor validateUser"
hotspots analyze src/ --mode delta
```

**Output:**
```
IMPROVED Functions ✅
src/auth/validateUser.ts:142  validateUser
  LRS: 12.4 → 6.2 (-6.2, -50%)

REGRESSED Functions ❌
src/api/billing.ts:89  processPlanUpgrade
  LRS: 10.1 → 11.3 (+1.2, +12%)
```

---

## Step 6: Enforce Quality in CI

```bash
hotspots analyze src/ --mode delta --policy
```

- Exit code 0 if no policy violations → CI passes
- Exit code 1 if policies fail → CI fails, prevents merge

**Built-in policies:**

| Policy | Severity | Trigger |
|--------|----------|---------|
| **Critical Introduction** | Blocking | New function with LRS ≥ 9.0 |
| **Excessive Regression** | Blocking | Function LRS increases by ≥ 1.0 |
| **Watch Threshold** | Warning | Function approaching moderate (2.5–3.0) |
| **Attention Threshold** | Warning | Function approaching high (5.5–6.0) |
| **Rapid Growth** | Warning | Function complexity +50% or more |

---

## Step 7: Generate Shareable Reports

```bash
hotspots analyze src/ --mode snapshot --format html
open .hotspots/report.html
```

The HTML report includes a sortable function table with driver badges, triage action recommendations, and trend charts (when ≥2 prior snapshots exist).

---

## Common Workflows

### Find Top 10 Hotspots

```bash
hotspots analyze src/ --top 10
```

### Analyze Single File

```bash
hotspots analyze src/auth/validateUser.ts
```

### Feed to AI for Refactoring

```bash
hotspots analyze src/ --format json --min-lrs 9.0 | pbcopy
# Paste into Claude: "These are my critical functions. Suggest refactoring strategies."
```

---

## React / JSX Projects

JSX and TSX are fully supported. `.tsx` and `.jsx` files are analyzed with the same accuracy as plain TypeScript/JavaScript.

```bash
# Analyze React component tree
hotspots analyze src/components/ --format json
```

Hotspots treats JSX elements as structured output — simple markup doesn't inflate complexity. Control flow inside JSX expressions (ternaries, `&&` conditionals, `map` callbacks) is counted correctly.

---

## Next Steps

**For solo developers:**
1. Add `hotspots analyze src/` to your workflow
2. Refactor one hotspot per week
3. Track your progress with delta mode

**For teams:**
1. [Set up CI/CD](../guide/ci-cd) — block risky code at merge time
2. [Configure policies](../guide/usage#policy-engine) — customize thresholds for your team

**For AI users:**
1. [AI Integration Guide](../integrations/ai-integration) — Claude Code and AI workflow examples

---

## Troubleshooting

### "No functions found"

Check file extensions and paths:
```bash
ls src/**/*.ts
hotspots analyze src/components/Button.tsx
```

### "Git repository required"

Delta mode needs git history. Initialize git or use snapshot mode:
```bash
git init
# or
hotspots analyze src/ --mode snapshot
```

### "Permission denied"

```bash
chmod +x /usr/local/bin/hotspots
```

---

**Need help?** [Open an issue](https://github.com/Stephen-Collins-tech/hotspots/issues) or [start a discussion](https://github.com/Stephen-Collins-tech/hotspots/discussions).
