# Hotspots

Multi-language complexity analysis — single binary, zero config, results in seconds.

Hotspots finds the small share of functions responsible for most of a codebase's
review burden: the ones combining high structural complexity with recent, frequent
change. It scores every function, ranks them, and — in a git repo — layers in churn
and touch frequency to separate active risk from dormant debt.

[Get started →](/quickstart) · [CLI reference →](/REFERENCE) · [GitHub →](https://github.com/Stephen-Collins-tech/hotspots)

## What it does

- **LRS scoring** — combines cyclomatic complexity, nesting depth, fan-out, and
  non-structured exits into one actionable number per function.
- **Git-enriched triage** — snapshot mode adds churn and touch frequency, placing
  every function in an active or quiet tier. Fan-in is reported alongside, but
  doesn't feed the live score.
- **Policy engine** — block PRs that introduce critical-risk functions or regress
  LRS. Works with GitHub Actions, GitLab CI, and any CI that checks exit codes.

## Where to go next

| | |
|---|---|
| [Quick Start](/quickstart) | Install the binary and run your first analysis. |
| [Commands](/COMMANDS) | Each command explained: why it exists, when to use it, how to read the output. |
| [Usage & Workflows](/USAGE) | Snapshot mode, delta mode, the policy engine, `hotspots coordinate`. |
| [CLI & Config Reference](/REFERENCE) | Every command, every flag, every config field. |
| [Architecture](/ARCHITECTURE) | How the analysis pipeline actually works, end to end. |
| [Score Stability & Predictability](/RISK_STABILITY) | What "the score didn't change for no reason" actually means and why it holds. |
| [Contributing](/CONTRIBUTING) | Dev setup, adding a language, release process. |
| [hotspots-public-research](https://github.com/Stephen-Collins-tech/hotspots-public-research) | The evidence behind the scoring formulas — what shipped, and what didn't. |

Live analyses of real open-source repositories run continuously on
[hotspots.dev](https://hotspots.dev) — see what this looks like against a
codebase you already know.
