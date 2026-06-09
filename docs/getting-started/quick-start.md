# Quick Start

Get your first results in under 5 minutes.

## 1. Install

```bash
# macOS
brew install Stephen-Collins-tech/tap/hotspots

# Linux
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh

# Any platform with Rust
cargo install hotspots-cli
```

Verify it worked:

```bash
hotspots --version
```

## 2. Analyze your codebase

Navigate to any git repository and run:

```bash
hotspots analyze src/
```

You'll see output like this:

```
CRITICAL (LRS ≥ 9.0)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/auth/validateUser.ts:142  validateUser        LRS: 12.4
src/api/billing.ts:89         processPlanUpgrade  LRS: 10.1

HIGH (LRS 6.0–9.0)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
src/db/migrations.ts:203      applySchema         LRS: 8.1
```

**Critical** means: refactor this now, or at minimum don't make it worse.
**High** means: refactor the next time you touch it.
**Moderate/Low** means: not worth the risk of disturbing it unless you have a specific reason.

## 3. Understand a result

LRS is a single number that combines four dimensions of structural complexity:

- **CC** — how many decision branches (if, switch, loops, try/catch)
- **ND** — how deep the nesting goes
- **FO** — how many other functions this one calls
- **NS** — non-structured exits (early returns, throws)

A function with LRS 12 isn't just "complex" — it's complex in ways that make it hard to test, hard to review, and likely to hide bugs. That's the starting point for a refactor conversation.

See [Metrics Reference](/reference/metrics) for how LRS is calculated.

---

## What next

**Focus on Critical first.** Pick the top offender. One critical function refactored per sprint moves the number.

**Set up CI** to block new critical functions from merging:

```bash
hotspots analyze src/ --mode delta --policy
```

Exit code 1 if a function crosses the critical threshold — CI fails, the author knows immediately. See [CI/CD Setup](/guide/ci-cd).

**Track progress over time** with snapshot mode:

```bash
hotspots analyze src/ --mode snapshot
# ... make changes ...
hotspots analyze src/ --mode delta
# Shows exactly what improved and what got worse
```

**Generate an HTML report** to share with your team:

```bash
hotspots analyze src/ --mode snapshot --format html
open .hotspots/report.html
```

---

Stuck? [Open an issue](https://github.com/Stephen-Collins-tech/hotspots/issues) or check the [full CLI reference](/reference/cli).
