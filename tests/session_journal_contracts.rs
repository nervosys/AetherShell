//! Reversible session contracts: `session_undo`, `session_redo`,
//! `session_history` and `session_end` over the journal.
//!
//! These use real files in temp directories, because the claim is about the
//! filesystem. Each test gets its own session directory through
//! `AETHER_SESSION_DIR`, so nothing leaks into the developer's own journal. The
//! journal is process-global, so the tests are serialised by the lock below.

use aethershell::journal;
use aethershell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Serialises these tests against each other: the journal and the session
/// environment variables are process-global.
static LOCK: Mutex<()> = Mutex::new(());

struct Session {
    dir: PathBuf,
}

impl Session {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ae_session_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("state")).expect("temp dir");
        std::env::set_var("AETHER_JOURNAL", "on");
        std::env::set_var("AETHER_SESSION_DIR", dir.join("state"));
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
        std::env::remove_var("AETHER_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> Result<Value, String> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
        .map_err(|e| e.to_string())
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        other => panic!("expected a record, got {other:?}"),
    }
}

fn len_of(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        other => panic!("expected an array, got {other:?}"),
    }
}

fn write_file(p: &str, text: &str) {
    call(
        "file_write",
        vec![Value::Str(p.into()), Value::Str(text.into())],
    )
    .expect("file_write should succeed");
}

/// Run `ae -c <code>` as a separate process, with the same session directory.
/// A separate process is the only honest test of persistence: it cannot see
/// this process's memory.
fn run_ae(session_dir: &Path, code: &str) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(["-c", code])
        .env("AETHER_JOURNAL", "on")
        .env("AETHER_SESSION_DIR", session_dir)
        .output()
        .expect("spawn ae")
}

#[test]
fn undo_redo_undo_round_trips_and_the_redo_is_itself_journaled() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("round_trip");
    let p = s.path("notes.txt");
    std::fs::write(&p, "v1").expect("seed");

    write_file(&p, "v2");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");

    let undone = call("session_undo", vec![]).expect("session_undo");
    assert_eq!(field(&undone, "restored"), Value::Int(1));
    assert_eq!(field(&undone, "complete"), Value::Bool(true));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");

    let redone = call("session_redo", vec![]).expect("session_redo");
    assert_eq!(field(&redone, "reapplied"), Value::Int(1));
    assert_eq!(field(&redone, "complete"), Value::Bool(true));
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v2",
        "redo must write the bytes the undone step produced"
    );

    // The redo was journaled, so it can itself be undone.
    call("session_undo", vec![]).expect("undo the redo");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");

    call("session_redo", vec![]).expect("redo again");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
}

#[test]
fn a_file_the_step_created_is_removed_then_recreated_with_the_same_bytes() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("created");
    let p = s.path("fresh.txt");
    assert!(!Path::new(&p).exists());

    write_file(&p, "made by the agent");
    assert!(Path::new(&p).is_file());

    let undone = call("session_undo", vec![]).expect("undo");
    assert_eq!(field(&undone, "removed"), Value::Int(1));
    assert!(!Path::new(&p).exists(), "undo must remove the created file");

    let redone = call("session_redo", vec![]).expect("redo");
    assert_eq!(field(&redone, "reapplied"), Value::Int(1));
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "made by the agent",
        "redo must recreate the file with the bytes it had"
    );
}

#[test]
fn a_new_write_after_undo_discards_the_redo_stack() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("discard");
    let p = s.path("doc.txt");
    std::fs::write(&p, "v1").expect("seed");

    write_file(&p, "v2");
    call("session_undo", vec![]).expect("undo");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");

    // A new recorded step ends the undone future.
    write_file(&p, "v3");

    let redone = call("session_redo", vec![]).expect("redo");
    assert_eq!(field(&redone, "reapplied"), Value::Int(0));
    assert_eq!(field(&redone, "complete"), Value::Bool(true));
    assert_eq!(len_of(&field(&redone, "steps")), 0);
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v3",
        "redo must not resurrect the discarded v2"
    );
}

#[test]
fn session_history_counts_move_with_undo_and_redo() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("history");
    let p = s.path("log.txt");
    std::fs::write(&p, "v1").expect("seed");
    write_file(&p, "v2");

    let h = call("session_history", vec![]).expect("history");
    assert_eq!(field(&h, "enabled"), Value::Bool(true));
    assert_eq!(len_of(&field(&h, "done")), 1);
    assert_eq!(len_of(&field(&h, "undone")), 0);

    call("session_undo", vec![]).expect("undo");
    let h = call("session_history", vec![]).expect("history");
    assert_eq!(len_of(&field(&h, "done")), 0);
    assert_eq!(len_of(&field(&h, "undone")), 1);

    call("session_redo", vec![]).expect("redo");
    let h = call("session_history", vec![]).expect("history");
    assert_eq!(len_of(&field(&h, "done")), 1);
    assert_eq!(len_of(&field(&h, "undone")), 0);
}

#[test]
fn session_end_keeps_the_changes_and_leaves_nothing_to_undo_or_redo() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("end");
    let p = s.path("keep.txt");
    std::fs::write(&p, "v1").expect("seed");
    write_file(&p, "v2");
    write_file(&p, "v3");
    call("session_undo", vec![]).expect("undo one step");

    let ended = call("session_end", vec![]).expect("session_end");
    assert_eq!(field(&ended, "kept"), Value::Int(1));
    assert_eq!(field(&ended, "discarded_redo"), Value::Int(1));
    assert_eq!(
        field(&ended, "note"),
        Value::Str("no files changed; these steps can no longer be undone or redone".into())
    );
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v2",
        "session_end must not touch files"
    );

    let undone = call("session_undo", vec![]).expect("undo after end");
    assert_eq!(field(&undone, "restored"), Value::Int(0));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");

    let redone = call("session_redo", vec![]).expect("redo after end");
    assert_eq!(field(&redone, "reapplied"), Value::Int(0));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
}

#[test]
fn session_end_counts_irreversible_steps_it_forgets() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _s = Session::new("irreversible");
    // A step that cannot be captured (a network call, say) is recorded as
    // irreversible, and the end of the session must say it was there. A call
    // that fails is rolled back, so this is recorded directly.
    journal::record_irreversible("net_send", aethershell::safety::Effect::Network, "sent");

    let ended = call("session_end", vec![]).expect("session_end");
    assert_eq!(field(&ended, "kept"), Value::Int(1));
    assert_eq!(field(&ended, "irreversible"), Value::Int(1));
}

#[test]
fn persistence_survives_a_new_process() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Every step here is its own process, so no in-memory state is shared.
    let s = Session::new("persist");
    let state = s.dir.join("state");
    let p = s.path("persist.txt");
    std::fs::write(&p, "v1").expect("seed");

    // Forward slashes: a backslash in an AetherShell string is an escape.
    let p_literal = p.replace('\\', "/");
    let wrote = run_ae(&state, &format!(r#"file_write("{p_literal}", "v2")"#));
    assert!(
        wrote.status.success(),
        "{}",
        String::from_utf8_lossy(&wrote.stderr)
    );
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");

    let undone = run_ae(&state, "session_undo()");
    assert!(
        undone.status.success(),
        "{}",
        String::from_utf8_lossy(&undone.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v1",
        "a later process must undo the step an earlier one recorded"
    );

    let redone = run_ae(&state, "session_redo()");
    assert!(
        redone.status.success(),
        "{}",
        String::from_utf8_lossy(&redone.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "v2",
        "a later process must redo the step an earlier one undid"
    );
}

#[test]
fn bad_arguments_are_refused_with_e_bad_arg() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _s = Session::new("badargs");
    let refused = [
        ("session_undo", vec![Value::Str("x".into())]),
        ("session_undo", vec![Value::Int(0)]),
        ("session_undo", vec![Value::Int(10_001)]),
        ("session_undo", vec![Value::Int(1), Value::Int(2)]),
        ("session_redo", vec![Value::Str("1".into())]),
        ("session_redo", vec![Value::Int(-1)]),
        ("session_redo", vec![Value::Int(10_001)]),
        ("session_redo", vec![Value::Int(1), Value::Int(1)]),
        ("session_history", vec![Value::Int(1)]),
        ("session_end", vec![Value::Bool(true)]),
    ];
    for (name, args) in refused {
        let err = call(name, args.clone()).expect_err(&format!("{name} {args:?} must be refused"));
        assert!(
            err.contains("E_BAD_ARG"),
            "{name}: not coded E_BAD_ARG: {err}"
        );
        assert!(
            err.contains(name),
            "{name}: the refusal must name the builtin: {err}"
        );
    }
}

#[test]
fn a_pipeline_input_is_refused_by_every_session_builtin() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _s = Session::new("piped");
    for name in [
        "session_undo",
        "session_redo",
        "session_history",
        "session_end",
    ] {
        let r = call_piped(name, vec![], Value::Int(1));
        assert!(r.is_err(), "{name} must refuse pipeline input");
        assert!(
            r.unwrap_err().contains("E_BAD_ARG"),
            "{name}: piped refusal must be coded E_BAD_ARG"
        );
    }
}

#[test]
fn a_refused_call_changes_nothing() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let s = Session::new("refused_noop");
    let p = s.path("keep.txt");
    std::fs::write(&p, "v1").expect("seed");
    write_file(&p, "v2");
    let _ = call("session_undo", vec![Value::Int(0)]);
    let _ = call("session_redo", vec![Value::Str("x".into())]);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
    let h = call("session_history", vec![]).expect("history");
    assert_eq!(len_of(&field(&h, "done")), 1);
}

#[test]
fn examples_and_module_aliases_execute_with_their_effects() {
    use aethershell::safety::Effect;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _s = Session::new("examples");
    // Rewinding and ending a session write files (recorded contents put
    // back, the journal deleted), so they are WriteLocal and jailed; only
    // reading the history is Pure. Their own writes are still never journaled.
    for (name, alias, effect) in [
        ("session_undo", "session.undo", Effect::WriteLocal),
        ("session_redo", "session.redo", Effect::WriteLocal),
        ("session_history", "session.history", Effect::Pure),
        ("session_end", "session.end", Effect::WriteLocal),
    ] {
        assert_eq!(aethershell::safety::effect_of(name), effect, "{name}");
        for (code, expected) in aethershell::signature::signature_of(name).unwrap().examples {
            for source in [code.to_string(), code.replacen(name, alias, 1)] {
                let mut env = aethershell::env::Env::default();
                env.register_module("session", aethershell::modules::session_module());
                let statements = aethershell::parser::parse_program(&source).unwrap();
                assert_eq!(
                    aethershell::eval::eval_program(&statements, &mut env)
                        .unwrap()
                        .to_display_string(),
                    *expected,
                    "example {source} must produce its declared result"
                );
            }
        }
    }
}
