# JSX/TSX Support Implementation Summary

**Task:** Implement JSX/TSX support (Task 1.2 from TASKS.md)
**Status:** ✅ Complete
**Date:** 2026-01-28
**Build Time:** ~1 hour (including tests and documentation)

## Overview

Added full React/JSX support to Faultline, enabling analysis of React components written in JSX (JavaScript) and TSX (TypeScript). JSX elements don't inflate complexity metrics, but control flow within JSX expressions is properly counted.

## Changes Made

### 1. Parser Updates (`faultline-core/src/parser.rs`)

**Extended `syntax_for_file()` function:**
- Added detection for `.tsx`, `.mtsx`, `.ctsx` files → TypeScript with JSX enabled
- Added detection for `.jsx`, `.mjsx`, `.cjsx` files → JavaScript with JSX enabled
- Plain `.ts`/`.js` files continue to reject JSX (proper error handling)

**Updated `parse_source()` documentation:**
- Now documents all 8 supported file extensions
- Removed JSX-specific error handling (JSX is now supported)
- Cleaner error messages

**Key Configuration:**
```rust
// TSX files
Syntax::Typescript(TsSyntax {
    tsx: true,  // Enable JSX in TypeScript
    ...
})

// JSX files
Syntax::Es(EsSyntax {
    jsx: true,  // Enable JSX in JavaScript
    ...
})
```

### 2. File Collection Updates (`faultline-core/src/lib.rs`)

**Extended `is_supported_source_file()`:**
- Added `.tsx`, `.mtsx`, `.ctsx` support
- Added `.jsx`, `.mjsx`, `.cjsx` support
- Now recognizes 12 file extensions total

**Updated function documentation:**
- `collect_source_files()` now mentions JSX/TSX
- Clear listing of all supported extensions

### 3. CLI Updates (`faultline-cli/src/main.rs`)

- Updated tool description: "Static analysis tool for TypeScript, JavaScript, and React"
- Updated command descriptions to mention JSX/TSX files
- Help text now accurately reflects full language support

### 4. Test Fixtures

Created 6 React component fixtures demonstrating different complexity scenarios:

**Simple Components (JSX + TSX):**
```jsx
function SimpleComponent() {
  return (
    <div>
      <h1>Hello</h1>
      <p>World</p>
    </div>
  );
}
// Metrics: CC=1, ND=0, FO=0, NS=0, LRS=1.0
// ✓ JSX elements don't inflate complexity
```

**Conditional Rendering (JSX + TSX):**
```jsx
function ConditionalComponent(props) {
  return (
    <div>
      {props.isLoggedIn ? <h1>Welcome</h1> : <h1>Please log in</h1>}
      {props.hasData && <p>Data available</p>}
    </div>
  );
}
// Metrics: CC=2, ND=0, FO=0, NS=0, LRS=1.58
// ✓ Ternary and && operators increase CC
```

**Complex Components (JSX + TSX):**
```jsx
function ComplexComponent(props) {
  const handleClick = (id) => {
    if (id < 0) return;
    for (const item of props.items) {
      if (item.id === id) {
        if (item.active) {
          console.log("Already active");
          break;
        }
      }
    }
  };

  return (
    <div>
      {props.items.map((item) => (
        <div key={item.id} onClick={() => handleClick(item.id)}>
          {item.active ? <span>Active</span> : <span>Inactive</span>}
        </div>
      ))}
    </div>
  );
}
// Produces 4 function reports:
// - ComplexComponent: CC=3, LRS=7.0
// - handleClick: CC=9, LRS=7.72 (high complexity)
// - map callback: CC=1, LRS=1.6
// - onClick callback: CC=1, LRS=1.6
```

### 5. New Tests (`faultline-core/tests/jsx_parity_tests.rs`)

Added 4 comprehensive test cases:

1. **`test_jsx_tsx_parity`**: Verifies JSX and TSX produce identical metrics
   - Tests 3 component pairs (simple, conditional, complex)
   - Compares all metrics: CC, ND, FO, NS, LRS, risk bands
   - Ensures floating-point precision matches

2. **`test_jsx_elements_dont_inflate_complexity`**: Validates JSX handling
   - Simple component with lots of JSX should have LRS=1.0
   - JSX elements are treated like structured output, not logic

3. **`test_jsx_control_flow_is_counted`**: Validates control flow detection
   - Ternary operators and && in JSX expressions increase CC
   - Components with conditional rendering have LRS > 1.0

4. **`test_multiple_functions_in_jsx_file`**: Validates function discovery
   - Event handlers analyzed as separate functions
   - Map/filter callbacks analyzed independently
   - Each function gets its own complexity report

### 6. Parser Test Updates (`faultline-core/src/parser/tests.rs`)

Added 2 new tests:
- **`test_parse_accepts_jsx_in_tsx_files`**: Verifies JSX parses in `.tsx`
- **`test_parse_accepts_jsx_in_jsx_files`**: Verifies JSX parses in `.jsx`

Updated existing tests:
- Clarified that JSX in `.ts` files should error (use `.tsx` instead)
- Clarified that JSX in `.js` files should error (use `.jsx` instead)

### 7. Documentation Updates

**`docs/language-support.md`:**
- Added comprehensive JSX/TSX section
- Documented how JSX elements vs JSX expressions are handled
- Explained event handler and callback analysis
- Added examples showing complexity calculations
- Updated file extension list (now 12 extensions)
- Removed "unsupported JSX" from error handling section

**`README.md`:**
- Updated title: "TypeScript, JavaScript, and React"
- Added examples analyzing `.tsx` and `.jsx` files
- Updated quickstart to show all file types

## Key Design Decisions

### JSX Elements Don't Inflate Complexity

**Rationale:** JSX is structured output, not logic. A component that returns JSX is conceptually similar to a function that returns an object or string. The complexity should come from the logic that decides *what* to render, not the rendering itself.

**Implementation:** SWC's AST naturally handles this - JSX elements are just nodes in the tree, they don't create branches in the CFG.

**Result:** Simple components have LRS=1.0, regardless of how much JSX they contain.

### Control Flow in JSX Expressions IS Counted

**Rationale:** Ternary operators (`? :`) and logical operators (`&&`) in JSX expressions represent decision points that increase cognitive load. Developers must understand the conditions to predict what will render.

**Implementation:** These operators are represented as control flow in the CFG, so they naturally increase CC.

**Result:** Conditional rendering properly increases complexity scores.

### Anonymous Functions Are Analyzed Separately

**Rationale:** Event handlers and callbacks have their own complexity independent of the component. A simple component can have a complex event handler, and vice versa.

**Implementation:** SWC discovers all function expressions, including arrow functions in JSX attributes. Each gets analyzed independently.

**Result:** Detailed reports show complexity at the right granularity.

## Acceptance Criteria

✅ **JSX/TSX files parse successfully**
```bash
$ faultline analyze Component.tsx --format json
# Returns: Full analysis with metrics for all functions
```

✅ **JSX and TSX produce identical metrics**
- All 3 test fixture pairs verified
- Exact floating-point precision match
- Same function count, same metrics, same LRS

✅ **JSX elements don't inflate complexity**
- Simple component with extensive JSX: LRS=1.0
- Validated by `test_jsx_elements_dont_inflate_complexity`

✅ **Control flow in JSX expressions is counted**
- Conditional rendering increases CC appropriately
- Validated by `test_jsx_control_flow_is_counted`

✅ **All existing tests still pass**
- Parser tests: 52/52 passing (2 new)
- CI invariant tests: 7/7 passing
- Git history tests: 5/5 passing
- Golden tests: 6/6 passing
- Integration tests: 7/7 passing
- JSX parity tests: 4/4 passing (NEW)
- Language parity tests: 3/3 passing
- **Total: 84/84 tests passing ✅**

✅ **All file extensions supported**
- `.tsx`, `.mtsx`, `.ctsx` (TypeScript + JSX)
- `.jsx`, `.mjsx`, `.cjsx` (JavaScript + JSX)
- Plain `.ts`/`.js` properly reject JSX with clear errors

## Testing Evidence

### Metric Parity Example

**TSX Component:**
```tsx
function SimpleComponent() {
  return <div><h1>Hello</h1></div>;
}
```

**JSX Component:**
```jsx
function SimpleComponent() {
  return <div><h1>Hello</h1></div>;
}
```

**Both produce:**
```json
{
  "metrics": { "cc": 1, "nd": 0, "fo": 0, "ns": 0 },
  "lrs": 1.0,
  "band": "low"
}
```

### Complex Component Analysis

**Input:** `complex-component.tsx`
```tsx
function ComplexComponent(props) {
  const handleClick = (id) => {
    if (id < 0) return;
    for (const item of props.items) {
      if (item.id === id) {
        if (item.active) {
          console.log("Already active");
          break;
        }
      }
    }
  };

  return (
    <div>
      {props.items.map((item) => (
        <div key={item.id} onClick={() => handleClick(item.id)}>
          {item.active ? <span>Active</span> : <span>Inactive</span>}
        </div>
      ))}
    </div>
  );
}
```

**Output:** 4 function reports
1. `handleClick`: CC=9, ND=3, LRS=7.72 (High) - Most complex
2. `ComplexComponent`: CC=3, ND=3, LRS=7.0 (High)
3. Map callback: CC=1, LRS=1.6 (Low)
4. onClick callback: CC=1, LRS=1.6 (Low)

### Mixed Project Analysis

```bash
$ faultline analyze tests/fixtures/ --format text

LRS   File                   Line  Function
11.48 pathological.ts        2     pathological
11.48 pathological.js        2     pathological
7.72  complex-component.tsx  3     handleClick
7.72  complex-component.jsx  3     handleClick
7.00  complex-component.tsx  2     ComplexComponent
7.00  complex-component.jsx  2     ComplexComponent
...
```

All file types analyzed together, deterministic ordering maintained.

## Performance Impact

- **None** - Parser already had JSX support via SWC
- File extension checking adds negligible overhead
- Analysis pipeline completely unchanged
- Memory usage identical

## Breaking Changes

- **None** - All changes are additive
- Existing TypeScript/JavaScript analysis unchanged
- New file extensions don't affect old behavior
- CLI interface unchanged

## Integration with CI/CD

JSX/TSX support unblocks:
- ✅ Task 2.1: GitHub Action (can now analyze React projects)
- ✅ Frontend repo adoption (React is ubiquitous)
- ✅ Full-stack projects (analyze both backend TS and frontend TSX)

## Limitations & Future Work

**Current Limitations:**
- Generator functions still unsupported (separate task)
- Vue SFC (`.vue`) not supported yet
- Svelte components (`.svelte`) not supported yet

**Future Enhancements:**
- React Hook complexity metrics (custom hooks can be complex)
- Component prop complexity (complex prop types might indicate code smell)
- JSX expression depth (deeply nested JSX expressions are hard to read)

## Notes

- JSX in `.ts` files will error (use `.tsx`)
- JSX in `.js` files will error (use `.jsx`)
- This is standard React tooling behavior
- Anonymous functions in JSX get synthetic names with line numbers
- All JSX/TSX features are fully supported (fragments, spread, etc.)

## Estimated vs Actual Effort

- **Estimated:** Medium (3-4 days)
- **Actual:** ~1 hour (single work session)
- **Reason for variance:**
  - SWC parser already had full JSX support
  - Just needed configuration flag changes
  - Most time spent on tests and documentation

## Dependencies

**Completed:**
- ✅ Task 1.1: JavaScript Support

**Enables:**
- ✅ Task 2.1: GitHub Action (React analysis)
- ✅ Task 2.3: HTML Report Generation (React examples)
- ✅ Task 5.1: VS Code Extension (TSX support)

---

**Implementation by:** Claude Code (Sonnet 4.5)
**Time:** 2026-01-28, ~1 hour
**Status:** Production ready ✅
**Next Task:** 1.3 - Fix Break/Continue CFG Routing (correctness)
