//! `--deterministic` / `IRONSHELL_DETERMINISTIC` must reach the Agent API.
//!
//! `ish -c` renders canonical JSON under the flag (`repl::run_one`), but
//! `agent_api::process_request` ignored it: result keys followed the map's
//! order and the response could carry run-dependent metadata. These tests pin
//! the library entry point, which the HTTP, WebSocket and stdio paths all use.
//!
//! Process environment is global, so every test takes ENV_LOCK and restores
//! the variables it touched. IRONSHELL_SESSION_DIR points at a temp directory so
//! nothing here touches the real session journal.

use iron_shell::agent_api::{process_request, AgentRequest};
use serde_json::{json, Value as JsonValue};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets the given variables for one test and restores them on drop.
struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
    session_dir: PathBuf,
}

impl EnvGuard {
    fn set(deterministic: Option<&str>) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let session_dir = std::env::temp_dir().join(format!(
            "ish-deterministic-agent-api-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&session_dir).expect("create temp session dir");

        let names = [
            "IRONSHELL_DETERMINISTIC",
            "IRONSHELL_MODE",
            "IRONSHELL_SESSION_DIR",
        ];
        let saved = names.iter().map(|n| (*n, std::env::var_os(n))).collect();

        std::env::remove_var("IRONSHELL_MODE");
        std::env::set_var("IRONSHELL_SESSION_DIR", &session_dir);
        match deterministic {
            Some(v) => std::env::set_var("IRONSHELL_DETERMINISTIC", v),
            None => std::env::remove_var("IRONSHELL_DETERMINISTIC"),
        }
        EnvGuard {
            saved,
            _lock: lock,
            session_dir,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
        let _ = std::fs::remove_dir_all(&self.session_dir);
    }
}

fn eval(code: &str) -> AgentRequest {
    AgentRequest::Eval {
        code: code.to_string(),
    }
}

/// Keys that carry run-dependent values and must not appear under deterministic mode.
const TIMING_AND_ID_KEYS: &[&str] = &[
    "execution_time_ms",
    "duration_ms",
    "elapsed_ms",
    "timestamp",
    "created_at",
    "started_at",
    "finished_at",
    "request_id",
];

#[test]
fn deterministic_eval_is_byte_identical_and_sorted() {
    let _env = EnvGuard::set(Some("1"));
    let request = eval("{b: 1, a: 2, c: [3]}");

    let first = process_request(&request);
    let second = process_request(&request);
    assert!(first.success, "eval should succeed: {:?}", first.error);

    let first_text = serde_json::to_string(&first).expect("serialize first");
    let second_text = serde_json::to_string(&second).expect("serialize second");
    assert_eq!(
        first_text, second_text,
        "two deterministic responses for the same request must be byte-identical"
    );

    let result_text = serde_json::to_string(&first.result).expect("serialize result");
    assert_eq!(result_text, r#"{"a":2,"b":1,"c":[3]}"#);
    let a = result_text.find("\"a\"").expect("key a present");
    let b = result_text.find("\"b\"").expect("key b present");
    let c = result_text.find("\"c\"").expect("key c present");
    assert!(a < b && b < c, "keys must be sorted: {result_text}");
}

#[test]
fn deterministic_response_has_no_timing_or_id_fields() {
    let _env = EnvGuard::set(Some("1"));
    let response = process_request(&eval("{b: 1, a: 2, c: [3]}"));
    assert!(
        response.success,
        "eval should succeed: {:?}",
        response.error
    );

    let metadata = response
        .metadata
        .as_ref()
        .and_then(JsonValue::as_object)
        .expect("metadata object present");
    for key in TIMING_AND_ID_KEYS {
        assert!(
            !metadata.contains_key(*key),
            "deterministic metadata must not carry `{key}`: {metadata:?}"
        );
    }
    // The fields that are reproducible stay.
    assert!(metadata.contains_key("code_executed"));
    assert!(metadata.contains_key("token_accounting"));
}

#[test]
fn deterministic_result_keeps_its_values() {
    let _env = EnvGuard::set(Some("true"));
    let response = process_request(&eval("{b: 1, a: 2, c: [3]}"));
    assert_eq!(response.result, Some(json!({"a": 2, "b": 1, "c": [3]})));
}

#[test]
fn without_the_flag_the_response_is_unchanged() {
    let _env = EnvGuard::set(None);
    let response = process_request(&eval("1 + 2 * 3"));
    assert!(
        response.success,
        "eval should succeed: {:?}",
        response.error
    );
    assert_eq!(response.result, Some(json!(7)));
    let metadata = response.metadata.expect("metadata present");
    assert!(metadata.get("code_executed").is_some());
}

/// The Agent API returned records as they were, so in agent mode a password
/// that `ish --agent -c` redacted came back in the clear over HTTP and MCP.
/// Both now scrub through `agent_api::redact_for_agent`, with or without the
/// deterministic flag.
#[test]
fn agent_mode_responses_redact_secrets() {
    for flag in [None, Some("1")] {
        let _env = EnvGuard::set(flag);
        std::env::set_var("IRONSHELL_MODE", "agent");
        let response = process_request(&eval(
            r#"{user: "ada", password: "hunter2-not-a-real-one"}"#,
        ));
        // `code_executed` echoes the caller's own input, which tells it
        // nothing new; the result is what must not carry the secret.
        let text = serde_json::to_string(&response.result).unwrap();
        assert!(response.success, "{text}");
        assert!(text.contains("ada"), "the record did not render: {text}");
        assert!(!text.contains("hunter2"), "flag {flag:?} leaked: {text}");
    }
}
