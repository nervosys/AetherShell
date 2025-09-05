//! Integration tests for AI, agents, and swarms.
//! Adjust crate name if needed (aurora_shell → your crate).

use aurora_shell::{ai::agents::run_sync, builtins, env::Env, value::Value};

fn run_builtin(name: &str, args: Vec<Value>) -> Value {
    let mut env = Env::default();
    builtins::call(name, args, &mut env).expect("builtin call failed")
}

#[test]
fn agents_run_sync_basic() {
    let mut env = Env::default();
    let goal = "List big files";
    let tools: [&str; 2] = ["ls", "print"];
    let out = run_sync(goal, &tools, 4, true, &mut env).expect("agents::run_sync should succeed");
    assert!(!out.trim().is_empty(), "expected non-empty output");
}

#[test]
fn agents_run_sync_uses_tools_and_respects_max_steps() {
    let mut env = Env::default();
    let out = run_sync("Summarize repo", &["git", "print"], 2, true, &mut env)
        .expect("agents::run_sync should succeed");
    assert!(!out.is_empty());
}

#[test]
fn agent_builtin_happy_path() {
    // agent(goal, [tools], [max_steps], [dry_run])
    let out = run_builtin(
        "agent",
        vec![
            Value::Str("Plan a cleanup".into()),
            Value::Array(vec![Value::Str("ls".into()), Value::Str("print".into())]),
            Value::Int(3),
            Value::Bool(true),
        ],
    );
    match out {
        Value::Str(s) => assert!(!s.is_empty(), "agent should return a non-empty String"),
        other => panic!("agent should return String, got {other:?}"),
    }
}

#[test]
fn agent_builtin_argument_errors() {
    // No args → error
    let mut env = Env::default();
    let err = builtins::call("agent", vec![], &mut env)
        .err()
        .expect("expected error");
    let msg = format!("{err}");
    assert!(msg.contains("goal"), "unexpected error: {msg}");

    // Bad tools entry → error
    let mut env = Env::default();
    let err = builtins::call(
        "agent",
        vec![
            Value::Str("X".into()),
            Value::Array(vec![Value::Int(1)]), // invalid tools entry
        ],
        &mut env,
    )
    .err()
    .expect("expected error");
    assert!(
        format!("{err}").contains("tools array"),
        "unexpected error: {err}"
    );
}

#[test]
fn swarm_builtin_happy_path_with_config_tools() {
    // swarm(goal, config_json_with_tools, [max_steps], [dry_run])
    let cfg = r#"{ "agents": ["a","b"], "tools": ["ls","print"], "models": ["llama3"] }"#;
    let out = run_builtin(
        "swarm",
        vec![
            Value::Str("Inventory workspace".into()),
            Value::Str(cfg.into()),
            Value::Int(5),
            Value::Bool(true),
        ],
    );
    match out {
        Value::Str(s) => assert!(!s.is_empty(), "swarm should return a non-empty String"),
        other => panic!("swarm should return String, got {other:?}"),
    }
}

#[test]
fn swarm_builtin_rejects_bad_config() {
    let mut env = Env::default();
    let bad_cfg = r#"{ "tools": [ 1, true, {} ] }"#; // invalid: not all strings
    let err = builtins::call(
        "swarm",
        vec![
            Value::Str("Do something".into()),
            Value::Str(bad_cfg.into()),
        ],
        &mut env,
    )
    .err()
    .expect("expected error");
    let msg = format!("{err}");
    assert!(
        msg.contains("tools") || msg.contains("array of strings"),
        "unexpected error: {msg}"
    );
}

#[test]
fn swarm_builtin_defaults_are_reasonable() {
    // Only goal provided → default max_steps/dry_run
    let out = run_builtin("swarm", vec![Value::Str("Quick plan".into())]);
    match out {
        Value::Str(s) => assert!(!s.is_empty()),
        other => panic!("expected String, got {other:?}"),
    }
}
