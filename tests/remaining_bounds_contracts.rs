//! Contracts for the remaining bounded runners: a silent child past a short
//! deadline is killed with its whole process tree and answers with a coded
//! budget error, and a grandchild that holds a pipe open does not hold the call.
//!
//! The sites: the container CLI runner (docker and podman), the checked tool
//! runner behind `run_tool`, the package manager's exec helper (Linux), the
//! external-tool manifest probes, and the `test_watch` builtin, which kills a
//! run past its `timeout_ms` and reports it in the result. The legacy openssl
//! decrypt must refuse plaintext that is not UTF-8 rather than convert it.
//!
//! Each test runs its call on a thread with a hard 60 s bound, so a regression
//! fails instead of hanging. Every child here is a harmless shell or PowerShell
//! command that sleeps, and every scratch file is removed at the end.

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Runs `f` on a thread and fails the test if it does not return within 60 s.
fn bounded<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(value) => value,
        Err(_) => panic!("{what} did not return within 60 s: the call was held by a child"),
    }
}

/// Asserts that a refusal carries the coded `code` in its text.
fn assert_code(e: &anyhow::Error, code: &str) {
    let text = e.to_string();
    assert!(
        text.contains(code),
        "expected a {code} refusal, got: {text}"
    );
}

/// A child that sleeps and prints nothing, past any short deadline.
#[cfg(unix)]
const SILENT: (&str, &[&str]) = ("sh", &["-c", "sleep 60"]);
/// A child that exits at once while a background sleep keeps its stdout open.
#[cfg(unix)]
const GRANDCHILD: (&str, &[&str]) = ("sh", &["-c", "sleep 30 & echo ok"]);
/// The same silent child, for a runner that starts its program through `env`.
#[cfg(unix)]
const ENV_SILENT: (&str, &[&str]) = ("env", &["sh", "-c", "sleep 60"]);
/// The same grandchild, for a runner that starts its program through `env`.
#[cfg(unix)]
const ENV_GRANDCHILD: (&str, &[&str]) = ("env", &["sh", "-c", "sleep 30 & echo ok"]);
/// The shell body of a manifest probe script that is silent past the deadline.
#[cfg(unix)]
const PROBE_SILENT: &str = "sleep 60";
/// The shell body of a manifest probe script that leaves a grandchild behind.
#[cfg(unix)]
const PROBE_GRANDCHILD: &str = "sleep 30 & echo ok";

#[cfg(windows)]
const SILENT: (&str, &[&str]) = ("powershell", &["-NoProfile", "-Command", "Start-Sleep 60"]);
#[cfg(windows)]
const GRANDCHILD: (&str, &[&str]) = (
    "cmd",
    &["/c", "start /b ping -n 30 127.0.0.1 >nul & echo ok"],
);
#[cfg(windows)]
const PROBE_SILENT: &str = "powershell -NoProfile -Command Start-Sleep 60";
#[cfg(windows)]
const PROBE_GRANDCHILD: &str = "start /b ping -n 30 127.0.0.1 >nul\r\necho ok";

/// A scratch directory unique to this run and test, empty on return.
fn scratch_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ae-remaining-bounds-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// Writes a manifest probe script whose body is `body`, executable on Unix.
fn write_probe(dir: &Path, body: &str) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("probe.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the probe");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod the probe");
        path
    }
    #[cfg(windows)]
    {
        let path = dir.join("probe.cmd");
        std::fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("write the probe");
        path
    }
}

// ---------------------------------------------------------------------------
// Container CLI runner (docker and podman share it)
// ---------------------------------------------------------------------------

#[test]
fn container_cli_kills_a_silent_child_past_its_deadline() {
    let (program, args) = SILENT;
    let (result, elapsed) = bounded("container_cli", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_container_cli(program, args, 1);
        (result, started.elapsed())
    });
    let err = result.expect_err("a silent child past its deadline must be refused");
    assert_code(&err, "E_BUDGET_EXCEEDED");
    assert!(
        elapsed < Duration::from_secs(30),
        "the deadline did not bound the call: {elapsed:?}"
    );
}

#[test]
fn container_cli_is_not_held_by_a_grandchild_that_keeps_the_pipe() {
    let (program, args) = GRANDCHILD;
    let (result, elapsed) = bounded("container_cli grandchild", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_container_cli(program, args, 30);
        (result, started.elapsed())
    });
    let text = result.expect("the child exited 0; its grandchild must not fail the call");
    assert!(text.contains("ok"), "the child's output was lost: {text:?}");
    assert!(
        elapsed < Duration::from_secs(30),
        "a grandchild held the call: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Checked tool runner (run_checked, behind run_tool)
// ---------------------------------------------------------------------------

#[test]
fn checked_tool_runner_kills_a_silent_child_past_its_deadline() {
    let (program, args) = SILENT;
    let (result, elapsed) = bounded("run_checked", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_run_checked(program, args, 1);
        (result, started.elapsed())
    });
    let err = result.expect_err("a silent child past its deadline must be refused");
    assert_code(&err, "E_BUDGET_EXCEEDED");
    assert!(
        elapsed < Duration::from_secs(30),
        "the deadline did not bound the call: {elapsed:?}"
    );
}

#[test]
fn checked_tool_runner_is_not_held_by_a_grandchild_that_keeps_the_pipe() {
    let (program, args) = GRANDCHILD;
    let (result, elapsed) = bounded("run_checked grandchild", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_run_checked(program, args, 30);
        (result, started.elapsed())
    });
    // The output stayed open after the child exited, so the run is refused as
    // a tool failure rather than answered in part; what matters is the bound.
    let err = result.expect_err("output held open after exit is refused");
    assert_code(&err, "E_TOOL_FAILED");
    assert!(
        elapsed < Duration::from_secs(30),
        "a grandchild held the call: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Package manager exec (Linux: it runs /usr/bin/<tool>)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[test]
fn pkg_exec_kills_a_silent_child_past_its_deadline() {
    let (result, elapsed) = bounded("pkg exec", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_pkg_exec(ENV_SILENT.0, ENV_SILENT.1, 1);
        (result, started.elapsed())
    });
    let err = result.expect_err("a silent child past its deadline must be refused");
    assert_code(&err, "E_BUDGET_EXCEEDED");
    assert!(
        elapsed < Duration::from_secs(30),
        "the deadline did not bound the call: {elapsed:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn pkg_exec_is_not_held_by_a_grandchild_that_keeps_the_pipe() {
    let (result, elapsed) = bounded("pkg exec grandchild", move || {
        let started = Instant::now();
        let result = aethershell::builtins::__test_pkg_exec(ENV_GRANDCHILD.0, ENV_GRANDCHILD.1, 30);
        (result, started.elapsed())
    });
    let output = result.expect("the child exited 0; its grandchild must not fail the call");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ok"),
        "the child's output was lost"
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "a grandchild held the call: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// External tool manifest probes (is_self_describing and the manifest load)
// ---------------------------------------------------------------------------

#[test]
fn manifest_probe_kills_a_silent_tool_past_its_deadline() {
    let dir = scratch_dir("probe-silent");
    let probe = write_probe(&dir, PROBE_SILENT);
    let (result, elapsed) = bounded("is_self_describing", move || {
        let started = Instant::now();
        let result = aethershell::external_tools::ExternalToolRegistry::is_self_describing(&probe);
        (result, started.elapsed())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let err = result.expect_err("a silent probe past its deadline must be refused");
    assert_code(&err, "E_BUDGET_EXCEEDED");
    assert!(
        elapsed < Duration::from_secs(40),
        "the probe deadline did not bound the call: {elapsed:?}"
    );
}

#[test]
fn manifest_probe_is_not_held_by_a_grandchild_that_keeps_the_pipe() {
    let dir = scratch_dir("probe-grandchild");
    let probe = write_probe(&dir, PROBE_GRANDCHILD);
    let (result, elapsed) = bounded("is_self_describing grandchild", move || {
        let started = Instant::now();
        let result = aethershell::external_tools::ExternalToolRegistry::is_self_describing(&probe);
        (result, started.elapsed())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let answered = result.expect("the probe exited 0; its grandchild must not fail it");
    assert!(answered, "a probe that exits 0 is self-describing");
    assert!(
        elapsed < Duration::from_secs(30),
        "a grandchild held the probe: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// test_watch: a run past timeout_ms is killed and reported, not left running
// ---------------------------------------------------------------------------

fn record_str(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn watch_options(timeout_ms: i64) -> Value {
    let mut opts = BTreeMap::new();
    opts.insert("max_runs".to_string(), Value::Int(1));
    opts.insert("timeout_ms".to_string(), Value::Int(timeout_ms));
    Value::Record(opts)
}

/// The first run of a `test_watch` record, as its field map.
fn first_run(result: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(rec) = result else {
        panic!("test_watch must return a Record, got {result:?}");
    };
    let Some(Value::Array(runs)) = rec.get("runs") else {
        panic!("test_watch record has no runs array: {rec:?}");
    };
    let Some(Value::Record(run)) = runs.first() else {
        panic!("test_watch made no run: {runs:?}");
    };
    run
}

#[test]
fn test_watch_kills_a_silent_run_past_its_timeout_and_reports_it() {
    let dir = scratch_dir("watch-silent");
    let watched = dir.to_string_lossy().into_owned();
    let command = Value::Array(
        std::iter::once(record_str(SILENT.0))
            .chain(SILENT.1.iter().map(|a| record_str(a)))
            .collect(),
    );
    let (result, elapsed) = bounded("test_watch", move || {
        let started = Instant::now();
        let mut env = Env::new();
        let result = aethershell::builtins::call(
            "test_watch",
            vec![
                command,
                Value::Array(vec![record_str(&watched)]),
                watch_options(500),
            ],
            &mut env,
        );
        (result, started.elapsed())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let value = result.expect("a run past timeout_ms is reported in the record, not raised");
    let run = first_run(&value);
    assert_eq!(
        run.get("timed_out"),
        Some(&Value::Bool(true)),
        "the silent run must be reported as timed out: {run:?}"
    );
    assert!(
        elapsed < Duration::from_secs(40),
        "the run timeout did not bound the call: {elapsed:?}"
    );
}

#[test]
fn test_watch_is_not_held_by_a_grandchild_that_keeps_the_pipe() {
    let dir = scratch_dir("watch-grandchild");
    let watched = dir.to_string_lossy().into_owned();
    let command = Value::Array(
        std::iter::once(record_str(GRANDCHILD.0))
            .chain(GRANDCHILD.1.iter().map(|a| record_str(a)))
            .collect(),
    );
    let (result, elapsed) = bounded("test_watch grandchild", move || {
        let started = Instant::now();
        let mut env = Env::new();
        let result = aethershell::builtins::call(
            "test_watch",
            vec![
                command,
                Value::Array(vec![record_str(&watched)]),
                watch_options(20_000),
            ],
            &mut env,
        );
        (result, started.elapsed())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let value = result.expect("the child exited 0; its grandchild must not hold the run");
    let run = first_run(&value);
    assert_eq!(run.get("timed_out"), Some(&Value::Bool(false)));
    assert!(
        elapsed < Duration::from_secs(30),
        "a grandchild held the run: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Legacy openssl decrypt: plaintext that is not UTF-8 is refused, not converted
// ---------------------------------------------------------------------------

/// Whether an openssl CLI is on PATH; the legacy decrypt tests need one.
#[cfg(unix)]
fn openssl_present() -> bool {
    std::process::Command::new("openssl")
        .arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Encrypts `plain` with the openssl CLI in the legacy format `crypto_decrypt`
/// reads. The bytes are written on a thread so the encryption cannot deadlock.
#[cfg(unix)]
fn openssl_encrypt_legacy(plain: &[u8], password: &str) -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("openssl")
        .args([
            "enc",
            "-aes-256-cbc",
            "-pbkdf2",
            "-e",
            "-base64",
            "-pass",
            "env:AE_TEST_LEGACY_PASS",
        ])
        .env("AE_TEST_LEGACY_PASS", password)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let data = plain.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(unix)]
#[test]
fn legacy_decrypt_refuses_plaintext_that_is_not_utf8() {
    if !openssl_present() {
        eprintln!("skipped: no openssl CLI on PATH");
        return;
    }
    let password = "remaining-bounds-test-password";
    // Bytes that are not UTF-8: a lossy conversion would turn each into U+FFFD.
    let plain: Vec<u8> = vec![b'a', 0xff, 0xfe, 0xc3, b'b'];
    let Some(ciphertext) = openssl_encrypt_legacy(&plain, password) else {
        eprintln!("skipped: openssl could not produce a legacy ciphertext here");
        return;
    };

    std::env::set_var("AETHER_CRYPTO_LEGACY_DECRYPT", "1");
    let got = bounded("legacy crypto_decrypt (not UTF-8)", move || {
        let mut env = Env::new();
        aethershell::builtins::call(
            "crypto_decrypt",
            vec![Value::Str(ciphertext), Value::Str(password.to_string())],
            &mut env,
        )
    });
    std::env::remove_var("AETHER_CRYPTO_LEGACY_DECRYPT");

    match got {
        Err(e) => assert_code(&e, "E_DECRYPT_FAILED"),
        Ok(value) => panic!("non-UTF-8 plaintext must not come back as a string: {value:?}"),
    }
}

#[cfg(unix)]
#[test]
fn legacy_decrypt_still_returns_utf8_plaintext_whole() {
    if !openssl_present() {
        eprintln!("skipped: no openssl CLI on PATH");
        return;
    }
    let password = "remaining-bounds-test-password";
    let plain = "caf\u{e9} and na\u{ef}ve text\n";
    let Some(ciphertext) = openssl_encrypt_legacy(plain.as_bytes(), password) else {
        eprintln!("skipped: openssl could not produce a legacy ciphertext here");
        return;
    };

    std::env::set_var("AETHER_CRYPTO_LEGACY_DECRYPT", "1");
    let got = bounded("legacy crypto_decrypt (UTF-8)", move || {
        let mut env = Env::new();
        aethershell::builtins::call(
            "crypto_decrypt",
            vec![Value::Str(ciphertext), Value::Str(password.to_string())],
            &mut env,
        )
    });
    std::env::remove_var("AETHER_CRYPTO_LEGACY_DECRYPT");

    match got {
        Ok(Value::Str(text)) => assert_eq!(text, plain),
        other => panic!("legacy decrypt of UTF-8 plaintext failed: {other:?}"),
    }
}
