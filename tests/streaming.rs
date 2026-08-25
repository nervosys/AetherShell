//! Streaming execute (docs/AGENTIC_FIRST_DESIGN.md §6.3): large array results are
//! split into ordered `chunk` events so a client consumes rows incrementally and
//! can early-stop, instead of receiving one atomic `complete`.

use aethershell::agent_api::server::stream_events_from_response;
use aethershell::agent_api::AgentResponse;
use serde_json::json;

fn resp(result: serde_json::Value, result_type: &str) -> AgentResponse {
    AgentResponse {
        success: true,
        result: Some(result),
        error: None,
        result_type: Some(result_type.to_string()),
        metadata: None,
    }
}

#[test]
fn large_array_streams_in_chunks() {
    let arr = json!((0..120).collect::<Vec<i64>>());
    let events = stream_events_from_response(resp(arr, "Array"), 50);

    let chunks = events.iter().filter(|e| e.event == "chunk").count();
    assert_eq!(chunks, 3, "120 rows / 50 → 3 chunks (50, 50, 20)");
    assert!(events.iter().any(|e| e.event == "start"));
    assert!(events.iter().any(|e| e.event == "complete"));

    // Chunks are ordered and carry the total.
    let first_chunk = events.iter().find(|e| e.event == "chunk").unwrap();
    assert_eq!(
        first_chunk.data.get("seq").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert_eq!(
        first_chunk.data.get("total").and_then(|v| v.as_u64()),
        Some(120)
    );
}

#[test]
fn small_result_is_not_chunked() {
    let events = stream_events_from_response(resp(json!([1, 2, 3]), "Array"), 50);
    assert_eq!(events.iter().filter(|e| e.event == "chunk").count(), 0);
    assert!(events.iter().any(|e| e.event == "complete"));
}

#[test]
fn failure_emits_error_event() {
    let r = AgentResponse {
        success: false,
        result: None,
        error: Some("boom".to_string()),
        result_type: None,
        metadata: None,
    };
    let events = stream_events_from_response(r, 50);
    assert!(events.iter().any(|e| e.event == "error"));
    assert_eq!(events.iter().filter(|e| e.event == "complete").count(), 0);
}

// ── Streaming *evaluation* (eval_stream): incremental, stage-by-stage ──────────
use aethershell::value::Value;

fn collect_stream(code: &str) -> (usize, Vec<Value>) {
    let mut env = aethershell::env::Env::new();
    let mut out: Vec<Value> = Vec::new();
    let n = {
        let mut emit = |v: Value| out.push(v);
        aethershell::eval::eval_stream(code, &mut env, &mut emit).expect("eval_stream")
    };
    (n, out)
}

#[test]
fn eval_stream_streams_array_pipeline_element_wise() {
    // map then where are element-independent → streamed per element; only the
    // surviving results are emitted (1→10 filtered, 2→20 filtered, 3→30, 4→40).
    let (n, out) =
        collect_stream("let d = [1,2,3,4]; d | map(fn(x) => x * 10) | where(fn(y) => y > 20)");
    assert_eq!(n, 2);
    assert_eq!(out, vec![Value::Int(30), Value::Int(40)]);
}

#[test]
fn eval_stream_falls_back_for_whole_collection_stage() {
    // `sort` needs the whole collection → not streamable → eager fallback, then the
    // sorted elements are emitted (correctness preserved).
    let (n, out) = collect_stream("[3,1,2] | sort()");
    assert_eq!(n, 3);
    assert_eq!(out, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
}

#[test]
fn eval_stream_emits_scalar_once() {
    let (n, out) = collect_stream("40 + 2");
    assert_eq!(n, 1);
    assert_eq!(out, vec![Value::Int(42)]);
}

// ── Laziness that pays: `take` stops the source ───────────────────────────────
//
// "Streaming" was true of the *output* and false of the *work*: a `take` made
// the pipeline unstreamable, so `xs | map(f) | take(3)` fell back to eager and
// called `f` once per element of `xs`. From outside, an implementation that
// materialises everything and one that reads three elements are indistinguish-
// able — same values, same order. `StreamStats::pulled` is the number that
// tells them apart, and these tests assert on it.

use aethershell::eval::StreamStats;

fn stream_stats(code: &str) -> (StreamStats, Vec<Value>) {
    let mut env = aethershell::env::Env::new();
    let mut out: Vec<Value> = Vec::new();
    let stats = {
        let mut emit = |v: Value| out.push(v);
        aethershell::eval::eval_stream_with_stats(code, &mut env, &mut emit).expect("eval_stream")
    };
    (stats, out)
}

#[test]
fn take_stops_pulling_the_source() {
    let (stats, out) = stream_stats(
        "let xs = range(0, 1000); xs | map(fn(x) => x * 2) | where(fn(y) => y >= 0) | take(3)",
    );
    assert_eq!(out, vec![Value::Int(0), Value::Int(2), Value::Int(4)]);
    assert_eq!(stats.emitted, 3);
    assert!(stats.streamed, "the element-wise path should have run");
    assert!(
        stats.short_circuited,
        "the source should have been abandoned"
    );
    assert_eq!(
        stats.pulled, 3,
        "a satisfied take must not keep reading: {} of 1000 elements were pushed \
         through the stages",
        stats.pulled
    );
}

#[test]
fn without_a_take_every_element_is_pulled() {
    // The control. If `pulled` were always small the test above would pass for
    // the wrong reason.
    let (stats, _) = stream_stats("let xs = range(0, 50); xs | map(fn(x) => x * 2)");
    assert_eq!(stats.pulled, 50);
    assert_eq!(stats.emitted, 50);
    assert!(!stats.short_circuited);
}

#[test]
fn a_take_never_abandons_a_stage_that_does_something() {
    // Short-circuiting is only sound when the skipped work was not the point.
    // `file_exists` is `ReadLocal`, not `Pure`, so the early exit is withheld
    // and every element still runs — the pipeline streams, it just does not
    // abandon the source.
    use aethershell::safety::{effect_of, Effect};
    assert_ne!(
        effect_of("file_exists"),
        Effect::Pure,
        "this test needs a non-Pure builtin to be meaningful"
    );

    let (stats, out) = stream_stats(
        "let xs = range(0, 20); xs | map(fn(x) => file_exists(\"no_such_file_xyzzy\")) | take(2)",
    );
    assert_eq!(out.len(), 2, "take still limits what is emitted");
    assert_eq!(stats.emitted, 2);
    assert!(
        !stats.short_circuited,
        "an effectful stage must not have its work skipped"
    );
    assert_eq!(
        stats.pulled, 20,
        "every element's side effect must still happen"
    );
}

#[test]
fn take_zero_emits_nothing_and_reads_almost_nothing() {
    let (stats, out) = stream_stats("let xs = range(0, 100); xs | map(fn(x) => x) | take(0)");
    assert!(out.is_empty());
    assert_eq!(stats.emitted, 0);
    assert!(stats.short_circuited);
    assert_eq!(stats.pulled, 0, "take(0) should read nothing at all");
}

#[test]
fn a_take_in_the_middle_limits_what_reaches_the_rest() {
    let (stats, out) = stream_stats("let xs = range(0, 100); xs | take(2) | map(fn(x) => x + 100)");
    assert_eq!(out, vec![Value::Int(100), Value::Int(101)]);
    assert_eq!(stats.emitted, 2);
    assert_eq!(stats.pulled, 2, "the third element is never read");
}

#[test]
fn a_taken_pipeline_agrees_with_the_eager_evaluator() {
    // Laziness that changes the answer is not an optimisation. The streamed
    // result must equal what the ordinary evaluator produces for the same code.
    let code = "let xs = range(0, 40); xs | map(fn(x) => x * 3) | where(fn(y) => y > 10) | take(4)";
    let (_, streamed) = stream_stats(code);

    let mut env = aethershell::env::Env::new();
    let stmts = aethershell::parser::parse_program(code).expect("parse");
    let eager = aethershell::eval::eval_program(&stmts, &mut env).expect("eval");
    let eager = match eager {
        Value::Array(items) => items,
        other => panic!("expected an array, got {other:?}"),
    };
    assert_eq!(streamed, eager);
}

// ── The source runs once ──────────────────────────────────────────────────────
//
// A pipeline source that is not an array was evaluated *twice*: the streaming
// path evaluated it, found it was not an array, and returned `None`, whereupon
// the caller eager-evaluated the whole statement — source included. The values
// agreed either way, which is why it survived. The second side effect did not.

/// How many bytes a run of `code` appended to its log.
fn appends_made(tag: &str, run: impl FnOnce(&str, &mut aethershell::env::Env)) -> String {
    let dir = std::env::temp_dir().join(format!("ae_stream_once_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let log = dir.join("appends.log");
    let p = log
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");

    // `file_append` returns a record, so this is a non-array source. `map` then
    // rejects it — under the eager evaluator too, so the error is not what
    // changed here. The number of appends is.
    let code = format!("file_append(\"{p}\", \"x\") | map(fn(r) => r)");
    let mut env = aethershell::env::Env::new();
    run(&code, &mut env);

    let appended = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    appended
}

#[test]
fn a_non_array_pipeline_source_runs_only_once() {
    let appended = appends_made("stream", |code, env| {
        let mut sink = |_v: Value| {};
        let _ = aethershell::eval::eval_stream_with_stats(code, env, &mut sink);
    });
    assert_eq!(
        appended, "x",
        "the source must run exactly once — \"xx\" means the streaming path \
         evaluated it and then handed the statement back to be evaluated again"
    );
}

#[test]
fn the_streaming_path_appends_no_more_than_the_eager_one() {
    // The claim is not "once" in the abstract, it is "the same as evaluating it
    // normally". Measured against the eager evaluator rather than a constant.
    let eager = appends_made("eager", |code, env| {
        let stmts = aethershell::parser::parse_program(code).expect("parse");
        let _ = aethershell::eval::eval_program(&stmts, env);
    });
    let streamed = appends_made("vs_eager", |code, env| {
        let mut sink = |_v: Value| {};
        let _ = aethershell::eval::eval_stream_with_stats(code, env, &mut sink);
    });
    assert_eq!(streamed, eager, "streaming must not double a side effect");
}
