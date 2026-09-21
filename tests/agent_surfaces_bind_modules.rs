//! Every surface that evaluates AetherShell *source* must have the module
//! namespaces bound.
//!
//! `POST /api/v1/eval` is Option 2 of `AGENTS.md` — the documented way for an
//! agent to drive the shell over HTTP. It built its environment with a bare
//! `Env::default()`, so `file.read(…)`, `sys.hostname()`, `http.get(…)` and
//! every other module-qualified call — all 108 namespaces — failed with
//!
//! ```text
//! cannot access field 'read' on non-record value: Null
//! ```
//!
//! a generic field-access error naming neither the module nor the real problem.
//! The registration loop had been copied into `main.rs` and into
//! `builtins::Session::new`, and simply not into `execute_eval`.
//!
//! Three copies of a loop is how that happens, so the fix is one constructor
//! (`modules::env_with_modules`). A test that asserted *which* constructor each
//! site calls would just be a second copy of the same knowledge, so these
//! assert the property an agent actually depends on: run source through the
//! surface, and see whether the module resolved.

use aethershell::agent_api::{process_request, AgentRequest};

/// Representative of the classes an agent reaches for: filesystem, system info,
/// string handling, data. If the namespace is unbound they all fail the same
/// way, so a single one would do — but a bound-but-empty namespace would pass
/// that weaker test, and these would not.
const PROBES: &[(&str, &str)] = &[
    ("str", r#"str.upper("ok")"#),
    ("json", r#"json.stringify([1, 2])"#),
    ("math", "math.sqrt(16)"),
    ("arr", "arr.range(3)"),
];

fn eval(code: &str) -> aethershell::agent_api::AgentResponse {
    process_request(&AgentRequest::Eval {
        code: code.to_string(),
    })
}

#[test]
fn the_eval_endpoint_can_call_module_functions() {
    for (module, code) in PROBES {
        let r = eval(code);
        assert!(
            r.success,
            "`{code}` failed through /api/v1/eval — is the `{module}` namespace \
             bound? error: {:?}",
            r.error
        );
    }
}

#[test]
fn an_unbound_namespace_is_what_this_test_would_catch() {
    // Non-vacuity, and it pins the exact symptom. `nope` is not a module, so it
    // reproduces the failure the real bug produced. If this ever succeeds, the
    // test above has stopped being able to distinguish bound from unbound.
    let r = eval("nope.read(\"x\")");
    assert!(
        !r.success,
        "a call on an unbound namespace must not succeed"
    );
    let err = r.error.unwrap_or_default();
    assert!(
        err.contains("nope") || err.contains("non-record"),
        "unexpected failure shape: {err}"
    );
}
