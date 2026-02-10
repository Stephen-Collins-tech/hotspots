# Hotspots Documentation

**Find and fix the code that's actually causing problems.**

Welcome to Hotspots—the tool that answers "Which code should I refactor?" with data, not guesswork.

---

## What is Hotspots?

Hotspots analyzes your codebase to find functions that are:

- **Complex** - High cyclomatic complexity, deep nesting, lots of branching
- **Risky** - The dangerous combination that causes bugs, incidents, and slowdowns

Instead of refactoring by gut feeling, you get objective metrics that prioritize what actually matters:

```bash
hotspots analyze src/

# Results show your true priorities:
LRS   File                  Line  Function
12.4  src/api/billing.ts    142   processPlanUpgrade  # Critical - fix this first
9.8   src/auth/session.ts    67   validateSession     # High - watch closely
```

---

## Why Use Hotspots?

### The Problem You're Solving

Every codebase has messy code. But most of it doesn't matter—it's stable, rarely changes, and never causes problems.

The code that **does** matter? Functions that are both complex AND change frequently. Those are your **hotspots**—the 20% of code responsible for 80% of your:

- 🐛 Production bugs
- 🔥 Incidents and outages
- ⏱️ Feature delays
- 😤 Developer frustration

### What Hotspots Gives You

✅ **Objective priorities** - Stop arguing about what to refactor. The numbers tell you.
✅ **Confidence** - Know which files are landmines before you touch them.
✅ **Protection** - Block complexity regressions in CI/CD automatically.
✅ **Progress tracking** - Show stakeholders: "Dropped from 31 critical functions to 23."
✅ **AI assistance** - Feed hotspots to Claude/Cursor/Copilot for refactoring suggestions.

---

## Quick Navigation

### 🚀 New to Hotspots?

Start here to get up and running fast:

- **[Installation](./getting-started/installation.md)** - Install Hotspots (2 minutes)
- **[Quick Start](./getting-started/quick-start.md)** - Your first analysis (5 minutes)
- **[React Projects](./getting-started/quick-start-react.md)** - React-specific guide

### 📖 Using Hotspots

Learn the CLI, configure for your project, integrate with CI:

- **[CLI Usage](./guide/usage.md)** - All commands and options
- **[Configuration](./guide/configuration.md)** - Customize thresholds and filters
- **[CI/CD Integration](./guide/ci-integration.md)** - Jenkins, GitLab CI, CircleCI
- **[GitHub Action](./guide/github-action.md)** - Zero-config GitHub integration
- **[Output Formats](./guide/output-formats.md)** - JSON, HTML, text outputs
- **[Suppression](./guide/suppression.md)** - Handle false positives gracefully

### 🤖 Integrations

Connect Hotspots with your tools:

- **[AI Agents](./integrations/ai-agents.md)** - Claude, Cursor, Copilot workflows
- **[MCP Server](./integrations/mcp-server.md)** - Claude Desktop/Code integration

### 📚 Reference

Technical deep-dives and specifications:

- **[CLI Reference](./reference/cli.md)** - Complete command documentation
- **[Metrics](./reference/metrics.md)** - How CC, ND, FO, NS, and LRS are calculated
- **[LRS Specification](./reference/lrs-spec.md)** - Formal specification
- **[Language Support](./reference/language-support.md)** - TypeScript, JS, Go, Python, Rust, Java
- **[JSON Schema](./reference/json-schema.md)** - Output schema for tooling
- **[Limitations](./reference/limitations.md)** - Known limitations and workarounds

### 🏗️ Architecture

Understand how Hotspots works:

- **[Overview](./architecture/overview.md)** - System design and architecture
- **[Design Decisions](./architecture/design-decisions.md)** - Why we made key choices
- **[Invariants](./architecture/invariants.md)** - Guarantees and contracts
- **[Multi-Language](./architecture/multi-language.md)** - How language support works
- **[Testing](./architecture/testing.md)** - Test strategy and coverage

### 🤝 Contributing

Help make Hotspots better:

- **[Contributing Guide](./contributing/index.md)** - How to contribute
- **[Development](./contributing/development.md)** - Set up local environment
- **[Adding Languages](./contributing/adding-languages.md)** - Add support for new languages
- **[Releases](./contributing/releases.md)** - Release process

---

## Common Workflows

### 🔍 Find Your Hotspots

```bash
# Analyze your codebase
hotspots analyze src/

# Focus on critical functions
hotspots analyze src/ --min-lrs 9.0

# Get JSON for tooling
hotspots analyze src/ --format json
```

### 🚦 Enforce Quality in CI

```yaml
# .github/workflows/hotspots.yml
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    policy: critical-introduction  # Block new high-risk functions
```

### 📊 Track Progress Over Time

```bash
# Create baseline
hotspots analyze src/ --mode snapshot

# Compare with baseline
hotspots analyze src/ --mode delta --policy

# See trends
hotspots trends src/
```

### 🤖 AI-Assisted Refactoring

```bash
# Install MCP server for Claude
npm install -g @hotspots/mcp-server

# Then ask Claude:
"Find my top 10 hotspots and suggest refactoring strategies"
```

---

## Supported Languages

Hotspots supports **6 languages** with full feature parity:

| Language | Extensions | Status |
|----------|-----------|---------|
| **TypeScript** | `.ts`, `.tsx`, `.mts`, `.cts` | ✅ Full support |
| **JavaScript** | `.js`, `.jsx`, `.mjs`, `.cjs` | ✅ Full support |
| **Go** | `.go` | ✅ Full support |
| **Python** | `.py` | ✅ Full support |
| **Rust** | `.rs` | ✅ Full support |
| **Java** | `.java` | ✅ Full support |

All languages get:
- ✅ Accurate CC, ND, FO, NS metrics
- ✅ LRS calculation
- ✅ Policy enforcement
- ✅ Suppression comments
- ✅ Delta analysis
- ✅ Git history integration

See [Language Support](./reference/language-support.md) for language-specific details.

---

## Understanding LRS

**Local Risk Score (LRS)** combines four metrics into a single risk indicator:

1. **Cyclomatic Complexity (CC)** - How many paths through the code?
2. **Nesting Depth (ND)** - How deeply nested are control structures?
3. **Fan-Out (FO)** - How many functions does this call?
4. **Non-Structured Exits (NS)** - How many early returns, breaks, throws?

### Risk Bands

- **Low (< 3.0)** - Safe. Don't overthink these.
- **Moderate (3.0 - 6.0)** - Monitor. Block increases.
- **High (6.0 - 9.0)** - Risky. Refactor when touched.
- **Critical (≥ 9.0)** - Dangerous. Refactor now.

### Example

```typescript
// LRS: 12.4 (Critical)
function processPlanUpgrade(user, newPlan, paymentMethod) {
  if (!user.isActive) return false;  // NS +1
  if (user.plan === newPlan) return true;  // NS +1

  if (paymentMethod.type === "card") {  // CC +1, ND +1
    if (paymentMethod.isExpired) {  // CC +1, ND +2
      try {  // CC +1
        paymentMethod = renewPaymentMethod(user);  // FO +1
      } catch (error) {  // CC +1
        logError(error);  // FO +2
        notifyUser(user, "payment_failed");  // FO +3
        return false;  // NS +3
      }
    }
    // ... more nested logic
  }
}
```

**Why this is risky:**
- 15 decision points (CC = 15)
- 4 levels of nesting (ND = 4)
- Calls 8 functions (FO = 8)
- 3 early exits (NS = 3)

**LRS = 12.4** → Critical. Refactor before it causes an incident.

See [Metrics](./reference/metrics.md) for detailed calculation formulas.

---

## Key Features

### 🚦 Policy Enforcement

Automatically block risky changes in CI/CD:

- **Critical Introduction** - Fail if new functions exceed LRS 9.0
- **Excessive Regression** - Fail if LRS increases ≥1.0
- **Watch/Attention** - Warn about functions approaching thresholds
- **Rapid Growth** - Catch >50% complexity increases

### 🔇 Suppression Comments

Handle legacy code pragmatically:

```typescript
// hotspots-ignore: legacy payment processor, rewrite scheduled Q2 2026
function complexLegacyCode() {
  // Still appears in reports but doesn't fail CI
}
```

### 📈 Git History Analysis

Track complexity over time:

- **Snapshot mode** - Create baseline
- **Delta mode** - Compare current vs baseline
- **Trends** - See complexity evolution

### 📊 Multiple Formats

- **Text** - Terminal-friendly, color-coded
- **JSON** - Machine-readable for tooling/AI
- **HTML** - Interactive reports for stakeholders

### ⚡ Fast & Deterministic

- **Sub-second** analysis for most files
- **Byte-for-byte identical** output for identical input
- **Suitable for CI/CD** - Fast enough to run on every commit

---

## Getting Help

### Documentation Issues

Found unclear docs? Have suggestions?

- 📖 [Open an issue](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 💬 [Start a discussion](https://github.com/Stephen-Collins-tech/hotspots/discussions)

### Tool Issues

Found a bug? Feature request?

- 🐛 [Report a bug](https://github.com/Stephen-Collins-tech/hotspots/issues/new?labels=bug)
- 💡 [Request a feature](https://github.com/Stephen-Collins-tech/hotspots/issues/new?labels=enhancement)

### Community

- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 Email: [support@hotspots.dev](mailto:support@hotspots.dev)

---

## Next Steps

**New to Hotspots?**
1. [Install Hotspots](./getting-started/installation.md) (2 minutes)
2. [Run your first analysis](./getting-started/quick-start.md) (5 minutes)
3. Identify your top 10 hotspots
4. Refactor the worst offender

**Using Hotspots in a team?**
1. [Set up GitHub Action](./guide/github-action.md) (5 minutes)
2. [Configure policies](./guide/usage.md#policy-engine) (10 minutes)
3. Enforce quality gates on every PR

**Want AI-assisted refactoring?**
1. [Install MCP server](./integrations/mcp-server.md) (2 minutes)
2. [Try example workflows](./integrations/ai-agents.md) (10 minutes)
3. Let AI suggest refactorings for your hotspots

---

**Stop refactoring guesswork. Start with data.**
