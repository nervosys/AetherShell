//! The `Privileged` class governed nothing, and self-elevation was `Pure`.
//!
//! `decide(Privileged, Agent)` is `Deny` — the strongest rule in the taxonomy —
//! and a sweep of the classifier found **no builtin classified `Privileged` at
//! all**. That is the `rm` bug inverted: policy that reads as coverage while
//! governing nothing.
//!
//! Most of the privilege-shaped names (`sudo_exec`, `user_add`, `acl_set`,
//! `fs_unmount`) turn out to be stubs that return "requires elevated
//! privileges" and perform no effect, so `Pure` is honest for them — measured,
//! not assumed. Two names are not stubs:
//!
//! - `rbac_grant(u, "effect:*")` writes a permission into the very store the
//!   guard consults, and
//! - `rbac_principal(u)` makes the shell act as that user.
//!
//! `guard` treats an authorized principal as a reason to skip approval
//! entirely (safety.rs, "RBAC: an authorized principal bypasses the approval
//! requirement"). Classified `Pure`, both were `Allow` in agent mode — so an
//! agent could grant itself `effect:*`, become that principal, and delete
//! whatever it liked without ever meeting an approval prompt.
//!
//! These tests hold that door shut.

use aethershell::env::Env;
use aethershell::value::Value;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<&str>) -> Result<Value, String> {
    let mut env = Env::new();
    let args = args
        .into_iter()
        .map(|a| Value::Str(a.to_string()))
        .collect();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn agent_mode() -> std::path::PathBuf {
    let ws = std::env::temp_dir().join("ae_privesc");
    std::fs::create_dir_all(&ws).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &ws);
    std::env::remove_var("AETHER_POLICY");
    aethershell::safety::set_principal(None);
    ws
}

fn human_mode() {
    std::env::set_var("AETHER_MODE", "human");
    aethershell::safety::set_principal(None);
}

#[test]
fn an_agent_cannot_grant_itself_permissions() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();
    let err = call("rbac_grant", vec!["escalator", "effect:*"])
        .expect_err("granting a permission must not be allowed in agent mode");
    assert!(
        err.contains("E_POLICY_DENY"),
        "expected a policy denial, got: {err}"
    );
}

#[test]
fn an_agent_cannot_change_who_it_acts_as() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();
    let err = call("rbac_principal", vec!["someone_else"])
        .expect_err("switching principal must not be allowed in agent mode");
    assert!(
        err.contains("E_POLICY_DENY"),
        "expected a policy denial, got: {err}"
    );
}

#[test]
fn the_escalation_path_end_to_end_is_closed() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let ws = agent_mode();
    let victim = ws.join("victim.txt");
    std::fs::write(&victim, "please do not delete me").unwrap();
    let path = victim.to_string_lossy().to_string();

    // Baseline: a destructive op inside the workspace is approval-gated.
    let before = call("rm", vec![&path]).expect_err("rm should be gated in agent mode");
    assert!(
        before.contains("E_NEEDS_APPROVAL"),
        "baseline is not an approval gate, so the rest proves nothing: {before}"
    );

    // The escalation attempt, in the order an agent would run it.
    let _ = call("rbac_grant", vec!["escalator", "effect:*"]);
    let _ = call("rbac_principal", vec!["escalator"]);

    // And the gate still holds.
    let after = call("rm", vec![&path]).expect_err("rm must still be gated after the attempt");
    assert!(
        after.contains("E_NEEDS_APPROVAL"),
        "self-granted privileges bypassed approval: {after}"
    );
    assert!(victim.exists(), "the file was deleted despite the gate");
    let _ = std::fs::remove_file(&victim);
}

#[test]
fn a_human_session_can_still_administer_rbac() {
    // Denying agents must not lock the human out of the feature: `Privileged`
    // is `Allow` in human mode, which is the whole point of the split.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human_mode();
    call("rbac_grant", vec!["operator", "effect:destructive"])
        .expect("a human session administers RBAC");
    call("rbac_principal", vec!["operator"]).expect("a human session may set the principal");
    assert_eq!(
        aethershell::safety::current_principal().as_deref(),
        Some("operator")
    );
    aethershell::safety::set_principal(None);
}

#[test]
fn reading_authorization_state_stays_ungated() {
    // The read side must not be swept up in the denial: an agent that cannot
    // ask what it is allowed to do will guess.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();
    call("rbac_can", vec!["effect:destructive"]).expect("asking is not escalating");
    let mut env = Env::new();
    aethershell::builtins::call("rbac_principal", vec![], &mut env)
        .expect("reading the current principal is not escalating");
}
