// benches/agent_path_performance.rs
// Benchmarks for the Agent API path: request dispatch, argument conversion,
// pipeline assembly, schema generation (cold and cached), and the ontology
// manifest. Everything runs in process on pure builtins: no network, no AI, no
// spawned programs and no file writes.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iron_shell::agent_api::{
    generate_schema_filtered, ontology_manifest_json, process_request, AgentRequest,
    PipelineStep,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A pure builtin with two required named arguments and one optional one.
fn call_request() -> AgentRequest {
    AgentRequest::Call {
        builtin: "crypto_hmac".to_string(),
        args: json!({"key": "secret", "data": "message", "algorithm": "sha256"}),
    }
}

fn eval_request() -> AgentRequest {
    AgentRequest::Eval {
        code: "str.upper(\"message\")".to_string(),
    }
}

fn pipeline_request() -> AgentRequest {
    AgentRequest::Pipeline {
        steps: vec![PipelineStep {
            builtin: "crypto_hmac".to_string(),
            args: json!({"key": "secret", "data": "message"}),
            select: None,
            predicate: None,
        }],
        input: None,
    }
}

/// Run a request once before timing it, so a request the path refuses shows
/// up as a failure here rather than as a quick, meaningless timing.
fn must_succeed(label: &str, request: &AgentRequest) {
    let response = process_request(request);
    assert!(
        response.success,
        "{label} failed before timing: {:?}",
        response.error
    );
}

fn benchmark_call_named_args(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_call");
    group.measurement_time(Duration::from_secs(10));

    let request = call_request();
    must_succeed("call", &request);
    group.bench_function("pure_builtin_named_args", |b| {
        b.iter(|| black_box(process_request(black_box(&request))))
    });

    group.finish();
}

fn benchmark_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_eval");
    group.measurement_time(Duration::from_secs(10));

    let request = eval_request();
    must_succeed("eval", &request);
    group.bench_function("str_upper", |b| {
        b.iter(|| black_box(process_request(black_box(&request))))
    });

    group.finish();
}

fn benchmark_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_pipeline");
    group.measurement_time(Duration::from_secs(10));

    let request = pipeline_request();
    must_succeed("pipeline", &request);
    group.bench_function("one_pure_step", |b| {
        b.iter(|| black_box(process_request(black_box(&request))))
    });

    group.finish();
}

fn benchmark_schema(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_schema");
    group.measurement_time(Duration::from_secs(15));

    // The schema cache is keyed on the filter as well as the format, PATH and
    // cwd. A filter string never seen before misses the cache, so each cold
    // iteration builds the catalogue again. The filter is unique per call and
    // matches nothing, so the answer stays small.
    static COLD_SEQ: AtomicUsize = AtomicUsize::new(0);
    group.bench_function("cold_unfiltered_miss", |b| {
        b.iter(|| {
            let n = COLD_SEQ.fetch_add(1, Ordering::Relaxed);
            let q = format!("no-such-builtin-{n}");
            black_box(generate_schema_filtered("json", None, Some(&q)))
        })
    });

    static COLD_MODULE_SEQ: AtomicUsize = AtomicUsize::new(0);
    group.bench_function("cold_module_miss", |b| {
        b.iter(|| {
            let n = COLD_MODULE_SEQ.fetch_add(1, Ordering::Relaxed);
            let q = format!("no-such-builtin-{n}");
            black_box(generate_schema_filtered("json", Some("file"), Some(&q)))
        })
    });

    // Cached: the same request repeatedly, after one call has filled the cache.
    let _ = generate_schema_filtered("json", None, None);
    group.bench_function("cached_unfiltered", |b| {
        b.iter(|| black_box(generate_schema_filtered("json", None, None)))
    });

    let _ = generate_schema_filtered("json", Some("file"), None);
    group.bench_function("cached_module", |b| {
        b.iter(|| black_box(generate_schema_filtered("json", Some("file"), None)))
    });

    group.finish();
}

fn benchmark_ontology_manifest(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_ontology_manifest");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("ontology_manifest_json", |b| {
        b.iter(|| black_box(ontology_manifest_json()))
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_call_named_args,
    benchmark_eval,
    benchmark_pipeline,
    benchmark_schema,
    benchmark_ontology_manifest
);
criterion_main!(benches);
