//! Declarations and refusals for the fallback journal, undo, handle, agent and
//! swarm builtins.
//!
//! Every test that touches the journal takes `JOURNAL_LOCK` and runs against a
//! temporary `IRONSHELL_SESSION_DIR`, so none of them reads or writes the real
//! journal. The agent and swarm cases are all refusals: each one is turned away
//! by its declaration or by the argument parser before a run starts, so no
//! model is ever called.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::eval::eval_program;
use iron_shell::journal;
use iron_shell::parser::parse_program;
use iron_shell::signature::signature_of;
use iron_shell::value::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// Serialises the journal tests: the journal and the session variables are
/// process-global, and the tests in this file run in parallel.
static JOURNAL_LOCK: Mutex<()> = Mutex::new(());

/// A journal in its own temporary session directory, held under the lock for
/// the life of the test.
struct Session {
    dir: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Session {
    fn new(tag: &str) -> Self {
        let lock = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("ae_fb_sess_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("state")).expect("temp session dir");
        std::env::set_var("IRONSHELL_JOURNAL", "on");
        std::env::set_var("IRONSHELL_SESSION_DIR", dir.join("state"));
        journal::clear();
        Self { dir, _lock: lock }
    }

    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        journal::clear();
        std::env::remove_var("IRONSHELL_JOURNAL");
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

/// Evaluate `call_src` inside `try`, and return the error code it caught. A
/// call that succeeds is a failure here: it means the argument was not refused.
fn refusal(call_src: &str) -> String {
    let src = format!("(try {{ {call_src} }} catch e {{ e.error.code }})");
    let stmts = parse_program(&src).unwrap_or_else(|e| panic!("parse {src}: {e}"));
    match eval_program(&stmts, &mut Env::new()) {
        Ok(Value::Str(code)) => code,
        other => panic!("`{call_src}` was not refused: {other:?}"),
    }
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        other => panic!("expected a record, got {other:?}"),
    }
}

fn expect_refused<S: AsRef<str>>(cases: &[(S, &str)]) {
    for (src, code) in cases {
        assert_eq!(
            refusal(src.as_ref()),
            *code,
            "refusal of `{}`",
            src.as_ref()
        );
    }
}

// ---- declarations -------------------------------------------------------

#[test]
fn every_name_in_this_tranche_is_declared() {
    for name in [
        "journal_clear",
        "undo",
        "rewind",
        "handle",
        "agent",
        "swarm",
    ] {
        assert!(signature_of(name).is_some(), "{name} is not declared");
    }
}

#[test]
fn rewind_is_an_alias_of_undo() {
    let undo = signature_of("undo").expect("undo declared");
    assert!(
        undo.aliases.contains(&"rewind"),
        "undo lists rewind as an alias"
    );
    let via_alias = signature_of("rewind").expect("rewind resolves");
    assert_eq!(via_alias.name, "undo");
}

#[test]
fn journal_clear_declares_no_parameters() {
    let sig = signature_of("journal_clear").expect("declared");
    assert!(sig.params.is_empty(), "journal_clear takes no arguments");
}

// ---- journal_clear ------------------------------------------------------

#[test]
fn journal_clear_refuses_a_surplus_argument_and_clears_nothing() {
    // The regression: `journal_clear("a", 1, 2)` used to clear the whole
    // journal, because the body ignored its arguments.
    let s = Session::new("clear_surplus");
    let target = s.path("kept.txt");
    call(
        "file_write",
        vec![Value::Str(target.clone()), Value::Str("hello".into())],
    )
    .expect("file_write records one step");
    assert_eq!(
        field(&call("journal", vec![]).unwrap(), "steps"),
        Value::Int(1)
    );

    assert_eq!(refusal(r#"journal_clear("a", 1, 2)"#), "E_BAD_ARG");
    assert_eq!(refusal(r#"journal_clear("a")"#), "E_BAD_ARG");
    assert_eq!(refusal("1 | journal_clear()"), "E_BAD_ARG");

    let after = call("journal", vec![]).expect("journal() still answers");
    assert_eq!(
        field(&after, "steps"),
        Value::Int(1),
        "the recorded step survives every refused journal_clear"
    );
    assert!(
        std::path::Path::new(&target).exists(),
        "the file the step wrote is untouched"
    );

    // The bare call still forgets exactly that step.
    let cleared = call("journal_clear", vec![]).expect("bare journal_clear");
    assert_eq!(field(&cleared, "forgotten"), Value::Int(1));
    assert_eq!(
        field(&call("journal", vec![]).unwrap(), "steps"),
        Value::Int(0)
    );
}

// ---- undo and rewind ----------------------------------------------------

#[test]
fn undo_and_rewind_refuse_bad_counts_and_surplus_arguments() {
    let s = Session::new("undo_refusals");
    let target = s.path("made.txt");
    call(
        "file_write",
        vec![Value::Str(target.clone()), Value::Str("new".into())],
    )
    .expect("file_write records one step");

    for name in ["undo", "rewind"] {
        expect_refused(&[
            (format!(r#"{name}("x")"#), "E_BAD_ARG"),
            (format!("{name}(1.5)"), "E_BAD_ARG"),
            // A negative count used to be clamped to 0 and reported as complete.
            (format!("{name}(-1)"), "E_BAD_ARG"),
            (format!("{name}(1, 2)"), "E_BAD_ARG"),
            (format!("1 | {name}()"), "E_BAD_ARG"),
        ]);
    }

    // Every refusal left the step in place and the file where it was.
    assert_eq!(
        field(&call("journal", vec![]).unwrap(), "steps"),
        Value::Int(1)
    );
    assert!(std::path::Path::new(&target).exists());

    // A valid count reverses it: the file did not exist before, so it is removed.
    let r = call("undo", vec![Value::Int(1)]).expect("undo(1) runs");
    assert_eq!(field(&r, "complete"), Value::Bool(true));
    assert_eq!(field(&r, "removed"), Value::Int(1));
    assert!(!std::path::Path::new(&target).exists());
}

// ---- handle -------------------------------------------------------------

#[test]
fn handle_refuses_missing_wrong_and_surplus_arguments() {
    expect_refused(&[
        ("handle()", "E_BAD_ARG"),
        ("handle(5)", "E_BAD_ARG"),
        (r#"handle("a", "b")"#, "E_BAD_ARG"),
        (r#"handle("no-such-handle")"#, "E_BAD_ARG"),
        (r#""x" | handle("a")"#, "E_BAD_ARG"),
    ]);
}

// ---- agent and swarm ----------------------------------------------------

/// Each case is refused before any run starts, so none reaches a model. `NAME`
/// is replaced by the builtin's name.
const AGENT_REFUSALS: &[(&str, &str)] = &[
    ("NAME()", "E_BAD_ARG"),
    ("NAME(5)", "E_BAD_ARG"),
    ("NAME(\"x\", [1])", "E_BAD_ARG"),
    ("NAME(\"x\", \"print\", 3, true, \"extra\")", "E_BAD_ARG"),
    ("NAME(\"x\", \"print\", true, 3)", "E_BAD_ARG"),
    ("NAME(\"x\", \"print\", 0)", "E_BAD_ARG"),
    ("NAME(\"x\", \"print\", 1001)", "E_BAD_ARG"),
    ("NAME(\"x\", [\"print\"], 3, true, 4)", "E_BAD_ARG"),
    ("NAME(\"x\", [\"no_such_tool_xyz\"])", "E_NOT_FOUND"),
    ("NAME(\"x\", \"no_such_tool_xyz\")", "E_NOT_FOUND"),
    ("NAME(\"x\", \"{bad\")", "E_BAD_ARG"),
    ("NAME(\"x\", \"{}\", \"y\")", "E_BAD_ARG"),
    ("NAME({goal: \"x\", model_uri: \"y\"})", "E_BAD_ARG"),
    ("NAME({tools: [\"print\"]})", "E_BAD_ARG"),
    ("NAME({goal: \"x\", tools: [1]})", "E_BAD_ARG"),
    ("NAME({goal: \"x\", max_steps: \"5\"})", "E_BAD_ARG"),
    ("NAME({goal: \"x\", max_steps: 0})", "E_BAD_ARG"),
    ("NAME({goal: \"x\", dry_run: 1})", "E_BAD_ARG"),
    ("NAME({goal: \"x\"}, \"y\")", "E_BAD_ARG"),
    ("\"text\" | NAME(\"x\")", "E_BAD_ARG"),
    ("{goal: \"x\"} | NAME(\"y\")", "E_BAD_ARG"),
];

#[test]
fn agent_refuses_every_malformed_form_before_any_run() {
    for (template, code) in AGENT_REFUSALS {
        let src = template.replace("NAME", "agent");
        assert_eq!(refusal(&src), *code, "refusal of `{src}`");
    }
}

#[test]
fn swarm_refuses_every_malformed_form_before_any_run() {
    for (template, code) in AGENT_REFUSALS {
        let src = template.replace("NAME", "swarm");
        assert_eq!(refusal(&src), *code, "refusal of `{src}`");
    }
}
