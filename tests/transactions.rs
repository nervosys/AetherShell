//! Tests for filesystem transactions / checkpoints (docs/AGENTIC_FIRST_DESIGN.md §9):
//! a file-effecting batch wrapped in tx_begin can be rolled back to its prior
//! state, or committed to keep the changes.

use aethershell::value::Value;
use std::sync::Mutex;

// The transaction journal and AETHER_WORKSPACE are process-global; serialize.
static LOCK: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

/// Best-effort clear of any leftover active transaction from a prior failure,
/// and reset mode so a leaked AETHER_MODE can't gate other tests.
fn reset_tx() {
    std::env::remove_var("AETHER_MODE");
    let mut env = aethershell::env::Env::new();
    let _ = aethershell::builtins::call("tx_rollback", vec![], &mut env);
}

fn rec_op(op: &str, path: &str, content: Option<&str>) -> Value {
    let mut m = std::collections::BTreeMap::new();
    m.insert("op".to_string(), Value::Str(op.to_string()));
    m.insert("path".to_string(), Value::Str(path.to_string()));
    if let Some(c) = content {
        m.insert("content".to_string(), Value::Str(c.to_string()));
    }
    Value::Record(m)
}

fn get_str(v: &Value, key: &str) -> String {
    match v {
        Value::Record(m) => match m.get(key) {
            Some(Value::Str(s)) => s.clone(),
            other => panic!("field {key} not a string: {other:?}"),
        },
        other => panic!("not a record: {other:?}"),
    }
}

#[test]
fn rollback_restores_prior_state() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_rb_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let existing = w.join("existing.txt");
    let deleteme = w.join("deleteme.txt");
    let newf = w.join("new.txt");
    std::fs::write(&existing, b"original").unwrap();
    std::fs::write(&deleteme, b"keep").unwrap();

    call("tx_begin", vec![]);
    // overwrite an existing file, delete a file, create a new file
    call("file_write", vec![s(&existing.to_string_lossy()), s("modified")]);
    aethershell::builtins::bi_rm(vec![s(&deleteme.to_string_lossy())], None).unwrap();
    call("file_write", vec![s(&newf.to_string_lossy()), s("created")]);

    // Mid-transaction the changes are live.
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "modified");
    assert!(!deleteme.exists());
    assert_eq!(std::fs::read_to_string(&newf).unwrap(), "created");

    match call("tx_rollback", vec![]) {
        Value::Record(m) => assert_eq!(m.get("rolled_back"), Some(&Value::Bool(true))),
        other => panic!("expected record, got {other:?}"),
    }

    // Everything restored to the pre-transaction state.
    assert_eq!(
        std::fs::read_to_string(&existing).unwrap(),
        "original",
        "overwrite undone"
    );
    assert_eq!(
        std::fs::read_to_string(&deleteme).unwrap(),
        "keep",
        "deletion undone"
    );
    assert!(!newf.exists(), "created file removed on rollback");

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn plan_apply_requires_approval_then_applies_atomically() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_plan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);
    std::env::set_var("AETHER_MODE", "agent");

    let f1 = w.join("a.txt");
    let f2 = w.join("b.txt");
    let ops = Value::Array(vec![
        rec_op("write", &f1.to_string_lossy(), Some("hello")),
        rec_op("write", &f2.to_string_lossy(), Some("world")),
    ]);

    // plan() yields a typed summary + a bound token, executing nothing.
    let plan = call("plan", vec![ops.clone()]);
    let token = get_str(&plan, "token");
    assert!(token.starts_with("apl_"));
    assert!(!f1.exists(), "plan must not execute");

    // apply() without approval in agent mode → needs_approval, still nothing done.
    let na = call("apply", vec![ops.clone()]);
    assert_eq!(get_str(&na, "status"), "needs_approval");
    assert!(!f1.exists() && !f2.exists());

    // Approve the plan token, then apply → atomic success, both files written.
    call("approve", vec![Value::Str(token)]);
    let ap = call("apply", vec![ops]);
    assert_eq!(get_str(&ap, "status"), "applied");
    assert_eq!(std::fs::read_to_string(&f1).unwrap(), "hello");
    assert_eq!(std::fs::read_to_string(&f2).unwrap(), "world");

    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn apply_supports_append_and_is_atomic() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_plan_append_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);
    // Human mode → no approval needed.

    let f = w.join("log.txt");
    // write then append in one atomic batch.
    let ops = Value::Array(vec![
        rec_op("write", &f.to_string_lossy(), Some("base")),
        rec_op("append", &f.to_string_lossy(), Some("+more")),
    ]);
    assert_eq!(get_str(&call("apply", vec![ops]), "status"), "applied");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "base+more");

    // A failing batch (append then rm-nonexistent) rolls back the append too.
    let g = w.join("g.txt");
    let bad = Value::Array(vec![
        rec_op("append", &g.to_string_lossy(), Some("data")),
        rec_op("rm", &w.join("nope.txt").to_string_lossy(), None),
    ]);
    assert_eq!(get_str(&call("apply", vec![bad]), "status"), "failed");
    assert!(!g.exists(), "append-created file removed on rollback");

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn apply_rolls_back_the_whole_batch_on_failure() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_plan_fail_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);
    // Human mode → no approval needed; still atomic.

    let good = w.join("good.txt");
    // Second op deletes a nonexistent file → fails → whole batch rolls back.
    let ops = Value::Array(vec![
        rec_op("write", &good.to_string_lossy(), Some("data")),
        rec_op("rm", &w.join("does_not_exist.txt").to_string_lossy(), None),
    ]);

    let res = call("apply", vec![ops]);
    assert_eq!(get_str(&res, "status"), "failed");
    assert!(
        !good.exists(),
        "the successful write must be rolled back when a later op fails"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn commit_keeps_changes_and_clears_state() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_ci_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let f = w.join("kept.txt");
    call("tx_begin", vec![]);
    call("file_write", vec![s(&f.to_string_lossy()), s("data")]);

    match call("tx_status", vec![]) {
        Value::Record(m) => assert_eq!(m.get("active"), Some(&Value::Bool(true))),
        other => panic!("expected record, got {other:?}"),
    }

    call("tx_commit", vec![]);

    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        "data",
        "committed change is kept"
    );
    match call("tx_status", vec![]) {
        Value::Record(m) => assert_eq!(m.get("active"), Some(&Value::Bool(false))),
        other => panic!("expected record, got {other:?}"),
    }

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}
