//! Reversible sessions — can a mutating step actually be taken back?
//!
//! These tests use real files in a temp directory, because the claim is about
//! the filesystem and a mocked one would prove nothing. They run single-threaded
//! (the journal and `AETHER_MODE` are process-global), which the harness is told
//! via the lock below rather than by hoping.

use aethershell::journal;
use aethershell::value::Value;
use std::path::PathBuf;
use std::sync::Mutex;

/// Serialises these tests against each other: the journal is process-global,
/// as is `AETHER_JOURNAL`.
static LOCK: Mutex<()> = Mutex::new(());

struct Session {
    dir: PathBuf,
}

impl Session {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ae_journal_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::env::set_var("AETHER_JOURNAL", "on");
        journal::clear();
        Self { dir }
    }
    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        journal::clear();
        std::env::remove_var("AETHER_JOURNAL");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn an_overwritten_file_is_restored_byte_for_byte() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("overwrite");
    let p = s.path("notes.txt");
    std::fs::write(&p, "original contents").expect("seed");

    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("clobbered".into())],
    )
    .expect("write should succeed");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "clobbered");

    let result = call("undo", vec![]).expect("undo");
    assert_eq!(field(&result, "restored"), Value::Int(1));
    assert_eq!(field(&result, "complete"), Value::Bool(true));
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "original contents",
        "undo must restore the exact prior bytes"
    );
}

#[test]
fn a_file_created_by_the_agent_is_removed_again() {
    // The other half of restoring: a step that created something must be
    // reversible by deleting it, or "undo" leaves litter behind.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("create");
    let p = s.path("new.txt");
    assert!(!PathBuf::from(&p).exists());

    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("fresh".into())],
    )
    .expect("write");
    assert!(PathBuf::from(&p).exists());

    let result = call("undo", vec![]).expect("undo");
    assert_eq!(field(&result, "removed"), Value::Int(1));
    assert!(
        !PathBuf::from(&p).exists(),
        "a file that did not exist before must not exist after undo"
    );
}

#[test]
fn several_steps_rewind_in_reverse_order() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("multi");
    let p = s.path("v.txt");
    std::fs::write(&p, "v0").expect("seed");

    for v in ["v1", "v2", "v3"] {
        call(
            "file_write",
            vec![Value::Str(p.clone()), Value::Str(v.into())],
        )
        .expect("write");
    }
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v3");

    let result = call("undo", vec![Value::Int(2)]).expect("undo 2");
    assert_eq!(field(&result, "restored"), Value::Int(2));
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v1",
        "rewinding two steps from v3 lands on v1, not v0"
    );
}

#[test]
fn an_irreversible_step_is_reported_and_never_counted_as_restored() {
    // The failure this module exists to prevent: a rewind that restores part of
    // the world and reports success. A directory cannot be captured, so undoing
    // must say so rather than claim completeness.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("irreversible");
    let sub = s.path("subdir");
    std::fs::create_dir_all(&sub).expect("subdir");

    // `rm` is Destructive and its target here is a directory.
    let _ = call("rm", vec![Value::Str(sub.clone())]);

    let j = call("journal", vec![]).expect("journal");
    let irreversible = field(&j, "irreversible");
    assert_eq!(
        irreversible,
        Value::Int(1),
        "a directory target must be journalled as irreversible, got {j:?}"
    );

    let result = call("undo", vec![]).expect("undo");
    assert_eq!(field(&result, "restored"), Value::Int(0));
    assert_eq!(field(&result, "skipped"), Value::Int(1));
    assert_eq!(
        field(&result, "complete"),
        Value::Bool(false),
        "an undo that could not reverse everything must not report completeness"
    );
}

#[test]
fn the_journal_reports_what_it_can_and_cannot_reverse() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("report");
    let p = s.path("a.txt");
    std::fs::write(&p, "x").expect("seed");
    call("file_write", vec![Value::Str(p), Value::Str("y".into())]).expect("write");

    let j = call("journal", vec![]).expect("journal");
    assert_eq!(field(&j, "enabled"), Value::Bool(true));
    assert_eq!(field(&j, "reversible"), Value::Int(1));
    match field(&j, "entries") {
        Value::Array(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(field(&rows[0], "reversible"), Value::Bool(true));
            assert_eq!(field(&rows[0], "builtin"), Value::Str("file_write".into()));
        }
        other => panic!("expected entries array, got {other:?}"),
    }
}

#[test]
fn a_read_only_call_is_never_journalled() {
    // Journalling every call would cost I/O on the common path for nothing.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("readonly");
    let p = s.path("r.txt");
    std::fs::write(&p, "data").expect("seed");

    let _ = call("cat", vec![Value::Str(p)]);
    assert_eq!(
        journal::entries().len(),
        0,
        "a read must not enter the journal"
    );
}

#[test]
fn undo_does_not_journal_itself() {
    // Otherwise a second undo would put the damage back.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("selfjournal");
    let p = s.path("s.txt");
    std::fs::write(&p, "before").expect("seed");
    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("after".into())],
    )
    .expect("write");

    call("undo", vec![]).expect("undo");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "before");
    assert_eq!(
        journal::entries().len(),
        0,
        "undo must consume the entry and add none of its own"
    );

    // A second undo has nothing to do, and must not resurrect anything.
    let again = call("undo", vec![]).expect("second undo");
    assert_eq!(field(&again, "restored"), Value::Int(0));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "before");
}

#[test]
fn journalling_is_off_for_humans_by_default() {
    // The dual-surface split: the human REPL behaves exactly as before.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_JOURNAL");
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    assert!(!journal::enabled(), "humans pay nothing for this");
    std::env::set_var("AETHER_MODE", "agent");
    assert!(journal::enabled(), "agents get recoverability");
    std::env::remove_var("AETHER_MODE");
}
