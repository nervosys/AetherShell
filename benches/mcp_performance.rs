use aether_shell::ai::{clear_mcp_cache, detect_mcp_servers, detect_mcp_servers_uncached};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

fn benchmark_mcp_detection_cached(c: &mut Criterion) {
    let mut group = c.benchmark_group("mcp_detection_cached");
    group.measurement_time(Duration::from_secs(15));

    group.bench_function("first_call", |b| {
        b.iter_with_large_drop(|| {
            // Clear cache before each measurement
            let _ = clear_mcp_cache();
            // First call should populate cache
            black_box(detect_mcp_servers())
        })
    });

    group.bench_function("cached_call", |b| {
        // Populate cache once
        let _ = detect_mcp_servers();

        b.iter(|| {
            // Subsequent calls should use cache
            black_box(detect_mcp_servers())
        })
    });

    group.finish();
}

fn benchmark_mcp_detection_uncached(c: &mut Criterion) {
    let mut group = c.benchmark_group("mcp_detection_uncached");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10); // Reduce sample size for network calls

    group.bench_function("uncached_call", |b| {
        b.iter(|| {
            // Always performs network calls
            black_box(detect_mcp_servers_uncached())
        })
    });

    group.finish();
}

fn benchmark_cache_performance_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_comparison");
    group.measurement_time(Duration::from_secs(10));

    // Warm up cache
    let _ = detect_mcp_servers();

    group.bench_function("cached", |b| b.iter(|| black_box(detect_mcp_servers())));

    group.bench_function("uncached_single_sample", |b| {
        b.iter(|| black_box(detect_mcp_servers_uncached()))
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_mcp_detection_cached,
    benchmark_mcp_detection_uncached,
    benchmark_cache_performance_comparison
);
criterion_main!(benches);
