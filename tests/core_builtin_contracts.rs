//! Contracts for the core builtins cd, rm, rmdir, sh, exit, dd_copy, pkill,
//! pgrep, chgrp and svc_start/stop/restart/enable/disable.
//!
//! Nothing here touches a file outside a fresh temporary directory, a service,
//! or a process this file did not start:
//!
//! - Refusals are argument checks that return before anything runs.
//! - `pkill` and `pgrep` are exercised against a copy of this test binary
//!   started under a unique name (`aepk` and eight digits), so no other process
//!   on the machine can match the prefix.
//! - `svc_*` are tested by refusal only. A real start or stop needs a service
//!   manager and root, which only a disposable machine should be given.
//! - `exit` is tested by refusal only; a call that passed would end this test.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use aethershell::builtins::call_with_input;
use aethershell::env::Env;
use aethershell::signature::signature_of;
use aethershell::value::Value;

/// cwd and AETHER_ALLOW_SH are process-global; tests that change them take this.
static GLOBAL: Mutex<()> = Mutex::new(());

fn global() -> std::sync::MutexGuard<'static, ()> {
    GLOBAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// A fresh directory for one test. The undo journal is kept out of the
/// repository by pointing AETHER_SESSION_DIR at a sibling directory.
fn scratch(tag: &str) -> PathBuf {
    static SESSION: std::sync::Once = std::sync::Once::new();
    SESSION.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_core_session_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    let dir = std::env::temp_dir().join(format!("ae_core_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn p(path: &std::path::Path) -> Value {
    Value::Str(path.to_string_lossy().into_owned())
}

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    call_with_input(name, args, None, &mut Env::new())
}

fn ok(name: &str, args: Vec<Value>) -> Value {
    run(name, args.clone()).unwrap_or_else(|e| panic!("{name} {args:?} failed: {e}"))
}

fn refused(name: &str, args: Vec<Value>, code: &str) -> String {
    let message = match run(name, args.clone()) {
        Ok(value) => panic!("{name} {args:?} answered {value:?} for a call it must refuse"),
        Err(e) => e.to_string(),
    };
    assert!(
        message.contains(code),
        "{name} {args:?} was not refused as {code}: {message}"
    );
    message
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    match value {
        Value::Record(map) => map
            .get(key)
            .unwrap_or_else(|| panic!("no field {key} in {value:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

const TRANCHE: &[&str] = &[
    "cd",
    "rm",
    "rmdir",
    "sh",
    "shell",
    "exit",
    "dd_copy",
    "pkill",
    "pgrep",
    "chgrp",
    "svc_start",
    "svc_stop",
    "svc_restart",
    "svc_enable",
    "svc_disable",
];

#[test]
fn every_core_builtin_is_declared() {
    for name in TRANCHE {
        assert!(signature_of(name).is_some(), "{name} has no Signature");
    }
}

#[test]
fn a_piped_subject_is_refused_by_every_core_builtin() {
    for name in TRANCHE {
        let message = match call_with_input(name, vec![s("x")], Some(s("x")), &mut Env::new()) {
            Ok(value) => panic!("{name} accepted a piped subject: {value:?}"),
            Err(e) => e.to_string(),
        };
        assert!(
            message.contains("E_BAD_ARG"),
            "{name} did not refuse a piped subject as E_BAD_ARG: {message}"
        );
    }
}

#[test]
fn surplus_arguments_are_refused_not_dropped() {
    refused("cd", vec![s("."), s(".")], "E_BAD_ARG");
    refused("rm", vec![s("a"), Value::Bool(true), s("b")], "E_BAD_ARG");
    refused(
        "rmdir",
        vec![s("a"), Value::Bool(true), s("b")],
        "E_BAD_ARG",
    );
    refused("sh", vec![s("echo"), s("hi")], "E_BAD_ARG");
    refused("exit", vec![Value::Int(0), Value::Int(1)], "E_BAD_ARG");
    refused("pkill", vec![s("aepk"), s("x")], "E_BAD_ARG");
    refused("pgrep", vec![s("aepk"), s("x")], "E_BAD_ARG");
    refused("chgrp", vec![s("g"), s("f"), s("x")], "E_BAD_ARG");
    for svc in [
        "svc_start",
        "svc_stop",
        "svc_restart",
        "svc_enable",
        "svc_disable",
    ] {
        refused(svc, vec![s("cron"), s("x")], "E_BAD_ARG");
    }
}

// ---------------------------------------------------------------------------
// rm and rmdir
// ---------------------------------------------------------------------------

#[test]
fn rm_deletes_one_file_and_force_excuses_only_absence() {
    let dir = scratch("rm");
    let file = dir.join("victim.txt");
    std::fs::write(&file, "x").unwrap();

    let out = ok("rm", vec![p(&file)]);
    assert!(!file.exists(), "rm left the file behind: {out:?}");

    // Already gone: an error without force, a success with it.
    refused("rm", vec![p(&file)], "E_");
    let forced = ok("rm", vec![p(&file), Value::Bool(true)]);
    assert!(
        matches!(&forced, Value::Str(t) if t.starts_with("already absent")),
        "{forced:?}"
    );

    // force does not turn a failed delete into a success: a directory is not a file.
    let sub = dir.join("sub");
    std::fs::create_dir(&sub).unwrap();
    refused("rm", vec![p(&sub), Value::Bool(true)], "E_");
    assert!(sub.is_dir(), "rm removed a directory");

    // A force that is not a Bool used to read as false without a word.
    std::fs::write(&file, "x").unwrap();
    refused("rm", vec![p(&file), s("yes")], "E_BAD_ARG");
    assert!(file.exists(), "a refused rm deleted the file");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rmdir_refuses_a_non_empty_directory_unless_recursive() {
    let dir = scratch("rmdir");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("inner")).unwrap();
    std::fs::write(tree.join("inner").join("f.txt"), "x").unwrap();

    refused("rmdir", vec![p(&tree)], "E_");
    assert!(
        tree.join("inner").join("f.txt").exists(),
        "a refused rmdir deleted content"
    );

    refused("rmdir", vec![p(&tree), s("yes")], "E_BAD_ARG");
    assert!(tree.exists());

    ok("rmdir", vec![p(&tree), Value::Bool(true)]);
    assert!(!tree.exists(), "recursive rmdir left the tree");

    let empty = dir.join("empty");
    std::fs::create_dir(&empty).unwrap();
    ok("rmdir", vec![p(&empty)]);
    assert!(!empty.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn recursive_rmdir_removes_a_symlink_without_following_it() {
    let dir = scratch("rmdir_link");
    let outside = dir.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), "keep").unwrap();
    let tree = dir.join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::os::unix::fs::symlink(&outside, tree.join("link")).unwrap();

    ok("rmdir", vec![p(&tree), Value::Bool(true)]);
    assert!(!tree.exists());
    assert!(
        outside.join("keep.txt").exists(),
        "recursive rmdir followed a symlink out of the tree"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// cd
// ---------------------------------------------------------------------------

#[test]
fn cd_changes_the_working_directory_and_refuses_bad_shapes() {
    let _g = global();
    let dir = scratch("cd");
    let before = std::env::current_dir().unwrap();

    let out = ok("cd", vec![p(&dir)]);
    let now = std::env::current_dir().unwrap();
    std::env::set_current_dir(&before).unwrap();
    assert_eq!(
        now.canonicalize().unwrap(),
        dir.canonicalize().unwrap(),
        "cd answered {out:?} but the directory is {now:?}"
    );

    refused("cd", vec![s("")], "E_BAD_ARG");
    refused("cd", vec![Value::Int(1)], "E_BAD_ARG");
    refused("cd", vec![p(&dir.join("missing"))], "E_");
    assert_eq!(
        std::env::current_dir().unwrap(),
        before,
        "a refused cd moved"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// sh
// ---------------------------------------------------------------------------

/// This test binary, run with `--list`, is a program present on every OS that
/// prints and exits 0; with an unknown flag it exits nonzero.
fn me() -> String {
    std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn sh_reports_a_malformed_call_before_the_opt_in() {
    let _g = global();
    std::env::remove_var("AETHER_ALLOW_SH");
    refused("sh", vec![Value::Array(vec![])], "E_BAD_ARG");
    refused("sh", vec![s("echo 'open")], "E_BAD_ARG");
    refused("sh", vec![Value::Array(vec![Value::Int(1)])], "E_BAD_ARG");
    refused("sh", vec![Value::Bool(true)], "E_BAD_ARG");
    // A well-formed call is refused by policy until sh is enabled.
    refused(
        "shell",
        vec![Value::Array(vec![s(&me()), s("--list")])],
        "E_POLICY_DENY",
    );
}

#[test]
fn sh_runs_a_program_directly_and_keeps_its_exit_status() {
    let _g = global();
    std::env::set_var("AETHER_ALLOW_SH", "true");
    let good = run("sh", vec![Value::Array(vec![s(&me()), s("--list")])]);
    let bad = run(
        "sh",
        vec![Value::Array(vec![s(&me()), s("--no-such-flag-zq")])],
    );
    std::env::remove_var("AETHER_ALLOW_SH");

    let good = good.expect("sh ran the test binary");
    assert_eq!(field(&good, "success"), &Value::Bool(true), "{good:?}");
    assert_eq!(field(&good, "exit_code"), &Value::Int(0));
    assert!(
        matches!(field(&good, "stdout"), Value::Str(t) if t.contains("sh_runs_a_program_directly")),
        "{good:?}"
    );

    // A nonzero exit is the program's answer, carried in the record.
    let bad = bad.expect("a nonzero exit is a result, not an error");
    assert_eq!(field(&bad, "success"), &Value::Bool(false), "{bad:?}");
    assert!(
        matches!(field(&bad, "exit_code"), Value::Int(c) if *c != 0),
        "{bad:?}"
    );
}

// ---------------------------------------------------------------------------
// exit
// ---------------------------------------------------------------------------

#[test]
fn exit_refuses_a_code_the_os_would_truncate() {
    refused("exit", vec![Value::Int(256)], "E_BAD_ARG");
    refused("exit", vec![Value::Int(-1)], "E_BAD_ARG");
    refused("exit", vec![s("1")], "E_BAD_ARG");
}

// ---------------------------------------------------------------------------
// dd_copy
// ---------------------------------------------------------------------------

#[test]
fn dd_copy_refuses_bad_options_before_running() {
    refused("dd_copy", vec![s(""), s("b")], "E_BAD_ARG");
    refused("dd_copy", vec![s("a"), s("")], "E_BAD_ARG");
    let mut opts = BTreeMap::new();
    opts.insert("skip".to_string(), Value::Int(1));
    refused(
        "dd_copy",
        vec![s("a"), s("b"), Value::Record(opts)],
        "E_BAD_ARG",
    );
    for bad in [Value::Int(0), s("0"), s("4MM"), s("-1"), s("x")] {
        let mut opts = BTreeMap::new();
        opts.insert("bs".to_string(), bad.clone());
        refused(
            "dd_copy",
            vec![s("a"), s("b"), Value::Record(opts)],
            "E_BAD_ARG",
        );
    }
    let mut opts = BTreeMap::new();
    opts.insert("count".to_string(), s("3"));
    refused(
        "dd_copy",
        vec![s("a"), s("b"), Value::Record(opts)],
        "E_BAD_ARG",
    );
    refused("dd_copy", vec![s("a"), s("b"), s("bs=1M")], "E_BAD_ARG");
}

#[cfg(windows)]
#[test]
fn dd_copy_is_unimplemented_on_windows_and_copies_nothing() {
    let dir = scratch("dd_win");
    let src = dir.join("in.bin");
    let dst = dir.join("out.bin");
    std::fs::write(&src, "abc").unwrap();
    refused("dd_copy", vec![p(&src), p(&dst)], "E_UNIMPLEMENTED");
    assert!(!dst.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn dd_copy_copies_the_requested_blocks() {
    let dir = scratch("dd_unix");
    let src = dir.join("in.bin");
    let dst = dir.join("out.bin");
    std::fs::write(&src, "abcdefghij").unwrap();
    let mut opts = BTreeMap::new();
    opts.insert("bs".to_string(), Value::Int(2));
    opts.insert("count".to_string(), Value::Int(3));
    let out = ok("dd_copy", vec![p(&src), p(&dst), Value::Record(opts)]);
    assert_eq!(field(&out, "success"), &Value::Bool(true));
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "abcdef");

    // A failed copy is an error, not a record that says so.
    refused(
        "dd_copy",
        vec![p(&dir.join("missing")), p(&dst)],
        "E_TOOL_FAILED",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// pkill and pgrep
// ---------------------------------------------------------------------------

#[test]
fn a_wildcard_or_empty_pattern_is_refused() {
    for name in ["pkill", "pgrep"] {
        for bad in ["", "  ", "*", "a*", "a?", "[a]", "a\nb"] {
            refused(name, vec![s(bad)], "E_BAD_ARG");
        }
        refused(name, vec![Value::Int(1)], "E_BAD_ARG");
    }
}

#[test]
fn no_match_is_an_empty_answer() {
    let prefix = format!("aenomatch{}", std::process::id());
    assert_eq!(ok("pgrep", vec![s(&prefix)]), Value::Array(vec![]));
    let out = ok("pkill", vec![s(&prefix)]);
    assert_eq!(field(&out, "killed"), &Value::Array(vec![]), "{out:?}");
}

/// Started by `pkill_kills_only_the_process_it_names` as a renamed copy of this
/// binary; it waits to be killed. Without the variable it returns at once, so
/// `--ignored` runs of this file never hang on it.
#[test]
#[ignore]
fn child_sleeper() {
    if std::env::var("AE_CORE_SLEEPER").as_deref() == Ok("1") {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

#[test]
fn pkill_kills_only_the_process_it_names() {
    let dir = scratch("pkill");
    // Linux reports at most 15 characters of a process name; this is 12.
    let stem = format!("aepk{:08}", std::process::id() % 100_000_000);
    let exe = dir.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    let mut child = std::process::Command::new(&exe)
        .args(["--ignored", "--exact", "child_sleeper", "--test-threads=1"])
        .env("AE_CORE_SLEEPER", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start the renamed copy");
    let pid = i64::from(child.id());

    // pgrep finds it, by name and pid.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let found = loop {
        let found = ok("pgrep", vec![s(&stem)]);
        let has = matches!(&found, Value::Array(rows) if rows.iter().any(|r| field(r, "pid") == &Value::Int(pid)));
        if has || std::time::Instant::now() > deadline {
            break found;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let Value::Array(rows) = &found else {
        panic!("pgrep answered {found:?}")
    };
    assert_eq!(rows.len(), 1, "only the child carries this name: {found:?}");
    assert_eq!(field(&rows[0], "pid"), &Value::Int(pid));
    assert!(
        matches!(field(&rows[0], "name"), Value::Str(n) if n.starts_with(&stem)),
        "{found:?}"
    );

    let out = run("pkill", vec![s(&stem)]);
    // Reap the child whatever happened, so a failure cannot leak it.
    let _ = child.kill();
    let status = child.wait().expect("wait for the child");
    let out = out.expect("pkill");
    assert_eq!(
        field(&out, "killed"),
        &Value::Array(vec![Value::Int(pid)]),
        "{out:?}"
    );
    assert!(!status.success(), "the child exited on its own: {status:?}");
    assert_eq!(ok("pgrep", vec![s(&stem)]), Value::Array(vec![]));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// chgrp
// ---------------------------------------------------------------------------

#[test]
fn chgrp_refuses_bad_shapes_before_running() {
    refused("chgrp", vec![s("staff"), s("")], "E_BAD_ARG");
    refused("chgrp", vec![s(""), s("f")], "E_BAD_ARG");
    refused("chgrp", vec![s("-R"), s("f")], "E_BAD_ARG");
    refused("chgrp", vec![s("staff"), Value::Int(1)], "E_BAD_ARG");
}

#[cfg(windows)]
#[test]
fn chgrp_names_a_windows_account_and_refuses_numeric_ids() {
    let dir = scratch("chgrp_win");
    let file = dir.join("f.txt");
    std::fs::write(&file, "x").unwrap();
    refused("chgrp", vec![s("1000"), p(&file)], "E_BAD_ARG");
    refused("chgrp", vec![s("ae-no-such-group-zq"), p(&file)], "E_NOT_FOUND");
    refused(
        "chgrp",
        vec![s("ae-no-such-group-zq"), p(&dir.join("missing"))],
        "E_NOT_FOUND",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn chgrp_sets_a_group_the_caller_already_has() {
    let dir = scratch("chgrp_unix");
    let file = dir.join("f.txt");
    std::fs::write(&file, "x").unwrap();
    let gid = String::from_utf8(
        std::process::Command::new("id")
            .arg("-g")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let out = ok("chgrp", vec![s(&gid), p(&file)]);
    assert_eq!(field(&out, "group"), &s(&gid));
    refused(
        "chgrp",
        vec![s(&gid), p(&dir.join("missing"))],
        "E_NOT_FOUND",
    );
    refused(
        "chgrp",
        vec![s("ae-no-such-group-zq"), p(&file)],
        "E_TOOL_FAILED",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// svc_*
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_service_name_is_refused_before_any_manager_runs() {
    for svc in [
        "svc_start",
        "svc_stop",
        "svc_restart",
        "svc_enable",
        "svc_disable",
    ] {
        for bad in ["", "-x", "--now", "a b", "a;b", "a/b", "a'b", "a$b"] {
            refused(svc, vec![s(bad)], "E_BAD_ARG");
        }
        refused(svc, vec![Value::Int(1)], "E_BAD_ARG");
        refused(svc, vec![s(&"a".repeat(257))], "E_BAD_ARG");
    }
}
