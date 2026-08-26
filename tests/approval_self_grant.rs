//! Can the agent approve its own denied call?
//!
//! §5.3's policy table reads:
//!
//! ```text
//! | Effect      | Human  | Agent    |
//! | Destructive | allow  | approve  |
//! | Exec        | allow  | approve  |
//! ```
//!
//! and §7.2 documents the loop as intended: "a guarded op returns
//! `E_NEEDS_APPROVAL` with a token → `approve(token)` → re-run succeeds."
//!
//! `approve` is a builtin (dispatch index 1104), `effect_of("approve")` is
//! `Pure`, `Pure` is not centrally enforced, and `bi_approve` calls
//! `grant_approval` unconditionally. So the same agent that was denied is the
//! one holding the token, and nothing stops it spending it.
//!
//! This file does not assert that is a bug. It asserts **what the control
//! actually is**, so the claim in the policy table can be read accurately:
//!
//! * against an agent that makes a *mistake*, `approve` is a real control — the
//!   dangerous call does not happen by default, a second deliberate step is
//!   required, and both steps are audited;
//! * against an agent that is *adversarial or prompt-injected*, the `approve`
//!   cell is equivalent to `allow`, because the attacker controls both steps.
//!
//! For a shell whose stated purpose is to be driven by language models, prompt
//! injection is the defining threat, so the distinction is not academic. Which
//! of the two the product intends to defend against is a decision for the
//! owner; what this test prevents is the gap being *rediscovered* rather than
//! *chosen*.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::safety::{self, effect_of, Effect};
use aethershell::value::Value;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

#[test]
fn an_agent_can_spend_the_token_it_was_just_handed() {
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");

    // 1. A Destructive call is refused, and the refusal carries the token.
    let denied = safety::guard_dispatch("git_clean", &[]);
    let err = denied.expect_err("git_clean must be refused in agent mode");
    assert_eq!(err.code, safety::ErrorCode::NeedsApproval);
    let token = err
        .approval
        .as_ref()
        .map(|a| a.token.clone())
        .expect("the refusal hands the caller a token");

    // 2. The agent spends it itself. Nothing gates this.
    let approved = call("approve", vec![Value::Str(token.clone())]);
    assert!(
        approved.is_ok(),
        "`approve` is reachable and ungated from the same surface that was \
         denied: {approved:?}"
    );

    // 3. The same call now succeeds.
    let after = safety::guard_dispatch("git_clean", &[]);
    safety::revoke_approval(&token);
    std::env::remove_var("AETHER_MODE");

    assert!(
        after.is_ok(),
        "the previously-denied call must now pass — this is the documented loop"
    );
}

#[test]
fn approve_is_itself_ungated() {
    // The property that makes the above possible, stated on its own so a change
    // to it is visible. `Pure` is not in `centrally_enforced()`, so
    // `guard_dispatch` returns `Ok` for `approve` before any policy is consulted.
    assert_eq!(
        effect_of("approve"),
        Effect::Pure,
        "if `approve` gains an effect class, the self-grant path changes and the \
         note at the top of this file needs rewriting"
    );
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");
    let r = safety::guard_dispatch("approve", &[Value::Str("apv_whatever".into())]);
    std::env::remove_var("AETHER_MODE");
    assert!(
        r.is_ok(),
        "`approve` passes the central gate untouched in agent mode"
    );
}

#[test]
fn the_binding_property_that_does_hold_still_holds() {
    // What §7.2 actually guarantees, and it is worth keeping: a token is bound
    // to one action descriptor, so it cannot be replayed against a different
    // call. That is a real property and this asserts it independently of the
    // self-grant question above.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");

    let a = safety::guard_dispatch("git_clean", &[])
        .expect_err("denied")
        .approval
        .map(|x| x.token)
        .expect("token");
    safety::grant_approval(&a);

    // A different builtin must not be let through by another call's token.
    //
    // The control has to be a builtin the *central* gate actually judges. The
    // first draft used `proc_kill`, which is in `SELF_GUARDED` -- so
    // `guard_dispatch` returns `Ok` for it before any token is consulted, and
    // the test failed while the property under test was fine. `git_reset` is
    // `Destructive` and guards centrally, so it is a valid control.
    let other = safety::guard_dispatch("git_reset", &[]);
    safety::revoke_approval(&a);
    std::env::remove_var("AETHER_MODE");

    assert!(
        other.is_err(),
        "a token bound to git_clean must not approve git_reset — this is the \
         content-binding property §7.2 claims, and it is the half that works"
    );
}
