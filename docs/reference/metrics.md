# Metrics Reference

Hotspots measures four independent dimensions of structural complexity per function, then combines them into a single risk score.

---

## The four metrics

### CC — Cyclomatic Complexity

The number of independent decision paths through a function. Each `if`, `else if`, `for`, `while`, `case`, `catch`, `&&`, `||`, and ternary adds one.

A function with CC 1 has a single straight-line path. CC 10 means at least 10 paths to test.

### ND — Nesting Depth

The maximum depth of nested control flow. Each `if`, loop, `try`, or block that contains another block adds a level.

Deep nesting is a reliable signal that a function is doing too many things at once. ND 4+ almost always indicates a function that should be split.

### FO — Fan-Out

The number of distinct functions this function calls. High fan-out means the function coordinates many dependencies — change any one of them and this function may break.

### NS — Non-Structured Exits

The count of early returns, throws, and panics inside the function body (not counting the final return). High NS makes control flow hard to trace and test paths hard to enumerate.

---

## LRS — Local Risk Score

LRS combines the four metrics using log-scaled transforms and weights:

```
LRS = r_cc + r_nd + r_fo + r_ns
```

Where each component is a log-scaled, capped transform of its raw metric. Logarithmic scaling means the difference between CC 1 and CC 3 is larger than the difference between CC 20 and CC 22 — early growth matters more.

### Risk bands

| Band     | LRS range | Meaning |
|----------|-----------|---------|
| Critical | ≥ 9.0     | Refactor now. These are your highest-probability bug sources. |
| High     | 6.0–8.9   | Refactor the next time you touch this function. |
| Moderate | 3.0–5.9   | Monitor. Block increases in CI. |
| Low      | < 3.0     | Not worth the risk of touching without a reason. |

Thresholds are configurable in `.hotspotsrc.json`. See [Configuration](/guide/configuration).

---

## What LRS is not

LRS is a structural risk proxy, not a defect predictor. A function can have LRS 12 and never cause a bug (simple domain, no changes planned) or LRS 4 and be the source of a production incident (critical path, subtle invariant).

LRS tells you where complexity is concentrated. What you do with that information requires judgment about which functions are actively changing, which are on critical paths, and which carry hidden invariants. The [HTML report](/getting-started/quick-start#what-next) and git integration help with that context.

---

## Full technical spec

See [LRS Specification](/reference/lrs-spec) for the exact formulas, transform definitions, and worked examples.
