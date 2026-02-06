# Language Support

This document describes the TypeScript, JavaScript, and JSX/TSX syntax features supported by hotspots's parser.

## Supported Languages

Hotspots analyzes **TypeScript**, **JavaScript**, and **React (JSX/TSX)** files with full feature parity. Analysis metrics (CC, ND, FO, NS, LRS) are computed identically across all languages.

### Supported File Extensions

**TypeScript:**
- `.ts` - TypeScript source files
- `.mts` - TypeScript ES modules
- `.cts` - TypeScript CommonJS modules
- `.d.ts` - Type declaration files (excluded from analysis)

**TypeScript with JSX (React):**
- `.tsx` - TypeScript React components
- `.mtsx` - TypeScript React ES modules
- `.ctsx` - TypeScript React CommonJS modules

**JavaScript:**
- `.js` - JavaScript source files
- `.mjs` - JavaScript ES modules
- `.cjs` - JavaScript CommonJS modules

**JavaScript with JSX (React):**
- `.jsx` - JavaScript React components
- `.mjsx` - JavaScript React ES modules
- `.cjsx` - JavaScript React CommonJS modules

## Parser Configuration

hotspots uses `swc_ecma_parser` version 33.0.0 with automatic language detection:

- **TypeScript files (.ts, .mts, .cts)**: Parsed with TypeScript syntax support (no JSX)
- **TSX files (.tsx, .mtsx, .ctsx)**: Parsed with TypeScript + JSX syntax support
- **JavaScript files (.js, .mjs, .cjs)**: Parsed with JavaScript syntax support (no JSX)
- **JSX files (.jsx, .mjsx, .cjsx)**: Parsed with JavaScript + JSX syntax support
- **Experimental Decorators**: Disabled
- **ES Version**: ES2022
- **Declaration Files**: `.d.ts` files are excluded from analysis

## Supported Features

### Common Features (TypeScript & JavaScript)

The following features are supported in both TypeScript and JavaScript:

### Function Forms
All function forms are supported and analyzed:
- Function declarations
- Function expressions
- Arrow functions
- Class methods (instance and static)
- Object literal methods
- Getter/setter methods

### Control Flow
- `if`/`else` statements
- `switch` statements
- `for` loops (standard, `for...in`, `for...of`)
- `while` loops
- `do...while` loops
- `try`/`catch`/`finally` blocks
- Labeled `break` and `continue`

### ES2022 Features
- Optional chaining (`?.`)
- Nullish coalescing (`??`)
- Async/await
- Generators (`function*`)
- Private class fields (`#field`)
- Static class blocks
- Top-level `await`
- Class static initialization blocks

### TypeScript-Only Features

When analyzing TypeScript files, the following additional features are supported:

- **Type Annotations**: Function parameters, return types, variable types
- **Advanced Types**: Union (`|`), intersection (`&`), generics, conditional types, mapped types
- **Type Declarations**: Interfaces, type aliases, enums, namespaces
- **Class Modifiers**: `public`, `private`, `protected`, `abstract`
- **Type Assertions**: `as`, `<>` syntax

**Note:** Type annotations do not affect complexity metrics. A TypeScript function and its JavaScript equivalent (with types removed) will have identical LRS scores.

## Explicitly Ignored Constructs

The following constructs are parsed but **ignored** in analysis (not counted as functions):

- **Interfaces**: Type-only declarations without runtime behavior
- **Type aliases**: Type-only declarations
- **Overload signatures without bodies**: Declaration-only function signatures
- **Ambient declarations**: `declare` statements

## JSX/TSX Support

### How JSX is Analyzed

Hotspots analyzes React components intelligently:

**JSX Elements do NOT inflate complexity:**
- Simple JSX markup (`<div>`, `<h1>`, etc.) does not increase complexity metrics
- JSX is treated as structured output, similar to template literals
- A component that just returns JSX has the same complexity as a function that returns a value

**Control Flow in JSX Expressions IS counted:**
- Ternary operators: `{condition ? <A/> : <B/>}` increases CC
- Logical AND: `{condition && <Component/>}` increases CC
- Map/filter with callbacks: Each callback function analyzed separately

**Example:**

```tsx
// Simple component - LRS = 1.0 (no complexity added by JSX elements)
function SimpleComponent() {
  return (
    <div>
      <h1>Title</h1>
      <p>Content</p>
    </div>
  );
}

// Conditional component - LRS > 1.0 (ternary adds CC)
function ConditionalComponent({ isActive }) {
  return (
    <div>
      {isActive ? <span>Active</span> : <span>Inactive</span>}
    </div>
  );
}
```

### Event Handlers and Callbacks

Anonymous functions in JSX (event handlers, map callbacks) are analyzed as separate functions:

```tsx
function ItemList({ items }) {
  return (
    <div>
      {items.map((item) => (  // This arrow function analyzed separately
        <div key={item.id} onClick={() => console.log(item)}>  // This too
          {item.name}
        </div>
      ))}
    </div>
  );
}
// Produces 3 function reports: ItemList, map callback, onClick callback
```

## Metric Parity

**Critical invariant**: TypeScript, JavaScript, JSX, and TSX files with identical structure produce **identical complexity metrics**.

Example - these two functions have identical LRS:

**TypeScript:**
```typescript
function calculate(x: number, y: number): number {
  if (x > 0 && y > 0 || x < 0) {
    return x + y;
  }
  return x * y;
}
```

**JavaScript:**
```javascript
function calculate(x, y) {
  if (x > 0 && y > 0 || x < 0) {
    return x + y;
  }
  return x * y;
}
```

Both yield: `CC=5, ND=1, FO=0, NS=1, LRS=5.48`

## Unsupported Features

The following features are **not yet supported**:

- **Experimental Decorators**: Standard decorators may be supported in future versions.
- **Generator Functions (`function*`)**: Encountering a generator will emit an error and skip that function.
- **Vue Single File Components**: `.vue` files are not supported yet.
- **Svelte Components**: `.svelte` files are not supported yet.

See [limitations.md](limitations.md) for full details on limitations and their impact.

## Error Handling

### JSX in Wrong File Extension
If JSX syntax is encountered in a `.ts` or `.js` file:
- The parser will emit a parse error
- Use `.tsx` for TypeScript + JSX
- Use `.jsx` for JavaScript + JSX
- Analysis of that file will abort with a clear error message

### Generator Functions
If a generator function (`function*`) is encountered:
- Analysis of that specific function is skipped
- An error is emitted for that function
- Analysis continues with remaining functions

### Parse Errors
General parse errors are reported with:
- Error message from the parser
- Context indicating the failure point

## Implementation Details

The parser automatically detects the file type based on extension and selects the appropriate syntax configuration (defined in `hotspots-core/src/parser.rs`):

**TypeScript files (.ts, .mts, .cts):**
```rust
Syntax::Typescript(TsSyntax {
    tsx: false,        // No JSX support yet
    decorators: false, // No experimental decorators
    dts: is_dts,       // Enable for .d.ts files
    ..Default::default()
})
```

**JavaScript files (.js, .mjs, .cjs):**
```rust
Syntax::Es(EsSyntax {
    jsx: false,        // No JSX support yet
    decorators: false, // No experimental decorators
    ..Default::default()
})
```

## Testing

Language parity is validated with parallel test fixtures:
- `tests/fixtures/*.ts` - TypeScript test cases
- `tests/fixtures/js/*.js` - JavaScript equivalents
- `tests/fixtures/tsx/*.tsx` - TypeScript React components
- `tests/fixtures/jsx/*.jsx` - JavaScript React components

All tests verify that equivalent code produces byte-for-byte identical metrics regardless of language.

## Future Support

Planned features for upcoming releases:
- Generator function analysis
- Vue Single File Components (`.vue`)
- Svelte components (`.svelte`)
- Angular component templates
