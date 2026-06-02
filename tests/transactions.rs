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

#[test]
fn relative_paths_are_workspace_relative_when_jailed() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_rel_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w); // jail active, workspace != CWD

    // A unique relative name so we can prove it does NOT land in the process CWD.
    let rel = format!("ae_rel_{}.txt", std::process::id());
    let _ = std::fs::remove_file(&rel);

    // Relative write must resolve under the workspace, not CWD.
    call("file_write", vec![s(&rel), s("hello")]);
    assert_eq!(
        std::fs::read_to_string(w.join(&rel)).unwrap(),
        "hello",
        "relative write must land in the workspace"
    );
    assert!(
        !std::path::Path::new(&rel).exists(),
        "relative write must NOT land in the process CWD"
    );

    // And it is transactional through the relative name.
    call("tx_begin", vec![]);
    call("file_write", vec![s(&rel), s("CHANGED")]);
    assert_eq!(std::fs::read_to_string(w.join(&rel)).unwrap(), "CHANGED");
    call("tx_rollback", vec![]);
    assert_eq!(
        std::fs::read_to_string(w.join(&rel)).unwrap(),
        "hello",
        "rollback restores the workspace-relative file"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
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
    call(
        "file_write",
        vec![s(&existing.to_string_lossy()), s("modified")],
    );
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
fn apply_supports_copy_and_move_atomically() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_plan_copymove_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    // Helper: an op record carrying a `dest` field.
    let op_dest = |op: &str, path: &str, dest: &str| {
        let mut m = std::collections::BTreeMap::new();
        m.insert("op".to_string(), Value::Str(op.to_string()));
        m.insert("path".to_string(), Value::Str(path.to_string()));
        m.insert("dest".to_string(), Value::Str(dest.to_string()));
        Value::Record(m)
    };

    let src = w.join("src.txt");
    let copy_dst = w.join("copy.txt");
    let move_dst = w.join("moved.txt");
    std::fs::write(&src, b"payload").unwrap();

    // copy src→copy.txt, then move src→moved.txt, atomically.
    let ops = Value::Array(vec![
        op_dest("copy", &src.to_string_lossy(), &copy_dst.to_string_lossy()),
        op_dest("move", &src.to_string_lossy(), &move_dst.to_string_lossy()),
    ]);
    assert_eq!(get_str(&call("apply", vec![ops]), "status"), "applied");
    assert_eq!(std::fs::read_to_string(&copy_dst).unwrap(), "payload");
    assert_eq!(std::fs::read_to_string(&move_dst).unwrap(), "payload");
    assert!(!src.exists(), "source removed by move");

    // A destination outside the workspace is rejected by the jail.
    std::fs::write(&src, b"again").unwrap();
    let outside = if cfg!(windows) {
        "C:/Windows/ae_escape.txt"
    } else {
        "/tmp/ae_escape.txt"
    };
    let bad = Value::Array(vec![op_dest("copy", &src.to_string_lossy(), outside)]);
    assert_eq!(get_str(&call("apply", vec![bad]), "status"), "failed");
    assert!(
        !std::path::Path::new(outside).exists(),
        "jailed dest never written"
    );

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
fn rollback_restores_a_deleted_directory_tree() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_tree_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    // A nested tree with content: dir/sub/leaf.txt + dir/top.txt
    let dir = w.join("project");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("top.txt"), b"top").unwrap();
    std::fs::write(dir.join("sub").join("leaf.txt"), b"leaf").unwrap();

    call("tx_begin", vec![]);
    aethershell::builtins::bi_rmdir(vec![s(&dir.to_string_lossy()), Value::Bool(true)], None)
        .unwrap();
    assert!(!dir.exists(), "tree deleted mid-transaction");

    call("tx_rollback", vec![]);

    // The whole tree, including nested file contents, is restored.
    assert_eq!(std::fs::read_to_string(dir.join("top.txt")).unwrap(), "top");
    assert_eq!(
        std::fs::read_to_string(dir.join("sub").join("leaf.txt")).unwrap(),
        "leaf",
        "nested file content restored"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn rollback_undoes_a_file_append() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_app_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let f = w.join("log.txt");
    std::fs::write(&f, b"line1\n").unwrap();

    call("tx_begin", vec![]);
    call("file_append", vec![s(&f.to_string_lossy()), s("line2\n")]);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "line1\nline2\n");

    call("tx_rollback", vec![]);
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        "line1\n",
        "append undone, original content restored"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn rollback_restores_a_sqlite_db_file() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_db_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let db = w.join("data.db");
    std::fs::write(&db, b"ORIGINAL_DB_BYTES").unwrap();

    call("tx_begin", vec![]);
    // db_sqlite_exec snapshots the db file before shelling out to sqlite3, so the
    // snapshot is recorded even when the sqlite3 CLI is unavailable on this host.
    // (We then stand in for the engine's mutation with a direct write — the point
    // under test is that exec captured a restorable snapshot.)
    let mut env = aethershell::env::Env::new();
    let _ = aethershell::builtins::call(
        "db_sqlite_exec",
        vec![s(&db.to_string_lossy()), s("DELETE FROM t WHERE 1=1")],
        &mut env,
    );
    std::fs::write(&db, b"MUTATED").unwrap();

    call("tx_rollback", vec![]);
    assert_eq!(
        std::fs::read(&db).unwrap(),
        b"ORIGINAL_DB_BYTES",
        "db file restored to its pre-transaction bytes on rollback"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn rollback_restores_a_key_value_store_mutation() {
    // The kv store is sqlite-backed: db_kv_set routes through db_sqlite_exec, so
    // the snapshot chokepoint there makes kv mutations transactional too. Guards
    // against a future refactor that bypasses exec.
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_kv_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let store = w.join("kv.db");
    std::fs::write(&store, b"KV_ORIGINAL").unwrap();

    call("tx_begin", vec![]);
    // db_kv_set → db_sqlite_exec → snapshot, taken before any sqlite3 shell-out
    // (so the test does not depend on the sqlite3 CLI being installed).
    let mut env = aethershell::env::Env::new();
    let _ = aethershell::builtins::call(
        "db_kv_set",
        vec![s(&store.to_string_lossy()), s("k"), s("v")],
        &mut env,
    );
    // Stand in for the engine writing the new value after the snapshot.
    std::fs::write(&store, b"KV_MUTATED").unwrap();

    call("tx_rollback", vec![]);
    assert_eq!(
        std::fs::read(&store).unwrap(),
        b"KV_ORIGINAL",
        "kv store file restored to its pre-transaction bytes"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn savepoint_enables_partial_rollback_then_commit() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_tx();
    let w = std::env::temp_dir().join(format!("ae_tx_sp_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let before = w.join("before.txt"); // written before the savepoint → kept
    let after = w.join("after.txt"); // written after the savepoint → reverted

    call("tx_begin", vec![]);
    call(
        "file_write",
        vec![s(&before.to_string_lossy()), s("keep-me")],
    );
    call("tx_savepoint", vec![s("sp1")]);
    call(
        "file_write",
        vec![s(&after.to_string_lossy()), s("discard-me")],
    );

    // Both live before the partial rollback.
    assert!(before.exists() && after.exists());

    match call("tx_rollback_to", vec![s("sp1")]) {
        Value::Record(m) => assert_eq!(m.get("restored"), Some(&Value::Int(1))),
        other => panic!("expected record, got {other:?}"),
    }

    // Post-savepoint creation is gone; pre-savepoint write survives; tx still open.
    assert!(!after.exists(), "post-savepoint op reverted");
    assert_eq!(std::fs::read_to_string(&before).unwrap(), "keep-me");
    match call("tx_status", vec![]) {
        Value::Record(m) => assert_eq!(
            m.get("active"),
            Some(&Value::Bool(true)),
            "transaction stays open after partial rollback"
        ),
        other => panic!("expected record, got {other:?}"),
    }

    // Commit keeps the surviving change.
    call("tx_commit", vec![]);
    assert_eq!(std::fs::read_to_string(&before).unwrap(), "keep-me");
    assert!(!after.exists());

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
