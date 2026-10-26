//! A rollback undoes file_patch, file_insert, file_move and save_json, and
//! an archive extraction, which it could not undo, is refused inside a
//! transaction. These run in their own process because a transaction is
//! process-wide state: in the library's unit tests, an open one would have
//! journaled, and rolled back, the writes of tests running beside it.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env)
}
/// Transactions are process-wide state, so these tests take one at a time.
static TX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Ends whatever transaction a failing assertion left open, so a panic in
/// one test cannot leak into the next. A clean run has none to end.
struct TxCleanup;

impl Drop for TxCleanup {
    fn drop(&mut self) {
        while aethershell::tx::rollback().is_ok() {}
    }
}

fn scratch_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ae-txc-{}-{name}", std::process::id()))
}

fn text(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn path_arg(p: &std::path::Path) -> Value {
    Value::Str(p.to_string_lossy().into_owned())
}

#[test]
fn file_patch_is_undone_by_rollback() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let p = scratch_path("patch.txt");
    std::fs::write(&p, "alpha beta\n").unwrap();

    let patch = Value::Record(BTreeMap::from([
        ("find".to_string(), text("alpha")),
        ("replace".to_string(), text("omega")),
    ]));
    aethershell::tx::begin().unwrap();
    call("file_patch", vec![path_arg(&p), Value::Array(vec![patch])]).unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "omega beta\n");
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "alpha beta\n");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn file_insert_is_undone_by_rollback() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let p = scratch_path("insert.txt");
    std::fs::write(&p, "alpha\nbeta\n").unwrap();

    aethershell::tx::begin().unwrap();
    call(
        "file_insert",
        vec![path_arg(&p), Value::Int(1), text("new")],
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "new\nalpha\nbeta\n");
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "alpha\nbeta\n");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn file_move_restores_source_and_removes_new_destination() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let src = scratch_path("move-src.txt");
    let dst = scratch_path("move-dst.txt");
    std::fs::write(&src, "moving\n").unwrap();
    let _ = std::fs::remove_file(&dst);

    aethershell::tx::begin().unwrap();
    call("file_move", vec![path_arg(&src), path_arg(&dst)]).unwrap();
    assert!(!src.exists());
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "moving\n");
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "moving\n");
    assert!(
        !dst.exists(),
        "rollback must remove the created destination"
    );
    let _ = std::fs::remove_file(&src);
}

#[test]
fn file_move_restores_a_destination_it_replaced() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let src = scratch_path("replace-src.txt");
    let dst = scratch_path("replace-dst.txt");
    std::fs::write(&src, "new\n").unwrap();
    std::fs::write(&dst, "old\n").unwrap();

    aethershell::tx::begin().unwrap();
    call("file_move", vec![path_arg(&src), path_arg(&dst)]).unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "new\n");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "old\n");
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
}

#[test]
fn save_json_is_undone_by_rollback() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let existing = scratch_path("save-existing.json");
    let created = scratch_path("save-created.json");
    std::fs::write(&existing, "\"before\"").unwrap();
    let _ = std::fs::remove_file(&created);

    aethershell::tx::begin().unwrap();
    call("save_json", vec![path_arg(&existing), text("after")]).unwrap();
    call("save_json", vec![path_arg(&created), text("made")]).unwrap();
    assert!(created.exists());
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "\"before\"");
    assert!(!created.exists(), "rollback must remove the created file");
    let _ = std::fs::remove_file(&existing);
}

#[test]
fn zip_extract_is_refused_inside_a_transaction() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let dest = scratch_path("extract-dest");
    aethershell::tx::begin().unwrap();
    let err = call(
        "zip_extract",
        vec![text("no-such-archive.zip"), path_arg(&dest)],
    )
    .expect_err("extraction cannot be rolled back, so it must be refused");
    assert!(
        format!("{err:#}").contains("cannot be rolled back"),
        "{err:#}"
    );
    assert!(!dest.exists(), "a refused extraction must write nothing");
    aethershell::tx::rollback().unwrap();
}
