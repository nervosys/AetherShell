//! Version, availability and query probes outside `builtins.rs` must have a
//! deadline. A silent child past the deadline has to come back in its
//! failure shape, and a normal probe has to keep its value.
//!
//! Every call runs on a thread with a 60 s hard bound, so a regression that
//! hangs fails the test instead of hanging the run.

use aethershell::model_plane::vault_run_within;
use aethershell::providers::ontology::tool_version_within;
use aethershell::safety::output_with_timeout;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Run `f` on its own thread and return its result, or fail if it does not
/// return within 60 s.
fn within_hard_bound<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(60))
        .expect("probe did not return within the 60 s hard bound")
}

/// A command that prints `text` and exits, for a normal probe.
fn printing_command(text: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &format!("Write-Output '{text}'")]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("echo {text}")]);
        cmd
    }
}

/// A command that stays silent for 60 s, for a hung probe.
fn silent_command() -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", "Start-Sleep 60"]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 60"]);
        cmd
    }
}

/// A directory of its own for each call: tests run in parallel and each removes
/// its directory, so a shared one let a test delete another's fake tool mid-run
/// (on Windows `cmd` then printed "cannot find", which read as a version).
fn scratch_dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("probe-deadline-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Write an executable fake tool named `name` into `dir` whose body is
/// `body` (a shell script on unix, a batch file on Windows). Returns its path.
fn write_fake_tool(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
    #[cfg(windows)]
    let (file, text) = (format!("{name}.cmd"), format!("@echo off\r\n{body}\r\n"));
    #[cfg(not(windows))]
    let (file, text) = (name.to_string(), format!("#!/bin/sh\n{body}\n"));
    let path = dir.join(file);
    std::fs::write(&path, text).expect("write fake tool");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake tool");
    }
    path
}

#[test]
fn output_with_timeout_stops_a_silent_child_with_timed_out() {
    let err = within_hard_bound(|| output_with_timeout(silent_command(), 1))
        .expect_err("a silent child must not succeed past its deadline");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "got: {err}");
}

#[test]
fn output_with_timeout_keeps_a_normal_result() {
    let out = within_hard_bound(|| output_with_timeout(printing_command("probe-ok"), 30))
        .expect("a normal probe returns its output");
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("probe-ok"),
        "stdout was {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn tool_version_probe_that_hangs_reports_unknown_version() {
    let dir = scratch_dir();
    let tool = write_fake_tool(
        &dir,
        "hung-tool",
        if cfg!(windows) {
            "powershell -NoProfile -Command \"Start-Sleep 60\""
        } else {
            "sleep 60"
        },
    );
    let tool_str = tool.to_string_lossy().into_owned();
    let version = within_hard_bound(move || tool_version_within(&tool_str, 1));
    assert_eq!(version, None, "a hung --version probe reports no version");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tool_version_probe_keeps_its_value() {
    let dir = scratch_dir();
    let tool = write_fake_tool(
        &dir,
        "fake-tool",
        if cfg!(windows) {
            "echo fake-tool 1.2.3"
        } else {
            "echo 'fake-tool 1.2.3'"
        },
    );
    let tool_str = tool.to_string_lossy().into_owned();
    let version = within_hard_bound(move || tool_version_within(&tool_str, 30));
    assert_eq!(version.as_deref(), Some("fake-tool 1.2.3"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `vault_run_within` reads the vault binary from `IRONVAULT_BIN`. This is the
/// only test in this file that touches the environment, so it sets and
/// restores the variable itself.
#[test]
fn vault_query_that_hangs_is_stopped_and_reported() {
    let previous = std::env::var("IRONVAULT_BIN").ok();
    let (bin, hang_args): (&str, &[&str]) = if cfg!(windows) {
        ("powershell", &["-NoProfile", "-Command", "Start-Sleep 60"])
    } else {
        ("sh", &["-c", "sleep 60"])
    };
    std::env::set_var("IRONVAULT_BIN", bin);

    let hang: Vec<String> = hang_args.iter().map(|s| s.to_string()).collect();
    let result = within_hard_bound(move || {
        let refs: Vec<&str> = hang.iter().map(String::as_str).collect();
        vault_run_within(&refs, 1)
    });

    let ok_args: &[&str] = if cfg!(windows) {
        &["-NoProfile", "-Command", "Write-Output vault-ok"]
    } else {
        &["-c", "echo vault-ok"]
    };
    let ok: Vec<String> = ok_args.iter().map(|s| s.to_string()).collect();
    let normal = within_hard_bound(move || {
        let refs: Vec<&str> = ok.iter().map(String::as_str).collect();
        vault_run_within(&refs, 30)
    });

    match previous {
        Some(v) => std::env::set_var("IRONVAULT_BIN", v),
        None => std::env::remove_var("IRONVAULT_BIN"),
    }

    let err = result.expect_err("a hung vault query must fail");
    assert!(
        err.to_string().contains("did not finish within 1s"),
        "unexpected error: {err}"
    );
    let text = normal.expect("a normal vault query returns its output");
    assert!(text.contains("vault-ok"), "stdout was {text:?}");
}
