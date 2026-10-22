//! proc_env and proc_cwd on Windows, against a child this test starts.
//!
//! The child is a `ping` with a unique environment variable and its own
//! working directory. It is killed in a guard's `Drop`, so a failed assertion
//! does not leave it running. The System process (pid 4) is never read
//! successfully: Windows does not give its environment or directory, so the
//! answer must be a coded policy refusal, not an empty record or string.

#![cfg(windows)]

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The code of an error: the first `E_...` word in its text.
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

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn expect_code(name: &str, args: Vec<Value>, want: &str) {
    match run(name, args.clone()) {
        Ok(v) => panic!("{name}{args:?} answered {v:?}; it should be {want}"),
        Err(e) => assert_eq!(code_of(&e), want, "{name}{args:?}: {e}"),
    }
}

/// A pid no live process holds.
const NOT_LIVE: i64 = 2_000_000_000;

/// The System process: always live on Windows, never readable by a user.
const SYSTEM_PID: i64 = 4;

/// A unique value for the marker variable, so a stale process cannot match.
fn unique_mark() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("procwin-{}-{nanos:x}", std::process::id())
}

/// A fresh directory under the temp dir, removed when the guard drops.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(unique_mark());
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A `ping` that carries AE_PROCWIN_MARK and runs in `dir`. Killed on drop.
struct Marked {
    child: Child,
    mark: String,
}

impl Marked {
    fn start(dir: &Path, mark: &str) -> Self {
        let child = Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .env("AE_PROCWIN_MARK", mark)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("ping starts");
        Marked {
            child,
            mark: mark.to_string(),
        }
    }

    fn pid(&self) -> i64 {
        i64::from(self.child.id())
    }
}

impl Drop for Marked {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The path with the verbatim prefix removed, resolved, lowercased, and with
/// no trailing separator, so two spellings of one directory compare equal.
fn normal_dir(p: &str) -> String {
    let resolved = std::fs::canonicalize(p)
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| p.to_string());
    let no_prefix = resolved.strip_prefix(r"\\?\").unwrap_or(&resolved);
    no_prefix.trim_end_matches(['\\', '/']).to_ascii_lowercase()
}

#[test]
fn proc_env_of_a_child_has_the_marker_it_was_started_with() {
    let dir = TempDir::new();
    let mark = unique_mark();
    let child = Marked::start(&dir.0, &mark);

    let env = run("proc_env", vec![Value::Int(child.pid())]).expect("proc_env of child");
    let Value::Record(r) = env else {
        panic!("proc_env did not answer a record");
    };
    assert!(
        !r.is_empty(),
        "proc_env of a live child answered an empty record"
    );
    assert_eq!(
        r.get("AE_PROCWIN_MARK"),
        Some(&Value::Str(child.mark.clone())),
        "marker of child {}",
        child.pid()
    );
}

#[test]
fn proc_cwd_of_a_child_is_the_directory_it_was_started_in() {
    let dir = TempDir::new();
    let mark = unique_mark();
    let child = Marked::start(&dir.0, &mark);

    let got = run("proc_cwd", vec![Value::Int(child.pid())]).expect("proc_cwd of child");
    let Value::Str(got) = got else {
        panic!("proc_cwd did not answer a string");
    };
    let want = dir.0.to_string_lossy().into_owned();
    assert_eq!(
        normal_dir(&got),
        normal_dir(&want),
        "proc_cwd {got:?} of child is not {want:?}"
    );
}

#[test]
fn proc_env_of_the_system_process_is_policy_denied_not_empty() {
    expect_code("proc_env", vec![Value::Int(SYSTEM_PID)], "E_POLICY_DENY");
}

#[test]
fn proc_cwd_of_the_system_process_is_policy_denied_not_empty() {
    expect_code("proc_cwd", vec![Value::Int(SYSTEM_PID)], "E_POLICY_DENY");
}

#[test]
fn proc_env_and_proc_cwd_of_a_pid_that_is_not_live_are_not_found() {
    expect_code("proc_env", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code("proc_cwd", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
}

#[test]
fn proc_env_and_proc_cwd_refuse_bad_pids_before_any_effect() {
    for name in ["proc_env", "proc_cwd"] {
        expect_code(name, vec![Value::Int(0)], "E_BAD_ARG");
        expect_code(name, vec![Value::Int(-1)], "E_BAD_ARG");
        expect_code(name, vec![Value::Str("not-a-pid".into())], "E_BAD_ARG");
        expect_code(name, vec![], "E_BAD_ARG");
        expect_code(
            name,
            vec![Value::Int(SYSTEM_PID), Value::Int(1)],
            "E_BAD_ARG",
        );
    }
}

#[test]
fn proc_env_and_proc_cwd_refuse_pipeline_input() {
    for name in ["proc_env", "proc_cwd"] {
        let mut env = Env::new();
        let got = call_with_input(
            name,
            vec![Value::Int(SYSTEM_PID)],
            Some(Value::Int(SYSTEM_PID)),
            &mut env,
        );
        assert!(got.is_err(), "{name} accepted pipeline input");
    }
}
