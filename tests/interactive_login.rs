//! The shell could authenticate nobody.
//!
//! `src/auth.rs` carries a complete `AuthManager` -- registration, password
//! verification, sessions, bearer tokens, API keys, its own audit trail -- and
//! a search of the crate finds no caller for any of it. The only route to a
//! principal was `rbac_principal(id)`, which *asserts* an identity: name any
//! user and the guard treats you as them.
//!
//! These tests cover the door that was missing, and two properties that make it
//! worth having:
//!
//! 1. A wrong password is refused, and refused without saying which half was
//!    wrong.
//! 2. Passwords are stored salted. The previous store was bare SHA-256, so two
//!    accounts with the same password produced byte-identical entries and one
//!    crack broke both.

use aethershell::env::Env;
use aethershell::value::Value;
use std::sync::Mutex;

/// `AETHER_MODE` and the acting principal are process-global; these tests set
/// both, so they take turns.
static ENV: Mutex<()> = Mutex::new(());

fn call(name: &str, args: &[&str]) -> Result<Value, String> {
    let mut env = Env::new();
    let args = args.iter().map(|a| Value::Str(a.to_string())).collect();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn human() {
    std::env::set_var("AETHER_MODE", "human");
    std::env::remove_var("AETHER_POLICY");
    aethershell::safety::set_principal(None);
}

fn field(v: &Value, key: &str) -> String {
    match v {
        Value::Record(r) => match r.get(key) {
            Some(Value::Str(s)) => s.clone(),
            other => panic!("field {key} is {other:?}, not a string"),
        },
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn a_registered_user_can_log_in_and_becomes_the_principal() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    let reg = call("rbac_register", &["ada", "correct horse battery staple"])
        .expect("registration should succeed");
    let uid = field(&reg, "user_id");

    assert_eq!(
        aethershell::safety::current_principal(),
        None,
        "registering is not logging in"
    );

    let session = call("rbac_login", &["ada", "correct horse battery staple"])
        .expect("login with the right password should succeed");
    assert_eq!(field(&session, "user"), "ada");
    assert_eq!(field(&session, "user_id"), uid);
    assert!(!field(&session, "session").is_empty());
    assert_eq!(
        aethershell::safety::current_principal(),
        Some(uid),
        "a successful login must set the acting principal"
    );

    call("rbac_logout", &[]).unwrap();
}

#[test]
fn a_wrong_password_is_refused_and_leaves_the_caller_anonymous() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    call("rbac_register", &["grace", "hopper-1906"]).expect("registration should succeed");
    aethershell::safety::set_principal(None);

    let err = call("rbac_login", &["grace", "hopper-1907"])
        .expect_err("the wrong password must not authenticate");
    assert!(
        err.contains("invalid credentials"),
        "expected a credential refusal, got: {err}"
    );
    assert!(
        !err.contains("password") || !err.to_lowercase().contains("user not found"),
        "the refusal names which half was wrong: {err}"
    );
    assert_eq!(
        aethershell::safety::current_principal(),
        None,
        "a failed login must not set a principal"
    );

    // And an unknown user is refused the same way -- same message, so the
    // failure does not disclose which usernames exist.
    let unknown = call("rbac_login", &["nobody_at_all", "hopper-1906"])
        .expect_err("an unknown user must not authenticate");
    assert_eq!(unknown, err, "the two refusals must be indistinguishable");
}

#[test]
fn logging_out_drops_the_principal_and_the_session() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    call("rbac_register", &["linus", "torvalds-1991"]).unwrap();
    call("rbac_login", &["linus", "torvalds-1991"]).unwrap();
    assert!(matches!(
        call("rbac_session", &[]).unwrap(),
        Value::Record(_)
    ));

    assert_eq!(call("rbac_logout", &[]).unwrap(), Value::Bool(true));
    assert_eq!(aethershell::safety::current_principal(), None);
    assert_eq!(call("rbac_session", &[]).unwrap(), Value::Null);
    // A second logout is honest about having done nothing.
    assert_eq!(call("rbac_logout", &[]).unwrap(), Value::Bool(false));
}

#[test]
fn passwords_are_salted_so_equal_passwords_do_not_collide() {
    // The regression this replaces: `hash_key` is unsalted SHA-256, so the two
    // stored entries below were byte-identical and a single rainbow-table hit
    // opened both accounts.
    use aethershell::auth::{hash_password, verify_password};
    let a = hash_password("same password").unwrap();
    let b = hash_password("same password").unwrap();
    assert_ne!(a, b, "two hashes of one password are identical -- unsalted");
    assert!(a.starts_with("$argon2"), "not a PHC argon2 string: {a}");
    assert!(verify_password("same password", &a));
    assert!(verify_password("same password", &b));
    assert!(!verify_password("different password", &a));
    // A junk stored value must verify false, not error into a success path.
    assert!(!verify_password("anything", "not-a-hash"));
    assert!(!verify_password("anything", ""));
}

#[test]
fn an_agent_cannot_log_itself_in() {
    // Logging in is how authority is acquired. `Privileged` is `Deny` in agent
    // mode, so an agent's principal comes from whoever launched it
    // (`AETHER_PRINCIPAL` or the RBAC config), not from itself.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    call("rbac_register", &["agent_target", "s3cret-passphrase"]).unwrap();
    aethershell::safety::set_principal(None);

    std::env::set_var("AETHER_MODE", "agent");
    for (name, args) in [
        ("rbac_login", &["agent_target", "s3cret-passphrase"][..]),
        ("rbac_register", &["another", "s3cret-passphrase"][..]),
    ] {
        match call(name, args) {
            Err(e) => assert!(
                e.contains("E_POLICY_DENY"),
                "{name} failed for the wrong reason: {e}"
            ),
            Ok(v) => panic!("{name} was allowed in agent mode, returning {v:?}"),
        }
    }
    assert_eq!(
        aethershell::safety::current_principal(),
        None,
        "the denied login must not have set a principal"
    );
    std::env::set_var("AETHER_MODE", "human");
}

#[test]
fn a_password_is_never_read_from_a_pipe() {
    // Under `cargo test` stdin is not a terminal, so the interactive branch
    // must refuse rather than block forever or consume piped data.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    let err = call("rbac_login", &["someone"])
        .expect_err("a passwordless login must not silently read stdin");
    assert!(
        err.contains("not a terminal"),
        "expected a terminal refusal, got: {err}"
    );
}

#[test]
fn an_unknown_username_costs_the_same_as_a_wrong_password() {
    // Identical *messages* are not enough: without a decoy verification the
    // unknown-user path returns before any hashing happens, and the two are a
    // stopwatch apart. That difference enumerates valid usernames from the
    // outside.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    human();
    call("rbac_register", &["timed", "a-real-password"]).unwrap();
    aethershell::safety::set_principal(None);

    let bench = |user: &str| {
        let t = std::time::Instant::now();
        for _ in 0..3 {
            let _ = call("rbac_login", &[user, "not-the-password"]);
        }
        t.elapsed()
    };
    // Warm the lazily-built decoy hash so it is not charged to the first run.
    let _ = call("rbac_login", &["no_such_user_warmup", "x"]);

    let known = bench("timed");
    let unknown = bench("definitely_no_such_user");
    let ratio = unknown.as_secs_f64() / known.as_secs_f64().max(f64::MIN_POSITIVE);
    assert!(
        ratio > 0.4,
        "an unknown username answers {ratio:.2}x as fast as a wrong password          ({unknown:?} vs {known:?}) -- that gap is a username oracle"
    );
}
