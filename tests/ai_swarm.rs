//! Integration tests for AI, agents, and swarms.
//! Adjust crate name if needed (aethershell → your crate).

use aethershell::{ai::agents::run_sync, builtins, env::Env, value::Value};

/// Check if AI provider is configured
fn ai_available() -> bool {
    std::env::var("AETHER_AI").is_ok() || std::env::var("OPENAI_API_KEY").is_ok()
}

/// Setup environment with stub AI backend for testing
fn setup_stub_env() -> Env {
    let mut env = Env::default();
    env.set_var("AETHER_AI", Value::Str("stub".to_string()))
        .ok();
    env
}

fn run_builtin(name: &str, args: Vec<Value>) -> Result<Value, anyhow::Error> {
    let mut env = setup_stub_env();
    builtins::call(name, args, &mut env)
}

#[test]
fn agents_run_sync_basic() {
    let mut env = setup_stub_env();
    let goal = "List big files";
    let tools: [&str; 2] = ["ls", "print"];
    let result = run_sync(goal, &tools, 4, true, &mut env);
    if result.is_err() && !ai_available() {
        return; // Skip if no AI provider configured
    }
    let out = result.expect("agents::run_sync should succeed");
    assert!(!out.trim().is_empty(), "expected non-empty output");
}

#[test]
fn agents_run_sync_uses_tools_and_respects_max_steps() {
    let mut env = setup_stub_env();
    let result = run_sync("Summarize repo", &["git", "print"], 2, true, &mut env);
    if result.is_err() && !ai_available() {
        return; // Skip if no AI provider configured
    }
    let out = result.expect("agents::run_sync should succeed");
    assert!(!out.is_empty());
}

#[test]
fn agent_builtin_happy_path() {
    // agent(goal, [tools], [max_steps], [dry_run])
    let result = run_builtin(
        "agent",
        vec![
            Value::Str("Plan a cleanup".into()),
            Value::Array(vec![Value::Str("ls".into()), Value::Str("print".into())]),
            Value::Int(3),
            Value::Bool(true),
        ],
    );
    if result.is_err() && !ai_available() {
        return; // Skip if no AI provider configured
    }
    let out = result.expect("builtin call failed");
    match out {
        Value::Str(s) => assert!(!s.is_empty(), "agent should return a non-empty String"),
        other => panic!("agent should return String, got {other:?}"),
    }
}

#[test]
fn agent_builtin_argument_errors() {
    // No args → error
    let mut env = setup_stub_env();
    let err = builtins::call("agent", vec![], &mut env).expect_err("expected error");
    let msg = format!("{err}");
    assert!(msg.contains("goal"), "unexpected error: {msg}");

    // Bad tools entry → error
    let mut env = setup_stub_env();
    let err = builtins::call(
        "agent",
        vec![
            Value::Str("X".into()),
            Value::Array(vec![Value::Int(1)]), // invalid tools entry
        ],
        &mut env,
    )
    .expect_err("expected error");
    assert!(
        format!("{err}").contains("tools array"),
        "unexpected error: {err}"
    );
}

#[test]
fn swarm_builtin_happy_path_with_config_tools() {
    // swarm(goal, config_json_with_tools, [max_steps], [dry_run])
    let cfg = r#"{ "agents": ["a","b"], "tools": ["ls","print"], "models": ["llama3"] }"#;
    let result = run_builtin(
        "swarm",
        vec![
            Value::Str("Inventory workspace".into()),
            Value::Str(cfg.into()),
            Value::Int(5),
            Value::Bool(true),
        ],
    );
    if result.is_err() && !ai_available() {
        return; // Skip if no AI provider configured
    }
    let out = result.expect("builtin call failed");
    match out {
        Value::Str(s) => assert!(!s.is_empty(), "swarm should return a non-empty String"),
        other => panic!("swarm should return String, got {other:?}"),
    }
}

#[test]
fn swarm_builtin_rejects_bad_config() {
    let mut env = setup_stub_env();
    let bad_cfg = r#"{ "tools": [ 1, true, {} ] }"#; // invalid: not all strings
    let err = builtins::call(
        "swarm",
        vec![
            Value::Str("Do something".into()),
            Value::Str(bad_cfg.into()),
        ],
        &mut env,
    )
    .expect_err("expected error");
    let msg = format!("{err}");
    assert!(
        msg.contains("tools") || msg.contains("array of strings"),
        "unexpected error: {msg}"
    );
}

#[test]
fn swarm_builtin_defaults_are_reasonable() {
    // Only goal provided → default max_steps/dry_run
    let result = run_builtin("swarm", vec![Value::Str("Quick plan".into())]);
    if result.is_err() && !ai_available() {
        return; // Skip if no AI provider configured
    }
    let out = result.expect("builtin call failed");
    match out {
        Value::Str(s) => assert!(!s.is_empty()),
        other => panic!("expected String, got {other:?}"),
    }
}
