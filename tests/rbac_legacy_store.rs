//! The older `role_*` store is inert as far as the guard is concerned — and the
//! reason that is *safe* is the reason bridging it is not a chore.
//!
//! Two RBAC stores coexist, and they are not two copies of one model:
//!
//! - `builtins::RBAC_ROLES` / `USER_ROLES`, written by `role_create`,
//!   `role_grant`, `role_revoke`, hold `(resource, actions)` pairs —
//!   `("config.toml", ["write"])`. This is what `rbac.check(user, res, action)`
//!   answers from, and it is the model the README documents.
//! - `auth::RbacManager`, reached through `safety::set_rbac_manager`, holds
//!   capability strings — `effect:destructive`, `effect:*`. This is the one
//!   `safety::guard` consults, and the only one that can admit an agent past an
//!   approval prompt.
//!
//! Roadmap item "optionally bridge the older `RBAC_ROLES` registry into
//! `RbacManager`" reads like plumbing. It is not, and this file is why:
//! `rbac_grant` is classified `Privileged` (agent mode: `Deny`) precisely
//! because writing into the store the guard reads is self-elevation —
//! `tests/privilege_escalation.rs` runs that sequence. `role_create`,
//! `role_grant` and `role_revoke` are `Privileged` too: they write the legacy
//! store, and classifying them is what keeps the two stores from being bridged
//! into an ungated spelling of the operation `rbac_grant` is denied for — the
//! same shape as the alias bypass fixed in `194a496`.
//!
//! So these tests pin the separation and the classification. If someone
//! bridges the stores, they go red and say what has to happen first.

use iron_shell::env::Env;
use iron_shell::safety::{effect_of, Effect};
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn agent_mode() -> std::path::PathBuf {
    let ws = std::env::temp_dir().join("ae_rbac_legacy");
    std::fs::create_dir_all(&ws).unwrap();
    std::env::set_var("IRONSHELL_MODE", "agent");
    std::env::set_var("IRONSHELL_WORKSPACE", &ws);
    std::env::remove_var("IRONSHELL_POLICY");
    iron_shell::safety::set_principal(None);
    ws
}

/// `[{resource: "*", actions: ["*"]}]` — the broadest grant the legacy model can
/// express.
fn everything() -> Value {
    let mut rec = BTreeMap::new();
    rec.insert("resource".to_string(), s("*"));
    rec.insert("actions".to_string(), Value::Array(vec![s("*")]));
    Value::Array(vec![Value::Record(rec)])
}

#[test]
fn the_two_grant_verbs_are_classified_differently_and_that_is_load_bearing() {
    assert_eq!(
        effect_of("rbac_grant"),
        Effect::Privileged,
        "rbac_grant writes into the store `guard` consults; if this is ever \
         relaxed, tests/privilege_escalation.rs is the one that matters"
    );
    assert_eq!(
        effect_of("role_grant"),
        Effect::Privileged,
        "role_grant writes the legacy store, which the guard does not read. It \
         is Privileged anyway: bridging the stores later must not be able to \
         hand an agent an ungated grant. See this file's header"
    );
}

/// Setup for the fixtures below: an operator administers the legacy store in
/// human mode, where `Privileged` is allowed. The agent-mode assertions are
/// made after switching back.
fn operator_setup() {
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
}

#[test]
fn a_role_granted_the_old_way_does_not_open_the_gate() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let ws = agent_mode();
    let victim = ws.join("legacy_rbac_target.txt");
    std::fs::write(&victim, "contents").unwrap();
    let path = victim.to_string_lossy().to_string();

    // Refused before: `rm` is Destructive and the agent holds no authority.
    let before =
        call("rm", vec![s(&path)]).expect_err("rm must be refused in agent mode with no principal");
    assert!(
        before.contains("approval"),
        "the baseline refusal must be the gate, not something else:\n{before}"
    );

    // The agent cannot run the legacy sequence at all: both verbs are
    // `Privileged`, so the gate refuses them before the body runs.
    assert!(
        call("role_create", vec![s("legacy_god"), everything()]).is_err(),
        "role_create must be refused for an agent with no principal"
    );
    assert!(
        call("role_grant", vec![s("legacy_agent"), s("legacy_god")]).is_err(),
        "role_grant must be refused for an agent with no principal"
    );

    // An operator sets the grant up in human mode, the way it always has. The
    // store then holds the broadest grant the legacy model can express.
    operator_setup();
    call("role_create", vec![s("legacy_god"), everything()]).expect("operator role_create");
    call("role_grant", vec![s("legacy_agent"), s("legacy_god")]).expect("operator role_grant");
    agent_mode();

    // Still refused. The grant went into a store `guard` does not read.
    let after = call("rm", vec![s(&path)]);
    assert!(
        after.is_err(),
        "a role granted through the legacy store conferred real authority — the \
         two RBAC stores have been bridged. `role_create`/`role_grant`/\
         `role_revoke` must be classified in `safety::classified_effect` before \
         that is safe: `rbac_grant` is `Privileged` for this exact reason, and \
         without the same treatment `role_grant` is now an ungated spelling of \
         a denied operation."
    );
    assert!(
        after.unwrap_err().contains("approval"),
        "still refused, but no longer by the gate — check what changed"
    );

    let _ = std::fs::remove_file(&victim);
}

#[test]
fn the_legacy_store_still_answers_its_own_question() {
    // Inert toward the guard is not the same as broken. `rbac.check` is
    // documented in the README and must keep working, or the fix in `d286ae0`
    // (the rbac.* module pointing at builtins that did not exist) is undone.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();

    let mut rec = BTreeMap::new();
    rec.insert("resource".to_string(), s("config.toml"));
    rec.insert("actions".to_string(), Value::Array(vec![s("write")]));
    let perms = Value::Array(vec![Value::Record(rec)]);

    operator_setup();
    call("role_create", vec![s("editor"), perms]).expect("operator role_create");
    call("role_grant", vec![s("alice"), s("editor")]).expect("operator role_grant");
    agent_mode();

    let allowed = call(
        "check_permission",
        vec![s("alice"), s("config.toml"), s("write")],
    )
    .expect("check_permission answers");
    assert_eq!(
        allowed,
        Value::Bool(true),
        "the legacy store must still answer the question the README shows"
    );

    let denied = call(
        "check_permission",
        vec![s("alice"), s("config.toml"), s("delete")],
    )
    .expect("check_permission answers");
    assert_eq!(denied, Value::Bool(false), "and must still say no");
}
