# Language Support

This document describes the languages and syntax features supported by Hotspots.

## Supported Languages

Hotspots supports multi-language analysis with consistent metrics across all languages:

- **ECMAScript** - TypeScript, JavaScript, and React (JSX/TSX) with full feature parity
- **Go** - Full Go language support including goroutines, defer, select, and channels
- **Rust** - Full Rust language support including match expressions, ? operator, unwrap/expect, and panic

Analysis metrics (CC, ND, FO, NS, LRS) are computed consistently across all supported languages.

## ECMAScript (TypeScript/JavaScript/React)

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

---

## Go Language Support

### Supported File Extensions

**Go:**
- `.go` - Go source files

### Parser

Hotspots uses `tree-sitter-go` version 0.23.2 for parsing Go source files. Tree-sitter provides:
- Error-tolerant parsing
- Precise syntax tree representation
- Fast incremental parsing

### Supported Features

#### Function Forms
All Go function forms are analyzed:
- Function declarations (`func name() {}`)
- Methods (receiver functions) (`func (t *Type) method() {}`)
- Value receiver methods (`func (t Type) method() {}`)
- Generic functions (Go 1.18+) (`func name[T any]() {}`)

**Note:** Anonymous functions (closures) are not yet supported.

#### Control Flow

All Go control flow constructs are fully supported:

**Conditionals:**
- `if` statements
- `if`/`else` chains
- `if`/`else if`/`else` ladders

**Loops:**
- `for` loops (traditional 3-clause)
- Range loops (`for _, item := range items`)
- While-style loops (`for condition {}`)
- Infinite loops (`for {}`)
- `break` and `continue` statements

**Switch Statements:**
- Expression switches (`switch x { ... }`)
- Type switches (`switch x.(type) { ... }`)
- Tagless switches (`switch { case x > 0: ... }`)
- Fallthrough support
- Multiple values per case

**Select Statements:**
- Channel select (`select { case <-ch: ... }`)
- Non-blocking select with `default`
- Send and receive cases

#### Go-Specific Constructs

**Defer:**
- `defer` statements are counted as **non-structured exits (NS)**
- Multiple defers are counted separately
- Defer contributes to fan-out if it calls a named function

**Goroutines:**
- `go` statements are counted as **fan-out (FO)**
- Each unique goroutine spawn is counted once
- Example: `go doWork()` increases FO by 1

**Panic/Recover:**
- `panic()` calls are counted as **non-structured exits (NS)**
- `recover()` calls are counted as **fan-out (FO)**

**Channels:**
- Channel operations in select statements contribute to **cyclomatic complexity (CC)**
- Channel sends/receives themselves don't directly affect metrics

### Metric Calculation

Go metrics are calculated using the same principles as other languages, with Go-specific adaptations:

#### Cyclomatic Complexity (CC)

Base formula: `CC = E - N + 2` (from Control Flow Graph)

Additional contributions:
- Each switch/select case: +1
- Each boolean operator (`&&`, `||`): +1
- Each `if` statement: counted in CFG
- Each loop: counted in CFG

**Example:**
```go
func process(x int) {
    if x > 0 && x < 100 {  // CC: +1 (if) +1 (&&) = +2
        switch x {
        case 1:            // CC: +1
            doA()
        case 2:            // CC: +1
            doB()
        default:           // CC: +1
            doC()
        }
    }
}
// Total CC = 1 (base) + 1 (if) + 1 (&&) + 3 (switch cases) = 6
```

#### Nesting Depth (ND)

Maximum depth of nested control structures:
- `if` statements
- `for` loops (all variants)
- `switch` statements (all types)
- `select` statements

**Example:**
```go
func nested() {
    if x > 0 {           // Depth 1
        for i := 0; i < 10; i++ {  // Depth 2
            if i > 5 {   // Depth 3
                select {
                case <-ch:  // Depth 4
                    // code
                }
            }
        }
    }
}
// ND = 4
```

#### Fan-Out (FO)

Count of unique function calls and goroutine spawns:
- Regular function calls
- Method calls
- `go` statements (each unique goroutine)

**Example:**
```go
func fanOut() {
    doWork()        // FO: +1
    doWork()        // FO: +0 (duplicate)
    doOther()       // FO: +1
    go doAsync()    // FO: +1 (goroutine)
    go doAsync()    // FO: +0 (duplicate goroutine)
}
// Total FO = 3 (doWork, doOther, go doAsync)
```

#### Non-Structured Exits (NS)

Count of exits that don't follow normal control flow:
- `return` statements (excluding final tail return)
- `defer` statements
- `panic()` calls
- Approximated via expression statements with calls

**Example:**
```go
func exits(x int) int {
    defer cleanup()   // NS: +1

    if x < 0 {
        return -1     // NS: +1 (early return)
    }

    if x == 0 {
        panic("zero") // NS: +1 (panic)
    }

    return x * 2      // NS: +0 (final tail return excluded)
}
// Total NS = 3
```

### Go-Specific Examples

#### Defer and Goroutines
```go
func processAsync(items []Item) {
    defer cleanup()  // NS: +1, FO: +1 (cleanup)

    for _, item := range items {
        go func(i Item) {  // FO: +1 (unique goroutine)
            process(i)      // FO: +1 (process)
        }(item)
    }
}
// CC=2 (base + loop), ND=1, FO=3, NS=1
```

#### Select Statement
```go
func selectExample(ch1, ch2 chan int) {
    select {
    case v := <-ch1:  // CC: +1
        handle(v)     // FO: +1
    case ch2 <- 42:   // CC: +1
        log()         // FO: +1
    default:          // CC: +1
        timeout()     // FO: +1
    }
}
// CC=4 (base + 3 cases), ND=1, FO=3, NS=0
```

#### Type Switch
```go
func typeSwitch(x interface{}) {
    switch v := x.(type) {
    case int:      // CC: +1
        handleInt(v)
    case string:   // CC: +1
        handleString(v)
    default:       // CC: +1
        handleOther(v)
    }
}
// CC=4, ND=1, FO=3, NS=0
```

### Implementation Details

The Go parser is implemented in `hotspots-core/src/language/go/`:
- `parser.rs` - Tree-sitter-based parser
- `cfg_builder.rs` - Control Flow Graph builder
- Metrics extracted in `hotspots-core/src/metrics.rs` (`extract_go_metrics()`)

### Unsupported Features

The following Go features are **not yet supported**:
- **Anonymous functions/closures** - Will be added in future release
- **Function literals** - Same as above
- **Label handling** - Labeled breaks/continues to specific loops

### Error Handling

Parse errors in Go files are handled gracefully:
- Tree-sitter provides error-tolerant parsing
- Functions with parse errors are skipped
- Analysis continues with remaining valid functions
- Error messages indicate the failure point

---

## Rust Language Support

### Supported File Extensions

**Rust:**
- `.rs` - Rust source files

### Parser

Hotspots uses `syn` version 2.0 for parsing Rust source files. Syn is the same parser used by rustc and provides:
- Accurate Rust grammar support
- Full feature coverage (syn's `full` feature enabled)
- Precise source location tracking
- Graceful error handling

### Supported Features

#### Function Forms
All Rust function forms are analyzed:
- Function declarations (`fn name() {}`)
- Methods (in `impl` blocks) (`impl Type { fn method(&self) {} }`)
- Associated functions (`impl Type { fn new() -> Self {} }`)
- Async functions (`async fn name() {}`)

**Note:** Closures and anonymous functions are not yet supported.

#### Control Flow

All Rust control flow constructs are fully supported:

**Conditionals:**
- `if` expressions
- `if`/`else` chains
- `if`/`else if`/`else` ladders

**Loops:**
- `loop` expressions (infinite loops)
- `while` loops
- `for` loops (iterator-based)
- `break` and `continue` statements

**Match Expressions:**
- Match expressions (`match x { ... }`)
- Pattern matching with guards
- Match arms with multiple patterns
- Exhaustive matching

#### Rust-Specific Constructs

**Question Mark Operator (`?`):**
- The `?` operator is counted as a **non-structured exit (NS)**
- Multiple `?` in the same expression are counted separately
- Works with both `Option` and `Result` types

**Unwrap/Expect:**
- `.unwrap()` calls are counted as **non-structured exits (NS)**
- `.expect()` calls are counted as **non-structured exits (NS)**
- These represent potential panic points

**Panic:**
- `panic!()` macro invocations are counted as **non-structured exits (NS)**
- Multiple panic sites are counted separately

**Macros:**
- Macro invocations (e.g., `println!()`) are counted as **fan-out (FO)**
- Macros are not expanded; treated as function calls

### Metric Calculation

Rust metrics are calculated using the same principles as other languages, with Rust-specific adaptations:

#### Cyclomatic Complexity (CC)

Base formula: `CC = E - N + 2` (from Control Flow Graph)

Additional contributions:
- Each match arm: +1
- Each boolean operator (`&&`, `||`): +1
- Each `if` expression: counted in CFG
- Each loop: counted in CFG

**Example:**
```rust
fn process(x: i32) {
    if x > 0 && x < 100 {  // CC: +1 (if) +1 (&&) = +2
        match x {
            1 => do_a(),       // CC: +1
            2 => do_b(),       // CC: +1
            _ => do_c(),       // CC: +1
        }
    }
}
// Total CC = 1 (base) + 1 (if) + 1 (&&) + 3 (match arms) = 6
```

#### Nesting Depth (ND)

Maximum depth of nested control structures:
- `if` expressions
- `loop` / `while` / `for` loops
- `match` expressions

**Example:**
```rust
fn nested() {
    if x > 0 {              // Depth 1
        for i in 0..10 {    // Depth 2
            if i > 5 {      // Depth 3
                match i {   // Depth 4
                    6 => {},
                    _ => {},
                }
            }
        }
    }
}
// ND = 4
```

#### Fan-Out (FO)

Count of unique function calls, method calls, and macro invocations:
- Regular function calls
- Method calls
- Macro invocations (e.g., `println!`, `panic!`)

**Example:**
```rust
fn fan_out() {
    do_work();        // FO: +1
    do_work();        // FO: +0 (duplicate)
    do_other();       // FO: +1
    println!("hi");   // FO: +1 (macro)
}
// Total FO = 3 (do_work, do_other, println!)
```

#### Non-Structured Exits (NS)

Count of exits that don't follow normal control flow:
- `return` statements (excluding final tail return)
- `?` operator usages
- `.unwrap()` calls
- `.expect()` calls
- `panic!()` macro invocations

**Example:**
```rust
fn exits(x: Option<i32>) -> Option<i32> {
    let value = x?;           // NS: +1 (?)

    if value < 0 {
        return None;          // NS: +1 (early return)
    }

    if value == 0 {
        panic!("zero");       // NS: +1 (panic)
    }

    Some(value * 2)           // NS: +0 (final tail expression excluded)
}
// Total NS = 3
```

### Rust-Specific Examples

#### Match Expressions
```rust
fn handle_result(res: Result<i32, String>) -> i32 {
    match res {
        Ok(n) if n > 0 => n * 2,     // CC: +1
        Ok(n) => n,                   // CC: +1
        Err(_) => -1,                 // CC: +1
    }
}
// CC=4 (base + 3 match arms), ND=1, FO=0, NS=0
```

#### Question Mark Operator
```rust
fn parse_and_process(input: &str) -> Result<i32, String> {
    let num = input.parse::<i32>()?;  // NS: +1 (?)
    let result = validate(num)?;      // NS: +1 (?)
    Ok(result * 2)
}
// CC=1, ND=0, FO=2 (parse, validate), NS=2
```

#### Unwrap and Panic
```rust
fn risky_operation(opt: Option<i32>) -> i32 {
    let value = opt.unwrap();  // NS: +1 (unwrap)

    if value < 0 {
        panic!("negative");    // NS: +1 (panic)
    }

    value
}
// CC=2 (base + if), ND=1, FO=1 (panic!), NS=2
```

### Implementation Details

The Rust parser is implemented in `hotspots-core/src/language/rust/`:
- `parser.rs` - syn-based parser with full Rust support
- `cfg_builder.rs` - Control Flow Graph builder
- Metrics extracted in `hotspots-core/src/metrics.rs` (`extract_rust_metrics()`)

### Unsupported Features

The following Rust features are **not yet supported**:
- **Closures** - Anonymous functions and closures
- **If let / While let** - Pattern matching in conditional expressions
- **Async blocks** - Async closures and blocks

These features will be added in future releases.

### Error Handling

Parse errors in Rust files are handled gracefully:
- syn provides detailed error messages
- Functions with parse errors are skipped
- Analysis continues with remaining valid functions
- Error messages include line numbers and context

## Testing

Language support is validated with comprehensive test fixtures:

**ECMAScript:**
- `tests/fixtures/*.ts` - TypeScript test cases
- `tests/fixtures/js/*.js` - JavaScript equivalents
- `tests/fixtures/tsx/*.tsx` - TypeScript React components
- `tests/fixtures/jsx/*.jsx` - JavaScript React components

**Go:**
- `tests/fixtures/go/simple.go` - Basic functions and early returns
- `tests/fixtures/go/loops.go` - Loop variants and nesting
- `tests/fixtures/go/switch.go` - Switch statements and type switches
- `tests/fixtures/go/go_specific.go` - Defer, goroutines, select, panic/recover
- `tests/fixtures/go/methods.go` - Methods, interfaces, generics
- `tests/fixtures/go/boolean_ops.go` - Boolean operators and deep nesting

**Rust:**
- `tests/fixtures/rust/simple.rs` - Basic functions, if/else, early returns
- `tests/fixtures/rust/loops.rs` - Loop variants (loop, while, for) and nesting
- `tests/fixtures/rust/match.rs` - Match expressions and pattern matching
- `tests/fixtures/rust/rust_specific.rs` - ? operator, unwrap/expect, panic
- `tests/fixtures/rust/methods.rs` - Methods, impl blocks, trait implementations
- `tests/fixtures/rust/boolean_ops.rs` - Boolean operators and complex conditions

**Golden File Tests:**
All languages have golden file tests that verify deterministic output:
- ECMAScript: 6 golden tests
- Go: 5 golden tests (go-simple, go-loops, go-switch, go-specific, determinism)
- Rust: 5 golden tests (rust-simple, rust-loops, rust-match, rust-specific, determinism)

**Test Coverage:**
- **209 total tests** across all languages
- 100% pass rate
- Determinism verified across multiple runs

## Future Support

### Planned Languages

**Priority:** P1 (Popular languages)
- **Python** - Using tree-sitter-python
- **Java** - Using tree-sitter-java
- **C/C++** - Using tree-sitter-c/cpp
- **Ruby** - Using tree-sitter-ruby

### Planned Features

ECMAScript:
- Generator function analysis (`function*`)
- Vue Single File Components (`.vue`)
- Svelte components (`.svelte`)
- Angular component templates

Go:
- Anonymous function/closure support
- Function literals in expressions

Rust:
- Closure support
- If let / while let expressions
- Async blocks

All languages:
- Incremental parsing for performance
- Parallel analysis across files
