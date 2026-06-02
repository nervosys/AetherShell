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
