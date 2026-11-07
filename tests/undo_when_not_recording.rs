//! `undo()` in a human session. The journal records a write only in agent mode
//! or with IRONSHELL_JOURNAL=on, so in a human session there is nothing to put
//! back. The undo used to answer `complete: true` with nothing restored, and
//! the file kept the write. It is now refused, and the refusal names the switch.
//!
//! These tests use real files in a temp directory and set the environment for
//! each case. The environment is process-global, so every case takes the lock.

use iron_shell::value::Value;
use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

const VARS: [&str; 4] = [
    "IRONSHELL_MODE",
    "IRONSHELL_AGENT",
    "IRONSHELL_JOURNAL",
    "IRONSHELL_WORKSPACE",
];

/// A temp directory with its own session state, and a clean mode, for one case.
struct Case {
    dir: PathBuf,
}

impl Case {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ae_undo_not_recording_{tag}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("state")).expect("temp dir");
        for v in VARS {
            std::env::remove_var(v);
        }
        std::env::set_var("IRONSHELL_SESSION_DIR", dir.join("state"));
        Self { dir }
    }
    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        for v in VARS {
            std::env::remove_var(v);
        }
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

/// The first `E_...` code in an error's rendered text.
fn code_of(e: &anyhow::Error) -> String {
    let msg = format!("{e:#}");
    let at = msg
        .find("E_")
        .unwrap_or_else(|| panic!("no code in: {msg}"));
    msg[at..]
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || *c == '_')
        .collect()
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn undo_in_a_human_session_is_refused_and_the_write_stays() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let c = Case::new("human");
    let p = c.path("n2.txt");
    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("x".into())],
    )
    .expect("the write itself succeeds");

    let err = call("undo", vec![]).expect_err("undo must not answer complete:true");
    assert_eq!(code_of(&err), "E_BAD_STATE");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not recording"),
        "message names the fact: {msg}"
    );
    assert!(
        msg.contains("--agent") && msg.contains("IRONSHELL_JOURNAL=on"),
        "hint names the switches: {msg}"
    );
    assert_eq!(
        std::fs::read_to_string(&p).expect("the file is still there"),
        "x",
        "a refused undo must not touch the file"
    );
}

#[test]
fn redo_in_a_human_session_is_refused_too() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _c = Case::new("redo");
    for name in ["session_undo", "session_redo"] {
        let err = call(name, vec![]).expect_err("a human-session journal call is refused");
        assert_eq!(code_of(&err), "E_BAD_STATE", "{name}");
    }
}

#[test]
fn undo_still_restores_when_the_journal_is_on() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let c = Case::new("on");
    std::env::set_var("IRONSHELL_JOURNAL", "on");
    let p = c.path("n2.txt");
    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("x".into())],
    )
    .expect("write");
    let r = call("undo", vec![]).expect("undo runs when recording");
    assert_eq!(field(&r, "complete"), Value::Bool(true));
    assert_eq!(field(&r, "removed"), Value::Int(1));
    assert!(
        !std::path::Path::new(&p).exists(),
        "the created file is removed"
    );
}

#[test]
fn undo_in_agent_mode_works_without_the_journal_variable() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let c = Case::new("agent");
    std::env::set_var("IRONSHELL_MODE", "agent");
    std::env::set_var("IRONSHELL_WORKSPACE", &c.dir);
    let p = c.path("n2.txt");
    call(
        "file_write",
        vec![Value::Str(p.clone()), Value::Str("x".into())],
    )
    .expect("write");
    let r = call("undo", vec![]).expect("undo runs in agent mode");
    assert_eq!(field(&r, "complete"), Value::Bool(true));
    assert!(!std::path::Path::new(&p).exists());
}
