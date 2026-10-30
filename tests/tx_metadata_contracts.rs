//! Filesystem transactions journal permission, ownership and symlink changes.

use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// The transaction journal and IRONSHELL_WORKSPACE are process-global; serialize.
static LOCK: Mutex<()> = Mutex::new(());

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

fn try_call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

fn call(name: &str, args: Vec<Value>) -> Value {
    try_call(name, args).unwrap_or_else(|e| panic!("{name} failed: {e}"))
}

fn s(p: impl AsRef<Path>) -> Value {
    Value::Str(p.as_ref().to_string_lossy().to_string())
}

fn reset_tx() {
    std::env::remove_var("IRONSHELL_MODE");
    let mut env = iron_shell::env::Env::new();
    for _ in 0..16 {
        if iron_shell::builtins::call("tx_rollback", vec![], &mut env).is_err() {
            break;
        }
    }
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        reset_tx();
        let root = std::env::temp_dir().join(format!("ae_txmeta_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::env::set_var("IRONSHELL_WORKSPACE", &root);
        Fixture { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        reset_tx();
        std::env::remove_var("IRONSHELL_WORKSPACE");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Comparable permission state: exact mode bits on Unix, read-only flag elsewhere.
fn perm_state(p: &Path) -> u32 {
    let m = std::fs::metadata(p).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        m.permissions().readonly() as u32
    }
}

fn make_file(f: &Fixture) -> PathBuf {
    let p = f.root.join("a.txt");
    std::fs::write(&p, "payload").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    p
}

#[test]
fn chmod_rolls_back() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("rb");
    let p = make_file(&f);
    let before = perm_state(&p);
    call("tx_begin", vec![]);
    call("fs_chmod", vec![s(&p), Value::Int(0o444)]);
    assert_ne!(perm_state(&p), before);
    // A second change in the same frame must not overwrite the first pre-image.
    call("fs_chmod", vec![s(&p), Value::Int(0o400)]);
    call("tx_rollback", vec![]);
    assert_eq!(perm_state(&p), before);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "payload");
}

#[test]
fn chmod_commit_keeps_change() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("cm");
    let p = make_file(&f);
    let before = perm_state(&p);
    call("tx_begin", vec![]);
    call("fs_chmod", vec![s(&p), Value::Int(0o444)]);
    call("tx_commit", vec![]);
    assert_ne!(perm_state(&p), before);
    // Restore so the fixture can be deleted on Windows.
    call("fs_chmod", vec![s(&p), Value::Int(0o644)]);
}

#[test]
fn chmod_nested_inner_rollback_restores_pre_inner_mode() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("nest");
    let p = make_file(&f);
    let original = perm_state(&p);
    call("tx_begin", vec![]);
    call("fs_chmod", vec![s(&p), Value::Int(0o444)]);
    let after_outer = perm_state(&p);
    call("tx_begin", vec![]);
    call("fs_chmod", vec![s(&p), Value::Int(0o644)]);
    call("tx_rollback", vec![]);
    assert_eq!(perm_state(&p), after_outer, "inner rollback");
    call("fs_chmod", vec![s(&p), Value::Int(0o600)]);
    call("tx_rollback", vec![]);
    assert_eq!(perm_state(&p), original, "outer rollback");
}

#[test]
fn content_snapshot_does_not_suppress_metadata_snapshot() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("dedup");
    let p = make_file(&f);
    let before = perm_state(&p);
    call("tx_begin", vec![]);
    call("file_write", vec![s(&p), Value::Str("changed".into())]);
    call("fs_chmod", vec![s(&p), Value::Int(0o444)]);
    call("tx_rollback", vec![]);
    assert_eq!(perm_state(&p), before);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "payload");
}

#[test]
fn symlink_to_file_rolls_back_and_keeps_target() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("sl_file");
    let target = f.root.join("t.txt");
    std::fs::write(&target, "keep me").unwrap();
    let link = f.root.join("link_file");
    call("tx_begin", vec![]);
    if let Err(e) = try_call("fs_symlink", vec![s(&target), s(&link)]) {
        let msg = e.to_string();
        // Windows needs a privilege most test hosts lack.
        assert!(
            cfg!(windows) && msg.contains("os error 1314"),
            "unexpected symlink failure: {msg}"
        );
        call("tx_rollback", vec![]);
        return;
    }
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    call("tx_rollback", vec![]);
    assert!(std::fs::symlink_metadata(&link).is_err(), "link removed");
    assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
}

#[cfg(unix)]
#[test]
fn symlink_to_directory_and_dangling_roll_back_without_following() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("sl_dir");
    let dir = f.root.join("d");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("inner.txt"), "inner").unwrap();
    let dlink = f.root.join("link_dir");
    let glink = f.root.join("link_dangling");
    call("tx_begin", vec![]);
    call("fs_symlink", vec![s(&dir), s(&dlink)]);
    call("fs_symlink", vec![s(f.root.join("missing")), s(&glink)]);
    assert!(std::fs::symlink_metadata(&glink).is_ok());
    call("tx_rollback", vec![]);
    assert!(std::fs::symlink_metadata(&dlink).is_err());
    assert!(std::fs::symlink_metadata(&glink).is_err());
    assert_eq!(std::fs::read(dir.join("inner.txt")).unwrap(), b"inner");
}

#[cfg(unix)]
#[test]
fn symlink_commit_keeps_link() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("sl_commit");
    let target = f.root.join("t.txt");
    std::fs::write(&target, "x").unwrap();
    let link = f.root.join("l");
    call("tx_begin", vec![]);
    call("fs_symlink", vec![s(&target), s(&link)]);
    call("tx_commit", vec![]);
    assert!(std::fs::symlink_metadata(&link).is_ok());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn chown_rolls_back_owner_and_group() {
    use std::os::unix::fs::MetadataExt;
    disposable::require();
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new("chown");
    let p = make_file(&f);
    let md = std::fs::metadata(&p).unwrap();
    let (uid, gid, mode) = (md.uid(), md.gid(), perm_state(&p));
    call("tx_begin", vec![]);
    call("fs_chown", vec![s(&p), Value::Int(1), Value::Int(1)]);
    let changed = std::fs::metadata(&p).unwrap();
    assert_eq!((changed.uid(), changed.gid()), (1, 1));
    call("tx_rollback", vec![]);
    let restored = std::fs::metadata(&p).unwrap();
    assert_eq!((restored.uid(), restored.gid()), (uid, gid));
    assert_eq!(perm_state(&p), mode);
}
