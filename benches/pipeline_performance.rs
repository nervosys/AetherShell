// benches/pipeline_performance.rs
// Benchmarks for AetherShell pipeline execution

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

fn benchmark_pipeline_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_map");

    for size in [10, 100, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | map(fn(x) => x * 2)", array);

        group.bench_with_input(BenchmarkId::new("double", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_filter");

    for size in [10, 100, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | where(fn(x) => x % 2 == 0)", array);

        group.bench_with_input(BenchmarkId::new("even", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_reduce(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_reduce");

    for size in [10, 100, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | reduce(fn(a, b) => a + b, 0)", array);

        group.bench_with_input(BenchmarkId::new("sum", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_chain");

    // Short chain (2 operations)
    for size in [10, 100, 500] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | map(fn(x) => x * 2) | where(fn(x) => x > 10)", array);

        group.bench_with_input(BenchmarkId::new("map_filter", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    // Long chain (4 operations)
    for size in [10, 100, 500] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!(
            "{} | map(fn(x) => x * 2) | where(fn(x) => x > 10) | map(fn(x) => x + 1) | reduce(fn(a, b) => a + b, 0)",
            array
        );

        group.bench_with_input(BenchmarkId::new("full_chain", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_array_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_array_ops");

    for size in [10, 100, 500] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // First/last operations
        let first_code = format!("{} | first()", array);
        group.bench_with_input(BenchmarkId::new("first", size), &first_code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });

        let last_code = format!("{} | last()", array);
        group.bench_with_input(BenchmarkId::new("last", size), &last_code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });

        // Flatten
        let nested = format!(
            "[{}]",
            (1..=(size / 5))
                .map(|_| "[1,2,3,4,5]")
                .collect::<Vec<_>>()
                .join(", ")
        );
        let flatten_code = format!("{} | flatten()", nested);
        group.bench_with_input(
            BenchmarkId::new("flatten", size),
            &flatten_code,
            |b, code| {
                let mut env = create_test_env();
                b.iter(|| parse_and_eval(black_box(code), &mut env))
            },
        );

        // Reverse
        let reverse_code = format!("{} | reverse()", array);
        group.bench_with_input(
            BenchmarkId::new("reverse", size),
            &reverse_code,
            |b, code| {
                let mut env = create_test_env();
                b.iter(|| parse_and_eval(black_box(code), &mut env))
            },
        );

        // Unique
        let with_dups = format!(
            "[{}]",
            (1..=size)
                .map(|i| (i % 10).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let unique_code = format!("{} | unique()", with_dups);
        group.bench_with_input(BenchmarkId::new("unique", size), &unique_code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_sort(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_sort");

    for size in [10, 100, 500, 1000] {
        // Random-ish order
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| ((i * 17) % size).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | sort()", array);

        group.bench_with_input(BenchmarkId::new("integers", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    // Already sorted (best case)
    for size in [100, 500, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | sort()", array);

        group.bench_with_input(BenchmarkId::new("sorted", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    // Reverse sorted (worst case for some algorithms)
    for size in [100, 500, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .rev()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let code = format!("{} | sort()", array);

        group.bench_with_input(BenchmarkId::new("reversed", size), &code, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });
    }

    group.finish();
}

fn benchmark_pipeline_any_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_any_all");

    for size in [10, 100, 1000] {
        let array = format!(
            "[{}]",
            (1..=size)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // any - early exit (first element matches)
        let any_early = format!("{} | any(fn(x) => x == 1)", array);
        group.bench_with_input(
            BenchmarkId::new("any_early", size),
            &any_early,
            |b, code| {
                let mut env = create_test_env();
                b.iter(|| parse_and_eval(black_box(code), &mut env))
            },
        );

        // any - late exit (last element matches)
        let any_late = format!("{} | any(fn(x) => x == {})", array, size);
        group.bench_with_input(BenchmarkId::new("any_late", size), &any_late, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });

        // any - no match
        let any_none = format!("{} | any(fn(x) => x > {})", array, size);
        group.bench_with_input(BenchmarkId::new("any_none", size), &any_none, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });

        // all - all match
        let all_true = format!("{} | all(fn(x) => x > 0)", array);
        group.bench_with_input(BenchmarkId::new("all_true", size), &all_true, |b, code| {
            let mut env = create_test_env();
            b.iter(|| parse_and_eval(black_box(code), &mut env))
        });

        // all - first fails
        let all_false_early = format!("{} | all(fn(x) => x > 1)", array);
        group.bench_with_input(
            BenchmarkId::new("all_false_early", size),
            &all_false_early,
            |b, code| {
                let mut env = create_test_env();
                b.iter(|| parse_and_eval(black_box(code), &mut env))
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_pipeline_map,
    benchmark_pipeline_filter,
    benchmark_pipeline_reduce,
    benchmark_pipeline_chain,
    benchmark_pipeline_array_ops,
    benchmark_pipeline_sort,
    benchmark_pipeline_any_all
);
criterion_main!(benches);
