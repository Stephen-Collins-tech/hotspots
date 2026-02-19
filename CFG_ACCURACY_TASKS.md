# CFG Accuracy Improvements

**Motivation:** Several language-specific control-flow constructs are currently not
modeled in the CFG (or only partially so), causing the cyclomatic complexity (CC) metric
to undercount decision points for Java and Python. Each item below is a known TODO in the
source that was deferred during initial implementation.

**Principle:** Each fix should be validated by a golden test with a known expected CC
before and after, so regressions are caught automatically.

---

## Hierarchy Overview

| Language | Gap                              | Impact                  |
|----------|----------------------------------|-------------------------|
| Java     | Ternary operator CC              | Undercounts CC          |
| Java     | Boolean short-circuit operators  | Undercounts CC          |
| Java     | Lambda with control flow in CFG  | Undercounts CC in lambdas |
| Python   | Match statement CFG precision    | Undercounts CC/ND for match |

---

## CA-1: Java Ternary Operator CC

**File:** `hotspots-core/src/language/java/cfg_builder.rs:507`

**Problem:** The Java CFG builder has a `// TODO: Check for conditional_expression
(ternary)` comment. Ternary `condition ? a : b` adds one decision point (CC +1) but
is not currently counted.

**Expected behavior:**
```java
// Should have CC = 2 (base + 1 ternary decision)
int abs(int x) {
    return x >= 0 ? x : -x;
}
```

**Tasks:**
- [ ] **CA-1a:** In `cfg_builder.rs`, handle `conditional_expression` tree-sitter node
  for Java. Add a CFG branch node for the ternary condition.
- [ ] **CA-1b:** Add a golden test `tests/golden/java/ternary_cc.java` verifying
  `CC = 2` for a one-ternary function and `CC = 3` for two nested ternaries.

**Effort:** Low. Tree-sitter node name is `conditional_expression`; pattern already
exists for similar constructs.

---

## CA-2: Java Boolean Short-Circuit Operators

**File:** `hotspots-core/src/language/java/cfg_builder.rs:508`

**Problem:** `// TODO: Check for binary_expression with && or ||`. In cyclomatic
complexity, each `&&` and `||` operand adds a decision point (modified McCabe). This
is already handled for Go and TypeScript but missing for Java.

**Expected behavior:**
```java
// Should have CC = 3 (base + && + ||)
boolean check(int a, int b, int c) {
    return a > 0 && b > 0 || c > 0;
}
```

**Tasks:**
- [ ] **CA-2a:** In the Java metrics extraction, walk `binary_expression` nodes and
  count operators with `&&` or `||` toward CC. (Check whether this is best done in
  `cfg_builder.rs` or `metrics.rs` — follow the Go precedent.)
- [ ] **CA-2b:** Add a golden test verifying CC counts for boolean compound expressions.

**Effort:** Low. Direct analogue to the existing Go/TS handling.

---

## CA-3: Java Lambda Control Flow in CFG

**File:** `hotspots-core/src/language/java/cfg_builder.rs:509`

**Problem:** `// TODO: Check for lambda_expression with control flow`. Lambda bodies
containing `if`, `switch`, or `return` currently don't contribute to the enclosing
method's CC. This inflates the apparent simplicity of methods that use inline lambdas
for filtering/mapping logic.

**Design decision needed (CA-3a research):**
- Option A: Inline lambda control flow into the enclosing method's CC (current
  TypeScript behavior for arrow functions).
- Option B: Analyze lambdas as separate anonymous functions (less noise, cleaner
  separation).

The TypeScript parser inlines arrow function bodies; Java anonymous inner class methods
are treated as separate functions. Lambdas sit between these extremes.

**Tasks:**
- [ ] **CA-3a (research):** Measure how often Java lambdas contain control flow in
  this repo and a sample open-source Java project. Determine which option produces
  more useful signal.
- [ ] **CA-3b:** Implement the chosen approach and add a golden test for a method
  with a lambda containing an `if` statement.

**Effort:** Low-Medium. Requires a design decision; implementation is straightforward
once the approach is chosen.

---

## CA-4: Python Match Statement CFG Precision

**File:** `hotspots-core/src/language/python/cfg_builder.rs:381`

**Problem:** `// TODO: Model match statement CFG more precisely`. Python 3.10+
`match` statements are partially handled — each `case` is counted as a CC decision
point — but the CFG edges between cases are not modeled precisely. This may affect
nesting depth (ND) and reachability analysis for functions using `match`.

**Expected behavior:**
```python
# Should have CC = 4 (base + 3 cases)
def classify(x):
    match x:
        case 0: return "zero"
        case 1: return "one"
        case _: return "other"
```

**Tasks:**
- [ ] **CA-4a:** Review the tree-sitter `match_statement` and `case_clause` node
  structure. Add proper CFG edges: match condition → each case → post-match merge.
- [ ] **CA-4b:** Add golden tests for match with guard clauses (`case x if x > 0:`)
  which should add an extra CC decision point.
- [ ] **CA-4c:** Verify ND counts correctly for nested match (match inside if, etc.).

**Effort:** Low-Medium. Tree-sitter parse tree is available; main work is CFG edge
modeling.

---

## Ordering / Dependencies

```
CA-1 (Java ternary)          — no dependencies, start now
CA-2 (Java boolean ops)      — no dependencies, start in parallel with CA-1
CA-3a (lambda research)      — no dependencies, run in parallel
CA-3b (lambda impl)          — blocked by CA-3a
CA-4 (Python match)          — no dependencies, start in parallel
```

All four items are independent of each other and of the dimensions feature work.
Each can be implemented, tested, and merged separately.

---

**Created:** 2026-02-19
