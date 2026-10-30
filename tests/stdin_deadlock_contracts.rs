//! Contracts for the three runners that once wrote a child's stdin synchronously
//! before draining its stdout, with no deadline: the legacy `crypto_decrypt`
//! path (openssl), the scheduler's tool runner (crontab, at, atrm, schtasks,
//! PowerShell) and the account tools' spawn helper (useradd and friends).
//!
//! A payload larger than the OS pipe buffer deadlocked those runners: the child
//! blocked writing its stdout while the parent blocked writing its stdin. All
//! three now go through one bounded runner that writes stdin on its own thread
//! and kills the process tree at a deadline. Each test here runs its call on a
//! thread with a hard 60 s bound, so a regression fails instead of hanging.
//!
//! Nothing here changes the host's schedules or accounts. The scheduler and
//! account tests feed a harmless program (`cat` on Unix, `more` and `ping` on
//! Windows) through the same helpers the builtins use.

#[cfg(unix)]
use iron_shell::env::Env;
#[cfg(unix)]
use iron_shell::value::Value;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Bytes in a payload that is larger than one MiB and than a typical pipe buffer.
const LARGE_PAYLOAD: usize = 1024 * 1024 + 4096;

/// Runs `f` on a thread and fails the test if it does not return within 60 s.
fn bounded<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(value) => value,
        Err(_) => {
            panic!("{what} did not return within 60 s: the stdin write and stdout read deadlocked")
        }
    }
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

#[cfg(unix)]
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

/// A payload of `LARGE_PAYLOAD` bytes made of short text lines.
fn large_text() -> String {
    let line = "the quick brown fox jumps over the lazy dog 0123456789\n";
    line.repeat(LARGE_PAYLOAD / line.len() + 1)
}

/// Encrypts with the openssl CLI in the legacy format `crypto_decrypt` reads.
/// The plaintext is written on a thread so the encryption cannot deadlock here.
#[cfg(unix)]
fn openssl_encrypt_legacy(plaintext: &str, password: &str) -> Option<String> {
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
    let data = plaintext.as_bytes().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Whether an openssl CLI is on PATH; the legacy test needs one.
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

/// Legacy `crypto_decrypt` (openssl) with a ciphertext over 1 MiB. Before the
/// fix the synchronous stdin write blocked while openssl blocked on its full
/// stdout pipe. Needs the openssl CLI and the legacy switch, so it is Unix-only
/// and skips when openssl is absent.
#[cfg(unix)]
#[test]
fn legacy_decrypt_of_a_payload_over_one_mib_round_trips() {
    if !openssl_present() {
        eprintln!("skipped: no openssl CLI on PATH");
        return;
    }
    let plaintext = large_text();
    let password = "stdin-deadlock-test-password";
    let Some(ciphertext) = openssl_encrypt_legacy(&plaintext, password) else {
        eprintln!("skipped: openssl could not produce a legacy ciphertext here");
        return;
    };
    assert!(
        ciphertext.len() > 1024 * 1024,
        "the ciphertext must exceed a pipe buffer to exercise the deadlock"
    );

    std::env::set_var("IRONSHELL_CRYPTO_LEGACY_DECRYPT", "1");
    let got = bounded("legacy crypto_decrypt", {
        let ciphertext = ciphertext.clone();
        move || {
            call(
                "crypto_decrypt",
                vec![Value::Str(ciphertext), Value::Str(password.to_string())],
            )
        }
    });
    std::env::remove_var("IRONSHELL_CRYPTO_LEGACY_DECRYPT");

    match got {
        Ok(Value::Str(text)) => assert!(
            text == plaintext,
            "the decrypted text differs from the plaintext ({} bytes back, {} expected)",
            text.len(),
            plaintext.len()
        ),
        other => panic!("legacy decrypt of a large payload failed: {other:?}"),
    }
}

/// Stdin the scheduler tool runner feeds to its child: a harmless program that
/// copies its input to its output.
#[cfg(unix)]
const COPY_TOOL: (&str, &[&str]) = ("cat", &[]);
/// `findstr /V` prints every line that matches nothing, so it copies stdin.
#[cfg(windows)]
const COPY_TOOL: (&str, &[&str]) = ("findstr", &["/V", "zzqqxx"]);

/// `total` bytes of short lines of `x`, so line-based copy tools keep every
/// byte. The count of `x` bytes is what the copy must preserve.
fn copy_payload(total: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(total + 1001);
    let line = [b'x'; 1000];
    while out.len() < total {
        out.extend_from_slice(&line);
        out.push(b'\n');
    }
    out.truncate(total);
    out
}

fn count_x(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'x').count()
}

/// The scheduler's tool runner with a payload over 1 MiB. The copy must come
/// back whole, and the call must return.
#[test]
fn scheduler_tool_runner_passes_a_payload_over_one_mib_through() {
    let payload = copy_payload(LARGE_PAYLOAD);
    let expected = count_x(&payload);
    let (tool, args) = COPY_TOOL;
    let got = bounded("scheduler run_tool", move || {
        iron_shell::builtins::__test_scheduler_run_tool(tool, args, Some(&payload))
    });
    let output = got.unwrap_or_else(|e| panic!("run_tool refused a large payload: {e}"));
    assert!(
        output.status.success(),
        "copy tool failed: {:?}",
        output.status
    );
    assert_eq!(
        count_x(&output.stdout),
        expected,
        "the copy tool did not return the whole payload"
    );
}

/// Output past the bound is a coded budget error, not a cut-short success.
#[test]
fn scheduler_tool_runner_refuses_output_past_its_bound() {
    let payload = copy_payload(17 * 1024 * 1024);
    let (tool, args) = COPY_TOOL;
    let got = bounded("scheduler run_tool", move || {
        iron_shell::builtins::__test_scheduler_run_tool(tool, args, Some(&payload))
    });
    match got {
        Err(e) => assert_eq!(code_of(&e), "E_BUDGET_EXCEEDED", "{e}"),
        Ok(out) => panic!(
            "17 MiB of output came back as a success of {} bytes",
            out.stdout.len()
        ),
    }
}

/// A tool that outlives the scheduler's deadline is killed and the call is a
/// coded budget error. Takes about 30 s. The tool is `sleep` on Unix and `ping`
/// on Windows, and the bound is 60 s.
#[test]
fn scheduler_tool_runner_kills_a_tool_past_its_deadline() {
    #[cfg(unix)]
    let (tool, args): (&str, &[&str]) = ("sleep", &["40"]);
    #[cfg(windows)]
    let (tool, args): (&str, &[&str]) = ("ping", &["-n", "40", "127.0.0.1"]);
    let started = Instant::now();
    let got = bounded("scheduler run_tool deadline", move || {
        iron_shell::builtins::__test_scheduler_run_tool(tool, args, None)
    });
    let elapsed = started.elapsed();
    match got {
        Err(e) => assert_eq!(code_of(&e), "E_BUDGET_EXCEEDED", "{e}"),
        Ok(_) => panic!("a tool that outlived its deadline returned a result"),
    }
    assert!(
        elapsed < Duration::from_secs(55),
        "the deadline did not bound the call: {elapsed:?}"
    );
}

/// The account tools' spawn helper on Linux: a payload over 1 MiB on stdin
/// comes back whole through `cat`.
#[cfg(target_os = "linux")]
#[test]
fn account_spawn_passes_a_payload_over_one_mib_through() {
    let payload = large_text();
    let expected = payload.clone();
    let got = bounded("account spawn", move || {
        iron_shell::builtins::__test_accounts_spawn("cat", &[], Some(&payload))
    });
    let output = got.unwrap_or_else(|e| panic!("account spawn refused a large payload: {e}"));
    assert!(output.status.success());
    assert_eq!(output.stdout, expected.as_bytes());
}

/// Off Linux the account tools are refused before anything runs.
#[cfg(not(target_os = "linux"))]
#[test]
fn account_spawn_is_refused_off_linux_with_a_large_payload() {
    let payload = large_text();
    let got = bounded("account spawn", move || {
        iron_shell::builtins::__test_accounts_spawn("cat", &[], Some(&payload))
    });
    match got {
        Err(e) => assert_eq!(code_of(&e), "E_UNIMPLEMENTED", "{e}"),
        Ok(_) => panic!("the account spawn ran off Linux"),
    }
}
