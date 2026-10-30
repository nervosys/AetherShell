//! `apply` checks every op's paths before the first op runs. A plan whose later
//! op is refused by the workspace jail must leave the disk untouched, open no
//! transaction, and need no rollback.

use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

// The transaction journal and IRONSHELL_WORKSPACE are process-global; serialize.
static LOCK: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

fn op(kind: &str, path: &str, content: Option<&str>, dest: Option<&str>) -> Value {
    let mut m = BTreeMap::new();
    m.insert("op".to_string(), s(kind));
    m.insert("path".to_string(), s(path));
    if let Some(c) = content {
        m.insert("content".to_string(), s(c));
    }
    if let Some(d) = dest {
        m.insert("dest".to_string(), s(d));
    }
    Value::Record(m)
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(m) => m.get(key).unwrap_or_else(|| panic!("missing field {key}")),
        other => panic!("not a record: {other:?}"),
    }
}

fn get_str(v: &Value, key: &str) -> String {
    match field(v, key) {
        Value::Str(s) => s.clone(),
        other => panic!("field {key} not a string: {other:?}"),
    }
}

/// Clear any transaction left active by an earlier failure, and reset the
/// mode so a leaked IRONSHELL_MODE cannot change how `apply` runs.
fn reset_tx() {
    std::env::remove_var("IRONSHELL_MODE");
    for _ in 0..16 {
        let mut env = iron_shell::env::Env::new();
        if iron_shell::builtins::call("tx_rollback", vec![], &mut env).is_err() {
            break;
        }
    }
}

/// A fresh workspace and a sibling directory outside it, plus a private session
/// directory so the journal never touches the real one.
fn fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ae_apply_pf_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let ws = base.join("ws");
    let outside = base.join("outside");
    let session = base.join("session");
    for d in [&ws, &outside, &session] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::env::set_var("IRONSHELL_WORKSPACE", &ws);
    std::env::set_var("IRONSHELL_SESSION_DIR", &session);
    (base, ws, outside)
}

fn teardown(base: &PathBuf) {
    std::env::remove_var("IRONSHELL_WORKSPACE");
    std::env::remove_var("IRONSHELL_SESSION_DIR");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn refused_later_write_leaves_earlier_write_undone_without_rollback() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let (base, ws, outside) = fixture("write");

    let fresh = ws.join("fresh.txt");
    let escape = outside.join("escape.txt");
    let plan = Value::Array(vec![
        op("write", &fresh.to_string_lossy(), Some("first"), None),
        op("write", &escape.to_string_lossy(), Some("second"), None),
    ]);

    let r = call("apply", vec![plan]);
    assert_eq!(get_str(&r, "status"), "failed");
    assert!(
        get_str(&r, "error").contains("outside the workspace"),
        "refusal names the jail: {r:?}"
    );
    assert_eq!(field(&r, "preflight"), &Value::Bool(true));
    assert_eq!(field(&r, "rolled_back"), &Value::Bool(false));

    // The first op never ran: no file, and no transaction was ever opened.
    assert!(
        !fresh.exists(),
        "first op must not run when a later op is refused"
    );
    assert!(!escape.exists());
    let status = call("tx_status", vec![]);
    assert_eq!(field(&status, "active"), &Value::Bool(false));

    teardown(&base);
}

#[test]
fn refused_copy_destination_outside_workspace_runs_nothing() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let (base, ws, outside) = fixture("copy");

    let src = ws.join("src.txt");
    std::fs::write(&src, b"keep").unwrap();
    let made = ws.join("made.txt");
    let escape = outside.join("copied.txt");
    let plan = Value::Array(vec![
        op("write", &made.to_string_lossy(), Some("new"), None),
        op(
            "copy",
            &src.to_string_lossy(),
            None,
            Some(&escape.to_string_lossy()),
        ),
    ]);

    let r = call("apply", vec![plan]);
    assert_eq!(get_str(&r, "status"), "failed");
    assert!(get_str(&r, "error").contains("destination"), "{r:?}");
    assert!(!made.exists(), "earlier write must not run");
    assert!(!escape.exists(), "refused copy must not run");
    assert_eq!(std::fs::read(&src).unwrap(), b"keep");

    teardown(&base);
}
