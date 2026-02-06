# Global Invariants

These invariants are **non-negotiable** and apply to **all phases** of implementation.

Any violation is considered a bug.

---

## 1. Analysis is strictly per-function

* Each function is analyzed independently
* No cross-function state is maintained
* No global analysis state exists
* Results are computed per-function and aggregated only at reporting time

**Enforcement:** Code must not maintain global mutable state for analysis.

---

## 2. No global mutable state

* All state must be local to functions or explicit data structures
* No static mutable variables
* No shared mutable state between functions during analysis

**Enforcement:** Use of `static mut`, global variables, or shared mutable references is prohibited.

---

## 3. No randomness, clocks, threads, or async

* No use of `rand`, `std::time`, `std::thread`, or async runtimes
* All operations must be deterministic
* No time-based or random behavior in analysis

**Enforcement:** Dependencies on randomness, timing, threading, or async are disallowed.

---

## 4. Deterministic traversal order must be explicit

* File traversal order must be deterministic (sorted by path)
* Function traversal within files must be deterministic (sorted by span start)
* All iteration over data structures must use explicit ordering

**Enforcement:** All collections must be sorted before iteration when order affects output.

---

## 5. Formatting, comments, and whitespace must not affect results

* AST parsing must ignore comments
* Whitespace changes must not affect analysis results
* Code formatting must not change metrics or LRS

**Enforcement:** Only structural AST nodes are used for analysis, never lexical details.

---

## 6. Identical input yields byte-for-byte identical output

* Running the same input twice must produce exactly the same output
* No timestamps, IDs, or non-deterministic elements in output
* Output must be deterministic and reproducible

**Enforcement:** All output serialization must be deterministic, including JSON key ordering and floating-point formatting.

---

## Violations

If any invariant is violated:

1. The implementation is incorrect
2. Tests must catch the violation
3. The violation must be fixed immediately

These invariants ensure that hotspots produces trusted, reproducible results suitable for static analysis workflows.
