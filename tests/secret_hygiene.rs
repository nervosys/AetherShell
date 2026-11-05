//! End-to-end tests for secret hygiene (§7.6 of the agentic-first design).
//!
//! Three live surfaces are exercised, not just the pure helpers:
//!   1. `render_agent` — the agent output path scrubs secret *shapes* and
//!      secret-*named* fields before a value reaches the model's context.
//!   2. `call("env", …)` — reading a secret-named env var in agent mode returns
//!      an opaque handle, while human mode returns the value (legibility).
//!   3. `safety::audit` — secrets never land in the persistent, hash-chained
//!      audit log, and the chain still verifies over the redacted content.

use iron_shell::builtins::render_agent;
use iron_shell::safety;
use iron_shell::value::Value;
use std::collections::BTreeMap;

// Env is process-global; serialize the env-mutating tests in this binary.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn clear() {
    for k in [
        "IRONSHELL_MODE",
        "IRONSHELL_AGENT",
        "IRONSHELL_POLICY",
        "IRONSHELL_REDACT",
        "IRONSHELL_SECRETS",
        "IRONSHELL_WORKSPACE",
        "IRONSHELL_AUDIT_LOG",
    ] {
        std::env::remove_var(k);
    }
}

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env).expect("builtin call")
}

#[test]
fn render_agent_redacts_secret_shapes_and_named_fields() {
    let _l = lock();
    clear();
    std::env::remove_var("IRONSHELL_REDACT");

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
    assert!(
        !out.contains("whatever-was-here"),
        "named secret gone: {out}"
    );
    assert!(out.contains("api.example.com"), "non-secret kept: {out}");

    clear();
}

#[test]
fn render_agent_opt_out_keeps_full_fidelity() {
    let _l = lock();
    clear();
    std::env::set_var("IRONSHELL_REDACT", "off");

    let mut rec = BTreeMap::new();
    rec.insert(
        "API_KEY".to_string(),
        Value::Str("sk-keepmewhenredactoff123456".to_string()),
    );
    let out = render_agent(&Value::Record(rec), None).expect("output");
    assert!(
        out.contains("sk-keepmewhenredactoff123456"),
        "IRONSHELL_REDACT=off must not redact: {out}"
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
    std::env::set_var("IRONSHELL_MODE", "agent");
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
    std::env::set_var("IRONSHELL_SECRETS", "allow");
    let permitted = call("env", vec![Value::Str("AE_SECRET_FIXTURE_KEY".to_string())]);
    assert_eq!(
        permitted,
        Value::Str("sk-do-not-leak-1234567890ab".to_string())
    );

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
    std::env::set_var("IRONSHELL_AUDIT_LOG", log.to_string_lossy().to_string());

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
    assert!(
        content.contains("[REDACTED]"),
        "redaction applied: {content}"
    );
    assert!(!content.contains("hunter2pass"), "url password gone");
    assert!(
        !content.contains("wJalrXUtnFEMI"),
        "secret-named value gone"
    );
    assert!(!content.contains("ghp_0123456789"), "shape secret gone");

    // The hash chain must still validate over the (redacted) content.
    let n = safety::verify_audit(&log).expect("chain verifies over redacted entry");
    assert_eq!(n, 1);

    let _ = std::fs::remove_file(&log);
    clear();
}

#[test]
fn a_password_argument_never_reaches_the_audit_log() {
    // `rbac_login` is Privileged, so in agent mode the dispatcher refuses it
    // and audits the refusal. The audit entry used to carry every string
    // argument as a target, the password included, and a password has no
    // secret *shape* for `redact_str` to catch.
    let _l = lock();
    clear();
    let dir = std::env::temp_dir().join(format!("ae_secret_pw_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("audit.log");
    let _ = std::fs::remove_file(&log);
    std::env::set_var("IRONSHELL_AUDIT_LOG", log.to_string_lossy().to_string());
    std::env::set_var("IRONSHELL_MODE", "agent");
    std::env::set_var("IRONSHELL_WORKSPACE", &dir);

    let mut env = iron_shell::env::Env::new();
    let out = iron_shell::builtins::call(
        "rbac_login",
        vec![
            Value::Str("alice".into()),
            Value::Str("plain-words-pw-7731".into()),
        ],
        &mut env,
    );
    assert!(
        out.is_err(),
        "rbac_login is Privileged and refused in agent mode"
    );

    let content = std::fs::read_to_string(&log).expect("the refusal is audited");
    assert!(content.contains("rbac_login"), "entry written: {content}");
    assert!(
        !content.contains("plain-words-pw-7731"),
        "the password reached the audit log: {content}"
    );
    // Non-vacuity: the user name is not a credential and is still recorded.
    assert!(
        content.contains("alice"),
        "the call is still attributable: {content}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn redaction_covers_secret_query_keys_and_more_vendor_prefixes() {
    // Secret-named query parameters are redacted in a URL or query string.
    for (input, secret) in [
        ("https://maps.example/v1?key=Sekrit998877", "Sekrit998877"),
        ("GET /api?api_key=abcdef123456&x=1", "abcdef123456"),
        ("https://h.example/x?apikey=zz998877aabb", "zz998877aabb"),
        (
            "https://h.example/x?a=1&access_token=tok123456789",
            "tok123456789",
        ),
        ("https://h.example/x?token=tok987654321", "tok987654321"),
        ("https://h.example/x?secret=shh-quiet-99", "shh-quiet-99"),
        ("https://h.example/x?password=hunter22xx", "hunter22xx"),
    ] {
        let (r, changed) = safety::redact_str(input);
        assert!(changed && !r.contains(secret), "{input} -> {r}");
        assert!(r.contains("[REDACTED]"), "{input} -> {r}");
    }
    // Vendor prefixes: Hugging Face, GitHub fine-grained PAT, GitLab PAT.
    for token in [
        "hf_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuv",
        "glpat-abcdefghijklmnopqrst12",
    ] {
        let (r, changed) = safety::redact_str(&format!("token is {token} ok"));
        assert!(changed && !r.contains(token), "{token} -> {r}");
    }
    // Prose and identifiers that only look near these shapes pass through.
    let prose = "the key=value syntax is documented; ?page=2 and hf_model_name are fine";
    let (r, changed) = safety::redact_str(prose);
    assert!(!changed, "ordinary text must not be redacted: {r}");
    assert_eq!(r, prose);
}

#[test]
fn secret_names_include_pat_creds_and_auth_suffixes_only() {
    for yes in ["GITHUB_PAT", "CI_CREDS", "DEPLOY_CREDENTIALS", "NPM_AUTH"] {
        assert!(safety::is_secret_name(yes), "{yes} should be a secret name");
    }
    // Suffix matches only: these contain `_PAT` or `_AUTH` but are not secrets.
    for no in ["LD_LIBRARY_PATH", "GIT_AUTHOR_NAME", "SSH_AUTH_SOCK"] {
        assert!(!safety::is_secret_name(no), "{no} must not be flagged");
    }
}
