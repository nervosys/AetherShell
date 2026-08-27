//! The audited party may not edit the evidence.
//!
//! `audit_path()` defaults to `<workspace>/.ae/audit.log` in agent mode — inside
//! the workspace jail, which is the one region a jailed builtin may write. So an
//! ordinary `file.write` to it was allowed, and because the hash chain is
//! **unkeyed** (`sha256_hex` takes only the entry text), the log could be
//! rewritten end to end with a fresh, internally consistent chain that
//! `audit_verify()` accepts. Tamper-evidence that anyone can recompute is
//! evidence against corruption, not against an author.
//!
//! `safety::is_audit_artifact` now refuses any guarded filesystem write whose
//! target is the log or its directory, compared lexically so that a write to a
//! path which is about to *become* the log is caught too.
//!
//! **What this does not close, stated so nobody reads more into it.** It stops a
//! *jailed filesystem* builtin. An approved `Exec` call can still reach the file,
//! and the chain remains unkeyed. Keying it needs key management — where the key
//! lives, who can read it, what happens on rotation — which is a design decision
//! rather than a patch, and is deliberately left open. AS-2026-02 stays partially
//! open for that reason.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::safety;
use aethershell::value::Value;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Ws {
    root: std::path::PathBuf,
}

impl Ws {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ae_audit_ws_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(root.join(".ae")).expect("workspace");
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", &root);
        std::env::remove_var("AETHER_AUDIT_LOG");
        Self { root }
    }
}

impl Drop for Ws {
    fn drop(&mut self) {
        std::env::remove_var("AETHER_WORKSPACE");
        std::env::remove_var("AETHER_MODE");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

#[test]
fn a_guarded_write_cannot_truncate_the_audit_log() {
    let _g = lock();
    let ws = Ws::new("trunc");
    let log = ws.root.join(".ae").join("audit.log");
    std::fs::write(&log, "seed\n").expect("seed the log");

    let r = call(
        "file_write",
        vec![
            Value::Str(log.to_string_lossy().into_owned()),
            Value::Str(String::new()),
        ],
    );

    let e = r.expect_err("writing the audit log must be refused");
    let text = format!("{e:#}").to_ascii_lowercase();
    assert!(
        text.contains("audit"),
        "the refusal should say what it is protecting: {text}"
    );
    // The log *grows*, because refusing the tamper is itself an audited event —
    // which is the behaviour you want. What must survive is the existing
    // content: erasing it is what the attacker wanted.
    let after = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        after.starts_with("seed\n"),
        "the existing entries must survive, got {after:?}"
    );
    assert!(
        after.contains("deny_audit_tamper"),
        "and the attempt must be recorded, got {after:?}"
    );
}

#[test]
fn the_directory_is_protected_too_so_the_log_cannot_be_removed_around() {
    // Deleting or replacing the containing directory is the same attack one
    // level up, so `is_audit_artifact` matches the parent as well.
    let _g = lock();
    let ws = Ws::new("dir");
    let inside = ws.root.join(".ae").join("anything.txt");

    let r = call(
        "file_write",
        vec![
            Value::Str(inside.to_string_lossy().into_owned()),
            Value::Str("x".into()),
        ],
    );
    assert!(
        r.is_err(),
        "a write anywhere inside the audit directory must be refused: {r:?}"
    );
}

#[test]
fn a_path_that_is_about_to_become_the_log_is_also_refused() {
    // The comparison is lexical rather than by inode, because the target need
    // not exist yet — creating the file first and editing it later is the same
    // attack with an extra step.
    let _g = lock();
    let ws = Ws::new("future");
    let log = ws.root.join(".ae").join("audit.log");
    let _ = std::fs::remove_file(&log);
    assert!(!log.exists());

    let r = call(
        "file_write",
        vec![
            Value::Str(log.to_string_lossy().into_owned()),
            Value::Str("forged\n".into()),
        ],
    );
    assert!(
        r.is_err(),
        "must be refused even before the log exists: {r:?}"
    );
    // The log may now exist — the refusal is audited, and that entry has to go
    // somewhere. What must not be there is the caller's content.
    let after = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !after.contains("forged"),
        "the refused content must not have landed, got {after:?}"
    );
}

#[test]
fn ordinary_workspace_writes_are_unaffected() {
    // The check that keeps this from being a ban on the workspace.
    let _g = lock();
    let ws = Ws::new("ok");
    let f = ws.root.join("notes.txt");

    let r = call(
        "file_write",
        vec![
            Value::Str(f.to_string_lossy().into_owned()),
            Value::Str("hello".into()),
        ],
    );
    assert!(r.is_ok(), "a normal workspace write must still work: {r:?}");
    assert_eq!(std::fs::read_to_string(&f).unwrap_or_default(), "hello");
}

#[test]
fn the_predicate_is_exact_about_what_it_claims() {
    // A check on the check. `is_audit_artifact` compares against the *active*
    // audit path, so with auditing off it must match nothing — otherwise it
    // would refuse arbitrary `.ae` paths in human mode, where there is no log.
    let _g = lock();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_WORKSPACE");
    std::env::remove_var("AETHER_AUDIT_LOG");
    assert!(
        !safety::is_audit_artifact("/anywhere/.ae/audit.log"),
        "with no active audit log there is nothing to protect"
    );

    let tmp = std::env::temp_dir().join(format!("ae_pred_{}.log", std::process::id()));
    std::env::set_var("AETHER_AUDIT_LOG", &tmp);
    let p = tmp.to_string_lossy().into_owned();
    assert!(safety::is_audit_artifact(&p), "the log itself");
    assert!(
        safety::is_audit_artifact(&p.to_ascii_uppercase()) || cfg!(not(windows)),
        "case-insensitive on Windows, where paths are"
    );
    assert!(
        !safety::is_audit_artifact(&format!("{p}.other")),
        "a sibling with a longer name is not the log"
    );
    std::env::remove_var("AETHER_AUDIT_LOG");
}
