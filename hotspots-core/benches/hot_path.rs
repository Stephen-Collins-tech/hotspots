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
use hotspots_core::language::{
    CParser, CSharpParser, ECMAScriptParser, GoParser, JavaParser, PythonParser, RustParser,
};
use hotspots_core::metrics::extract_metrics;
use swc_common::sync::Lrc;
use swc_common::SourceMap;

/// A fixture file repeated 16 times, for languages with no large real source
/// file in the repo. None of the repeated fixtures carry imports, package or
/// using lines, so the concatenation stays valid.
macro_rules! repeated {
    ($path:literal) => {
        concat!(
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path),
            include_str!($path)
        )
    };
}

const RUST_SMALL: &str = include_str!("../../tests/fixtures/rust/simple.rs");
const RUST_MEDIUM: &str = include_str!("../../tests/fixtures/rust/methods.rs");
const RUST_LARGE: &str = include_str!("../src/metrics.rs");

const TS_SMALL: &str = include_str!("../../tests/fixtures/simple.ts");
const TS_MEDIUM: &str = include_str!("../../tests/fixtures/patterns_tier1.ts");
const TS_LARGE: &str = include_str!("../../action/src/main.ts");

const JAVA_SMALL: &str = include_str!("../../tests/fixtures/java/Simple.java");
const JAVA_MEDIUM: &str = include_str!("../../tests/fixtures/java/SwitchAndTernary.java");
const JAVA_LARGE: &str = repeated!("../../tests/fixtures/java/SwitchAndTernary.java");

const PYTHON_SMALL: &str = include_str!("../../tests/fixtures/python/simple.py");
const PYTHON_MEDIUM: &str = include_str!("../../tests/fixtures/python/python_specific.py");
const PYTHON_LARGE: &str = repeated!("../../tests/fixtures/python/python_specific.py");

const C_SMALL: &str = include_str!("../../tests/fixtures/c/simple.c");
const C_MEDIUM: &str = include_str!("../../tests/fixtures/c/control_flow.c");
const C_LARGE: &str = repeated!("../../tests/fixtures/c/control_flow.c");

const CSHARP_SMALL: &str = include_str!("../../tests/fixtures/csharp/Simple.cs");
const CSHARP_MEDIUM: &str = include_str!("../../tests/fixtures/csharp/Switches.cs");
const CSHARP_LARGE: &str = repeated!("../../tests/fixtures/csharp/Switches.cs");

const GO_SMALL: &str = include_str!("../../tests/fixtures/go/simple.go");
const GO_MEDIUM: &str = include_str!("../../tests/fixtures/go/go_specific.go");
// go_specific.go repeated would duplicate its `package`/`import` block, which
// tree-sitter-go's own grammar is not lenient about re-parsing cleanly, so
// `large` is pre-generated once (package/import stripped, body repeated 16x)
// rather than repeated at compile time like the other languages — see
// benches/generated/README.md.
const GO_LARGE: &str = include_str!("generated/go_large.go");

struct Language {
    name: &'static str,
    filename: &'static str,
    parser: Box<dyn LanguageParser>,
    fixtures: [(&'static str, &'static str); 3],
}

fn languages() -> Vec<Language> {
    vec![
        Language {
            name: "rust",
            filename: "fixture.rs",
            parser: Box::new(RustParser),
            fixtures: [
                ("small", RUST_SMALL),
                ("medium", RUST_MEDIUM),
                ("large", RUST_LARGE),
            ],
        },
        Language {
            name: "typescript",
            filename: "fixture.ts",
            parser: Box::new(ECMAScriptParser::new(Lrc::new(SourceMap::default()))),
            fixtures: [
                ("small", TS_SMALL),
                ("medium", TS_MEDIUM),
                ("large", TS_LARGE),
            ],
        },
        Language {
            name: "java",
            filename: "Fixture.java",
            parser: Box::new(JavaParser::new().expect("java parser")),
            fixtures: [
                ("small", JAVA_SMALL),
                ("medium", JAVA_MEDIUM),
                ("large", JAVA_LARGE),
            ],
        },
        Language {
            name: "python",
            filename: "fixture.py",
            parser: Box::new(PythonParser::new().expect("python parser")),
            fixtures: [
                ("small", PYTHON_SMALL),
                ("medium", PYTHON_MEDIUM),
                ("large", PYTHON_LARGE),
            ],
        },
        Language {
            name: "c",
            filename: "fixture.c",
            parser: Box::new(CParser::new().expect("c parser")),
            fixtures: [("small", C_SMALL), ("medium", C_MEDIUM), ("large", C_LARGE)],
        },
        Language {
            name: "csharp",
            filename: "Fixture.cs",
            parser: Box::new(CSharpParser::new().expect("csharp parser")),
            fixtures: [
                ("small", CSHARP_SMALL),
                ("medium", CSHARP_MEDIUM),
                ("large", CSHARP_LARGE),
            ],
        },
        Language {
            name: "go",
            filename: "fixture.go",
            parser: Box::new(GoParser::new().expect("go parser")),
            fixtures: [
                ("small", GO_SMALL),
                ("medium", GO_MEDIUM),
                ("large", GO_LARGE),
            ],
        },
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

    for lang in languages() {
        for (size, source) in lang.fixtures {
            group.bench_with_input(BenchmarkId::new(lang.name, size), source, |b, source| {
                b.iter(|| {
                    lang.parser
                        .parse(source, lang.filename)
                        .expect("must parse")
                });
            });
        }
    }

    group.finish();
}

fn bench_cfg(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfg");

    for lang in languages() {
        for (size, source) in lang.fixtures {
            let functions = discover_functions(lang.parser.as_ref(), source, lang.filename);
            group.bench_with_input(
                BenchmarkId::new(lang.name, size),
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
    }

    group.finish();
}

fn bench_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics");

    for lang in languages() {
        for (size, source) in lang.fixtures {
            let functions = discover_functions(lang.parser.as_ref(), source, lang.filename);
            let cfgs: Vec<_> = functions
                .iter()
                .map(|f| get_builder_for_function(f).build(f))
                .collect();
            let inputs: Vec<_> = functions.iter().zip(cfgs.iter()).collect();
            group.bench_with_input(BenchmarkId::new(lang.name, size), &inputs, |b, inputs| {
                b.iter(|| {
                    for (function, cfg) in inputs {
                        extract_metrics(function, cfg);
                    }
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_parse, bench_cfg, bench_metrics);
criterion_main!(benches);
