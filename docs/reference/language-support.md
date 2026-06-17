# Language Support

Hotspots supports seven languages. All produce the same metrics (CC, ND, FO, NS, LRS) with consistent semantics.

## Supported languages

| Language | File extensions |
|----------|----------------|
| TypeScript | `.ts` `.tsx` `.mts` `.cts` |
| JavaScript | `.js` `.jsx` `.mjs` `.cjs` |
| Go | `.go` |
| Python | `.py` |
| Rust | `.rs` |
| Java | `.java` |
| C | `.c` `.h` |

---

## Language notes

### TypeScript / JavaScript

JSX and TSX are fully supported. Short-circuit operators (`&&`, `||`) and ternaries count toward CC. Arrow functions, class methods, and standalone functions are all analyzed.

Arrow functions and function expressions assigned to a variable or property are named after their binding — `const validate = (x) => …` appears in output as `validate`, not as an anonymous function. This applies to `.ts`, `.tsx`, `.mts`, and `.cts` files.

### Go

Goroutines, defer, select, and channel operations are supported. Each `case` in a `select` counts toward CC.

### Python

Async/await, comprehensions, context managers, and `match` statements (Python 3.10+) are supported. Comprehensions with conditionals contribute to CC.

### Rust

`match` arms, `?` operator, `unwrap`/`expect`, closures, and `impl` blocks are supported. Each `match` arm counts toward CC.

### Java

Java 8+ including lambdas, streams, try-with-resources, and switch expressions (Java 14+). Lambda bodies are analyzed as separate scopes.

### C

Standard C with support for all control flow constructs: `if`/`else`, `switch`, `for`, `while`, `do`/`while`, `goto`. Ternary operators and boolean short-circuit operators (`&&`, `||`) count toward CC. Header files are analyzed when function definitions are present.

---

## What counts as a function

Hotspots analyzes named, callable units of code:

- Named functions and methods
- Class methods and constructors
- Arrow functions assigned to a named variable or property
- Closures assigned to a named binding

Anonymous inline functions (callbacks passed directly to `.map()` or similar) are not analyzed as standalone units — their complexity folds into the containing named function's FO count.

---

## Exclusions

Hotspots automatically skips:

- Test files (`*.test.*`, `*_test.*`, `*spec*`, `tests/`, `__tests__/`)
- Vendored dependencies (`vendor/`, `node_modules/`, `third_party/`, `external/`, `contrib/`, `deps/`)
- Generated files (detected by heuristic — minified files, protobuf output, etc.)
- Type declaration files (`.d.ts`)

Override exclusions in `.hotspotsrc.json` with `exclude` patterns. See [Configuration](/guide/configuration).
