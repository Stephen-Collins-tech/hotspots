# Known Limitations

This document lists known limitations of hotspots MVP.

## Control Flow

### Break/Continue Statements

**Status:** Partially supported

- Break and continue statements are counted toward Non-Structured Exits (NS)
- However, they currently route to CFG exit rather than the correct loop exit/header
- Full loop context tracking is needed for proper CFG construction

**Impact:** NS metric is accurate, but CFG structure may be simplified for break/continue

### Labeled Break/Continue

**Status:** Supported but simplified

- Labeled break/continue are supported and counted
- Label resolution is static and deterministic
- However, routing to the correct labeled target is not fully implemented

**Impact:** Functions with labeled breaks/continues work but CFG may be simplified

## TypeScript Features

### JSX/TSX

**Status:** Not supported

- JSX/TSX syntax causes a parse error
- Analysis aborts when JSX is encountered

**Workaround:** Analyze only plain TypeScript files (`.ts`, not `.tsx`)

### Generator Functions

**Status:** Not supported

- Generator functions (`function*`) cause analysis errors
- Functions with generators are skipped

**Impact:** Projects using generators will have incomplete analysis

### Experimental Decorators

**Status:** Not supported

- Experimental decorator syntax is disabled
- Standard decorators (ES2022) may work but are untested

**Impact:** Projects using experimental decorators may have parsing issues

## Analysis Scope

### Global Functions

**Status:** Analyzed but without context

- Global functions are analyzed independently
- No cross-function dependency analysis
- Module-level complexity is not measured

**Impact:** Functions are analyzed in isolation, which is intentional

### Async Functions

**Status:** Supported but simplified

- Async/await syntax is parsed correctly
- However, async control flow is treated as sequential
- Promise chains are not analyzed as control flow

**Impact:** Async functions are analyzed but complexity may be underestimated

### Type Information

**Status:** Not used

- Type annotations are parsed but not used in analysis
- Only structural control flow is analyzed
- Type-driven complexity (overloads, generics) is not measured

**Impact:** Type complexity does not affect LRS (this is intentional - structural only)

## Output

### Floating Point Precision

**Status:** Deterministic but platform-dependent

- Internal calculations use full `f64` precision
- JSON output preserves full precision
- Text output rounds to 2 decimal places
- Precision may vary slightly across platforms due to floating point representation

**Impact:** Minimal - results are deterministic within platform

### File Path Normalization

**Status:** Simplified

- Paths are normalized to absolute paths
- Symlinks are not resolved
- Case sensitivity follows filesystem rules

**Impact:** Results may vary slightly on case-insensitive filesystems

## Performance

### Large Codebases

**Status:** Not optimized

- Analysis is single-threaded
- Memory usage is not optimized
- No caching of parsed ASTs

**Impact:** Very large codebases may be slow to analyze

### Incremental Analysis

**Status:** Not supported

- Full analysis runs every time
- No incremental updates
- No change detection

**Impact:** Analyzing large codebases repeatedly may be slow

## Future Improvements

These limitations are planned for future versions:

1. **Full loop context tracking** for break/continue
2. **JSX/TSX support** for React/TypeScript projects
3. **Generator function analysis**
4. **Performance optimizations** for large codebases
5. **Incremental analysis** support
