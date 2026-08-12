//! Redaction must hide secrets without destroying the agent's own data.
//!
//! `safety::is_secret_name` matches substrings, so `TOKEN` also matches
//! `full_tokens`, `compact_tokens`, `page_tokens` and `tokens_in`. Those hold
//! **counts**. Blanking them made the token-economy surface — the thing this
//! project measures itself by — report `[REDACTED]` to agents instead of
//! numbers, and it broke `plan()`, which returned `token="[REDACTED]"` in its
//! machine-readable field while printing the real token in `hint`.
//!
//! Found by driving the shell as an agent. These tests pin both directions:
//! secrets stay hidden, and the agent's own data survives.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

fn render(v: &Value) -> String {
    aethershell::builtins::render_agent(v, None).expect("agent render")
}

#[test]
fn a_real_secret_is_still_hidden() {
    // The property that must not regress while fixing the false positives.
    let v = rec(&[
        ("api_key", Value::Str("sk-live-abcdefghijklmnop".into())),
        ("password", Value::Str("hunter2".into())),
        ("client_secret", Value::Str("shhh".into())),
    ]);
    let out = render(&v);
    assert!(!out.contains("sk-live"), "api_key leaked: {out}");
    assert!(!out.contains("hunter2"), "password leaked: {out}");
    assert!(!out.contains("shhh"), "client_secret leaked: {out}");
}

#[test]
fn a_token_count_is_not_a_secret() {
    // `full_tokens` contains "TOKEN" and holds an integer. A number cannot be a
    // credential, and blanking it destroys the answer the agent asked for.
    let v = rec(&[
        ("full_tokens", Value::Int(4180)),
        ("compact_tokens", Value::Int(216)),
        ("page_tokens", Value::Int(81)),
    ]);
    let out = render(&v);
    assert!(out.contains("4180"), "token counts must survive: {out}");
    assert!(out.contains("216"), "token counts must survive: {out}");
    assert!(!out.contains("REDACTED"), "nothing here is secret: {out}");
}

#[test]
fn an_approval_token_survives_because_the_agent_must_echo_it_back() {
    // `apv_`/`apl_` values are capabilities, not credentials. Hiding one in the
    // structured field while `hint` prints it is incoherent, and it leaves a
    // caller reading the machine-readable field unable to proceed.
    let v = rec(&[
        ("token", Value::Str("apl_9fa896b743ee6f2f".into())),
        ("operations", Value::Int(1)),
    ]);
    let out = render(&v);
    assert!(
        out.contains("apl_9fa896b743ee6f2f"),
        "the plan token must reach the agent: {out}"
    );
}

#[test]
fn a_secret_string_under_a_token_name_is_still_hidden() {
    // The exemption is narrow: it covers capability handles by prefix, not
    // everything that happens to sit under a field called `token`.
    let v = rec(&[("auth_token", Value::Str("ghp_realcredentialvalue".into()))]);
    let out = render(&v);
    assert!(
        !out.contains("ghp_realcredentialvalue"),
        "a genuine credential must stay hidden: {out}"
    );
}

#[test]
fn a_secret_nested_inside_a_container_is_still_found() {
    // Containers are walked rather than blanked wholesale, so this checks the
    // walk still catches what it should.
    let v = Value::Array(vec![rec(&[(
        "credentials",
        rec(&[("password", Value::Str("nested-secret".into()))]),
    )])]);
    let out = render(&v);
    assert!(
        !out.contains("nested-secret"),
        "nested secret leaked: {out}"
    );
}

// ── The JSON layer (audit entries) ──────────────────────────────────────────
//
// `redact_json` had the same two flaws as the value layer. It matters more here:
// an audit entry reading `needs_approval` with the token blanked cannot be
// correlated with the grant that followed it, which is most of what an audit
// log is for.

#[test]
fn an_audit_entry_keeps_the_approval_token_it_is_about() {
    let mut detail = serde_json::json!({ "token": "apv_1f67575ecb24b5fe" });
    aethershell::safety::redact_json(&mut detail);
    assert_eq!(
        detail["token"], "apv_1f67575ecb24b5fe",
        "the audit trail must stay correlatable: {detail}"
    );
}

#[test]
fn an_audit_entry_still_hides_a_real_credential() {
    let mut detail = serde_json::json!({
        "auth_token": "ghp_realcredentialvalue",
        "password": "hunter2",
    });
    aethershell::safety::redact_json(&mut detail);
    let text = detail.to_string();
    assert!(!text.contains("ghp_real"), "credential leaked: {text}");
    assert!(!text.contains("hunter2"), "password leaked: {text}");
}

#[test]
fn an_audit_entry_keeps_numeric_counts() {
    let mut detail = serde_json::json!({ "page_tokens": 81, "tokens_in": 12 });
    aethershell::safety::redact_json(&mut detail);
    assert_eq!(detail["page_tokens"], 81);
    assert_eq!(detail["tokens_in"], 12);
}

#[test]
fn both_redaction_layers_agree_on_what_a_capability_is() {
    // The rule lives in one place precisely so the value layer and the JSON
    // layer cannot drift into disagreeing about the same string.
    use aethershell::safety::is_capability_token;
    assert!(is_capability_token("apv_deadbeef"));
    assert!(is_capability_token("apl_deadbeef"));
    assert!(!is_capability_token("ghp_realcredentialvalue"));
    assert!(!is_capability_token("sk-live-abc"));
}
