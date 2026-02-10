# Testing Strategy

Hotspots testing approach ensuring correctness, determinism, and cross-language consistency.

## Overview

Hotspots employs a multi-layered testing strategy:

1. **Unit Tests** - Test individual components in isolation
2. **Integration Tests** - Test end-to-end analysis pipeline
3. **Golden Tests** - Verify deterministic output
4. **Language Parity Tests** - Ensure cross-language consistency
5. **CI Invariant Tests** - Enforce critical invariants
6. **Suppression Tests** - Validate suppression comments

**Test Coverage:** >80% (target 90%)
**Total Tests:** 220+ tests across all categories

---

## Test Types

### Unit Tests

Test individual functions and modules in isolation.

**Location:** `hotspots-core/src/**/tests.rs` (inline with code)

**Run Command:**
```bash
cargo test
```

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyclomatic_complexity() {
        let cfg = build_test_cfg();
        let cc = calculate_cc(&cfg);
        assert_eq!(cc, 5);
    }
}
```

**Coverage:**
- CFG construction
- Metrics calculation (CC, ND, FO, NS)
- LRS formula
- Risk band classification
- Configuration loading
- Suppression parsing

---

### Integration Tests

Test complete analysis pipeline from source code to reports.

**Location:** `hotspots-core/tests/integration_tests.rs`

**Run Command:**
```bash
cargo test --test integration_tests
```

**What They Test:**
- Full analysis pipeline
- File discovery
- Parser integration
- CFG building
- Metrics extraction
- Report generation

**Example:**
```rust
#[test]
fn test_analyze_typescript_file() {
    let path = PathBuf::from("tests/fixtures/simple.ts");
    let options = AnalysisOptions {
        min_lrs: None,
        top_n: None,
    };

    let reports = analyze(&path, options).unwrap();

    assert_eq!(reports.len(), 2);  // simpleFunction, complexFunction
    assert_eq!(reports[0].metrics.cc, 1);
    assert_eq!(reports[1].metrics.cc, 4);
}
```

**Key Tests:**
- Single file analysis
- Directory analysis
- Multi-language projects
- Config file loading
- Output formatting (JSON, text)
- Error handling (invalid syntax, missing files)

---

### Golden Tests

Verify byte-for-byte deterministic output against saved snapshots.

**Location:** `hotspots-core/tests/golden_tests.rs`

**Run Command:**
```bash
cargo test --test golden_tests
```

**How They Work:**

1. **Fixture** - Test input file (e.g., `tests/fixtures/simple.ts`)
2. **Analysis** - Run Hotspots analysis
3. **Golden File** - Expected output (e.g., `tests/golden/simple.json`)
4. **Comparison** - Assert actual output matches golden file exactly

**Example:**
```rust
#[test]
fn test_simple_golden() {
    let fixture = "tests/fixtures/simple.ts";
    let golden = "tests/golden/simple.json";

    let reports = analyze(fixture, options).unwrap();
    let actual = render_json(&reports);
    let expected = read_golden(golden);

    // Parse as JSON, normalize paths, compare
    assert_eq!(parse_json(actual), parse_json(expected));
}
```

**Path Normalization:**

Golden files use absolute paths, which vary by machine. The test harness normalizes paths:

```rust
fn normalize_paths(json: &mut Value, project_root: &Path) {
    // Extract path after "hotspots/" and normalize to current root
    if let Some(idx) = path.find("hotspots/") {
        let suffix = &path[idx + "hotspots/".len()..];
        *path = project_root.join(suffix).to_string();
    }
}
```

**Generating Golden Files:**

```bash
# Build release binary
cargo build --release

# Generate golden output
./target/release/hotspots analyze tests/fixtures/simple.ts --format json > tests/golden/simple.json

# Verify manually
cat tests/golden/simple.json | jq .

# Commit golden file
git add tests/golden/simple.json
```

**When to Update Golden Files:**
- New feature changes output format
- Metric calculation improved
- Bug fix changes results

**Never update golden files to make tests pass without understanding why output changed!**

---

### Language Parity Tests

Ensure identical code structure produces identical metrics across languages.

**Location:** `hotspots-core/tests/language_parity_tests.rs`

**Run Command:**
```bash
cargo test --test language_parity_tests
```

**Critical Invariant:**

> Functions with identical control flow structure MUST produce identical complexity metrics regardless of language.

**Example:**

TypeScript:
```typescript
function example(x: number): number {
    if (x > 0) {
        return x * 2;
    }
    return 0;
}
```

JavaScript:
```javascript
function example(x) {
    if (x > 0) {
        return x * 2;
    }
    return 0;
}
```

**Expected:** Both must have CC=2, ND=1, FO=0, NS=1

**Test Implementation:**
```rust
#[test]
fn test_typescript_javascript_parity() {
    let ts_reports = analyze("tests/fixtures/example.ts", options).unwrap();
    let js_reports = analyze("tests/fixtures/js/example.js", options).unwrap();

    assert_eq!(ts_reports.len(), js_reports.len());

    for (ts, js) in ts_reports.iter().zip(js_reports.iter()) {
        assert_eq!(ts.metrics.cc, js.metrics.cc, "CC must match");
        assert_eq!(ts.metrics.nd, js.metrics.nd, "ND must match");
        assert_eq!(ts.metrics.fo, js.metrics.fo, "FO must match");
        assert_eq!(ts.metrics.ns, js.metrics.ns, "NS must match");
        assert_eq!(ts.lrs, js.lrs, "LRS must match");
    }
}
```

**Fixtures:**
- `tests/fixtures/simple.ts` ↔ `tests/fixtures/js/simple.js`
- `tests/fixtures/nested-branching.ts` ↔ `tests/fixtures/js/nested-branching.js`
- `tests/fixtures/loop-breaks.ts` ↔ `tests/fixtures/js/loop-breaks.js`
- `tests/fixtures/pathological.ts` ↔ `tests/fixtures/js/pathological.js`

---

### CI Invariant Tests

Enforce critical behavioral invariants.

**Location:** `hotspots-core/tests/ci_invariant_tests.rs`

**Run Command:**
```bash
cargo test --test ci_invariant_tests
```

**Invariants Tested:**

#### 1. Determinism

> Running analysis twice on identical code MUST produce byte-for-byte identical output.

```rust
#[test]
fn test_determinism() {
    let run1 = analyze("tests/fixtures/simple.ts", options).unwrap();
    let run2 = analyze("tests/fixtures/simple.ts", options).unwrap();

    assert_eq!(render_json(&run1), render_json(&run2));
}
```

#### 2. Ordering

> Functions MUST always appear in source order.

```rust
#[test]
fn test_function_ordering() {
    let reports = analyze("tests/fixtures/multiple-functions.ts", options).unwrap();

    for i in 1..reports.len() {
        assert!(reports[i-1].line < reports[i].line, "Functions must be ordered by line number");
    }
}
```

#### 3. Monotonicity

> Adding control flow MUST increase or maintain CC (never decrease).

```rust
#[test]
fn test_cc_monotonicity() {
    let simple_cc = analyze_snippet("return x;").metrics.cc;
    let with_if_cc = analyze_snippet("if (x > 0) return x; return 0;").metrics.cc;

    assert!(with_if_cc >= simple_cc, "Adding 'if' must increase CC");
}
```

#### 4. Non-Negativity

> All metrics MUST be non-negative.

```rust
#[test]
fn test_non_negative_metrics() {
    let reports = analyze("tests/fixtures/pathological.ts", options).unwrap();

    for report in reports {
        assert!(report.metrics.cc >= 0);
        assert!(report.metrics.nd >= 0);
        assert!(report.metrics.fo >= 0);
        assert!(report.metrics.ns >= 0);
        assert!(report.lrs >= 0.0);
    }
}
```

---

### Suppression Tests

Validate suppression comment parsing and behavior.

**Location:** `hotspots-core/tests/suppression_tests.rs`

**Run Command:**
```bash
cargo test --test suppression_tests
```

**What They Test:**
- Suppression comment detection
- Reason extraction
- Metrics still calculated (suppression doesn't skip analysis)
- Report includes suppression metadata

**Example:**
```rust
#[test]
fn test_suppression_comment() {
    let source = r#"
        // @hotspots-ignore: Legacy code, refactor planned
        function legacyFunction(x) {
            // complex logic...
        }
    "#;

    let reports = analyze_source(source).unwrap();

    assert_eq!(reports.len(), 1);
    assert!(reports[0].suppressed);
    assert_eq!(reports[0].suppression_reason, Some("Legacy code, refactor planned"));
    assert_eq!(reports[0].metrics.cc, 8);  // Still calculated!
}
```

---

## Test Fixtures

### Directory Structure

```
tests/fixtures/
├── typescript/
│   ├── simple.ts
│   ├── nested-branching.ts
│   ├── loop-breaks.ts
│   ├── try-catch-finally.ts
│   └── pathological.ts
├── javascript/
│   ├── simple.js
│   ├── nested-branching.js
│   └── loop-breaks.js
├── go/
│   ├── simple.go
│   ├── loops.go
│   └── branching.go
├── python/
│   ├── simple.py
│   ├── comprehensions.py
│   └── exceptions.py
├── rust/
│   ├── simple.rs
│   ├── pattern-matching.rs
│   └── iterators.rs
└── java/
    ├── Simple.java
    ├── Loops.java
    └── Exceptions.java
```

### Fixture Categories

#### 1. Simple

Basic functions with minimal complexity.

**Purpose:** Baseline testing, smoke tests

**Example:** `simple.ts`
```typescript
function simple(x: number): number {
    return x + 1;
}

function withEarlyReturn(x: number): number {
    if (x < 0) return 0;
    return x;
}
```

**Expected Metrics:**
- `simple`: CC=1, ND=0, FO=0, NS=0
- `withEarlyReturn`: CC=2, ND=1, FO=0, NS=1

#### 2. Nested Branching

Deeply nested if/else structures.

**Purpose:** Test ND calculation, CC with nested conditions

**Example:** `nested-branching.ts`
```typescript
function nested(x: number): number {
    if (x > 0) {
        if (x < 100) {
            if (x % 2 === 0) {
                return x * 2;
            }
        }
    }
    return 0;
}
```

**Expected Metrics:** CC=4, ND=3

#### 3. Loop Breaks

Loops with break/continue.

**Purpose:** Test NS calculation, loop CFG routing

**Example:** `loop-breaks.ts`
```typescript
function loopWithBreak(items: number[]): number {
    for (const item of items) {
        if (item > 10) break;
        if (item < 0) continue;
    }
    return items[0];
}
```

**Expected Metrics:** CC=3, NS=2 (break + continue)

#### 4. Try-Catch-Finally

Exception handling.

**Purpose:** Test CC with catch clauses

**Example:** `try-catch-finally.ts`
```typescript
function tryCatch(x: number): number {
    try {
        return x / 2;
    } catch (err) {
        return 0;
    } finally {
        console.log("done");
    }
}
```

**Expected Metrics:** CC=2 (try + catch), FO=1 (console.log)

#### 5. Pathological

Extremely complex functions.

**Purpose:** Stress testing, performance benchmarks

**Example:** `pathological.ts`
```typescript
function pathological(data: any): any {
    if (data.a) {
        if (data.b) {
            if (data.c) {
                for (let i = 0; i < 10; i++) {
                    if (i % 2 === 0) {
                        if (data.items && data.items[i]) {
                            try {
                                return process(data.items[i]);
                            } catch (e) {
                                if (e.code === 500) {
                                    throw e;
                                } else {
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    return null;
}
```

**Expected Metrics:** CC=11, ND=7

---

## Running Tests

### All Tests

```bash
cargo test
```

### Specific Test Suite

```bash
# Unit tests only
cargo test --lib

# Integration tests
cargo test --test integration_tests

# Golden tests
cargo test --test golden_tests

# Language parity
cargo test --test language_parity_tests

# CI invariants
cargo test --test ci_invariant_tests
```

### Specific Test

```bash
# By name
cargo test test_simple_golden

# With output
cargo test test_simple_golden -- --nocapture

# Filter by pattern
cargo test typescript
```

### Watch Mode

```bash
# Rerun tests on file changes
cargo watch -x test
```

---

## Continuous Integration

### GitHub Actions Workflow

`.github/workflows/test.yml`:
```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run tests
        run: cargo test --verbose

      - name: Run golden tests
        run: cargo test --test golden_tests

      - name: Run language parity tests
        run: cargo test --test language_parity_tests

      - name: Run CI invariant tests
        run: cargo test --test ci_invariant_tests
```

### Required Tests for PR

All PRs must pass:
- ✅ All unit tests
- ✅ All integration tests
- ✅ All golden tests
- ✅ All language parity tests
- ✅ All CI invariant tests
- ✅ `cargo clippy` (zero warnings)
- ✅ `cargo fmt -- --check` (formatted)

---

## Coverage

### Generate Coverage Report

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage
cargo tarpaulin --out Html --output-dir coverage

# View report
open coverage/index.html
```

### Coverage Targets

- **Overall:** >80% (target 90%)
- **Core modules:** >90%
  - `metrics.rs` - 95%
  - `cfg/builder.rs` - 90%
  - `language/*/parser.rs` - 85%
- **Less critical:** >70%
  - `cli.rs` - 70%
  - `render.rs` - 75%

---

## Performance Benchmarks

### Benchmark Suite

`benches/analysis_benchmark.rs`:
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hotspots_core::analyze;

fn bench_simple_file(c: &mut Criterion) {
    c.bench_function("analyze simple.ts", |b| {
        b.iter(|| analyze(black_box("tests/fixtures/simple.ts"), options))
    });
}

fn bench_complex_file(c: &mut Criterion) {
    c.bench_function("analyze pathological.ts", |b| {
        b.iter(|| analyze(black_box("tests/fixtures/pathological.ts"), options))
    });
}

criterion_group!(benches, bench_simple_file, bench_complex_file);
criterion_main!(benches);
```

### Run Benchmarks

```bash
cargo bench
```

### Performance Targets

- **Simple file (<50 LOC):** <5ms
- **Medium file (50-500 LOC):** <30ms
- **Complex file (500+ LOC):** <100ms
- **Directory (100 files):** <3s

---

## Best Practices

### Writing Tests

1. **Test one thing** - Each test should verify a single behavior
2. **Clear names** - Test name should describe what's being tested
3. **Arrange-Act-Assert** - Follow AAA pattern
4. **Independent** - Tests should not depend on each other
5. **Deterministic** - No randomness, no flakiness

**Good:**
```rust
#[test]
fn test_if_statement_adds_one_to_cc() {
    let source = "if (x > 0) return x;";
    let cc = analyze_snippet(source).metrics.cc;
    assert_eq!(cc, 2);  // Baseline 1 + if statement 1
}
```

**Bad:**
```rust
#[test]
fn test_stuff() {
    let reports = analyze("tests/fixtures/simple.ts", options).unwrap();
    assert!(reports.len() > 0);
    assert!(reports[0].metrics.cc > 0);
}
```

### Updating Golden Files

1. **Understand why** - Don't blindly regenerate
2. **Manual verification** - Check output looks correct
3. **Document reason** - Explain in commit message
4. **Review carefully** - Golden file changes are high-risk

**Good workflow:**
```bash
# 1. Make code change
# 2. Test fails
cargo test test_simple_golden

# 3. Investigate difference
diff <(./target/release/hotspots analyze tests/fixtures/simple.ts --format json) tests/golden/simple.json

# 4. If change is expected, regenerate
./target/release/hotspots analyze tests/fixtures/simple.ts --format json > tests/golden/simple.json

# 5. Verify
cat tests/golden/simple.json | jq .

# 6. Commit with explanation
git add tests/golden/simple.json
git commit -m "test: update simple.json golden file after CC fix"
```

---

## Related Documentation

- [Adding Language Support](../contributing/adding-languages.md) - Testing new languages
- [Development Setup](../contributing/development.md) - Running tests locally
- [Invariants](./invariants.md) - Critical invariants enforced by tests

---

**Testing is critical to Hotspots' reliability. When in doubt, add more tests!** ✅
