//! `lsof PATH` on Windows, through the Restart Manager.
//!
//! Read-only and local: nothing is written outside a temp directory, no service
//! is touched, and the only child process is a PowerShell that holds one temp
//! file open and is killed by the test. Each test is in-process and isolates
//! AETHER_SESSION_DIR, as tests/web_tranche_contracts.rs does.

#![cfg(windows)]

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, Once};

// ── in-process calls ────────────────────────────────────────────────────

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_lsof_path_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
}

/// `AETHER_MODE` is process-global. Every in-process call takes this lock so
/// that no call sees another test's mode.
static MODE_LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Human mode, whatever the environment says.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// The first `E_...` code in an error's rendered text.
fn code_of(r: &anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => panic!("expected a coded refusal, got {v:?}"),
        Err(e) => {
            let msg = format!("{e:#}");
            let at = msg
                .find("E_")
                .unwrap_or_else(|| panic!("no code in: {msg}"));
            msg[at..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect()
        }
    }
}

/// The `pid` of every row lsof returned, as the strings it reports.
fn holder_pids(r: &anyhow::Result<Value>) -> Vec<String> {
    let rows = match r {
        Ok(Value::Array(rows)) => rows,
        other => panic!("expected an array of holders, got {other:?}"),
    };
    rows.iter()
        .filter_map(|row| match row {
            Value::Record(rec) => match rec.get("pid") {
                Some(Value::Str(pid)) => Some(pid.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// A fresh temp directory for one test, removed by the test when it is done.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_lsof_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch directory");
    dir
}

fn as_text(path: &Path) -> String {
    path.to_str().expect("temp path is UTF-8").to_string()
}

/// A child that holds a file open, killed when the guard is dropped, so a failed
/// assertion never leaves it running.
struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ── tests ───────────────────────────────────────────────────────────────

/// (a) A file this test process holds open lists our pid; once the handle is
/// dropped, our pid is no longer listed.
#[test]
fn a_file_this_process_holds_open_lists_our_pid_until_it_is_closed() {
    let dir = scratch("self");
    let path = dir.join("held.txt");
    std::fs::write(&path, b"held by the test").expect("write temp file");
    let our_pid = std::process::id().to_string();

    let handle = std::fs::File::open(&path).expect("open temp file");
    let held = call("lsof", vec![text(&as_text(&path))]);
    assert!(
        holder_pids(&held).contains(&our_pid),
        "lsof should list pid {our_pid} while its handle is open, got {:?}",
        holder_pids(&held)
    );

    drop(handle);
    let released = call("lsof", vec![text(&as_text(&path))]);
    assert!(
        !holder_pids(&released).contains(&our_pid),
        "lsof should not list pid {our_pid} after the handle is closed, got {:?}",
        holder_pids(&released)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (b) A path that does not exist is E_NOT_FOUND.
#[test]
fn a_missing_path_is_not_found() {
    let dir = scratch("missing");
    let path = dir.join("no-such-file.txt");
    let r = call("lsof", vec![text(&as_text(&path))]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");
    let _ = std::fs::remove_dir_all(&dir);
}

/// (c) A child process that holds the file open is listed by its pid, and the
/// path is passed to the child through an environment variable, not interpolated
/// into the command. The child reports on stdout once the file is open.
#[test]
fn a_child_process_holding_the_file_is_listed_by_its_pid() {
    let dir = scratch("child");
    let path = dir.join("child-held.txt");
    std::fs::write(&path, b"held by a child").expect("write temp file");

    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$f = [IO.File]::Open($env:AE_LSOF_TARGET, 'Open', 'Read', 'ReadWrite'); \
             [Console]::Out.WriteLine('held'); [Console]::Out.Flush(); \
             Start-Sleep -Seconds 120",
        ])
        .env("AE_LSOF_TARGET", as_text(&path))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn powershell");
    let mut holder = Holder(child);
    let child_pid = holder.0.id().to_string();

    // The child writes "held" only after its Open has succeeded. EOF before that
    // line means the child failed, and the test fails rather than waits.
    let stdout = holder.0.stdout.take().expect("child stdout is piped");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let n = reader.read_line(&mut line).expect("read child stdout");
    assert!(
        n > 0 && line.trim() == "held",
        "child did not open the file: {line:?}"
    );

    let rows = call("lsof", vec![text(&as_text(&path))]);
    assert!(
        holder_pids(&rows).contains(&child_pid),
        "lsof should list the child pid {child_pid}, got {:?}",
        holder_pids(&rows)
    );
    // The command is the image name without .exe, as lsof's socket form
    // reports it, not the Restart Manager's description of the program.
    let command = match &rows {
        Ok(Value::Array(all)) => all.iter().find_map(|row| match row {
            Value::Record(rec) if rec.get("pid") == Some(&Value::Str(child_pid.clone())) => {
                rec.get("command").cloned()
            }
            _ => None,
        }),
        _ => None,
    };
    assert_eq!(
        command,
        Some(Value::Str("powershell".to_string())),
        "{rows:?}"
    );

    drop(holder);
    let _ = std::fs::remove_dir_all(&dir);
}

/// (d) A directory is refused with E_BAD_ARG: the Restart Manager lists files only.
#[test]
fn a_directory_is_refused_because_the_restart_manager_lists_files_only() {
    let dir = scratch("dir");
    let r = call("lsof", vec![text(&as_text(&dir))]);
    assert_eq!(code_of(&r), "E_BAD_ARG");
    let _ = std::fs::remove_dir_all(&dir);
}
