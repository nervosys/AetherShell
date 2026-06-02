//! End-to-end tests for the safety core wired into effecting builtins.
//!
//! These exercise the real `bi_rm` dispatch path (not just the policy engine in
//! isolation) to prove that the guard is actually wired in: agent mode gates
//! destructive ops behind approval and the workspace jail, while human mode is
//! unchanged.

use aethershell::safety;
use aethershell::value::Value;

// Env is process-global and tests run in parallel threads — serialize them.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire the env lock, recovering from poisoning so that a panic in one test
/// cannot cascade and mask the real failure in every other test.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn clear() {
    for k in [
        "AETHER_MODE",
        "AETHER_AGENT",
        "AETHER_POLICY",
        "AETHER_APPROVE",
        "AETHER_APPROVE_ALL",
        "AETHER_WORKSPACE",
        "AETHER_AUDIT_LOG",
        "AETHER_MAX_OPS",
        "AETHER_MAX_FILES",
        "AETHER_MAX_PROCS",
        "AETHER_TIMEOUT_MS",
        "AETHER_RBAC_CONFIG",
        "AETHER_PRINCIPAL",
    ] {
        std::env::remove_var(k);
    }
    // Reset process-global safety state so tests can't contaminate each other.
    safety::set_principal(None);
    safety::clear_rbac_manager();
    safety::governor_reset();
}

/// A unique workspace dir + an audit log redirected into it.
fn fresh_workspace(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_safety_it_{}_{}", tag, std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("AETHER_WORKSPACE", &dir);
    std::env::set_var(
        "AETHER_AUDIT_LOG",
        dir.join("audit.log").to_string_lossy().to_string(),
    );
    dir
}

fn token_from_error(err: &anyhow::Error) -> String {
    let rendered = format!("{}", err);
    let json: serde_json::Value =
        serde_json::from_str(&rendered).expect("safety error should render as JSON");
    json["error"]["approval"]["token"]
        .as_str()
        .expect("needs-approval error carries an approval token")
        .to_string()
}

#[test]
fn rm_in_human_mode_just_works() {
    let _l = lock();
    clear();
    let dir = std::env::temp_dir().join(format!("ae_human_rm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("victim.txt");
    std::fs::write(&file, b"bye").unwrap();

    let res =
        aethershell::builtins::bi_rm(vec![Value::Str(file.to_string_lossy().to_string())], None);
    assert!(res.is_ok(), "human-mode rm should succeed: {:?}", res.err());
    assert!(!file.exists(), "file should be gone");
    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn rm_in_agent_mode_requires_then_accepts_approval() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("rm_approve");
    let file = dir.join("victim.txt");
    std::fs::write(&file, b"bye").unwrap();
    let arg = Value::Str(file.to_string_lossy().to_string());

    // First call: blocked, file untouched, structured needs-approval error.
    let err = aethershell::builtins::bi_rm(vec![arg.clone()], None).unwrap_err();
    let rendered = format!("{}", err);
    assert!(rendered.contains("E_NEEDS_APPROVAL"), "got: {rendered}");
    assert!(file.exists(), "file must NOT be deleted before approval");

    // Re-call with the bound token: now it proceeds.
    let token = token_from_error(&err);
    std::env::set_var("AETHER_APPROVE", &token);
    let res = aethershell::builtins::bi_rm(vec![arg], None);
    assert!(res.is_ok(), "approved rm should succeed: {:?}", res.err());
    assert!(!file.exists(), "file should be gone after approval");

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn governor_file_budget_blocks_second_rm_in_agent_mode() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    // Pre-approve so rm proceeds past the approval gate — we want the governor,
    // not approval, to be the thing that stops the second delete.
    std::env::set_var("AETHER_APPROVE_ALL", "1");
    let dir = fresh_workspace("rm_governor");
    safety::governor_reset();
    std::env::set_var("AETHER_MAX_FILES", "1");

    let f1 = dir.join("a.txt");
    let f2 = dir.join("b.txt");
    std::fs::write(&f1, b"a").unwrap();
    std::fs::write(&f2, b"b").unwrap();

    // First delete is within the file budget.
    let r1 = aethershell::builtins::bi_rm(vec![Value::Str(f1.to_string_lossy().to_string())], None);
    assert!(r1.is_ok(), "first rm should succeed: {:?}", r1.err());
    assert!(!f1.exists());

    // Second delete exceeds AETHER_MAX_FILES → E_BUDGET_EXCEEDED, file untouched.
    let err =
        aethershell::builtins::bi_rm(vec![Value::Str(f2.to_string_lossy().to_string())], None)
            .unwrap_err();
    let rendered = format!("{}", err);
    assert!(rendered.contains("E_BUDGET_EXCEEDED"), "got: {rendered}");
    assert!(
        f2.exists(),
        "file must NOT be deleted once the budget is exhausted"
    );

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn rbac_config_loaded_at_startup_authorizes_principal() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("rbac_cfg");

    // A config that grants the `ci` principal the `destructive` effect via a role.
    let cfg = dir.join("rbac.toml");
    std::fs::write(
        &cfg,
        r#"
principal = "ci"
[[role]]
name = "deployer"
permissions = ["effect:destructive"]
[[user]]
id = "ci"
roles = ["deployer"]
"#,
    )
    .unwrap();
    std::env::set_var("AETHER_RBAC_CONFIG", cfg.to_string_lossy().to_string());

    // Boot-time load installs the manager and sets the acting principal.
    safety::init_rbac_from_env();
    assert_eq!(safety::current_principal().as_deref(), Some("ci"));

    // The authorized principal bypasses the approval gate for a destructive rm.
    let file = dir.join("v.txt");
    std::fs::write(&file, b"x").unwrap();
    let res =
        aethershell::builtins::bi_rm(vec![Value::Str(file.to_string_lossy().to_string())], None);
    assert!(
        res.is_ok(),
        "RBAC-authorized principal should bypass approval: {:?}",
        res.err()
    );
    assert!(!file.exists());

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn rm_outside_workspace_is_blocked_in_agent_mode() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let _dir = fresh_workspace("rm_jail");

    let outside = if cfg!(windows) {
        "C:/Windows/System32/drivers/etc/hosts"
    } else {
        "/etc/hosts"
    };
    let err =
        aethershell::builtins::bi_rm(vec![Value::Str(outside.to_string())], None).unwrap_err();
    let rendered = format!("{}", err);
    assert!(rendered.contains("E_OUTSIDE_WORKSPACE"), "got: {rendered}");

    let _ = std::fs::remove_dir_all(&_dir);
    clear();
}

#[test]
fn approve_builtin_unblocks_guarded_rm_and_audit_verify_passes() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("approve_builtin");
    let file = dir.join("v.txt");
    std::fs::write(&file, b"x").unwrap();
    let arg = Value::Str(file.to_string_lossy().to_string());
    let mut env = aethershell::env::Env::new();

    // Trigger the block via the real guarded builtin, then pull the bound token.
    let err = aethershell::builtins::bi_rm(vec![arg.clone()], None).unwrap_err();
    let token = token_from_error(&err);

    // approve(token) via the dispatch table (proves index 1104 → bi_approve).
    let approved = aethershell::builtins::call("approve", vec![Value::Str(token)], &mut env)
        .expect("approve builtin runs");
    match approved {
        Value::Record(m) => assert_eq!(m.get("approved"), Some(&Value::Bool(true))),
        other => panic!("approve should return a record, got {other:?}"),
    }

    // Now the guarded rm proceeds (in-process grant honored).
    assert!(
        aethershell::builtins::bi_rm(vec![arg], None).is_ok(),
        "rm should succeed after approve()"
    );
    assert!(!file.exists());

    // audit_verify() via the dispatch table (proves index 1105 → bi_audit_verify).
    let verified = aethershell::builtins::call("audit_verify", vec![], &mut env)
        .expect("audit_verify builtin runs");
    match verified {
        Value::Record(m) => assert_eq!(m.get("valid"), Some(&Value::Bool(true))),
        other => panic!("audit_verify should return a record, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn try_catch_binds_structured_safety_error() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("trycatch");

    // A guarded destructive builtin reached through the evaluator: in agent mode
    // it refuses, and try/catch should bind `e` to a structured Record, not a
    // string — so an agent can branch on `e.error.code`.
    let src = r#"try { db_sqlite_delete("x.db", "t", "1=1") } catch e { e }"#;
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::eval::eval_program(&stmts, &mut env).expect("eval");

    match result {
        Value::Record(m) => match m.get("error") {
            Some(Value::Record(e)) => assert_eq!(
                e.get("code"),
                Some(&Value::Str("E_NEEDS_APPROVAL".to_string())),
                "caught error should carry the stable code"
            ),
            other => panic!("error should be a nested record, got {other:?}"),
        },
        other => panic!("catch should bind a structured record, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn in_shell_rbac_principal_and_grant_bypass_approval() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("rbac_shell");
    let file = dir.join("v.txt");
    std::fs::write(&file, b"x").unwrap();
    let arg = Value::Str(file.to_string_lossy().to_string());
    let mut env = aethershell::env::Env::new();

    // Configure RBAC entirely through builtins (dispatch indices 1106-1108).
    aethershell::builtins::call("rbac_principal", vec![Value::Str("alice".into())], &mut env)
        .expect("set principal");
    // Before any grant, alice cannot perform destructive ops.
    let can_before = aethershell::builtins::call(
        "rbac_can",
        vec![Value::Str("effect:destructive".into())],
        &mut env,
    )
    .expect("rbac_can");
    assert_eq!(can_before, Value::Bool(false));

    aethershell::builtins::call(
        "rbac_grant",
        vec![
            Value::Str("alice".into()),
            Value::Str("effect:destructive".into()),
        ],
        &mut env,
    )
    .expect("grant");
    let can_after = aethershell::builtins::call(
        "rbac_can",
        vec![Value::Str("effect:destructive".into())],
        &mut env,
    )
    .expect("rbac_can");
    assert_eq!(can_after, Value::Bool(true), "grant should take effect");

    // The guarded destructive op now proceeds for alice without approval.
    assert!(
        aethershell::builtins::bi_rm(vec![arg], None).is_ok(),
        "authorized principal bypasses approval"
    );
    assert!(!file.exists());

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn safety_status_reports_the_operating_envelope() {
    let _l = lock();
    clear();
    let mut env = aethershell::env::Env::new();

    // Human mode: everything allowed.
    let st = aethershell::builtins::call("safety_status", vec![], &mut env).unwrap();
    match st {
        Value::Record(m) => {
            assert_eq!(m.get("mode"), Some(&Value::Str("human".into())));
            assert_eq!(m.get("transaction_active"), Some(&Value::Bool(false)));
            if let Some(Value::Record(p)) = m.get("policy") {
                assert_eq!(p.get("destructive"), Some(&Value::Str("allow".into())));
            } else {
                panic!("policy record missing");
            }
        }
        other => panic!("expected record, got {other:?}"),
    }

    // Agent mode: dangerous classes are gated.
    std::env::set_var("AETHER_MODE", "agent");
    let st = aethershell::builtins::call("safety_status", vec![], &mut env).unwrap();
    match st {
        Value::Record(m) => {
            assert_eq!(m.get("mode"), Some(&Value::Str("agent".into())));
            if let Some(Value::Record(p)) = m.get("policy") {
                assert_eq!(p.get("destructive"), Some(&Value::Str("approve".into())));
                assert_eq!(p.get("exec"), Some(&Value::Str("approve".into())));
                assert_eq!(p.get("privileged"), Some(&Value::Str("deny".into())));
                assert_eq!(p.get("read_local"), Some(&Value::Str("allow".into())));
            } else {
                panic!("policy record missing");
            }
        }
        other => panic!("expected record, got {other:?}"),
    }
    clear();
}

#[test]
fn remote_and_platform_deletes_are_gated_in_agent_mode() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("remote_del");
    let mut env = aethershell::env::Env::new();

    // platform_db_delete and k8s_delete are gated before they touch the db /
    // invoke kubectl — the guard short-circuits in agent mode.
    let pdb = aethershell::builtins::call(
        "platform_db_delete",
        vec![Value::Str("some_key".into())],
        &mut env,
    )
    .unwrap_err();
    assert!(format!("{pdb}").contains("E_NEEDS_APPROVAL"), "got: {pdb}");

    let k8s = aethershell::builtins::call(
        "k8s_delete",
        vec![Value::Str("pod".into()), Value::Str("nginx".into())],
        &mut env,
    )
    .unwrap_err();
    assert!(format!("{k8s}").contains("E_NEEDS_APPROVAL"), "got: {k8s}");

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn audit_tail_returns_recent_decisions() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    let dir = fresh_workspace("tail");

    // A guarded rm without approval writes a `needs_approval` audit entry.
    let target = dir.join("ae_tail_target.txt").to_string_lossy().to_string();
    let _ = aethershell::builtins::bi_rm(vec![Value::Str(target)], None);

    let mut env = aethershell::env::Env::new();
    let tail = aethershell::builtins::call("audit_tail", vec![Value::Int(10)], &mut env).unwrap();
    match tail {
        Value::Array(entries) => {
            assert!(!entries.is_empty(), "audit tail has recent entries");
            let has_rm = entries.iter().any(|e| {
                matches!(e, Value::Record(m) if m.get("builtin") == Some(&Value::Str("rm".into())))
            });
            assert!(has_rm, "the rm decision appears in the audit tail");
        }
        other => panic!("expected array, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn audit_log_is_written_and_verifies_in_agent_mode() {
    let _l = lock();
    clear();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_APPROVE_ALL", "1");
    let dir = fresh_workspace("audit");
    let log = dir.join("audit.log");
    let file = dir.join("a.txt");
    std::fs::write(&file, b"x").unwrap();

    aethershell::builtins::bi_rm(vec![Value::Str(file.to_string_lossy().to_string())], None)
        .expect("approve-all permits rm");

    assert!(log.exists(), "audit log should be written in agent mode");
    let n = safety::verify_audit(&log).expect("audit chain verifies");
    assert!(n >= 1, "at least one audited entry");

    let _ = std::fs::remove_dir_all(&dir);
    clear();
}

#[test]
fn path_jail_applies_only_in_agent_mode() {
    // A path outside the project directory: a normal interactive (human) shell may
    // read it; an agent-mode/sandboxed shell must not.
    let _l = lock();
    clear();
    let outside = std::env::temp_dir();
    let outside = outside.to_str().expect("temp dir is valid UTF-8");

    // Human mode (no AETHER_MODE / AETHER_WORKSPACE): unrestricted, like bash/zsh.
    assert!(
        aethershell::security::validate_read_path(outside).is_ok(),
        "human mode must NOT sandbox to the project directory"
    );

    // Agent mode with a workspace: the same outside path is blocked by the jail.
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var(
        "AETHER_WORKSPACE",
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    assert!(
        aethershell::security::validate_read_path(outside).is_err(),
        "agent mode must jail reads to the workspace"
    );

    clear();
}
