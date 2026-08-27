//! An agent may not approve its own denied call.
//!
//! §5.3's policy table grades `Destructive` and `Exec` as **approve** in agent
//! mode, and §7.2 documented the loop as `E_NEEDS_APPROVAL` → `approve(token)` →
//! re-run succeeds. That loop is right for a human at a REPL and wrong for an
//! agent, because the agent holds the token it was just refused with.
//!
//! It was demonstrated before it was fixed: the denied call succeeded on retry,
//! because `approve` is classified `Pure`, `Pure` is not centrally enforced, and
//! `bi_approve` called `grant_approval` unconditionally. Against an agent that
//! *errs* the second step is a real speed bump; against one that is *adversarial
//! or prompt-injected* the `approve` cell was simply `allow`.
//!
//! The fix refuses the in-band path **in agent mode only**:
//!
//! * human mode is untouched — the documented REPL loop still works;
//! * the out-of-band channel the design already provides (`AETHER_APPROVE`,
//!   `AETHER_APPROVE_ALL`, set by whoever launched the agent) still works, so no
//!   capability was removed that was not already reachable by someone outside
//!   the agent's control.
//!
//! The assumption behind that choice, stated so it can be overridden: **the
//! adversarial threat model is in scope.** For a shell built to be driven by
//! language models, prompt injection is the defining threat, and hardening in
//! that direction is the conservative error.

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
fn an_agent_cannot_spend_the_token_it_was_just_handed() {
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

    // 2. Spending it in-band is now refused. This is the fix.
    let self_grant = call("approve", vec![Value::Str(token.clone())]);

    // 3. And the call is still refused afterwards.
    let after = safety::guard_dispatch("git_clean", &[]);
    safety::revoke_approval(&token);
    std::env::remove_var("AETHER_MODE");

    let e = self_grant.expect_err("an agent must not approve its own call");
    let text = format!("{e:#}").to_ascii_lowercase();
    assert!(
        text.contains("own call") || text.contains("self-service"),
        "the refusal should say why, and point at the out-of-band path: {text}"
    );
    assert!(
        text.contains("aether_approve"),
        "the refusal must name the channel that does work, or it is a dead end: {text}"
    );
    assert!(
        after.is_err(),
        "the denied call must stay denied after a refused self-approval"
    );
}

#[test]
fn a_human_can_still_approve_at_the_repl() {
    // The check that keeps the fix from being a removal. §7.2's loop is correct
    // for a person: they are not the party being constrained.
    let _g = lock();
    std::env::remove_var("AETHER_MODE");
    let r = call("approve", vec![Value::Str("apv_human_test".into())]);
    safety::revoke_approval("apv_human_test");
    assert!(
        r.is_ok(),
        "human mode must be untouched — the documented REPL loop still works: {r:?}"
    );
}

#[test]
fn the_out_of_band_channel_still_works_in_agent_mode() {
    // The fix would be a wall rather than a redirection if this did not hold.
    // `AETHER_APPROVE` is set by whoever launches the agent, which is the whole
    // point: the approver is not the approved.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");

    let token = safety::guard_dispatch("git_clean", &[])
        .expect_err("denied")
        .approval
        .map(|a| a.token)
        .expect("token");

    std::env::set_var("AETHER_APPROVE", &token);
    let after = safety::guard_dispatch("git_clean", &[]);
    std::env::remove_var("AETHER_APPROVE");
    std::env::remove_var("AETHER_MODE");

    assert!(
        after.is_ok(),
        "an out-of-band approval must still let the call through: {after:?}"
    );
}

#[test]
fn approve_is_still_pure_and_that_is_now_fine() {
    // `approve` remains unclassified, so `guard_dispatch` still waves it through.
    // That is no longer the whole story: the refusal moved into the body, which
    // is where the mode is known. Asserted so a future reclassification is a
    // deliberate act rather than a surprise.
    assert_eq!(effect_of("approve"), Effect::Pure);
}

#[test]
fn the_binding_property_that_does_hold_still_holds() {
    // What §7.2 actually guarantees, and it survives the fix: a token is bound to
    // one action descriptor, so it cannot be replayed against a different call.
    //
    // The control has to be a builtin the *central* gate judges. The first draft
    // used `proc_kill`, which is in `SELF_GUARDED` — `guard_dispatch` returns
    // `Ok` for it before any token is consulted, so the test failed while the
    // property was fine. A bad control produces a false alarm as readily as a
    // real finding.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");

    let a = safety::guard_dispatch("git_clean", &[])
        .expect_err("denied")
        .approval
        .map(|x| x.token)
        .expect("token");
    safety::grant_approval(&a);

    let other = safety::guard_dispatch("git_reset", &[]);
    safety::revoke_approval(&a);
    std::env::remove_var("AETHER_MODE");

    assert!(
        other.is_err(),
        "a token bound to git_clean must not approve git_reset — this is the \
         content-binding property §7.2 claims, and it holds"
    );
}
