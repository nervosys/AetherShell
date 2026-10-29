//! Contracts for sudo_exec on Windows, where it runs argv directly in an
//! elevated process (see src/services.rs, win_sudo).
//!
//! Compiled on Windows only. Each test is `#[ignore]`d and also returns at once
//! unless AE_DISPOSABLE_HOST=1, which only the disposable Windows runner sets.
//! The runner is elevated, so each test first checks that with sudo_check; on
//! a host that is not elevated the test fails rather than being skipped, since
//! the runner is the only place these are meant to run.

#![cfg(windows)]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

/// Every test takes this lock, so the runs never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

/// True only on the disposable Windows runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn argv(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|item| s(item)).collect())
}

fn options(timeout_ms: i64) -> Value {
    Value::Record(BTreeMap::from([(
        "timeout_ms".to_string(),
        Value::Int(timeout_ms),
    )]))
}

fn fields(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Record(map) => map,
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

/// Fails unless this process is elevated: every test below needs it.
fn require_elevated(env: &mut Env) {
    let elevated = aethershell::builtins::call("sudo_check", vec![], env).unwrap();
    assert_eq!(elevated, Value::Bool(true), "the runner must be elevated");
}

/// How many ping.exe processes the system lists now.
fn ping_count() -> usize {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq PING.EXE", "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist runs on the runner");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("ping.exe"))
        .count()
}

#[test]
#[ignore = "runs commands: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn elevated_command_runs_in_a_high_integrity_token() {
    if !disposable() {
        return;
    }
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut env = Env::default();
    require_elevated(&mut env);

    let run = fields(
        aethershell::builtins::call(
            "sudo_exec",
            vec![argv(&[r"C:\Windows\System32\whoami.exe", "/groups"])],
            &mut env,
        )
        .unwrap(),
    );
    assert_eq!(run.get("exit_code"), Some(&Value::Int(0)));
    assert_eq!(run.get("timed_out"), Some(&Value::Bool(false)));
    assert!(
        text(&run, "stdout").contains("S-1-16-12288"),
        "expected the High mandatory level in {:?}",
        text(&run, "stdout")
    );
}

#[test]
#[ignore = "runs commands: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn nonzero_exit_is_reported_in_exit_code() {
    if !disposable() {
        return;
    }
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut env = Env::default();
    require_elevated(&mut env);

    let run = fields(
        aethershell::builtins::call(
            "sudo_exec",
            vec![argv(&[r"C:\Windows\System32\cmd.exe", "/c", "exit 3"])],
            &mut env,
        )
        .unwrap(),
    );
    assert_eq!(run.get("exit_code"), Some(&Value::Int(3)));
    assert_eq!(run.get("timed_out"), Some(&Value::Bool(false)));
}

#[test]
#[ignore = "runs commands: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn missing_file_argument_is_a_nonzero_exit_with_stderr() {
    if !disposable() {
        return;
    }
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut env = Env::default();
    require_elevated(&mut env);

    let run = fields(
        aethershell::builtins::call(
            "sudo_exec",
            vec![argv(&[
                r"C:\Windows\System32\where.exe",
                "no-such-ae-thing",
            ])],
            &mut env,
        )
        .unwrap(),
    );
    assert!(
        matches!(run.get("exit_code"), Some(Value::Int(code)) if *code != 0),
        "{run:?}"
    );
    assert!(!text(&run, "stderr").is_empty(), "{run:?}");
    assert_eq!(run.get("timed_out"), Some(&Value::Bool(false)));
}

#[test]
#[ignore = "runs commands: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn timeout_kills_the_command_and_its_tree() {
    if !disposable() {
        return;
    }
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut env = Env::default();
    require_elevated(&mut env);

    // ping.exe is a direct child with no children of its own, so the check
    // below is on the process itself. Any ping.exe already listed before the
    // call is excluded by comparing counts, not by assuming none exist.
    let before = ping_count();
    let started = std::time::Instant::now();
    let run = fields(
        aethershell::builtins::call(
            "sudo_exec",
            vec![
                argv(&[r"C:\Windows\System32\ping.exe", "-n", "30", "127.0.0.1"]),
                options(500),
            ],
            &mut env,
        )
        .unwrap(),
    );
    let elapsed = started.elapsed();

    assert_eq!(run.get("timed_out"), Some(&Value::Bool(true)));
    assert_eq!(run.get("exit_code"), Some(&Value::Null));
    assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
    assert!(
        ping_count() <= before,
        "a ping.exe started by the call is still running"
    );
}
