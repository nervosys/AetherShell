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

/// `crypto_hex_decode` sliced byte offsets: an odd length read past the end
/// and panicked, and invalid pairs were skipped ("zz41" decoded to "A").
#[test]
fn hex_decode_refuses_what_is_not_hex() {
    for bad in ["abc", "zz41", "é1"] {
        let e = call("crypto_hex_decode", vec![s(bad)]).expect_err(bad);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{bad}");
    }
    let e = call("crypto_hex_encode", vec![]).expect_err("encoded nothing");
    assert_eq!(code_of(&e), "E_BAD_ARG");
    assert_eq!(call("crypto_hex_decode", vec![s("6869")]).unwrap(), s("hi"));
}

/// role_create dropped malformed permission entries and still reported
/// "created", and a piped name took the description as its permissions.
#[test]
fn role_create_refuses_a_permission_it_cannot_read() {
    for bad in [
        eval(r#"role_create("r1", [{actions: ["read"]}])"#),
        eval(r#"role_create("r2", [{resource: "docs", actions: "read"}])"#),
        eval(r#"role_create("r3", ["docs"])"#),
    ] {
        assert_eq!(code_of(&bad.expect_err("accepted")), "E_BAD_ARG");
    }
    let piped = eval(r#""r4" | role_create([{resource: "docs", actions: ["read"]}], "desc")"#)
        .expect("piped form");
    let Value::Record(r) = piped else { panic!() };
    assert_eq!(r.get("role"), Some(&s("r4")));
}

/// Unknown notification levels were shown as info, a non-bool success was
/// reported as success, and invalid JSON rendered as null.
#[test]
fn a2ui_refuses_what_it_would_have_misreported() {
    for bad in [
        r#"a2ui_notify("x", "critical")"#,
        r#"a2ui_toast("x", "critical")"#,
        r#"a2ui_agent_completed("a1", "failed")"#,
        r#"a2ui_render({json: "{not json"})"#,
    ] {
        assert_eq!(code_of(&eval(bad).expect_err(bad)), "E_BAD_ARG", "{bad}");
    }
}

/// md5 shared crypto_hash's dispatch row, whose default is SHA-256, so
/// md5("hello") returned the SHA-256 of "hello". An unknown algorithm fell
/// through to SHA-256 too, and a piped subject hashed the algorithm's name.
#[test]
fn a_hash_is_the_hash_it_names() {
    const MD5: &str = "5d41402abc4b2a76b9719d911017c592";
    const SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
    const SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    assert_eq!(eval(r#"md5("hello")"#).unwrap(), s(MD5));
    assert_eq!(eval(r#"sha256("hello")"#).unwrap(), s(SHA256));
    assert_eq!(eval(r#"crypto_hash("hello", "sha1")"#).unwrap(), s(SHA1));
    assert_eq!(eval(r#""hello" | crypto_hash("md5")"#).unwrap(), s(MD5));
    assert_eq!(eval(r#"hash("hello")"#).unwrap(), s(SHA256));
    let e = eval(r#"crypto_hash("hello", "blake3")"#).expect_err("blake3 answered");
    assert_eq!(code_of(&e), "E_BAD_ARG");
    let e = eval(r#"md5("hello", "sha256")"#).expect_err("md5 took an algorithm");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

/// base64 ran `base64` (which wraps at 76 columns) on Unix and PowerShell
/// (which does not) on Windows, so a long input encoded differently by OS.
#[test]
fn base64_is_the_same_on_every_os_and_round_trips() {
    let long = "x".repeat(200);
    let Value::Str(enc) = call("base64_encode", vec![s(&long)]).unwrap() else {
        panic!()
    };
    assert!(!enc.contains('\n'), "wrapped: {enc}");
    assert_eq!(call("base64_decode", vec![s(&enc)]).unwrap(), s(&long));
    let e = call("base64_decode", vec![s("not base64!")]).expect_err("decoded garbage");
    assert_eq!(code_of(&e), "E_BAD_ARG");
    let e = call("base64_encode", vec![]).expect_err("encoded nothing");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

/// proc_kill parsed a name as pid 0 and passed negatives through: to kill(1),
/// 0 is the caller's own process group and -1 every process it may signal.
#[test]
fn a_pid_is_positive_and_numeric() {
    for (name, args) in [
        ("proc_kill", vec![s("firefox")]),
        ("proc_kill", vec![Value::Int(-1)]),
        ("proc_kill", vec![Value::Int(0)]),
        ("proc_info", vec![s("abc")]),
        ("proc_exists", vec![s("abc")]),
        ("proc_set_priority", vec![Value::Int(-1), Value::Int(5)]),
    ] {
        let e = call(name, args).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}: {e}");
    }
    // An unknown signal was sent as TERM. Refused before anything runs.
    let e = call("proc_kill", vec![Value::Int(999_999_999), s("SIGKILL")]).expect_err("signal");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

/// db_json_to_csv wrote non-string cells with Debug formatting (`Int(5)`),
/// took columns from the first record only, and answered "" for bad input.
#[test]
fn csv_cells_are_values_and_every_column_is_kept() {
    let v = eval(r#"db_json_to_csv([{a: 5, b: "x,y"}, {a: 6, c: true}])"#).unwrap();
    assert_eq!(v, s("a,b,c\n5,\"x,y\",\n6,,true\n"));
    for bad in [
        r#"db_json_to_csv("{not json")"#,
        r#"db_json_to_csv("[1, 2]")"#,
        "db_json_to_csv(5)",
    ] {
        assert_eq!(code_of(&eval(bad).expect_err(bad)), "E_BAD_ARG", "{bad}");
    }
}

/// db_sqlite_count matched only a String count, and the query returns an Int,
/// so every table counted 0.
#[test]
fn a_sqlite_count_counts() {
    if std::process::Command::new("sqlite3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipped: sqlite3 is not installed here");
        return;
    }
    let dir = std::env::temp_dir().join(format!("ae-count-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("t.db");
    let made = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("create table t(a int); insert into t values (1),(2),(3);")
        .status()
        .unwrap();
    assert!(made.success());
    let n = call("db_sqlite_count", vec![s(db.to_str().unwrap()), s("t")]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(n.unwrap(), Value::Int(3));
}
