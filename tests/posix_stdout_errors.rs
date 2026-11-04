//! A failed write to stdout in `ish -b` is reported and makes the run fail.
//!
//! The POSIX interpreter wrote each top-level command's output and dropped the
//! error, so `ish -b -c 'echo x' > /dev/full` exited 0 where bash exits 1. A
//! closed pipe is different: it is not reported, as `ish -b ... | head` must
//! not start printing errors.
//!
//! `/dev/full` exists only on Linux, so these run only there.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Session {
    root: PathBuf,
}

impl Session {
    fn new(tag: &str) -> Session {
        let root =
            std::env::temp_dir().join(format!("ae-posix-stdout-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("session")).unwrap();
        Session { root }
    }

    /// A `ish -b -c` command isolated from the user's session state.
    fn command(&self, src: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
        cmd.args(["-b", "-c", src])
            .current_dir(&self.root)
            .env("NO_COLOR", "1")
            .env("IRONSHELL_SESSION_DIR", self.root.join("session"));
        cmd
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn full_device() -> Stdio {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("open /dev/full");
    Stdio::from(file)
}

fn run_to_full_device(s: &Session, src: &str) -> Output {
    s.command(src)
        .stdout(full_device())
        .stderr(Stdio::piped())
        .output()
        .expect("run ish")
}

#[test]
fn a_write_to_a_full_device_is_a_failure() {
    let s = Session::new("full");
    let out = run_to_full_device(&s, "echo x");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("write error"), "stderr was: {err}");
}

#[test]
fn a_closed_pipe_is_not_reported() {
    let s = Session::new("pipe");
    let mut child = s
        .command("echo x")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ish");
    // Close the read end before the child writes. Whether the write then
    // reaches the closed pipe depends on timing, so the exit code is not
    // asserted; only the absence of the report is.
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("wait for ish");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("write error"), "stderr was: {err}");
}

#[test]
fn a_failed_write_keeps_a_command_s_own_status() {
    let s = Session::new("status");
    let out = run_to_full_device(&s, "echo x; exit 3");
    assert_eq!(out.status.code(), Some(3), "{out:?}");
}
