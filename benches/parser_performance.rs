// benches/parser_performance.rs
// Benchmarks for the AetherShell parser

use aethershell::parser::parse_program;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn benchmark_parser_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_simple");

    // Simple expressions
    group.bench_function("literal_int", |b| b.iter(|| parse_program(black_box("42"))));

    group.bench_function("literal_string", |b| {
        b.iter(|| parse_program(black_box(r#""hello world""#)))
    });

    group.bench_function("literal_array", |b| {
        b.iter(|| parse_program(black_box("[1, 2, 3, 4, 5]")))
    });

    group.bench_function("arithmetic", |b| {
        b.iter(|| parse_program(black_box("1 + 2 * 3 - 4 / 5")))
    });

    group.bench_function("comparison", |b| {
        b.iter(|| parse_program(black_box("x > 10 && y < 20 || z == 30")))
    });

    group.bench_function("assignment", |b| {
        b.iter(|| parse_program(black_box("x = 42")))
    });

    group.bench_function("function_call", |b| {
        b.iter(|| parse_program(black_box(r#"print("hello")"#)))
    });

    group.bench_function("lambda", |b| {
        b.iter(|| parse_program(black_box("fn(x) => x * 2")))
    });

    group.finish();
}

fn benchmark_parser_complex(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_complex");

    // Pipeline expressions
    group.bench_function("pipeline_simple", |b| {
        b.iter(|| parse_program(black_box("[1,2,3] | map(fn(x) => x * 2)")))
    });

    group.bench_function("pipeline_chain", |b| {
        b.iter(|| {
            parse_program(black_box(
                "[1,2,3,4,5] | where(fn(x) => x > 2) | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)",
            ))
        })
    });

    // Record expressions
    group.bench_function("record", |b| {
        b.iter(|| parse_program(black_box(r#"{ name: "test", value: 42, active: true }"#)))
    });

    // Match expressions
    group.bench_function("match_simple", |b| {
        b.iter(|| {
            parse_program(black_box(
                r#"match x { 1 => "one", 2 => "two", _ => "other" }"#,
            ))
        })
    });

    // If expressions
    group.bench_function("if_else", |b| {
        b.iter(|| parse_program(black_box("if x > 10 { x * 2 } else { x / 2 }")))
    });

    // Nested expressions
    group.bench_function("nested_lambdas", |b| {
        b.iter(|| parse_program(black_box("fn(x) => fn(y) => fn(z) => x + y + z")))
    });

    group.finish();
}

fn benchmark_parser_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_scaling");

    // Test parsing performance with increasing array sizes
    for size in [10, 50, 100, 500] {
        let array_str = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        group.bench_with_input(BenchmarkId::new("array", size), &array_str, |b, code| {
            b.iter(|| parse_program(black_box(code)))
        });
    }

    // Test parsing with multiple statements
    for size in [5, 10, 20, 50] {
        let code = (1..=size)
            .map(|i| format!("x{} = {}", i, i))
            .collect::<Vec<_>>()
            .join("\n");

        group.bench_with_input(BenchmarkId::new("statements", size), &code, |b, code| {
            b.iter(|| parse_program(black_box(code)))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_parser_simple,
    benchmark_parser_complex,
    benchmark_parser_scaling
);
criterion_main!(benches);
