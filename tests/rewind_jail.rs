//! `undo` and `redo` honour the workspace jail.
//!
//! The journal persists across processes, keyed by working directory, so a
//! step recorded in human mode (nothing jailed) can be replayed by a later
//! agent-mode session in the same directory. Rewinding rewrites or deletes the
//! recorded file, and undo carries no path argument for the central jail check
//! to see, so agent-mode undo used to rewrite a file outside the workspace.
//!
//! Every test runs against its own temporary `IRONSHELL_SESSION_DIR` and
//! directories, under one lock, because the journal and the mode variables are
//! process-global.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::journal;
use iron_shell::safety::{effect_of, Effect};
use iron_shell::value::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

struct Scene {
    root: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Scene {
    fn new(tag: &str) -> Self {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root =
            std::env::temp_dir().join(format!("ae_rewind_jail_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["state", "workspace", "outside"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::env::set_var("IRONSHELL_JOURNAL", "on");
        std::env::set_var("IRONSHELL_SESSION_DIR", root.join("state"));
        human();
        journal::clear();
        Self { root, _lock: lock }
    }

    fn file(&self, dir: &str, name: &str) -> String {
        self.root
            .join(dir)
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn agent(&self) {
        std::env::set_var("IRONSHELL_MODE", "agent");
        std::env::set_var("IRONSHELL_WORKSPACE", self.root.join("workspace"));
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        journal::clear();
        human();
        std::env::remove_var("IRONSHELL_JOURNAL");
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn human() {
    for v in [
        "IRONSHELL_MODE",
        "IRONSHELL_AGENT",
        "IRONSHELL_WORKSPACE",
        "IRONSHELL_POLICY",
    ] {
        std::env::remove_var(v);
    }
}

fn call(name: &str, args: Vec<Value>) -> Value {
    builtins::call(name, args, &mut Env::new()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn steps() -> Value {
    match call("journal", vec![]) {
        Value::Record(m) => m.get("steps").cloned().unwrap_or(Value::Null),
        other => panic!("journal() answered {other:?}"),
    }
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn write_step(path: &str, text: &str) {
    call(
        "file_write",
        vec![Value::Str(path.to_string()), Value::Str(text.to_string())],
    );
}

#[test]
fn agent_mode_undo_leaves_a_file_outside_the_workspace_alone() {
    let s = Scene::new("undo_outside");
    let target = s.file("outside", "notes.txt");
    std::fs::write(&target, "before").unwrap();
    write_step(&target, "after");
    assert_eq!(read(&target), "after");
    assert_eq!(steps(), Value::Int(1));

    s.agent();
    for name in ["undo", "rewind", "session_undo"] {
        let _ = call(name, vec![Value::Int(1)]);
        assert_eq!(
            read(&target),
            "after",
            "{name} rewrote a file outside the jail"
        );
        assert_eq!(
            steps(),
            Value::Int(1),
            "{name} dropped the step it left alone"
        );
    }

    // Outside agent mode the same step is undone, so nothing was lost.
    human();
    let _ = call("undo", vec![Value::Int(1)]);
    assert_eq!(read(&target), "before");
}

#[test]
fn agent_mode_undo_still_works_inside_the_workspace() {
    let s = Scene::new("undo_inside");
    s.agent();
    let target = s.file("workspace", "draft.txt");
    std::fs::write(&target, "before").unwrap();
    write_step(&target, "after");
    assert_eq!(read(&target), "after");
    let _ = call("undo", vec![Value::Int(1)]);
    assert_eq!(
        read(&target),
        "before",
        "undo inside the workspace must still restore"
    );

    // And redo puts it back.
    let _ = call("session_redo", vec![Value::Int(1)]);
    assert_eq!(read(&target), "after");
}

#[test]
fn agent_mode_redo_leaves_a_file_outside_the_workspace_alone() {
    let s = Scene::new("redo_outside");
    let target = s.file("outside", "notes.txt");
    std::fs::write(&target, "before").unwrap();
    write_step(&target, "after");
    let _ = call("undo", vec![Value::Int(1)]);
    assert_eq!(read(&target), "before");

    s.agent();
    let _ = call("session_redo", vec![Value::Int(1)]);
    assert_eq!(
        read(&target),
        "before",
        "redo rewrote a file outside the jail"
    );

    human();
    let _ = call("session_redo", vec![Value::Int(1)]);
    assert_eq!(read(&target), "after");
}

#[test]
fn rewinding_is_classified_as_writing() {
    for name in [
        "undo",
        "rewind",
        "session_undo",
        "session_redo",
        "journal_clear",
        "session_end",
    ] {
        assert_eq!(effect_of(name), Effect::WriteLocal, "{name}");
    }
}
