//! Windows proc_suspend and proc_resume, against a child this test starts.
//!
//! The child is a PowerShell loop that writes a counter to a temp file every
//! 100 ms. Suspension is proved by the counter: it stops moving while the
//! process is suspended and moves again after proc_resume. Every child is
//! killed and reaped by a Drop guard, including on panic.
//!
//! Pids the tests name by value are refusals checked before any effect:
//! 2_000_000_000 cannot be live, 4 is the System process (refused by the
//! operating system), 0 and -1 are refused by value, and this test process's
//! own pid is refused by proc_refuse_self.

#![cfg(windows)]

use iron_shell::builtins::call;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A pid no live process holds.
const NOT_LIVE: i64 = 2_000_000_000;

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

fn own_pid() -> i64 {
    i64::from(std::process::id())
}

/// A PowerShell child that writes an increasing counter to a file every
/// 100 ms. Killed, reaped and its file removed when dropped.
struct Counter {
    child: Child,
    file: PathBuf,
}

impl Counter {
    fn start() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let file =
            std::env::temp_dir().join(format!("procsusp-{}-{nanos}.txt", std::process::id()));
        // The path is this test's own temp path; quotes are doubled anyway.
        let quoted = file.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$i=0; while ($true) {{ $i++; Set-Content -Path '{quoted}' -Value $i; Start-Sleep -Milliseconds 100 }}"
        );
        let child = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the counter child");
        Counter { child, file }
    }

    fn pid(&self) -> i64 {
        i64::from(self.child.id())
    }

    /// The counter now, or None while the file is missing or mid-write.
    fn read(&self) -> Option<i64> {
        std::fs::read_to_string(&self.file)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// The counter, retrying through a transient read during a write.
    fn value(&self) -> i64 {
        for _ in 0..50 {
            if let Some(n) = self.read() {
                return n;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "the counter file {} never became readable",
            self.file.display()
        );
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Polls `f` until it holds, or panics naming `what` after `limit`.
fn wait_until(what: &str, limit: Duration, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{what} did not happen within {limit:?}");
}

#[test]
fn suspend_freezes_the_counter_and_resume_lets_it_advance() {
    let counter = Counter::start();
    let pid = counter.pid();
    // The child must be running before it can be suspended. PowerShell
    // takes a moment to start, so wait for the counter to pass 2.
    wait_until("the counter starts", Duration::from_secs(30), || {
        counter.read().is_some_and(|n| n >= 2)
    });

    assert_eq!(
        run("proc_suspend", vec![Value::Int(pid)]).expect("proc_suspend"),
        Value::Bool(true),
        "proc_suspend should answer true"
    );
    // A write that was in flight when the suspend landed may still finish;
    // sample only after it has.
    std::thread::sleep(Duration::from_millis(300));
    let frozen = counter.value();
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        counter.value(),
        frozen,
        "the counter moved while process {pid} was suspended"
    );

    assert_eq!(
        run("proc_resume", vec![Value::Int(pid)]).expect("proc_resume"),
        Value::Bool(true),
        "proc_resume should answer true"
    );
    wait_until(
        "the counter advances after proc_resume",
        Duration::from_secs(3),
        || counter.read().is_some_and(|n| n > frozen),
    );
}

#[test]
fn refusals_are_coded_and_the_child_keeps_running() {
    let counter = Counter::start();
    let pid = counter.pid();
    wait_until("the counter starts", Duration::from_secs(30), || {
        counter.read().is_some_and(|n| n >= 1)
    });

    expect_code("proc_suspend", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code("proc_resume", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    // The System process: the operating system refuses PROCESS_SUSPEND_RESUME.
    expect_code("proc_suspend", vec![Value::Int(4)], "E_OS_DENIED");
    expect_code("proc_resume", vec![Value::Int(4)], "E_OS_DENIED");
    expect_code("proc_suspend", vec![Value::Int(own_pid())], "E_BAD_ARG");
    expect_code("proc_resume", vec![Value::Int(own_pid())], "E_BAD_ARG");
    expect_code("proc_suspend", vec![Value::Int(0)], "E_BAD_ARG");
    expect_code("proc_suspend", vec![Value::Int(-1)], "E_BAD_ARG");
    expect_code("proc_suspend", vec![Value::Str("abc".into())], "E_BAD_ARG");
    expect_code("proc_suspend", vec![], "E_BAD_ARG");
    expect_code(
        "proc_suspend",
        vec![Value::Int(pid), Value::Int(pid)],
        "E_BAD_ARG",
    );

    // No refusal sent a signal: the counter still advances.
    let before = counter.value();
    wait_until("the counter still advances", Duration::from_secs(5), || {
        counter.read().is_some_and(|n| n > before)
    });
}
