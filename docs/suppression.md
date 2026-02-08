# Suppression Comments

Suppression comments allow you to exclude specific functions from policy violations while keeping them tracked in reports. This is useful for handling false positives, legacy code, and intentionally complex algorithms.

## Quick Start

Place a comment immediately before the function:

```typescript
// hotspots-ignore: legacy code, refactor planned for Q2 2026
function complexLegacyParser(input: string) {
  // High complexity code...
}
```

## Comment Format

**Required format:**
```
// hotspots-ignore: <reason>
```

**Rules:**
1. Comment must be on the line **immediately before** the function
2. Format starts with `// hotspots-ignore:`
3. Reason is **required** after the colon (warning if missing)
4. Blank lines between comment and function break the suppression

## Examples

### Valid Suppressions

```typescript
// hotspots-ignore: complex algorithm with proven test coverage
function fibonacci(n: number): number {
  // ...
}

// hotspots-ignore: generated code from protocol buffers
class MessageHandler {
  handle() { /* ... */ }
}

// hotspots-ignore: legacy parser, migration to new implementation in progress
const parse = (input: string) => {
  // ...
};
```

### Invalid Suppressions

```typescript
// hotspots-ignore: reason here

function foo() { }  // ❌ Blank line breaks suppression

// This is a comment
// hotspots-ignore: reason
function bar() { }  // ❌ Other comment in between

// hotspots-ignore
function baz() { }  // ⚠️  Missing colon - treated as missing reason

// hotspots-ignore:
function qux() { }  // ⚠️  Warning: suppression without reason
```

## What Suppressions Affect

### Excluded From (Policy Filtering)

Suppressed functions are **excluded** from:

1. **Critical Introduction** - Won't fail if function becomes critical
2. **Excessive Risk Regression** - Won't fail if LRS increases by ≥1.0
3. **Watch Threshold** - No warning when entering watch range
4. **Attention Threshold** - No warning when entering attention range
5. **Rapid Growth** - No warning for rapid LRS increases

### Included In (Still Tracked)

Suppressed functions are **included** in:

1. **Analysis Reports** - Visible with `suppression_reason` field
2. **Net Repo Regression** - Counted in total repository LRS
3. **Snapshots** - Persisted to `.hotspots/snapshots/`
4. **HTML Reports** - Displayed with suppression indicator
5. **JSON Output** - Contains `suppression_reason` field

## Validation

### Missing Reason Warning

Functions suppressed without a reason trigger a warning:

```typescript
// hotspots-ignore:
function foo() { }
```

**Policy output:**
```json
{
  "warnings": [
    {
      "id": "suppression-missing-reason",
      "severity": "warning",
      "function_id": "src/foo.ts::foo",
      "message": "Function src/foo.ts::foo suppressed without reason"
    }
  ]
}
```

This is a **warning only** (non-blocking), but encourages documenting suppressions.

## JSON Output

Suppressed functions include a `suppression_reason` field:

```json
{
  "file": "src/legacy.ts",
  "function": "oldParser",
  "line": 42,
  "metrics": { "cc": 15, "nd": 5, "fo": 8, "ns": 3 },
  "lrs": 12.5,
  "band": "critical",
  "suppression_reason": "legacy code, refactor planned for Q2 2026"
}
```

**Notes:**
- Functions without suppressions omit this field (not `null`)
- Empty reason shows as `"suppression_reason": ""`

## Best Practices

### When to Suppress

✅ **Good reasons to suppress:**
- Complex algorithms with established test coverage
- Legacy code pending scheduled migration
- Generated code (protocol buffers, GraphQL, etc.)
- Intentionally complex code (e.g., optimized parsers, state machines)
- Well-documented algorithms (e.g., cryptographic functions)

### When NOT to Suppress

❌ **Bad reasons to suppress:**
- New code that should be refactored
- "I'll fix it later" without a concrete plan
- Code that could be simplified but you don't want to
- Avoiding code review feedback
- Hiding poor design choices

### Documentation Guidelines

**Good suppression reasons include:**

1. **What** - What makes this complex
2. **Why** - Why it's intentionally complex or not being fixed now
3. **When** - When it will be addressed (if applicable)

**Examples:**

```typescript
// ✅ Good: Specific, actionable, dated
// hotspots-ignore: RSA encryption algorithm, well-tested, cannot be simplified

// ✅ Good: Clear plan
// hotspots-ignore: legacy parser, migration to TreeSitter in Q2 2026

// ❌ Bad: Vague, no plan
// hotspots-ignore: TODO fix this later

// ❌ Bad: No reason
// hotspots-ignore:
```

### Code Review Guidelines

**Require review for suppressions:**
1. All new suppressions should be reviewed
2. Ensure reason is documented and valid
3. Verify suppression is the right choice (vs. refactoring)
4. Consider adding a tracking issue/ticket

**Periodic audits:**
- Review suppressed functions quarterly
- Check if reasons are still valid
- Remove suppressions when code is refactored
- Update reasons if plans change

## Technical Details

### Determinism

Suppression extraction is **fully deterministic**:
- Pure function of (source code, function span, source map)
- No I/O, randomness, or timestamps
- Same source → same suppression → same results
- Byte-for-byte identical snapshots

### Persistence

Suppressions are persisted in snapshots:
- `FunctionSnapshot.suppression_reason` field
- `FunctionDeltaEntry.suppression_reason` field
- Tracked across commits in delta mode
- Auditable in git history

### Schema Compatibility

Suppression fields are **backward compatible**:
- Optional fields with `skip_serializing_if = "Option::is_none"`
- Old snapshots work with new code (field is `None`)
- New snapshots work with old code (field is ignored)
- No schema version bump required

## Examples in Different Contexts

### CI/CD Integration

```yaml
# .github/workflows/complexity.yml
- name: Check complexity
  run: |
    hotspots analyze . --mode delta --policies --format json > delta.json

    # Exit code 1 if any blocking policies failed
    # Suppressed functions won't cause failures
```

### Delta Mode

When comparing commits, suppression status comes from the **current** version:

```typescript
// Commit A
function foo() { }  // Not suppressed

// Commit B
// hotspots-ignore: newly suppressed
function foo() { }  // Now suppressed - won't trigger policies
```

### Refactoring Workflow

```bash
# 1. Suppress complex function before refactor
# Add: // hotspots-ignore: refactoring in progress

# 2. Refactor the function
# ... make changes ...

# 3. Remove suppression comment
# Function now subject to policies again
```

## Troubleshooting

### Suppression Not Working

**Check:**
1. Comment is immediately before function (no blank lines)
2. Format is correct: `// hotspots-ignore: reason`
3. Running in delta mode with `--policies` flag
4. Verify in JSON output: `suppression_reason` field present

### Warning About Missing Reason

**Fix:**
```typescript
// Before (warning)
// hotspots-ignore:
function foo() { }

// After (no warning)
// hotspots-ignore: legacy code, refactor planned
function foo() { }
```

### Suppression Applies to Wrong Function

**Cause:** Comment on wrong line or blank line in between

**Fix:**
```typescript
// Wrong
// hotspots-ignore: reason

function foo() { }  // Suppression not applied

// Correct
// hotspots-ignore: reason
function foo() { }  // Suppression applied
```

## See Also

- [Policy Engine Documentation](USAGE.md#policy-engine)
- [Configuration Guide](USAGE.md#configuration)
- [CI/CD Integration](USAGE.md#cicd-integration)
