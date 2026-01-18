# TypeScript Syntax Support

This document describes the TypeScript syntax features supported by faultline's parser.

## Parser Configuration

faultline uses `swc_ecma_parser` version 33.0.0 with the following configuration:

- **Syntax**: TypeScript only (no JavaScript)
- **JSX**: Disabled (not supported in MVP)
- **Experimental Decorators**: Disabled
- **ES Version**: ES2022
- **Declaration Files**: Supported (`.d.ts` files)

## Supported TypeScript Features

The parser supports all standard TypeScript syntax features including:

### Type Annotations
- Function parameter and return types
- Variable type annotations
- Class property types
- Type assertions (`as`, `<>`)

### Advanced Types
- Union types (`|`)
- Intersection types (`&`)
- Generic types (`<T>`)
- Conditional types
- Mapped types
- Template literal types

### Type Declarations
- Interface declarations
- Type alias declarations
- Enum declarations
- Namespace declarations

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

### Other Features
- Classes (public, private, protected members)
- Abstract classes and methods
- Optional chaining (`?.`)
- Nullish coalescing (`??`)
- Async/await
- Generators (`function*`)

## Explicitly Ignored Constructs

The following constructs are parsed but **ignored** in analysis (not counted as functions):

- **Interfaces**: Type-only declarations without runtime behavior
- **Type aliases**: Type-only declarations
- **Overload signatures without bodies**: Declaration-only function signatures
- **Ambient declarations**: `declare` statements

## Unsupported Features (MVP)

The following features are **not supported** in the MVP:

- **JSX/TSX**: JSX syntax will cause a parse error. Only plain TypeScript files (`.ts`) are supported.
- **Experimental Decorators**: Standard decorators are supported, experimental decorators are disabled.
- **Generator Functions (`function*`)**: Encountering a generator will emit an error and skip that function.

See [limitations.md](limitations.md) for full details on limitations and their impact.

## Error Handling

### JSX Syntax
If JSX syntax is encountered in the source:
- The parser will emit a clear error message
- Analysis of that file will abort
- Error message: "JSX syntax is not supported in MVP. Plain TypeScript only."

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

The parser configuration is defined in `faultline-core/src/parser.rs`:

```rust
Syntax::Typescript(TsSyntax {
    tsx: false,        // No JSX support
    decorators: false, // No experimental decorators
    dts: true,         // Allow .d.ts files
    ..Default::default()
})
```

## Future Support

Future versions may add support for:
- JSX/TSX syntax
- Generator function analysis
- Experimental decorator syntax
