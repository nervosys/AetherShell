//! End-to-end tests for secret hygiene (§7.6 of the agentic-first design).
//!
//! Three live surfaces are exercised, not just the pure helpers:
//!   1. `render_agent` — the agent output path scrubs secret *shapes* and
//!      secret-*named* fields before a value reaches the model's context.
//!   2. `call("env", …)` — reading a secret-named env var in agent mode returns
//!      an opaque handle, while human mode returns the value (legibility).
//!   3. `safety::audit` — secrets never land in the persistent, hash-chained
//!      audit log, and the chain still verifies over the redacted content.

use aethershell::builtins::render_agent;
use aethershell::safety;
use aethershell::value::Value;
use std::collections::BTreeMap;

// Env is process-global; serialize the env-mutating tests in this binary.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn clear() {
    for k in [
        "AETHER_MODE",
        "AETHER_AGENT",
        "AETHER_POLICY",
        "AETHER_REDACT",
        "AETHER_SECRETS",
        "AETHER_WORKSPACE",
        "AETHER_AUDIT_LOG",
    ] {
        std::env::remove_var(k);
    }
}

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

#[test]
fn render_agent_redacts_secret_shapes_and_named_fields() {
    let _l = lock();
    clear();
    std::env::remove_var("AETHER_REDACT");

    let mut rec = BTreeMap::new();
    rec.insert(
        "endpoint".to_string(),
        Value::Str("https://api.example.com".to_string()),
    );
    // A secret-shaped value in an ordinarily-named field is scrubbed by shape.
    rec.insert(
        "note".to_string(),
        Value::Str("auth with sk-abcdefghijklmnopqrstuvwxyz12 then call".to_string()),
    );
    // A secret-named field is replaced wholesale, even with an innocuous value.
    rec.insert(
        "API_KEY".to_string(),
        Value::Str("whatever-was-here".to_string()),
    );

    let out = render_agent(&Value::Array(vec![Value::Record(rec)]), None).expect("output");
    assert!(out.contains("[REDACTED]"), "marker present: {out}");
    assert!(!out.contains("sk-abcdefghij"), "shape secret gone: {out}");
    assert!(!out.contains("whatever-was-here"), "named secret gone: {out}");
    assert!(out.contains("api.example.com"), "non-secret kept: {out}");

    clear();
}

#[test]
fn render_agent_opt_out_keeps_full_fidelity() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_REDACT", "off");

    let mut rec = BTreeMap::new();
    rec.insert(
        "API_KEY".to_string(),
        Value::Str("sk-keepmewhenredactoff123456".to_string()),
    );
    let out = render_agent(&Value::Record(rec), None).expect("output");
    assert!(
        out.contains("sk-keepmewhenredactoff123456"),
        "AETHER_REDACT=off must not redact: {out}"
    );

    clear();
}

#[test]
fn env_read_is_gated_in_agent_mode_only() {
    let _l = lock();
    clear();
    std::env::set_var("AE_SECRET_FIXTURE_KEY", "sk-do-not-leak-1234567890ab");

    // Human mode: the person reading their own env gets the value.
    let human = call("env", vec![Value::Str("AE_SECRET_FIXTURE_KEY".to_string())]);
    assert_eq!(
        human,
        Value::Str("sk-do-not-leak-1234567890ab".to_string()),
        "human mode returns the value"
    );

    // Agent mode: a secret-named var becomes an opaque handle.
    std::env::set_var("AETHER_MODE", "agent");
    let agent = call("env", vec![Value::Str("AE_SECRET_FIXTURE_KEY".to_string())]);
    assert_eq!(
        agent,
        Value::Str("[REDACTED:AE_SECRET_FIXTURE_KEY]".to_string()),
        "agent mode returns a handle, never the secret"
    );

    // A non-secret name still reads through in agent mode.
    std::env::set_var("AE_PLAIN_FIXTURE", "plainvalue");
    let plain = call("env", vec![Value::Str("AE_PLAIN_FIXTURE".to_string())]);
    assert_eq!(plain, Value::Str("plainvalue".to_string()));

    // Explicit permission re-opens clear reads of the secret.
    std::env::set_var("AETHER_SECRETS", "allow");
    let permitted = call("env", vec![Value::Str("AE_SECRET_FIXTURE_KEY".to_string())]);
    assert_eq!(permitted, Value::Str("sk-do-not-leak-1234567890ab".to_string()));

    std::env::remove_var("AE_SECRET_FIXTURE_KEY");
    std::env::remove_var("AE_PLAIN_FIXTURE");
    clear();
}

#[test]
fn audit_log_never_persists_secrets_and_still_verifies() {
    let _l = lock();
    clear();
    let dir = std::env::temp_dir().join(format!("ae_secret_audit_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("audit.log");
    let _ = std::fs::remove_file(&log);
    std::env::set_var("AETHER_AUDIT_LOG", log.to_string_lossy().to_string());

    safety::audit(
        "env",
        safety::Effect::ReadLocal,
        "allow",
        "connect postgres://u:hunter2pass@db/app",
        serde_json::json!({
            "AWS_SECRET_ACCESS_KEY": "wJalrXUtnFEMI",
            "note": "token ghp_0123456789abcdefABCDEFghijklmnop12",
        }),
    )
    .expect("audit write");

    let content = std::fs::read_to_string(&log).expect("log written");
    assert!(content.contains("[REDACTED]"), "redaction applied: {content}");
    assert!(!content.contains("hunter2pass"), "url password gone");
    assert!(!content.contains("wJalrXUtnFEMI"), "secret-named value gone");
    assert!(!content.contains("ghp_0123456789"), "shape secret gone");

    // The hash chain must still validate over the (redacted) content.
    let n = safety::verify_audit(&log).expect("chain verifies over redacted entry");
    assert_eq!(n, 1);

    let _ = std::fs::remove_file(&log);
    clear();
}
