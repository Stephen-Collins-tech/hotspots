# Quick Start

Hotspots finds the **functions that are both complex and frequently changed** — the 20% of code causing 80% of bugs and slowdowns.

## Install

```bash
brew install Stephen-Collins-tech/tap/hotspots   # macOS
npm install -g @stephencollinstech/hotspots       # any platform
pip install hotspots-cli                          # any platform
cargo install hotspots-cli                        # Rust toolchain
```

## Run it

```bash
hotspots analyze src/
```

Output:
```
Critical (LRS ≥ 9.0):
  processPlanUpgrade   src/api/billing.ts:142   LRS 12.4  CC 15  ND 4  FO 8  NS 3

High (6.0 ≤ LRS < 9.0):
  validateSession      src/auth/session.ts:67   LRS 9.8   CC 11  ND 3  FO 7  NS 2
```

**Critical** = refactor now. **High** = refactor next time you touch it. **Low/Moderate** = leave it alone.

## What the score means

Each function gets a **Local Risk Score (LRS)** computed from four structural metrics:

| Metric | Measures |
|---|---|
| **CC** — Cyclomatic Complexity | Independent decision paths |
| **ND** — Nesting Depth | Maximum nesting of control structures |
| **FO** — Fan-Out | Distinct functions called |
| **NS** — Non-Structured Exits | Early returns, throws, breaks |

`LRS = 1.0×CC + 0.8×ND + 0.6×FO + 0.7×NS` (log-scaled, then summed)

## Add to CI

Block PRs that introduce new critical-risk functions:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
- uses: Stephen-Collins-tech/hotspots-action@v1
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
```

That's it — the action posts PR comments and exits 1 on policy violations.

## Next steps

- [Usage Guide](USAGE.md) — snapshot mode, delta diffs, output formats, policy config
- [CLI Reference](REFERENCE.md) — all flags, config schema, JSON schema
