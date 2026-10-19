# hotspots-core micro-benchmarks

Criterion benchmarks for the pipeline phases that dominate wall time, per the
`--timing-report` breakdown added in hotspots#267 part 1: `parse`, `metrics`,
and `cfg` (in that order), with `discover`/`scoring`/`aggregate`/`report`
negligible by comparison. This is a micro-benchmark harness for isolating
per-function cost in those three phases — not the macro benchmark (full
`analyze` run against a real corpus, wall-clock/memory via `runlog`) or the
CI regression gate, which are tracked as separate follow-up work.

## Running

```bash
cargo bench -p hotspots-core
```

Run a subset with a filter on the benchmark ID (`<group>/<language>/<size>`):

```bash
cargo bench -p hotspots-core -- parse/rust
cargo bench -p hotspots-core -- cfg/typescript/large
```

`cargo bench -p hotspots-core -- --test` runs every benchmark once without
collecting statistics — useful as a fast compile-and-smoke-test pass.

## What's covered

`hot_path.rs` benchmarks three groups, each across Rust (via `syn`) and
TypeScript (via SWC), at three fixed input sizes (small/medium/large):

- **`parse`** — `LanguageParser::parse` (the per-language parser entry
  point): raw source text to a `ParsedModule`.
- **`cfg`** — `get_builder_for_function(..).build(..)` for every function
  discovered in the module: control-flow graph construction.
- **`metrics`** — `extract_metrics` for every function, given its CFG.

Fixtures are deliberately fixed, not generated: small/medium are existing
`tests/fixtures/` files reused from the test suite, and large is a real
source file already in the repo (`hotspots-core/src/metrics.rs` for Rust,
`action/src/main.ts` for TypeScript) rather than a synthetic blob, so the
benchmarks track real-world function/branch shapes.

## Reading output

Criterion reports a confidence interval per benchmark, e.g.:

```
parse/rust/large        time:   [2.9402 ms 2.9465 ms 2.9559 ms]
```

The middle value is the point estimate; the outer two are the 95% confidence
interval. Compare runs by benchmark ID across commits/branches — Criterion
also writes HTML reports with historical comparison under
`target/criterion/<group>/<language>/<size>/report/index.html` when run
locally (requires `gnuplot` for plots; falls back to the `plotters` backend
otherwise).
