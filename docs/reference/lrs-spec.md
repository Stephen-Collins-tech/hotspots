# Local Risk Score (LRS) Specification

## Overview

The Local Risk Score (LRS) is a composite metric that quantifies the complexity and maintenance risk of individual TypeScript functions. It combines four fundamental software metrics into a single weighted score.

## Metrics

### 1. Cyclomatic Complexity (CC)

**Definition:** Number of linearly independent paths through a function's control flow.

**Formula:** `CC = E - N + 2` where:
- `E` = number of edges in the CFG
- `N` = number of nodes in the CFG (excluding entry and exit)

**Additional Increments:**
- Each boolean short-circuit operator (`&&`, `||`): +1
- Each switch case: +1
- Each catch clause: +1

**Minimum Value:** 1 (for empty functions)

### 2. Nesting Depth (ND)

**Definition:** Maximum depth of nested control structures in the AST.

**Counted Constructs:**
- `if` statements
- Loops (`for`, `while`, `do-while`, `for-in`, `for-of`)
- `switch` statements
- `try` blocks

**Not Counted:**
- Lexical scopes (block statements without control flow)
- Object/array literals

**Range:** 0 to unbounded (capped at 8 for risk calculation)

### 3. Fan-Out (FO)

**Definition:** Number of distinct functions called from within the function.

**Rules:**
- Count each call expression
- For chained calls like `foo().bar().baz()`, count each call:
  - `foo`
  - `foo().bar`
  - `foo().bar().baz`
- Deduplicate by string representation
- Ignore intrinsics and operators
- Self-calls (recursion) are counted

**Range:** 0 to unbounded

### 4. Non-Structured Exits (NS)

**Definition:** Number of early exit statements that break structured control flow.

**Counted:**
- Early `return` statements (excluding final tail return)
- `break` statements
- `continue` statements
- `throw` statements

**Not Counted:**
- Final `return` statement in a function (tail return)
- Implicit returns in arrow functions when they're the final expression

**Range:** 0 to unbounded (capped at 6 for risk calculation)

## Risk Transforms

Each raw metric is transformed to a risk component using monotonic, bounded functions:

### R_cc (Risk from Cyclomatic Complexity)

```
R_cc = min(log2(CC + 1), 6)
```

- Monotonic: increases with CC
- Bounded: maximum value of 6
- Logarithmic scaling reduces impact of very high CC

### R_nd (Risk from Nesting Depth)

```
R_nd = min(ND, 8)
```

- Linear scaling up to depth 8
- Maximum value of 8

### R_fo (Risk from Fan-Out)

```
R_fo = min(log2(FO + 1), 6)
```

- Monotonic: increases with FO
- Bounded: maximum value of 6
- Logarithmic scaling reduces impact of very high FO

### R_ns (Risk from Non-Structured Exits)

```
R_ns = min(NS, 6)
```

- Linear scaling up to 6 exits
- Maximum value of 6

## LRS Calculation

The Local Risk Score is a weighted sum of the risk components:

```
LRS = 1.0 * R_cc + 0.8 * R_nd + 0.6 * R_fo + 0.7 * R_ns
```

**Weights:**
- R_cc: 1.0 (highest weight - control flow complexity is primary risk)
- R_nd: 0.8 (high weight - deep nesting is hard to understand)
- R_ns: 0.7 (medium-high weight - non-structured exits complicate reasoning)
- R_fo: 0.6 (medium weight - dependencies add complexity but are somewhat expected)

## Risk Bands

Functions are classified into risk bands based on LRS:

| Band      | Range      | Interpretation                          |
|-----------|------------|-----------------------------------------|
| Low       | LRS < 3    | Simple, maintainable functions          |
| Moderate  | 3 ≤ LRS < 6| Moderate complexity, review recommended |
| High      | 6 ≤ LRS < 9| High complexity, refactor recommended   |
| Critical  | LRS ≥ 9    | Very high complexity, urgent refactor   |

## Examples

### Example 1: Simple Function

```typescript
function simple(x: number): number {
  return x * 2;
}
```

**Metrics:**
- CC = 1 (base formula)
- ND = 0
- FO = 0
- NS = 0

**Risk Components:**
- R_cc = min(log2(1 + 1), 6) = min(1.0, 6) = 1.0
- R_nd = min(0, 8) = 0
- R_fo = min(log2(0 + 1), 6) = min(0.0, 6) = 0.0
- R_ns = min(0, 6) = 0

**LRS:** 1.0 * 1.0 + 0.8 * 0 + 0.6 * 0.0 + 0.7 * 0 = **1.0** (Low)

### Example 2: Nested Branching

```typescript
function nested(x: number, y: number): number {
  if (x > 0) {
    if (y > 0) {
      return x + y;
    } else {
      return x - y;
    }
  } else {
    return 0;
  }
}
```

**Metrics:**
- CC = 3 (two if statements)
- ND = 2 (nested if)
- FO = 0
- NS = 0 (all returns are structured)

**Risk Components:**
- R_cc = min(log2(3 + 1), 6) = min(2.0, 6) = 2.0
- R_nd = min(2, 8) = 2
- R_fo = 0.0
- R_ns = 0

**LRS:** 1.0 * 2.0 + 0.8 * 2 + 0.6 * 0.0 + 0.7 * 0 = **3.6** (Moderate)

### Example 3: Complex Function

```typescript
function complex(arr: number[]): number {
  let sum = 0;
  for (const item of arr) {
    if (item < 0) {
      break;
    }
    if (item > 100) {
      continue;
    }
    sum += item;
  }
  return sum;
}
```

**Metrics:**
- CC = 3 (loop + 2 ifs)
- ND = 2 (loop with nested if)
- FO = 0
- NS = 2 (break + continue)

**Risk Components:**
- R_cc = min(log2(3 + 1), 6) = 2.0
- R_nd = min(2, 8) = 2
- R_fo = 0.0
- R_ns = min(2, 6) = 2

**LRS:** 1.0 * 2.0 + 0.8 * 2 + 0.6 * 0.0 + 0.7 * 2 = **4.6** (Moderate)

## Properties

### Determinism

LRS is deterministic:
- Identical input produces identical LRS
- Formatting, comments, and whitespace do not affect LRS
- Function order in file does not affect LRS

### Monotonicity

All risk transforms are monotonic:
- Increasing CC increases R_cc (capped at 6)
- Increasing ND increases R_nd (capped at 8)
- Increasing FO increases R_fo (capped at 6)
- Increasing NS increases R_ns (capped at 6)

### Boundedness

LRS has a theoretical maximum:
- Maximum R_cc = 6
- Maximum R_nd = 8
- Maximum R_fo = 6
- Maximum R_ns = 6
- **Maximum LRS** = 1.0 * 6 + 0.8 * 8 + 0.6 * 6 + 0.7 * 6 = **22.0**

In practice, functions rarely approach this maximum.

## Precision

- Internal calculations use full `f64` precision
- Final LRS is not rounded internally
- Text output displays 2 decimal places
- JSON output uses full `f64` precision

## References

- Cyclomatic Complexity: McCabe, T. J. (1976). "A Complexity Measure"
- Fan-Out: Yourdon, E. & Constantine, L. L. (1979). "Structured Design"
