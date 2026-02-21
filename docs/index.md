# Hotspots

**Find and fix the code that's actually causing problems.**

Hotspots analyzes your codebase to find functions that are both complex and frequently changed — the 20% of code responsible for 80% of your bugs, incidents, and feature delays.

```bash
hotspots analyze src/

# Results show your true priorities:
LRS   File                  Line  Function
12.4  src/api/billing.ts    142   processPlanUpgrade  # Critical — fix this first
9.8   src/auth/session.ts    67   validateSession     # High — watch closely
3.2   src/utils/format.ts    12   formatDate          # Low — safe to ignore
```

---

## Why Hotspots?

- **Objective priorities** — Stop arguing about what to refactor. The numbers tell you.
- **CI protection** — Block new high-complexity functions before they merge.
- **Progress tracking** — Show stakeholders: "Dropped from 31 critical functions to 23."

---

## Get Started

<div style="display: flex; gap: 1rem; flex-wrap: wrap; margin: 1.5rem 0;">
  <a href="/getting-started/installation" style="padding: 0.75rem 1.5rem; background: var(--vp-c-brand-1); color: white; border-radius: 6px; text-decoration: none; font-weight: 600;">Install Hotspots →</a>
  <a href="/guide/ci-cd" style="padding: 0.75rem 1.5rem; background: var(--vp-c-bg-soft); border-radius: 6px; text-decoration: none; font-weight: 600;">Set Up CI →</a>
  <a href="/reference/cli" style="padding: 0.75rem 1.5rem; background: var(--vp-c-bg-soft); border-radius: 6px; text-decoration: none; font-weight: 600;">CLI Reference →</a>
</div>

---

## Supported Languages

| Language | Extensions | Status |
|----------|-----------|---------|
| **TypeScript** | `.ts`, `.tsx`, `.mts`, `.cts` | Full support |
| **JavaScript** | `.js`, `.jsx`, `.mjs`, `.cjs` | Full support |
| **Go** | `.go` | Full support |
| **Python** | `.py` | Full support |
| **Rust** | `.rs` | Full support |
| **Java** | `.java` | Full support |

All languages get accurate CC, ND, FO, NS metrics, LRS calculation, policy enforcement, suppression comments, delta analysis, and git history integration.
