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
//! `tests/privilege_escalation.rs` runs that sequence. `role_grant` carries no
//! such classification, because today it writes into a store the guard never
//! reads. **Bridging the two without classifying `role_*` first would hand the
//! agent surface an ungated spelling of the operation `rbac_grant` is denied
//! for** — the same shape as the alias bypass fixed in `194a496`, arrived at
//! from the other direction.
//!
//! So these tests pin the *current* separation. If someone bridges the stores,
//! they go red and say what has to happen first.

use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn agent_mode() -> std::path::PathBuf {
    let ws = std::env::temp_dir().join("ae_rbac_legacy");
    std::fs::create_dir_all(&ws).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &ws);
    std::env::remove_var("AETHER_POLICY");
    aethershell::safety::set_principal(None);
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
        Effect::Pure,
        "role_grant is unclassified because it writes into a store the guard \
         never reads. That is only defensible while the stores stay separate — \
         see this file's header before bridging them"
    );
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

    // The legacy sequence, in full, granting the broadest role it can express.
    // Both calls are expected to *succeed* — they are `Pure`, and that is
    // exactly the point: they are allowed because they confer nothing.
    call("role_create", vec![s("legacy_god"), everything()]).expect("role_create is not gated");
    call("role_grant", vec![s("legacy_agent"), s("legacy_god")]).expect("role_grant is not gated");

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

    call("role_create", vec![s("editor"), perms]).expect("role_create");
    call("role_grant", vec![s("alice"), s("editor")]).expect("role_grant");

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
