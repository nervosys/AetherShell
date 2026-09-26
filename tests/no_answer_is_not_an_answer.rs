//! Silent successes found by `benches/agentic/silent-success.mjs`.
//!
//! Handed `{unexpected: true}`, these returned `[]` or `false` at exit 0. A
//! differential against the bare call separated them from builtins that simply
//! ignore arguments: the bare call refused (E_BAD_ARG, "requires job_id"), the
//! record was accepted. The record had been stringified and looked up as a
//! name, found nothing, and reported that nothing as the answer.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {s}"),
    }
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

#[test]
fn a_lookup_key_must_be_a_string_or_an_id() {
    let odd = rec(&[("unexpected", Value::Bool(true))]);
    for name in [
        "user_roles",
        "kg_query",
        "rag_search",
        "job_status",
        "job_results",
        "cluster_remove_node",
        "finetune_status",
        "compliance_check",
    ] {
        let e = call(name, vec![odd.clone()]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}({{unexpected: true}})");
        let e = call(name, vec![Value::Array(vec![Value::Int(1)])]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}([1])");
    }
}

#[test]
fn a_well_typed_miss_is_still_an_empty_answer() {
    // Non-vacuity: the fix must not turn a legitimate "nothing found" into
    // an error. A user with no roles has no roles.
    let v = call("user_roles", vec![s("nobody-with-this-name")]).unwrap();
    assert_eq!(v, Value::Array(vec![]));
}

fn require(spec: Value) -> anyhow::Result<(bool, Vec<Value>)> {
    let Value::Record(r) = call("platform_require", vec![spec])? else {
        panic!("platform_require did not return a record")
    };
    let Some(Value::Bool(sat)) = r.get("satisfied") else {
        panic!()
    };
    let Some(Value::Array(missing)) = r.get("missing") else {
        panic!()
    };
    Ok((*sat, missing.clone()))
}

#[test]
fn platform_require_refuses_what_it_cannot_read() {
    for bad in [
        s("linux"),
        Value::Int(1),
        rec(&[("os", Value::Int(42))]),
        rec(&[("arch", Value::Bool(true))]),
        rec(&[("cargo", Value::Int(1))]),
        rec(&[("cargo", s("banana"))]),
    ] {
        let e = require(bad.clone()).expect_err(&format!("{bad:?}"));
        assert_eq!(code_of(&e), "E_BAD_ARG", "{bad:?}");
    }
}

#[test]
fn platform_require_answers_what_it_can() {
    let (sat, _) = require(rec(&[("os", s(std::env::consts::OS))])).unwrap();
    assert!(sat);
    let (sat, missing) = require(rec(&[("os", s("plan9"))])).unwrap();
    assert!(!sat);
    assert_eq!(missing.len(), 1);
    let (sat, _) = require(rec(&[("no-such-tool-aeth", Value::Bool(true))])).unwrap();
    assert!(!sat);
    let (sat, _) = require(rec(&[("no-such-tool-aeth", Value::Bool(false))])).unwrap();
    assert!(sat, "`tool: false` means not required");
}

/// The requirement was a TODO: any installed version satisfied it. `cargo` is
/// on PATH wherever these tests run, which is what makes this checkable.
#[test]
fn platform_require_compares_versions() {
    let (sat, missing) = require(rec(&[("cargo", s(">=1.0"))])).unwrap();
    assert!(sat, "cargo >=1.0: {missing:?}");
    let (sat, missing) = require(rec(&[("cargo", s("1"))])).unwrap();
    assert!(sat, "a bare version means >=: {missing:?}");
    let (sat, missing) = require(rec(&[("cargo", s(">=999.0"))])).unwrap();
    assert!(!sat);
    let Value::Str(why) = &missing[0] else {
        panic!()
    };
    assert!(why.contains("have 1."), "{why}");
    let (sat, _) = require(rec(&[("cargo", s("<1.0"))])).unwrap();
    assert!(!sat);
}

/// Every git builtin read stdout and ignored git's exit status. Outside a
/// repository git writes only to stderr, so `git_status()` said `[]` --
/// a clean tree -- at exit 0.
#[test]
fn git_outside_a_repository_is_a_state_error() {
    let dir = std::env::temp_dir().join(format!("ae-not-a-repo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Stop git from finding an enclosing repository above the temp dir.
    let inside_repo = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true);
    if inside_repo {
        eprintln!("skipped: the temp dir is inside a git work tree here");
        return;
    }
    let path = s(dir.to_str().unwrap());
    for name in ["git_status", "git_diff", "git_diff_staged"] {
        let e = call(name, vec![path.clone()]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_STATE", "{name}: {e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wrongly_typed_argument_is_refused_not_defaulted() {
    let odd = rec(&[("unexpected", Value::Bool(true))]);
    for name in [
        "git_status",
        "git_show",
        "git_log",
        "session_restore",
        "search_by_size",
        "proc_env",
        "proc_cpu_usage",
        "platform_has_tool",
        "clipboard_set",
    ] {
        let e = call(name, vec![odd.clone()]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}");
    }
    for name in ["proc_env", "proc_cpu_usage", "platform_has_tool"] {
        let e = call(name, vec![]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}()");
    }
}

/// Null is the shell's one spelling of "unset". `env_venv` said "".
#[test]
fn no_virtualenv_is_null_like_any_unset_variable() {
    std::env::remove_var("VIRTUAL_ENV");
    assert_eq!(call("env_venv", vec![]).unwrap(), Value::Null);
    assert_eq!(
        call("env", vec![s("AE_SURELY_UNSET_VARIABLE")]).unwrap(),
        Value::Null,
        "the convention env_venv now follows"
    );
}

fn eval(code: &str) -> anyhow::Result<Value> {
    let stmts = aethershell::parser::parse_program(code)?;
    aethershell::eval::eval_program(&stmts, &mut aethershell::env::Env::new())
}

/// `each` ran its action with `let _ =`, so a failing side effect left it
/// reporting success with the array unchanged.
#[test]
fn each_reports_an_error_in_its_action() {
    let e = eval("[1, 2] | each(fn(x) => x / 0)").expect_err("each swallowed the error");
    assert!(code_of(&e).starts_with("E_"), "{e}");
    // Non-vacuity: a well-behaved action still returns the array unchanged.
    assert_eq!(
        eval("[1, 2] | each(fn(x) => x * 2)").unwrap(),
        Value::Array(vec![Value::Int(1), Value::Int(2)])
    );
}

/// The dispatch row dropped the pipe input, so a piped value became "".
#[test]
fn echo_renders_a_piped_value() {
    assert_eq!(eval(r#""hello" | echo()"#).unwrap(), s("hello"));
    assert_eq!(eval(r#"echo("a", 1)"#).unwrap(), s("a 1"));
}
