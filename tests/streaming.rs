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
    assert_eq!(first_chunk.data.get("seq").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(first_chunk.data.get("total").and_then(|v| v.as_u64()), Some(120));
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
