<p align="center">
  <img src="assets/logo.svg" alt="Hotspots" width="96" height="96" />
</p>

# Hotspots

[![CI](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/ci.yml/badge.svg)](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/ci.yml)
[![Security](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/security.yml/badge.svg)](https://github.com/Stephen-Collins-tech/hotspots/actions/workflows/security.yml)

**Website:** https://hotspots.dev &nbsp;|&nbsp; **Docs:** https://docs.hotspots.dev &nbsp;|&nbsp; **Crates.io:** [![crates.io](https://img.shields.io/crates/v/hotspots-cli.svg)](https://crates.io/crates/hotspots-cli)

**Install:** `brew install Stephen-Collins-tech/tap/hotspots` &nbsp;|&nbsp; `npm install -g @stephencollinstech/hotspots` &nbsp;|&nbsp; `pip install hotspots-cli` &nbsp;|&nbsp; `cargo install hotspots-cli`

**Find the code that's actually causing problems.**

Your codebase has thousands of functions. Some are messy but never break. Others are complex AND change constantly — those are your **hotspots**, the 20% of code causing 80% of your bugs, incidents, and slowdowns.

---

## Install

```bash
brew install Stephen-Collins-tech/tap/hotspots   # macOS
npm install -g @stephencollinstech/hotspots       # any platform
pip install hotspots-cli                          # any platform
cargo install hotspots-cli                        # Rust toolchain
curl -fsSL https://raw.githubusercontent.com/Stephen-Collins-tech/hotspots/main/install.sh | sh  # Linux
```

Windows: download the binary from [GitHub Releases](https://github.com/Stephen-Collins-tech/hotspots/releases/latest).

Verify: `hotspots --version`

**GitHub Action:**
```yaml
- uses: Stephen-Collins-tech/hotspots/action@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

---

## Quick Start

```bash
# Find your hotspots
hotspots analyze src/

# Critical (LRS ≥ 9.0):
# processPlanUpgrade    src/api/billing.ts:142    LRS 12.4  CC 15  ND 4  FO 8  NS 3
#
# High (6.0 ≤ LRS < 9.0):
# validateSession       src/auth/session.ts:67    LRS 9.8   CC 11  ND 3  FO 7  NS 2
```

**Critical** = refactor now. **High** = refactor next time you touch it. **Moderate** = block increases. **Low** = leave it alone.

### Common commands

```bash
# Per-function explanations with refactoring advice
hotspots analyze . --mode snapshot --format text --explain --top 10

# Block complexity regressions in CI
hotspots analyze src/ --mode delta --policy

# Compare any two git refs
hotspots diff main HEAD --top 10 --policy

# Interactive HTML report
hotspots analyze src/ --mode snapshot --format html

# JSON for tooling/AI
hotspots analyze src/ --format json

# Track trends over time
hotspots trends .

# Train a repo-specific ranker from your bug history
hotspots train . --blame --eval
```

---

## How It Works

Hotspots computes a **Local Risk Score (LRS)** per function from four structural metrics:

| Metric | What it measures |
|---|---|
| **CC** — Cyclomatic Complexity | Independent decision paths (if/loop/catch/&&/\|\|) |
| **ND** — Nesting Depth | Maximum depth of nested control structures |
| **FO** — Fan-Out | Distinct functions called |
| **NS** — Non-Structured Exits | Early returns, throws, breaks |

```
LRS = 1.0×R_cc + 0.8×R_nd + 0.6×R_fo + 0.7×R_ns
```

where each component is log-scaled and capped to prevent outliers from dominating.

In **snapshot mode**, LRS is combined with git history (churn, touch frequency, recency) and call graph metrics (fan-in, PageRank, SCC membership) to compute an **Activity Risk Score** and place each function in a triage quadrant:

| Quadrant | Complexity | Activity | Action |
|---|---|---|---|
| `fire` | High | High | Refactor now |
| `debt` | High | Low | Schedule before next push |
| `watch` | Low | High | Monitor |
| `ok` | Low | Low | Leave it alone |

**Risk bands:** Low (< 3) · Moderate (3–6) · High (6–9) · Critical (≥ 9)

---

## Supported Languages

TypeScript · JavaScript · Go · Python · Rust · Java · C/C headers · C# · Vue

All 12 file extensions (`.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.go`, `.py`, `.rs`, `.java`, `.c`, `.h`, `.cs`, `.vue`) work out of the box.

---

## CI/CD Integration

### GitHub Action (zero config)

```yaml
name: Hotspots
on: [pull_request, push]
jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: Stephen-Collins-tech/hotspots-action@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

Posts PR comments, generates HTML reports, fails builds on policy violations.

### Manual CI

```bash
# Fail CI if new critical functions introduced or LRS increases ≥ 1.0
hotspots analyze src/ --mode delta --policy
```

Exit code 1 on blocking violations, 0 on warnings only.

---

## Key Features

**Policy engine** — blocks: new critical-risk functions, LRS regressions ≥ 1.0, net repo regression ≥ 5.0. Warns: approaching thresholds, rapid growth > 50%.

**Driver labels** — each function gets a primary diagnosis: `high_complexity`, `deep_nesting`, `exit_heavy`, `high_churn_low_cc`, `high_fanout_churning`, `high_fanin_complex`, `cyclic_dep`, or `composite`.

**Pattern detection** — 13 named patterns in two tiers: structural (always, e.g. `complex_branching`, `god_function`) and enriched (snapshot mode, e.g. `churn_magnet`, `cyclic_hub`, `volatile_god`).

**Suppression comments** — exclude functions from CI failures while keeping them visible:
```typescript
// hotspots-ignore: legacy payment processor, rewrite scheduled Q2 2026
function legacyBillingLogic() { ... }
```

**Trained ranker** — fit a RandomForest from your repo's bug-fix history:
```bash
hotspots train . --blame --eval   # train + check P@K vs base rate
```

**Output formats** — `text` (terminal), `json` (machine), `jsonl` (streaming), `html` (interactive), `sarif` (GitHub Code Scanning).

**Configuration** — `.hotspotsrc.json` in project root (auto-discovered):
```json
{
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts"],
  "thresholds": { "moderate": 3.0, "high": 6.0, "critical": 9.0 },
  "weights": { "cc": 1.0, "nd": 0.8, "fo": 0.6, "ns": 0.7 }
}
```

---

## Documentation

- [docs/USAGE.md](docs/USAGE.md) — workflows, CI setup, output formats, policy engine, suppression, training, snapshot management
- [docs/REFERENCE.md](docs/REFERENCE.md) — complete CLI reference, all flags, config options, JSON schema, metrics formula
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — system design, analysis pipeline, invariants, design decisions
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) — dev setup, adding languages, release process

---

## Why Hotspots?

**vs ESLint complexity rules** — ESLint checks one metric in isolation with no git context. Hotspots combines four metrics, git history, and call graph topology into a single prioritized list.

**vs SonarQube / CodeClimate** — Enterprise platforms requiring server infrastructure. Hotspots is a single binary, zero config, works offline, results in seconds.

**vs code reviews** — Reviews catch complexity subjectively and miss gradual drift. Hotspots enforces objective thresholds automatically on every commit.

---

## Contributing

- Bug reports: [GitHub Issues](https://github.com/Stephen-Collins-tech/hotspots/issues)
- Feature requests: [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- PRs: see [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)

---

## License

MIT — see [LICENSE-MIT](LICENSE-MIT).
