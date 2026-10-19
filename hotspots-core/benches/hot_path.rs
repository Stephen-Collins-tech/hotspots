//! Micro-benchmarks for the hottest phases of the analyze pipeline
//! (hotspots#267 part 2). Targets are picked from the `--timing-report`
//! breakdown added in part 1: on a real analyze run, `parse`, `metrics`,
//! and `cfg` dominate wall time (in that order), while `discover`,
//! `scoring`, `aggregate`, and `report` are negligible. See
//! `benches/README.md` for how to run these and read the output.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use hotspots_core::ast::FunctionNode;
use hotspots_core::language::cfg_builder::get_builder_for_function;
use hotspots_core::language::parser::LanguageParser;
use hotspots_core::language::{ECMAScriptParser, RustParser};
use hotspots_core::metrics::extract_metrics;
use swc_common::sync::Lrc;
use swc_common::SourceMap;

const RUST_SMALL: &str = include_str!("../../tests/fixtures/rust/simple.rs");
const RUST_MEDIUM: &str = include_str!("../../tests/fixtures/rust/methods.rs");
const RUST_LARGE: &str = include_str!("../src/metrics.rs");

const TS_SMALL: &str = include_str!("../../tests/fixtures/simple.ts");
const TS_MEDIUM: &str = include_str!("../../tests/fixtures/patterns_tier1.ts");
const TS_LARGE: &str = include_str!("../../action/src/main.ts");

fn rust_fixtures() -> [(&'static str, &'static str); 3] {
    [
        ("small", RUST_SMALL),
        ("medium", RUST_MEDIUM),
        ("large", RUST_LARGE),
    ]
}

fn typescript_fixtures() -> [(&'static str, &'static str); 3] {
    [
        ("small", TS_SMALL),
        ("medium", TS_MEDIUM),
        ("large", TS_LARGE),
    ]
}

fn discover_functions(
    parser: &dyn LanguageParser,
    source: &str,
    filename: &str,
) -> Vec<FunctionNode> {
    let module = parser.parse(source, filename).expect("fixture must parse");
    module.discover_functions(0, source)
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    let rust_parser = RustParser;
    for (size, source) in rust_fixtures() {
        group.bench_with_input(BenchmarkId::new("rust", size), source, |b, source| {
            b.iter(|| rust_parser.parse(source, "fixture.rs").expect("must parse"));
        });
    }

    let ts_parser = ECMAScriptParser::new(Lrc::new(SourceMap::default()));
    for (size, source) in typescript_fixtures() {
        group.bench_with_input(BenchmarkId::new("typescript", size), source, |b, source| {
            b.iter(|| ts_parser.parse(source, "fixture.ts").expect("must parse"));
        });
    }

    group.finish();
}

fn bench_cfg(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfg");

    let rust_parser = RustParser;
    for (size, source) in rust_fixtures() {
        let functions = discover_functions(&rust_parser, source, "fixture.rs");
        group.bench_with_input(
            BenchmarkId::new("rust", size),
            &functions,
            |b, functions| {
                b.iter(|| {
                    for function in functions {
                        get_builder_for_function(function).build(function);
                    }
                });
            },
        );
    }

    let ts_parser = ECMAScriptParser::new(Lrc::new(SourceMap::default()));
    for (size, source) in typescript_fixtures() {
        let functions = discover_functions(&ts_parser, source, "fixture.ts");
        group.bench_with_input(
            BenchmarkId::new("typescript", size),
            &functions,
            |b, functions| {
                b.iter(|| {
                    for function in functions {
                        get_builder_for_function(function).build(function);
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics");

    let rust_parser = RustParser;
    for (size, source) in rust_fixtures() {
        let functions = discover_functions(&rust_parser, source, "fixture.rs");
        let cfgs: Vec<_> = functions
            .iter()
            .map(|f| get_builder_for_function(f).build(f))
            .collect();
        let inputs: Vec<_> = functions.iter().zip(cfgs.iter()).collect();
        group.bench_with_input(BenchmarkId::new("rust", size), &inputs, |b, inputs| {
            b.iter(|| {
                for (function, cfg) in inputs {
                    extract_metrics(function, cfg);
                }
            });
        });
    }

    let ts_parser = ECMAScriptParser::new(Lrc::new(SourceMap::default()));
    for (size, source) in typescript_fixtures() {
        let functions = discover_functions(&ts_parser, source, "fixture.ts");
        let cfgs: Vec<_> = functions
            .iter()
            .map(|f| get_builder_for_function(f).build(f))
            .collect();
        let inputs: Vec<_> = functions.iter().zip(cfgs.iter()).collect();
        group.bench_with_input(
            BenchmarkId::new("typescript", size),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    for (function, cfg) in inputs {
                        extract_metrics(function, cfg);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_parse, bench_cfg, bench_metrics);
criterion_main!(benches);
