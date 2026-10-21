//! The streamed pipeline reports progress only for steps that really completed,
//! and its final `complete` is the result the non-streaming path gives.
//!
//! Tested through `server::stream_pipeline_events`, the function the SSE handler
//! runs (the handler itself is only reachable over HTTP).

use aethershell::agent_api::server::{stream_pipeline_events, StreamEvent};
use aethershell::agent_api::{process_request, AgentRequest, AgentResponse, PipelineStep};
use serde_json::{json, Value as JsonValue};

fn steps(v: JsonValue) -> Vec<PipelineStep> {
    serde_json::from_value(v).expect("valid pipeline steps")
}

fn non_streaming(steps: &[PipelineStep], input: Option<&JsonValue>) -> AgentResponse {
    process_request(&AgentRequest::Pipeline {
        steps: steps.to_vec(),
        input: input.cloned(),
    })
}

fn kinds(events: &[StreamEvent]) -> Vec<&str> {
    events.iter().map(|e| e.event.as_str()).collect()
}

#[test]
fn two_step_pipeline_reports_each_completed_step_then_the_same_result() {
    let steps = steps(json!([
        { "builtin": "reverse" },
        { "builtin": "len" }
    ]));
    let input = json!([1, 2, 3]);

    let events = stream_pipeline_events(&steps, Some(&input), None);

    assert_eq!(
        kinds(&events),
        ["start", "progress", "progress", "complete"]
    );
    assert_eq!(events[1].data["current"], 1);
    assert_eq!(events[1].data["total"], 2);
    assert_eq!(
        events[1].data["message"],
        "Completed step 1/2: reverse".to_string()
    );
    assert_eq!(events[2].data["current"], 2);
    assert_eq!(
        events[2].data["message"],
        "Completed step 2/2: len".to_string()
    );

    let direct = non_streaming(&steps, Some(&input));
    assert!(
        direct.success,
        "non-streaming pipeline failed: {:?}",
        direct.error
    );
    let complete = events.last().expect("complete event");
    assert_eq!(complete.data["result"], direct.result.clone().unwrap());
    assert_eq!(
        complete.data["result_type"].as_str(),
        direct.result_type.as_deref()
    );
}

#[test]
fn failing_second_step_gives_one_progress_then_an_error_naming_step_two() {
    let steps = steps(json!([
        { "builtin": "reverse" },
        { "builtin": "no_such_builtin_zz" }
    ]));
    let input = json!([1, 2, 3]);

    let events = stream_pipeline_events(&steps, Some(&input), None);

    assert_eq!(kinds(&events), ["start", "progress", "error"]);
    assert_eq!(events[1].data["current"], 1);
    let message = events[2].data["error"].as_str().expect("error message");
    assert!(
        message.contains("Step 2/2") && message.contains("no_such_builtin_zz"),
        "error should name step 2 and its builtin, got: {message}"
    );

    let direct = non_streaming(&steps, Some(&input));
    assert!(
        !direct.success,
        "the non-streaming pipeline should also fail"
    );
}

#[test]
fn empty_pipeline_still_errors_without_progress() {
    let events = stream_pipeline_events(&[], None, None);

    assert_eq!(kinds(&events), ["start", "error"]);
    assert_eq!(
        events[1].data["error"],
        "Pipeline must have at least one step".to_string()
    );
}
