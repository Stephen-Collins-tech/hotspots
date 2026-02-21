# Hotspots

**Website:** https://hotspots.dev &nbsp;|&nbsp; **Docs:** https://docs.hotspots.dev &nbsp;|&nbsp; **Install:** `curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh`

**Find the code that's actually causing problems.**

Your codebase has thousands of functions. Some are messy but never break. Others are complex AND change constantly—those are your **hotspots**, the 20% of code causing 80% of your bugs, incidents, and slowdowns.

Stop refactoring code that doesn't matter. Focus on what's hurting you right now.

---

## The Problem

You know your codebase has tech debt. But which code should you actually refactor?

❌ **Refactor by gut feeling** → Waste weeks on code that rarely causes issues
❌ **Refactor everything** → Impossible, and you'll rewrite stable code that doesn't need touching
❌ **Refactor nothing** → Tech debt compounds until "fix this bug" becomes "rewrite everything"

**The real question:** Which functions are both complex AND frequently changed?

Those are the functions causing production incidents, slowing down features, and burning out your team.

---

## The Solution

Hotspots analyzes your codebase and git history to find functions that are:

1. **Complex** - High cyclomatic complexity, deep nesting, lots of branching
2. **Volatile** - Changed frequently in recent commits
3. **Risky** - The dangerous combination of both

Instead of guessing what to refactor, you get a prioritized list:

```bash
hotspots analyze src/

# Output:
LRS   File                  Line  Function            Risk
12.4  src/api/billing.ts    142   processPlanUpgrade  Critical
9.8   src/auth/session.ts    67   validateSession     High
8.1   src/db/migrations.ts  203   applySchema         High
```

Now you know exactly where to focus.

---

## What You Get

### ✅ Refactor What Actually Matters

Stop wasting time on code that "looks messy" but never causes problems. Focus on the 20% of functions responsible for 80% of your incidents.

### ✅ Block Complexity Regressions in CI

Catch risky changes before they merge:

```bash
# Run in CI with policy checks
hotspots analyze src/ --mode delta --policy
# Exit code 1 if policies fail → CI fails
```

Your CI fails if someone introduces high-risk code. No manual review needed.

> **GitHub Action coming soon.** A native `hotspots-action` for GitHub Actions is not yet available. Use the CLI directly in your workflows in the meantime.

### ✅ Ship with Confidence, Not Crossed Fingers

Know which files are landmines before you touch them. See complexity trends over time. Make informed decisions about refactoring vs rewriting vs leaving it alone.

### ✅ Get AI-Assisted Refactoring

Hotspots integrates with Claude, Cursor, and GitHub Copilot. Point your AI at the hottest functions and get refactoring suggestions that actually improve your codebase.

```bash
# Install MCP server for Claude Desktop
npm install -g @hotspots/mcp-server

# Then ask Claude:
"Analyze my codebase for hotspots and suggest refactoring plans"
```

---

## Quick Start

### 1. Install

**macOS/Linux:**
```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-$(uname -s)-$(uname -m) -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/
```

**GitHub Action:** Coming soon. Use the CLI directly in your workflows for now.

### 2. Analyze Your Code

```bash
# Find your hotspots
hotspots analyze src/

# Filter to critical functions only
hotspots analyze src/ --min-lrs 9.0

# Get per-function explanations with driver labels
hotspots analyze . --mode snapshot --explain --top 10

# Get JSON for tooling/AI
hotspots analyze src/ --format json

# Stream JSONL for pipeline processing (requires --mode)
hotspots analyze src/ --mode snapshot --format jsonl --no-persist

# Compare with previous commit (delta mode)
hotspots analyze src/ --mode delta --policy
```

### 3. Act on Results

**Critical functions (LRS ≥ 9.0):** Refactor now. These are your top priority.
**High functions (LRS 6.0-9.0):** Watch closely. Refactor before they become critical.
**Moderate functions (LRS 3.0-6.0):** Keep an eye on them. Block complexity increases.
**Low functions (LRS < 3.0):** You're good. Don't overthink these.

---

## Supported Languages

- **TypeScript** - `.ts`, `.tsx`, `.mts`, `.cts`
- **JavaScript** - `.js`, `.jsx`, `.mjs`, `.cjs`
- **Go** - `.go`
- **Python** - `.py`
- **Rust** - `.rs`
- **Java** - `.java`

Full language parity across all metrics and features. See [docs/reference/language-support.md](docs/reference/language-support.md) for details.

---

## How It Works

Hotspots computes a **Local Risk Score (LRS)** for each function based on:

1. **Cyclomatic Complexity (CC)** - How many paths through the code?
2. **Nesting Depth (ND)** - How deeply nested are your if/for/while statements?
3. **Fan-Out (FO)** - How many other functions does this call?
4. **Non-Structured Exits (NS)** - How many early returns, breaks, throws?

These metrics combine into a single **Local Risk Score (LRS)**. Higher LRS = higher risk of bugs, incidents, and developer confusion.

LRS is then combined with **Activity Risk** signals from git history and the call graph:

- **Churn** — lines changed in the last 30 days (volatile code)
- **Touch frequency** — commit count touching this function
- **Recency** — days since last change (branch-aware)
- **Fan-in** — how many other functions call this one (call graph)
- **Cyclic dependency** — SCC membership (tightly coupled code)
- **Neighbor churn** — lines changed in direct dependencies

The call graph engine resolves imports to detect fan-in, PageRank, betweenness centrality, and SCC membership. Functions that are both complex AND heavily depended upon by other changing code rise to the top.

**Example:**

```typescript
// LRS: 12.4 (Critical) - Complex AND frequently changed
function processPlanUpgrade(user, newPlan, paymentMethod) {
  if (!user.isActive) return false;
  if (user.plan === newPlan) return true;

  if (paymentMethod.type === "card") {
    if (paymentMethod.isExpired) {
      try {
        paymentMethod = renewPaymentMethod(user);
      } catch (error) {
        logError(error);
        notifyUser(user, "payment_failed");
        return false;
      }
    }

    if (newPlan.price > user.plan.price) {
      const prorated = calculateProration(user, newPlan);
      if (!chargeCard(paymentMethod, prorated)) {
        return false;
      }
    }
  } else if (paymentMethod.type === "invoice") {
    // Different logic for invoice customers...
  }

  updateDatabase(user, newPlan);
  sendConfirmation(user);
  return true;
}
```

**This function:**
- CC: 15 (lots of branching)
- ND: 4 (deeply nested)
- FO: 8 (calls many functions)
- NS: 3 (multiple early returns)
- **LRS: 12.4** ← This is a hotspot

Refactor this before it causes a production incident.

---

## Features

### 🚦 Policy Enforcement (CI/CD)

Block risky code before it merges:

- **Critical Introduction** - Fail CI if new functions exceed LRS 9.0
- **Excessive Regression** - Fail CI if LRS increases by ≥1.0
- **Watch/Attention Warnings** - Warn about functions approaching thresholds
- **Rapid Growth Detection** - Catch functions growing >50% in complexity

```bash
# Run in CI with policy checks
hotspots analyze src/ --mode delta --policy
# Exit code 1 if policies fail → CI fails
```

### 🔍 Driver Labels & Explain Mode

Understand *why* a function is flagged and get concrete refactoring advice:

```bash
hotspots analyze . --mode snapshot --explain --top 10
```

Each function shows its primary **driver** (`high_complexity`, `deep_nesting`,
`high_churn_low_cc`, `high_fanout_churning`, `high_fanin_complex`, `cyclic_dep`,
`composite`) plus an **Action** line with dimension-specific guidance:

```
#1 processPayment [CRITICAL] [high_complexity]
   Risk Score: 14.52 (complexity base: 12.88)
   Risk Breakdown:
     • Complexity:   12.88  (cyclomatic=15, nesting=3, fanout=13)
     • Churn:         0.32  (63 lines changed recently)
     • Activity:      0.33  (11 commits in last 30 days)
   Action: Stable debt: schedule a refactor. Extract sub-functions to reduce CC.
```

Use `--level file` or `--level module` for higher-level aggregated views.

### 📊 Multiple Output Formats

**Terminal (human-readable):**
```
LRS   File                  Line  Function
12.4  src/api/billing.ts    142   processPlanUpgrade
```

**JSON (machine-readable):**
```json
[
  {
    "file": "src/api/billing.ts",
    "function": "processPlanUpgrade",
    "line": 142,
    "lrs": 12.4,
    "band": "critical",
    "metrics": { "cc": 15, "nd": 4, "fo": 8, "ns": 3 }
  }
]
```

**JSONL (streaming per-function):**
```bash
hotspots analyze src/ --mode snapshot --format jsonl | grep '"band":"critical"'
```
One JSON object per line — ideal for large repos and shell pipeline processing.

**HTML (interactive reports):**
- Sortable, filterable tables
- Risk band visualization
- Shareable with stakeholders
- Upload as CI artifacts

### 🔇 Suppression Comments

Have complex code you can't refactor yet? Suppress warnings with a reason:

```typescript
// hotspots-ignore: legacy payment processor, rewrite scheduled Q2 2026
function legacyBillingLogic() {
  // Complex but can't touch it yet
}
```

Functions with suppressions:
- ✅ Still appear in reports (visibility)
- ❌ Don't fail CI policies (pragmatism)
- 📝 Require a reason (accountability)

### ⚙️ Configuration

Customize thresholds, weights, and file patterns:

```json
{
  "thresholds": {
    "moderate": 3.0,
    "high": 6.0,
    "critical": 9.0
  },
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts", "**/__mocks__/**"]
}
```

See [docs/guide/configuration.md](docs/guide/configuration.md) for all options.

### 🤖 AI Integration

**Claude Desktop/Code:**
```bash
npm install -g @hotspots/mcp-server
```

**Cursor/GitHub Copilot:**
```bash
hotspots analyze src/ --format json | jq '.[] | select(.lrs > 9)'
# Feed results to your AI coding assistant
```

See [docs/integrations/ai-agents.md](docs/integrations/ai-agents.md) for complete guide.

### 📈 Git History Analysis

Track complexity over time:

```bash
# Create baseline snapshot
hotspots analyze src/ --mode snapshot

# Compare current code vs baseline
hotspots analyze src/ --mode delta

# See complexity trends
hotspots trends src/

# Prune unreachable snapshots (after force-push or branch deletion)
hotspots prune --unreachable --older-than 30

# Compact snapshot history
hotspots compact --level 0
```

Delta mode shows:
- Functions that got more complex
- Functions that were simplified
- New high-complexity functions introduced
- Overall repository complexity trend

### ⚙️ Configuration Commands

```bash
# Show resolved configuration (weights, thresholds, filters)
hotspots config show

# Validate configuration file without running analysis
hotspots config validate
```

---

## Documentation

- 🚀 [Quick Start](docs/getting-started/quick-start.md) - Get started in 5 minutes
- 📖 [CLI Reference](docs/reference/cli.md) - All commands and options
- 🎯 GitHub Action - CI/CD integration *(coming soon)*
- 🤖 [AI Integration](docs/integrations/ai-agents.md) - Claude, Cursor, Copilot
- 🏗️ [Architecture](docs/architecture/overview.md) - How it works
- 🤝 [Contributing](docs/contributing/index.md) - Add languages, fix bugs, improve docs

**Full documentation:** [docs/index.md](docs/index.md)

---

## Why Hotspots?

### vs ESLint Complexity Rules

**ESLint:** Checks individual metrics (CC > 10). No context about change frequency or real-world risk.
**Hotspots:** Combines multiple metrics into LRS. Integrates git history. Prioritizes based on actual risk.

### vs SonarQube / CodeClimate

**SonarQube:** Enterprise platform, complex setup, slow scans, requires server infrastructure.
**Hotspots:** Single binary, instant analysis, zero config, works offline, git history built-in.

### vs Code Reviews

**Reviews:** Catch complexity subjectively. Miss gradual regressions. Don't track trends.
**Hotspots:** Objective metrics. Catches every change. Shows trends over time. Enforces policies automatically.

**Use both:** Hotspots + code reviews = comprehensive quality control.

---

## Real-World Use Cases

### 🔥 Incident Prevention
"We had 3 production incidents in Q1. All originated from the same 5 functions. Hotspots flagged all 5 as critical. We refactored them in Q2. Zero incidents since."

### 🚀 Faster Onboarding
"New engineers use Hotspots to identify risky code before touching it. 'This function is LRS 11.2, be careful' = instant context."

### 🎯 Refactoring Sprints
"We allocate 1 sprint per quarter to reduce our top 10 hotspots. Dropped average LRS from 6.2 to 4.1 over 6 months."

### 🤖 AI-Guided Refactoring
"Feed hotspots JSON to Claude. It suggests refactorings for critical functions. Accept, commit, verify LRS dropped. Repeat."

### ⚖️ Technical Debt Metrics
"Execs ask 'How's our tech debt?' I show them: 23 critical functions (down from 31), average LRS 4.8 (down from 5.3). Clear progress."

---

## Installation

### Prebuilt Binaries (Fastest)

**macOS (Apple Silicon):**
```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-darwin-arm64 -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/
```

**macOS (Intel):**
```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-darwin-x64 -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/
```

**Linux:**
```bash
curl -L https://github.com/Stephen-Collins-tech/hotspots/releases/latest/download/hotspots-linux-x64 -o hotspots
chmod +x hotspots
sudo mv hotspots /usr/local/bin/
```

### Build from Source

```bash
git clone https://github.com/Stephen-Collins-tech/hotspots
cd hotspots
cargo build --release
sudo mv target/release/hotspots /usr/local/bin/
```

**Requirements:** Rust 1.75 or later

---

## Contributing

We welcome contributions!

- 🐛 [Report bugs](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 💡 [Request features](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 🔧 [Submit PRs](docs/contributing/index.md)
- 📖 [Improve docs](docs/contributing/index.md)

**Want to add a language?** See [docs/contributing/adding-languages.md](docs/contributing/adding-languages.md) - we have a proven pattern for adding TypeScript, JavaScript, Go, Python, Rust, and Java.

---

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT) for details.

---

## Next Steps

1. ⚡ [Install Hotspots](#installation) (2 minutes)
2. 🔍 Run your first analysis: `hotspots analyze src/`
3. 🎯 Identify your top 10 hotspots
4. 🛠️ Refactor the worst offender
5. 📊 Add to CI/CD: `hotspots analyze src/ --mode delta --policy` (GitHub Action coming soon)
6. 🤖 Integrate with AI: [AI Integration Guide](docs/integrations/ai-agents.md)

**Questions?** Open a [GitHub Discussion](https://github.com/Stephen-Collins-tech/hotspots/discussions).

**Found a bug?** Open an [issue](https://github.com/Stephen-Collins-tech/hotspots/issues).

---

**Stop refactoring guesswork. Start with Hotspots.**
