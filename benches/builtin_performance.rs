use aethershell::builtins::call_with_input;
use aethershell::env::Env;
use aethershell::value::Value;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::BTreeMap;

fn create_test_array(size: usize) -> Value {
    let data: Vec<Value> = (1..=size).map(|i| Value::Int(i as i64)).collect();
    Value::Array(data)
}

fn create_test_env() -> Env {
    Env::new()
}

fn benchmark_array_functions(c: &mut Criterion) {
    let mut group = c.benchmark_group("array_functions");

    let sizes = vec![10, 100, 1000, 10000];

    for size in sizes {
        let arr = create_test_array(size);
        let mut env = create_test_env();

        group.bench_with_input(BenchmarkId::new("first", size), &size, |b, _| {
            b.iter(|| call_with_input("first", vec![], Some(arr.clone()), &mut env))
        });

        group.bench_with_input(BenchmarkId::new("first_n", size), &size, |b, _| {
            let n = std::cmp::min(10, size);
            b.iter(|| {
                call_with_input(
                    "first",
                    vec![Value::Int(n as i64)],
                    Some(arr.clone()),
                    &mut env,
                )
            })
        });

        group.bench_with_input(BenchmarkId::new("last", size), &size, |b, _| {
            b.iter(|| call_with_input("last", vec![], Some(arr.clone()), &mut env))
        });

        group.bench_with_input(BenchmarkId::new("last_n", size), &size, |b, _| {
            let n = std::cmp::min(10, size);
            b.iter(|| {
                call_with_input(
                    "last",
                    vec![Value::Int(n as i64)],
                    Some(arr.clone()),
                    &mut env,
                )
            })
        });

        // Create a simple lambda for any/all tests
        let lambda_gt_500 = Value::Lambda(aethershell::value::Lambda {
            params: vec!["x".to_string()],
            body: Box::new(aethershell::ast::Expr::Binary {
                op: aethershell::ast::BinOp::Gt,
                left: Box::new(aethershell::ast::Expr::Ident("x".to_string())),
                right: Box::new(aethershell::ast::Expr::LitInt(500)),
            }),
            // References only its own parameter, so nothing is free to capture.
            captured: Default::default(),
        });

        group.bench_with_input(
            BenchmarkId::new("any_with_predicate", size),
            &size,
            |b, _| {
                b.iter(|| {
                    call_with_input(
                        "any",
                        vec![lambda_gt_500.clone()],
                        Some(arr.clone()),
                        &mut env,
                    )
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("all_with_predicate", size),
            &size,
            |b, _| {
                b.iter(|| {
                    call_with_input(
                        "all",
                        vec![lambda_gt_500.clone()],
                        Some(arr.clone()),
                        &mut env,
                    )
                })
            },
        );
    }

    group.finish();
}

fn benchmark_builtin_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("builtin_lookup");
    let mut env = create_test_env();
    let test_arr = create_test_array(100);

    // Test builtin lookup performance
    group.bench_function("known_builtin", |b| {
        b.iter(|| call_with_input("first", vec![], Some(test_arr.clone()), &mut env))
    });

    group.bench_function("unknown_builtin", |b| {
        b.iter(|| {
            let result = call_with_input(
                "nonexistent_function",
                vec![],
                Some(test_arr.clone()),
                &mut env,
            );
            black_box(result)
        })
    });

    group.finish();
}

fn benchmark_value_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_operations");

    // Test Value cloning performance for different sizes
    let sizes = vec![10, 100, 1000];

    for size in sizes {
        let arr = create_test_array(size);

        group.bench_with_input(BenchmarkId::new("clone_array", size), &size, |b, _| {
            b.iter(|| black_box(arr.clone()))
        });

        // Test array iteration
        if let Value::Array(ref vec) = arr {
            group.bench_with_input(BenchmarkId::new("iterate_array", size), &size, |b, _| {
                b.iter(|| {
                    let mut sum = 0i64;
                    for item in vec {
                        if let Value::Int(n) = item {
                            sum += n;
                        }
                    }
                    black_box(sum)
                })
            });
        }
    }

    group.finish();
}

fn benchmark_record_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_operations");

    // Create test records of different sizes
    let sizes = vec![10, 50, 100];

    for size in sizes {
        let mut record = BTreeMap::new();
        for i in 0..size {
            record.insert(format!("key_{}", i), Value::Int(i as i64));
        }
        let test_record = Value::Record(record);

        group.bench_with_input(BenchmarkId::new("clone_record", size), &size, |b, _| {
            b.iter(|| black_box(test_record.clone()))
        });

        // Test record field access
        group.bench_with_input(BenchmarkId::new("access_field", size), &size, |b, _| {
            b.iter(|| {
                if let Value::Record(ref map) = test_record {
                    let result = map.get("key_5");
                    black_box(result)
                } else {
                    black_box(None)
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_array_functions,
    benchmark_builtin_lookup,
    benchmark_value_operations,
    benchmark_record_operations
);
criterion_main!(benches);
