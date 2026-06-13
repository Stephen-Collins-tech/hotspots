# Hotspots

**Find where your engineering attention has the highest expected value.**

Most of the pain in a codebase comes from a small fraction of it — the same files that generate the most incidents, the slowest reviews, and the hardest bugs to track down. Hotspots finds that fraction before it costs you.

```bash
hotspots analyze src/

LRS   File                      Line  Function
12.4  src/api/billing.ts         142  processPlanUpgrade   critical
 9.8  src/auth/session.ts         67  validateSession      high
 3.2  src/utils/format.ts         12  formatDate           low
```

Each result is ranked by **Local Risk Score (LRS)** — a weighted combination of cyclomatic complexity, nesting depth, fan-out, and exit paths. High LRS means the function is structurally hard to reason about, test, and safely change.

---

## Get Started

<div style="display: flex; gap: 1rem; flex-wrap: wrap; margin: 1.5rem 0;">
  <a href="/getting-started/quick-start" style="padding: 0.75rem 1.5rem; background: var(--vp-c-brand-1); color: white; border-radius: 6px; text-decoration: none; font-weight: 600;">Quick Start →</a>
  <a href="/guide/ci-cd" style="padding: 0.75rem 1.5rem; background: var(--vp-c-bg-soft); border-radius: 6px; text-decoration: none; font-weight: 600;">Set Up CI →</a>
  <a href="/reference/cli" style="padding: 0.75rem 1.5rem; background: var(--vp-c-bg-soft); border-radius: 6px; text-decoration: none; font-weight: 600;">CLI Reference →</a>
</div>

---

## What it gives you

**An objective refactor list.** Stop debating what to clean up. The highest-LRS functions in your codebase are the ones most likely to slow your next feature and hide your next bug.

**CI enforcement.** Block new critical-complexity functions before they merge. Delta mode shows exactly which functions got worse in a PR — and by how much.

**Progress you can show.** "We dropped from 31 critical functions to 18 this quarter" is a concrete metric. Hotspots makes that trackable without extra tooling.

**Everything stays local.** Analysis runs on your machine. No source code leaves, no account required, no data sent anywhere.

---

## Supported Languages

TypeScript · JavaScript · Go · Python · Rust · Java · C

All languages produce the same metrics with consistent semantics. See [Language Support](/reference/language-support).

---

## How scoring works

Each function receives a **Local Risk Score (LRS)** derived from four structural metrics (CC, ND, FO, NS), then optionally enriched with git-based activity signals to produce an **Activity Risk Score**. Functions are grouped into quadrants (fire / debt / watch / ok) and assigned a **driver label** naming the dominant risk dimension.

See [Scoring Methodology](/reference/scoring) for the full pipeline — transforms, weights, pattern detection, quadrant logic, and ranking order.

---

## Where this is going

**Risk Hotspots** — structurally complex, frequently changed code — are shipped. Several more categories are in active research:

- **Review Hotspots** — changes that need senior eyes, not rubber stamps
- **Test Hotspots** — coverage gaps where CI misses real failures
- **Ownership Hotspots** — knowledge silos and review bottlenecks
- **Impact Hotspots** — code with outsized blast radius (auth, billing, schema contracts)

The goal is a continuous picture of where your engineering attention matters most — before something ships, not after it pages you.

---

## For Contributors

Start with the [Codebase Guide](/code-architecture/) for the implementation map, then the [Contributing Guide](/contributing/) for dev workflow.
