//! Stateful sessions (docs/AGENTIC_FIRST_DESIGN.md §6.3): bindings persist
//! across `session_eval` calls in one environment, so an agent accrues state
//! without re-sending prior context.

use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

#[test]
fn session_persists_bindings_across_calls() {
    let id = match call("sess_open", vec![]) {
        Value::Record(m) => match m.get("session") {
            Some(Value::Str(id)) => id.clone(),
            other => panic!("expected session id, got {other:?}"),
        },
        other => panic!("expected record, got {other:?}"),
    };

    // First call binds x; a later call uses it — proving the env persists.
    call("sess_eval", vec![s(&id), s("let x = 41")]);
    let result = call("sess_eval", vec![s(&id), s("x + 1")]);
    assert_eq!(
        result,
        Value::Int(42),
        "binding from a prior call is visible"
    );

    // Module namespaces are available in a session (not just bare builtins).
    let upper = call("sess_eval", vec![s(&id), s(r#"str.upper("hi")"#)]);
    assert_eq!(
        upper,
        Value::Str("HI".into()),
        "module calls resolve in a session"
    );

    // Running token usage accumulates across the evals.
    match call("sess_usage", vec![s(&id)]) {
        Value::Record(m) => {
            assert_eq!(m.get("evals"), Some(&Value::Int(3)));
            let total = match m.get("tokens_total") {
                Some(Value::Int(n)) => *n,
                _ => panic!("missing tokens_total"),
            };
            assert!(total > 0, "session tracks a running token budget");
        }
        other => panic!("expected record, got {other:?}"),
    }

    // Closing the session frees it; further eval fails.
    match call("sess_close", vec![s(&id)]) {
        Value::Record(m) => assert_eq!(m.get("closed"), Some(&Value::Bool(true))),
        other => panic!("expected record, got {other:?}"),
    }
    let mut env = aethershell::env::Env::new();
    let after = aethershell::builtins::call("sess_eval", vec![s(&id), s("1")], &mut env);
    assert!(after.is_err(), "eval on a closed session should error");
}

#[test]
fn distinct_sessions_are_isolated() {
    let id1 = match call("sess_open", vec![]) {
        Value::Record(m) => get_session(&m),
        other => panic!("{other:?}"),
    };
    let id2 = match call("sess_open", vec![]) {
        Value::Record(m) => get_session(&m),
        other => panic!("{other:?}"),
    };
    assert_ne!(id1, id2);

    call("sess_eval", vec![s(&id1), s("let y = 7")]);
    // y is not defined in session 2.
    let mut env = aethershell::env::Env::new();
    let cross = aethershell::builtins::call("sess_eval", vec![s(&id2), s("y")], &mut env);
    assert!(
        !matches!(cross, Ok(Value::Int(7))),
        "session 2 must not see session 1's binding"
    );

    call("sess_close", vec![s(&id1)]);
    call("sess_close", vec![s(&id2)]);
}

fn get_session(m: &std::collections::BTreeMap<String, Value>) -> String {
    match m.get("session") {
        Some(Value::Str(id)) => id.clone(),
        other => panic!("expected session id, got {other:?}"),
    }
}

// ── Reachability over the Agent API (the `Call` action routes to builtins, and
//    session/plan state lives in global stores, so it persists across requests) ──

#[test]
fn agent_api_call_drives_stateful_sessions() {
    use aethershell::agent_api::{process_request, AgentRequest};
    use serde_json::json;

    let open = process_request(&AgentRequest::Call {
        builtin: "sess_open".into(),
        args: json!([]),
    });
    assert!(open.success, "sess_open failed: {:?}", open.error);
    let id = open
        .result
        .as_ref()
        .and_then(|r| r.get("session"))
        .and_then(|v| v.as_str())
        .expect("session id")
        .to_string();

    // Bind in one request, use it in a later, separate request.
    let _ = process_request(&AgentRequest::Call {
        builtin: "sess_eval".into(),
        args: json!([id, "let x = 41"]),
    });
    let r = process_request(&AgentRequest::Call {
        builtin: "sess_eval".into(),
        args: json!([id, "x + 1"]),
    });
    assert!(r.success, "sess_eval failed: {:?}", r.error);
    assert_eq!(
        r.result,
        Some(json!(42)),
        "binding persisted across separate Agent API requests"
    );

    let _ = process_request(&AgentRequest::Call {
        builtin: "sess_close".into(),
        args: json!([id]),
    });
}

#[test]
fn agent_api_call_drives_plan() {
    use aethershell::agent_api::{process_request, AgentRequest};
    use serde_json::json;

    // plan takes a single arg: the ops array. (Human mode → plan only reviews.)
    let resp = process_request(&AgentRequest::Call {
        builtin: "plan".into(),
        args: json!([[{ "op": "write", "path": "/tmp/ae_api_plan.txt", "content": "hi" }]]),
    });
    assert!(resp.success, "plan failed: {:?}", resp.error);
    let token = resp
        .result
        .as_ref()
        .and_then(|r| r.get("token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        token.starts_with("apl_"),
        "plan returns a bound token: {token}"
    );
}
