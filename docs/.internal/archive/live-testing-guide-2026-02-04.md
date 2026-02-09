# Live Testing Guide

This guide provides step-by-step instructions for testing the new functionality, including Aggregates, DeltaAggregates, Policy Engine, and Trend Semantics.

## Quick Reference

### Run All Tests
```bash
# Unit tests
cargo test --lib

# Comprehensive integration tests
python3 test_comprehensive.py

# Or use Make
make test-all
```

### Test Specific Features
```bash
# Aggregates
cargo test --lib aggregates
./dev.sh analyze --mode snapshot src/ --format json | jq '.aggregates'

# Policy Engine
cargo test --lib policy
./dev.sh analyze --mode delta --policy src/ --format json | jq '.policy'

# Trend Semantics
./dev.sh trends --window 10 --format json . | jq '.risk_velocities'
```

### Verify Build
```bash
cargo build --release
./dev.sh --version
```

---

## Prerequisites

1. **Build the release binary:**
   ```bash
   cargo build --release
   ```

2. **Ensure Python 3 is available:**
   ```bash
   python3 --version
   ```

3. **Optional: Install `jq` for JSON parsing:**
   ```bash
   # macOS
   brew install jq
   
   # Linux
   sudo apt-get install jq
   ```

---

## 1. Unit Tests

### Test Aggregates Module

```bash
# Run all aggregates unit tests
cargo test --lib aggregates

# Expected output:
# running 3 tests
# test aggregates::tests::test_is_high_plus ... ok
# test aggregates::tests::test_file_aggregates ... ok
# test aggregates::tests::test_directory_aggregates ... ok
# test result: ok. 3 passed; 0 failed
```

### Test Policy Module

```bash
# Run all policy unit tests
cargo test --lib policy

# Expected output:
# running 8 tests
# test policy::tests::test_excessive_risk_regression_below_threshold ... ok
# test policy::tests::test_critical_introduction_modified_function ... ok
# ... (6 more tests)
# test result: ok. 8 passed; 0 failed
```

### Run All Library Tests

```bash
# Run all unit tests
cargo test --lib

# Expected output:
# test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured
```

---

## 2. Comprehensive Integration Tests

### Run Full Test Suite

The comprehensive test suite creates a temporary git repository, adds multiple commits with varying complexity, and validates all features.

```bash
# Run comprehensive test suite
python3 test_comprehensive.py

# Or using Make
make test-comprehensive
```

**What it tests:**
1. ✅ Snapshot analysis
2. ✅ Policy engine (Critical Introduction, Excessive Risk Regression, Net Repo Regression)
3. ✅ Aggregation views (file and delta aggregates)
4. ✅ Trend semantics (risk velocity, hotspot stability)
5. ✅ Text output formats

**Expected output:**
```
=== Hotspots Comprehensive Test Suite ===

Test directory: /path/to/test-repo-comprehensive

1. Initializing test git repository...
2. Creating initial TypeScript file...
   Commit 1: <sha>
   Running snapshot analysis...
✓ Snapshot analysis completed

3. Adding high complexity function...
   Commit 2: <sha>
   Running snapshot analysis...
   Testing delta with policy...
   Checking for policy violations...
✓ Policy evaluation working
   Failed policies: 1
   Warnings: 1

4. Adding critical function...
   Commit 3: <sha>
   ...

5. Testing aggregation views...
✓ Snapshot aggregates working
   File aggregates: 1
   Directory aggregates: 2
✓ Delta aggregates working
   File aggregates: 1

6. Testing trend semantics...
   Risk velocities: 3
   Hotspots: 4
   Refactors: 0
✓ Hotspot analysis working

7. Testing text output formats...
   ...

=== Test Summary ===
Commits created: 3
Snapshots: 3
Deltas tested: 2

✓ Comprehensive test completed
```

**Test artifacts:**
- Test repository: `./test-repo-comprehensive/`
- Snapshot JSON files: `./test-repo-comprehensive/snapshot-*.json`
- Delta JSON files: `./test-repo-comprehensive/delta-*.json`
- Trends JSON: `./test-repo-comprehensive/trends.json`

---

## 3. Manual Testing - Aggregates

### Test Snapshot Aggregates

#### Using Dev Script

```bash
# Analyze a file and output snapshot with aggregates
./dev.sh analyze --mode snapshot hotspots-core/src/aggregates.rs --format json

# Extract just the aggregates section
./dev.sh analyze --mode snapshot hotspots-core/src/aggregates.rs --format json | jq '.aggregates'
```

**Expected output structure:**
```json
{
  "aggregates": {
    "files": [
      {
        "file": "hotspots-core/src/aggregates.rs",
        "sum_lrs": 15.2,
        "max_lrs": 8.5,
        "high_plus_count": 2
      }
    ],
    "directories": [
      {
        "directory": "hotspots-core/src",
        "sum_lrs": 15.2,
        "max_lrs": 8.5,
        "high_plus_count": 2
      },
      {
        "directory": "hotspots-core",
        "sum_lrs": 15.2,
        "max_lrs": 8.5,
        "high_plus_count": 2
      }
    ]
  }
}
```

#### Analyze a Directory

```bash
# Analyze entire directory with aggregates
./dev.sh analyze --mode snapshot hotspots-core/src/ --format json | jq '.aggregates.directories[] | select(.directory | contains("aggregates"))'
```

**Verify recursive rollup:**
- Files in `src/api/` should contribute to both `src/api/` and `src/` directories
- Sum LRS should accumulate correctly up the directory tree

### Test Delta Aggregates

#### Create a Test Repository

```bash
# Create temporary test directory
mkdir -p /tmp/hotspots-test
cd /tmp/hotspots-test

# Initialize git repo
git init
git config user.name "Test User"
git config user.email "test@example.com"

# Create initial file
cat > src/main.ts << 'EOF'
function simple() {
  return 1;
}
EOF

git add src/main.ts
git commit -m "Initial commit"

# Run first snapshot
/path/to/hotspots analyze --mode snapshot src/ --format json > snapshot1.json

# Modify file to add complexity
cat > src/main.ts << 'EOF'
function simple() {
  return 1;
}

function complex() {
  if (true) {
    if (true) {
      if (true) {
        return 1;
      }
    }
  }
  return 2;
}
EOF

git add src/main.ts
git commit -m "Add complex function"

# Run delta analysis
/path/to/hotspots analyze --mode delta --format json > delta1.json

# Check delta aggregates
cat delta1.json | jq '.aggregates'
```

**Expected output:**
```json
{
  "aggregates": {
    "files": [
      {
        "file": "src/main.ts",
        "net_lrs_delta": 5.2,
        "regression_count": 1
      }
    ]
  }
}
```

#### Verify Regression Count

```bash
# Count functions with positive LRS delta
cat delta1.json | jq '.deltas[] | select(.delta.lrs > 0) | .function_id'
```

This should match the `regression_count` in aggregates.

---

## 4. Manual Testing - Policy Engine

### Test Critical Introduction Policy

```bash
# Create a file with a Critical function
cat > test-critical.ts << 'EOF'
function criticalFunction() {
  if (true) {
    if (true) {
      if (true) {
        if (true) {
          if (true) {
            if (true) {
              if (true) {
                if (true) {
                  return 1;
                }
              }
            }
          }
        }
      }
    }
  }
}
EOF

# Run delta with policy
./dev.sh analyze --mode delta --policy test-critical.ts --format json | jq '.policy.failed[] | select(.id == "critical-introduction")'
```

**Expected output:**
```json
{
  "id": "critical-introduction",
  "severity": "blocking",
  "function_id": "test-critical.ts::criticalFunction",
  "message": "Function test-critical.ts::criticalFunction introduced as Critical"
}
```

### Test Excessive Risk Regression

```bash
# Create initial file
cat > test-regression.ts << 'EOF'
function moderate() {
  if (true) {
    return 1;
  }
  return 2;
}
EOF

# Commit and snapshot
git add test-regression.ts
git commit -m "Initial"
./dev.sh analyze --mode snapshot test-regression.ts > /dev/null

# Add significant complexity
cat > test-regression.ts << 'EOF'
function moderate() {
  if (true) {
    if (true) {
      if (true) {
        if (true) {
          if (true) {
            return 1;
          }
        }
      }
    }
  }
  return 2;
}
EOF

# Test delta with policy
./dev.sh analyze --mode delta --policy test-regression.ts --format json | jq '.policy.failed[] | select(.id == "excessive-risk-regression")'
```

**Expected output:**
```json
{
  "id": "excessive-risk-regression",
  "severity": "blocking",
  "function_id": "test-regression.ts::moderate",
  "message": "Function test-regression.ts::moderate LRS increased by 2.5 (threshold: 1.0)",
  "metadata": {
    "delta_lrs": 2.5
  }
}
```

### Test Net Repo Regression (Warning)

```bash
# Run delta with policy and check warnings
./dev.sh analyze --mode delta --policy . --format json | jq '.policy.warnings[] | select(.id == "net-repo-regression")'
```

**Expected output:**
```json
{
  "id": "net-repo-regression",
  "severity": "warning",
  "message": "Repository total LRS increased by 3.2",
  "metadata": {
    "total_delta": 3.2
  }
}
```

### Test Exit Codes

```bash
# No policy violations - exit 0
./dev.sh analyze --mode delta . --format json
echo "Exit code: $?"  # Should be 0

# Policy violations - exit 1
./dev.sh analyze --mode delta --policy . --format json
echo "Exit code: $?"  # Should be 1 if blocking failures exist
```

---

## 5. Manual Testing - Trend Semantics

### Test Risk Velocity

```bash
# Run trends analysis
./dev.sh trends --window 10 --top 5 --format json . | jq '.risk_velocities[0]'
```

**Expected output:**
```json
{
  "function_id": "src/main.ts::functionName",
  "velocity": 0.5,
  "direction": "increasing",
  "first_lrs": 2.0,
  "last_lrs": 7.0,
  "data_points": 10
}
```

### Test Hotspot Stability

```bash
# Get hotspot stability analysis
./dev.sh trends --window 10 --format json . | jq '.hotspot_stability[0]'
```

**Expected output:**
```json
{
  "function_id": "src/main.ts::functionName",
  "stability": "stable",
  "overlap_ratio": 1.0,
  "appearances": 10,
  "window_size": 10
}
```

### Test Refactor Effectiveness

```bash
# Check for refactors
./dev.sh trends --window 10 --format json . | jq '.refactor_effectiveness'
```

**Expected output:**
```json
{
  "refactors": [
    {
      "function_id": "src/main.ts::functionName",
      "improvement_commit": "abc123",
      "improvement_delta": -2.5,
      "rebound_commit": null,
      "status": "effective"
    }
  ],
  "summary": {
    "total_refactors": 1,
    "effective": 1,
    "rebounded": 0
  }
}
```

---

## 6. Testing Output Formats

### JSON Format

```bash
# Snapshot with aggregates
./dev.sh analyze --mode snapshot src/ --format json | jq 'keys'
# Should include: schema_version, commit, analysis, functions, aggregates

# Delta with aggregates and policy
./dev.sh analyze --mode delta --policy src/ --format json | jq 'keys'
# Should include: schema_version, commit, baseline, deltas, policy, aggregates
```

### Text Format

```bash
# Snapshot text output
./dev.sh analyze --mode snapshot src/ --format text

# Delta with policy text output
./dev.sh analyze --mode delta --policy src/ --format text

# Trends text output
./dev.sh trends --window 10 --format text .
```

---

## 7. Testing Edge Cases

### Empty Repository

```bash
mkdir -p /tmp/empty-test
cd /tmp/empty-test
git init
./dev.sh analyze --mode snapshot . --format json | jq '.aggregates'
# Should have empty files and directories arrays
```

### Single File

```bash
mkdir -p /tmp/single-test
cd /tmp/single-test
git init
echo "function test() { return 1; }" > test.ts
./dev.sh analyze --mode snapshot test.ts --format json | jq '.aggregates.files | length'
# Should be 1
```

### Deeply Nested Directories

```bash
mkdir -p /tmp/nested-test/src/a/b/c/d/e
cd /tmp/nested-test
git init
echo "function test() { return 1; }" > src/a/b/c/d/e/test.ts
./dev.sh analyze --mode snapshot src/ --format json | jq '.aggregates.directories | length'
# Should include all parent directories
```

### Path Normalization

```bash
# Test with absolute paths
./dev.sh analyze --mode snapshot $(pwd)/src/main.ts --format json | jq '.aggregates.files[0].file'
# Should be normalized relative to repo root
```

---

## 8. Quick Verification Checklist

Run these commands to quickly verify all features:

```bash
# 1. Unit tests
cargo test --lib aggregates && echo "✓ Aggregates tests passed"
cargo test --lib policy && echo "✓ Policy tests passed"

# 2. Build
cargo build --release && echo "✓ Build successful"

# 3. Version
./dev.sh --version && echo "✓ Version working"

# 4. Comprehensive test
python3 test_comprehensive.py && echo "✓ Comprehensive tests passed"

# 5. Snapshot aggregates
./dev.sh analyze --mode snapshot hotspots-core/src/aggregates.rs --format json | jq -e '.aggregates.files | length > 0' && echo "✓ Snapshot aggregates working"

# 6. Delta aggregates (requires git repo)
cd /tmp && mkdir -p delta-test && cd delta-test
git init && git config user.name "Test" && git config user.email "test@test.com"
echo "function test() { return 1; }" > test.ts
git add test.ts && git commit -m "Initial"
echo "function test() { if(true) { if(true) { return 1; } } }" > test.ts
git add test.ts && git commit -m "Complex"
/path/to/hotspots analyze --mode delta --format json | jq -e '.aggregates.files | length > 0' && echo "✓ Delta aggregates working"
```

---

## 9. Troubleshooting

### Test Repository Cleanup

If the comprehensive test fails to clean up:

```bash
# Remove test repository
rm -rf ./test-repo-comprehensive

# Or if permission denied
sudo rm -rf ./test-repo-comprehensive
```

### JSON Parsing Errors

If `jq` fails to parse JSON:

```bash
# Check if output is valid JSON
./dev.sh analyze --mode snapshot src/ --format json | python3 -m json.tool > /dev/null && echo "Valid JSON"

# Or use Python
./dev.sh analyze --mode snapshot src/ --format json | python3 -c "import json, sys; json.load(sys.stdin)"
```

### Missing Aggregates in Output

If aggregates are missing:

1. Check that you're using `--mode snapshot` or `--mode delta`
2. Verify the file/directory exists and has TypeScript functions
3. Check that aggregates are computed (should appear in JSON output)

### Policy Not Triggering

If policies aren't triggering:

1. Verify `--policy` flag is used
2. Check that functions meet policy thresholds:
   - Critical Introduction: Function must be Critical band
   - Excessive Risk Regression: LRS delta must be ≥ 1.0
3. Check baseline deltas (policies are skipped for baselines)

---

## 10. Continuous Testing

### Run All Tests

```bash
# Using Make
make test-all

# Or manually
cargo test --lib && python3 test_comprehensive.py
```

### CI/CD Integration

The comprehensive test is integrated into GitHub Actions:

```yaml
# .github/workflows/test-comprehensive.yml
```

Run locally to match CI:

```bash
# Same as CI
python3 test_comprehensive.py
```

---

## Summary

This guide covers:

1. ✅ **Unit Tests** - Fast, isolated tests
2. ✅ **Comprehensive Tests** - Full integration test suite
3. ✅ **Manual Testing** - Step-by-step feature verification
4. ✅ **Edge Cases** - Boundary condition testing
5. ✅ **Troubleshooting** - Common issues and solutions

For automated testing, use the comprehensive test suite. For feature verification, use the manual testing steps.
