# Known Limitations

## Language coverage

### Generator functions (JavaScript/TypeScript)

Generator functions (`function*`) parse and analyze correctly, but `yield` expressions are not counted as control-flow branches. CC will be slightly underestimated for generators with conditional yield paths.

### Async/await

Async functions are analyzed as sequential control flow. Promise chains are not traced as control flow paths. CC accurately reflects the synchronous branching structure of the function body; the implicit async error path is not counted.

### Type-level complexity

Type annotations, generics, and overloads are parsed but not factored into metrics. LRS measures structural control-flow complexity only — type complexity does not affect the score. This is intentional.

---

## Analysis scope

### Cross-function dependencies

Each function is analyzed in isolation. LRS does not account for the complexity of functions a function calls — that dimension is captured by Fan-Out (FO), which counts the number of distinct callees, not their internal complexity.

### Module-level code

Top-level module statements outside of function bodies are not analyzed as a function unit.

---

## Performance

### Large codebases

Analysis is single-threaded. Very large repos (100k+ functions) will be slow. Most real-world codebases complete in under 30 seconds.

### No incremental analysis

Every run re-analyzes all matched files. Delta mode compares outputs between runs — it does not skip re-analyzing unchanged files.

---

## Output

### Floating point

LRS values are computed in full `f64` precision. Text output rounds to 1 decimal place. Results are deterministic within a platform; rare floating-point edge cases may produce sub-0.001 differences across architectures.

### Symlinks

File paths are not symlink-resolved. Results may vary if the same file is reachable via multiple paths.
