// benches/eval_performance.rs
// Benchmarks for the AetherShell evaluator

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn create_test_env() -> Env {
    Env::new()
}

fn parse_and_eval(code: &str, env: &mut Env) {
    if let Ok(stmts) = parse_program(code) {
        let _ = eval_program(&stmts, env);
    }
}

fn benchmark_eval_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_arithmetic");

    group.bench_function("add_ints", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("1 + 2"), &mut env))
    });

    group.bench_function("multiply_ints", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("3 * 4"), &mut env))
    });

    group.bench_function("complex_arithmetic", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("(1 + 2) * (3 - 4) / 5 + 6"), &mut env))
    });

    group.bench_function("float_arithmetic", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("3.14 * 2.0 + 1.5"), &mut env))
    });

    group.bench_function("modulo", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("17 % 5"), &mut env))
    });

    group.finish();
}

fn benchmark_eval_strings(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_strings");

    group.bench_function("concat_short", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box(r#""hello" + " " + "world""#), &mut env))
    });

    group.bench_function("string_repeat", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box(r#""ab" * 10"#), &mut env))
    });

    group.bench_function("string_builtin", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box(r#"str_upper("hello world")"#), &mut env))
    });

    group.finish();
}

fn benchmark_eval_arrays(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_arrays");

    group.bench_function("array_create_small", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("[1, 2, 3, 4, 5]"), &mut env))
    });

    group.bench_function("array_index", |b| {
        let mut env = create_test_env();
        // Set up array in env first
        parse_and_eval("arr = range(1, 100)", &mut env);
        b.iter(|| parse_and_eval(black_box("arr[50]"), &mut env))
    });

    group.bench_function("array_len", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("len([1,2,3,4,5])"), &mut env))
    });

    group.bench_function("range", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("range(1, 100)"), &mut env))
    });

    group.finish();
}

fn benchmark_eval_records(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_records");

    group.bench_function("record_create", |b| {
        let mut env = create_test_env();
        b.iter(|| {
            parse_and_eval(
                black_box(r#"{ name: "test", value: 42, active: true }"#),
                &mut env,
            )
        })
    });

    group.bench_function("record_field_access", |b| {
        let mut env = create_test_env();
        // Set up record in env first
        parse_and_eval(r#"rec = { name: "test", value: 42 }"#, &mut env);
        b.iter(|| parse_and_eval(black_box("rec.value"), &mut env))
    });

    group.finish();
}

fn benchmark_eval_lambdas(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_lambdas");

    group.bench_function("lambda_create", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("fn(x) => x * 2"), &mut env))
    });

    group.bench_function("lambda_call", |b| {
        let mut env = create_test_env();
        // First define the lambda
        parse_and_eval("double = fn(x) => x * 2", &mut env);
        b.iter(|| parse_and_eval(black_box("double(21)"), &mut env))
    });

    group.bench_function("lambda_nested", |b| {
        let mut env = create_test_env();
        parse_and_eval("add = fn(x) => fn(y) => x + y", &mut env);
        b.iter(|| parse_and_eval(black_box("add(5)(3)"), &mut env))
    });

    group.finish();
}

fn benchmark_eval_conditionals(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_conditionals");

    group.bench_function("if_true", |b| {
        let mut env = create_test_env();
        b.iter(|| parse_and_eval(black_box("if true { 1 } else { 2 }"), &mut env))
    });

    group.bench_function("if_comparison", |b| {
        let mut env = create_test_env();
        parse_and_eval("x = 15", &mut env);
        b.iter(|| parse_and_eval(black_box("if x > 10 { x * 2 } else { x }"), &mut env))
    });

    group.bench_function("match_simple", |b| {
        let mut env = create_test_env();
        parse_and_eval("x = 2", &mut env);
        b.iter(|| {
            parse_and_eval(
                black_box(r#"match x { 1 => "one", 2 => "two", _ => "other" }"#),
                &mut env,
            )
        })
    });

    group.finish();
}

fn benchmark_eval_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_scaling");

    // Arithmetic operations scaling
    for size in [10, 50, 100] {
        let expr = (1..=size)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" + ");

        group.bench_with_input(BenchmarkId::new("sum_chain", size), &expr, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    // Array creation scaling
    for size in [10, 50, 100, 500] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        group.bench_with_input(BenchmarkId::new("array_create", size), &array, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_eval_arithmetic,
    benchmark_eval_strings,
    benchmark_eval_arrays,
    benchmark_eval_records,
    benchmark_eval_lambdas,
    benchmark_eval_conditionals,
    benchmark_eval_scaling
);
criterion_main!(benches);
