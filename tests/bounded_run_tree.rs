//! A bounded run ends at its deadline or at its child's exit, and never waits on
//! a pipe that a grandchild the child started still holds. At a deadline the
//! child's whole process tree is killed.
//!
//! The calls go through `os_tools::run_capped`, the public entry point of the
//! shared bounded runner (`builtins::run_tree`). Each call runs on its own thread
//! under a hard bound, so a regression fails the test rather than hanging the
//! suite.

use iron_shell::os_tools::{run_capped, ToolExecutionResult};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// No bounded call may take longer than this before the test gives up on it.
const HARD_BOUND: Duration = Duration::from_secs(30);

/// Runs `program` through the bounded runner on a thread. Returns the result and
/// how long the call took, or fails the test if it has not returned in
/// [`HARD_BOUND`].
fn bounded_call(
    program: &'static str,
    args: Vec<String>,
    deadline_secs: u64,
) -> (anyhow::Result<ToolExecutionResult>, Duration) {
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let result = run_capped(
            "tool_exec",
            "bounded_tree",
            program,
            &args,
            64 * 1024,
            64 * 1024,
            deadline_secs,
        );
        let _ = tx.send(result);
    });
    match rx.recv_timeout(HARD_BOUND) {
        Ok(result) => (result, started.elapsed()),
        Err(_) => panic!(
            "the bounded call did not return within {HARD_BOUND:?}: it is waiting on a pipe that a descendant holds"
        ),
    }
}

fn expect_ok(result: anyhow::Result<ToolExecutionResult>) -> ToolExecutionResult {
    match result {
        Ok(out) => out,
        Err(e) => panic!("the call should have finished, got: {e}"),
    }
}

fn expect_err(result: anyhow::Result<ToolExecutionResult>) -> anyhow::Error {
    match result {
        Ok(out) => panic!(
            "the call should have been refused, but it finished with exit {:?}",
            out.exit_code
        ),
        Err(e) => e,
    }
}

/// The coded error name in a message, such as `E_BUDGET_EXCEEDED`.
fn code_of(err: &anyhow::Error) -> String {
    let text = err.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

/// A fresh directory for the pid file of one test.
fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ironshell-bounded-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// The pid that a script wrote to `path`. Fails the test if none appears.
fn read_pid(path: &Path) -> u32 {
    let started = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                return pid;
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the child never wrote its pid to {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

/// Polls `done` until it is true or `limit` has passed.
fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if done() {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

// ─────────────────────────────────────────────────────────────────────────
// unix
// ─────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn sh(script: &str) -> Vec<String> {
    vec!["-c".to_string(), script.to_string()]
}

/// Whether the process `pid` is gone. A zombie has exited and only awaits
/// reaping, so it counts as gone.
#[cfg(unix)]
fn process_gone(pid: u32) -> bool {
    let pid = pid as i32;
    // SAFETY: signal 0 only probes whether the process exists; it sends nothing.
    let probe = unsafe { libc::kill(pid, 0) };
    if probe == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        return true;
    }
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('Z')),
        Err(_) => false,
    }
}

#[cfg(unix)]
#[test]
fn a_grandchild_holding_the_pipe_does_not_hold_the_call() {
    let (result, elapsed) = bounded_call("sh", sh("sleep 30 & echo started"), 60);
    let out = expect_ok(result);
    assert!(
        !out.stdout_truncated,
        "a pipe a grandchild still held is not output past the bound"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the call waited {elapsed:?} on a pipe that a grandchild held open"
    );
    assert!(out.success, "the child exited 0; stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("started"),
        "the output the child wrote before exiting is kept, got {:?}",
        out.stdout
    );
}

#[cfg(unix)]
#[test]
fn the_deadline_kills_every_process_the_child_started() {
    let dir = scratch_dir("unix-deadline");
    let pid_file = dir.join("grandchild.pid");
    let script = format!("sleep 60 & echo $! > '{}'; sleep 60", pid_file.display());
    let (result, elapsed) = bounded_call("sh", sh(&script), 2);
    let err = expect_err(result);
    assert_eq!(code_of(&err), "E_BUDGET_EXCEEDED", "message: {err}");
    assert!(
        elapsed < Duration::from_secs(8),
        "the deadline (2 s) should bound the call, it took {elapsed:?}"
    );
    let pid = read_pid(&pid_file);
    assert!(
        wait_until(Duration::from_secs(5), || process_gone(pid)),
        "the grandchild {pid} is still running after the deadline killed the call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ─────────────────────────────────────────────────────────────────────────
// windows
// ─────────────────────────────────────────────────────────────────────────

/// Whether a `ping.exe` with this pid is still in the process list.
#[cfg(windows)]
fn ping_alive(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let output = std::process::Command::new("tasklist")
        .args(["/FI", filter.as_str(), "/NH"])
        .output()
        .expect("tasklist runs on Windows");
    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains("ping.exe")
}

#[cfg(windows)]
#[test]
fn a_grandchild_holding_the_pipe_does_not_hold_the_call() {
    let script = "start /b ping -n 30 127.0.0.1 >nul & echo started";
    let (result, elapsed) = bounded_call("cmd", vec!["/c".to_string(), script.to_string()], 60);
    let out = expect_ok(result);
    assert!(
        !out.stdout_truncated,
        "a pipe a grandchild still held is not output past the bound"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the call waited {elapsed:?} on a pipe that a grandchild held open"
    );
    assert!(
        out.stdout.contains("started"),
        "the output the child wrote before exiting is kept, got {:?}",
        out.stdout
    );
}

#[cfg(windows)]
#[test]
fn the_deadline_kills_every_process_the_child_started() {
    let dir = scratch_dir("windows-deadline");
    let pid_file = dir.join("grandchild.pid");
    // PowerShell starts ping as its own child (no shell window, same pipes),
    // records its pid, and then waits past the deadline.
    let script = format!(
        "$si = New-Object System.Diagnostics.ProcessStartInfo 'ping.exe', '-n 60 127.0.0.1'; \
         $si.UseShellExecute = $false; $si.CreateNoWindow = $true; \
         $p = [System.Diagnostics.Process]::Start($si); \
         Set-Content -LiteralPath '{}' -Value $p.Id; Start-Sleep -Seconds 60",
        pid_file.display()
    );
    let args = vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        script,
    ];
    let (result, elapsed) = bounded_call("powershell", args, 15);
    let err = expect_err(result);
    assert_eq!(code_of(&err), "E_BUDGET_EXCEEDED", "message: {err}");
    assert!(
        elapsed < Duration::from_secs(25),
        "the deadline (15 s) should bound the call, it took {elapsed:?}"
    );
    let pid = read_pid(&pid_file);
    let gone = wait_until(Duration::from_secs(5), || !ping_alive(pid));
    if !gone {
        // Leave nothing behind from a failed run: this pid is the test's own.
        let pid_text = pid.to_string();
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", pid_text.as_str(), "/T", "/F"])
            .output();
    }
    assert!(
        gone,
        "the grandchild ping.exe ({pid}) is still running after the deadline killed the call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
