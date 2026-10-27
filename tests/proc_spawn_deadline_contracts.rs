//! Contracts for the `{timeout_ms}` option of `proc_spawn`.
//!
//! A malformed options Record is refused before anything is spawned: each
//! refusal names a program that does not exist, so a spawn that got past the
//! checks would answer E_TOOL_MISSING and not E_BAD_ARG.
//!
//! The deadline tests start children this file owns. Each one is stopped by a
//! guard whatever the test did, so a failed assertion cannot leave a `sleep`
//! or `ping` running. Every wait is bounded. Human mode allows the Process
//! effect, so these calls need no approval.

use aethershell::{builtins::call, env::Env, value::Value};
#[cfg(unix)]
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, sync::Mutex};
#[cfg(unix)]
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the environment this file needs. The session directory and the audit
/// log live in the temp directory, so no test touches the real ones.
fn sandbox() {
    for key in ["AETHER_MODE", "AETHER_AGENT", "AETHER_POLICY"] {
        std::env::remove_var(key);
    }
    let dir = std::env::temp_dir().join(format!("proc_spawn_deadline_{}", std::process::id()));
    std::env::set_var("AETHER_SESSION_DIR", &dir);
    let log = dir.join("audit.log");
    std::env::set_var("AETHER_AUDIT_LOG", log);
    std::env::set_var("AETHER_AUDIT_KEY", "33".repeat(32));
}

fn spawn(args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call("proc_spawn", args, &mut env)
}

fn wait(pid: i64, ms: i64) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call("proc_wait", vec![Value::Int(pid), Value::Int(ms)], &mut env)
}

fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => "none".to_string(),
    }
}

fn options(pairs: &[(&str, Value)]) -> Value {
    let mut rec = BTreeMap::new();
    for (k, v) in pairs {
        rec.insert((*k).to_string(), v.clone());
    }
    Value::Record(rec)
}

fn timeout(ms: i64) -> Value {
    options(&[("timeout_ms", Value::Int(ms))])
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// Stops a pid this file spawned, and its group on Unix, if it is still
/// there. Only used as cleanup: the assertions are made through proc_wait.
struct Stop(i64);

impl Drop for Stop {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let native = libc::pid_t::try_from(self.0).unwrap_or(libc::pid_t::MAX);
            // SAFETY: kill(2) takes a pid and a signal number and touches no memory.
            unsafe {
                libc::kill(-native, libc::SIGKILL);
                libc::kill(native, libc::SIGKILL);
            };
            // SAFETY: waitpid writes only to the status it is given, if any.
            unsafe { libc::waitpid(native, std::ptr::null_mut(), libc::WNOHANG) };
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/T", "/F", "/PID", &self.0.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

/// Whether a pid names a running process. A zombie has ended.
#[cfg(unix)]
fn running(pid: u32) -> bool {
    let spid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[spid]), true);
    match sys.process(spid) {
        Some(p) => p.status() != ProcessStatus::Zombie,
        None => false,
    }
}

/// The running children of `parent` named `name`, read from the process table.
#[cfg(unix)]
fn children_named(parent: u32, name: &str) -> Vec<u32> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .filter(|(_, p)| {
            p.parent().map(|pp| pp.as_u32()) == Some(parent)
                && p.name().to_string_lossy() == name
                && p.status() != ProcessStatus::Zombie
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Polls `done` every 20 ms for at most `limit`.
#[cfg(unix)]
fn within(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

// ── (a) malformed options are refused before any spawn ────────────────────

#[test]
fn malformed_options_are_refused_before_anything_is_spawned() {
    let _g = lock();
    sandbox();
    // The program does not exist, so a call that reached the spawn would be
    // E_TOOL_MISSING. Each of these must be E_BAD_ARG instead.
    let missing = "ae-no-such-program-spawn-deadline";
    let refused = [
        vec![s(missing), timeout(0)],
        vec![s(missing), timeout(-1)],
        vec![s(missing), timeout(86_400_001)],
        vec![s(missing), options(&[("timeout_ms", s("1"))])],
        vec![s(missing), options(&[("timeout_ms", Value::Bool(true))])],
        vec![s(missing), options(&[("nope", Value::Int(1))])],
        vec![
            s(missing),
            options(&[("timeout_ms", Value::Int(1)), ("nope", Value::Int(1))]),
        ],
        // A Record that is not last is an argument, and an argument must be a String.
        vec![
            s(missing),
            options(&[("timeout_ms", Value::Int(1))]),
            s("x"),
        ],
        // A Record alone is not an options record: there is no program before it.
        vec![options(&[("timeout_ms", Value::Int(1))])],
    ];
    for args in refused {
        let e = spawn(args.clone()).expect_err("a malformed call must be refused");
        assert_eq!(code_of(&e), "E_BAD_ARG", "args {args:?}: {e}");
    }
}

#[test]
fn an_empty_options_record_is_the_plain_spawn() {
    let _g = lock();
    sandbox();
    // A program that exits at once and is on every host.
    #[cfg(windows)]
    let program = vec![s("cmd"), s("/C"), s("exit"), s("0")];
    #[cfg(not(windows))]
    let program = vec![s("true")];
    let mut args = program;
    args.push(options(&[]));
    let v = spawn(args).expect("an empty options record spawns");
    let Value::Int(pid) = v else {
        panic!("proc_spawn answers an Int pid, got {v:?}");
    };
    let _stop = Stop(pid);
    assert!(matches!(wait(pid, 5000), Ok(Value::Bool(true))));
}

// ── (b) the deadline stops a child that would run on ──────────────────────

#[cfg(unix)]
#[test]
fn a_deadline_stops_a_sleeping_child_and_proc_wait_reports_it_ended() {
    let _g = lock();
    sandbox();
    let started = Instant::now();
    let v = spawn(vec![s("sleep"), s("30"), timeout(500)]).expect("spawn with a deadline");
    let Value::Int(pid) = v else {
        panic!("proc_spawn answers an Int pid, got {v:?}");
    };
    let _stop = Stop(pid);
    let ended = wait(pid, 5000).expect("proc_wait");
    assert!(matches!(ended, Value::Bool(true)), "{ended:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the child was stopped late: {:?}",
        started.elapsed()
    );
}

// ── (c) the whole process group is stopped, not only the leader ───────────

#[cfg(unix)]
#[test]
fn a_deadline_stops_the_shell_and_the_child_it_started() {
    let _g = lock();
    sandbox();
    let v = spawn(vec![s("sh"), s("-c"), s("sleep 30 & wait"), timeout(1000)])
        .expect("spawn sh with a deadline");
    let Value::Int(pid) = v else {
        panic!("proc_spawn answers an Int pid, got {v:?}");
    };
    let _stop = Stop(pid);

    // Find the sleep before the deadline: the shell forks it at once.
    let shell = u32::try_from(pid).expect("pid fits u32");
    let mut sleeper = None;
    let found_by = Instant::now() + Duration::from_millis(800);
    while Instant::now() < found_by && sleeper.is_none() {
        sleeper = children_named(shell, "sleep").into_iter().next();
        if sleeper.is_none() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let sleeper = sleeper.expect("the shell forked a sleep before its deadline");
    let _stop_sleep = Stop(i64::from(sleeper));

    // The shell is the group leader, so proc_wait reaps it and reports it ended.
    let ended = wait(pid, 5000).expect("proc_wait on the shell");
    assert!(matches!(ended, Value::Bool(true)), "{ended:?}");

    // Its sleep was in the group and is stopped too. A zombie has ended.
    let gone = within(Duration::from_secs(5), || !running(sleeper));
    assert!(gone, "the sleep in the shell's group is still running");
}

// ── (d) a child that ends before its deadline is not touched ──────────────

#[cfg(unix)]
#[test]
fn a_child_that_ends_before_its_deadline_is_left_alone() {
    let _g = lock();
    sandbox();
    let started = Instant::now();
    let v = spawn(vec![s("true"), timeout(2000)]).expect("spawn with a deadline");
    let Value::Int(pid) = v else {
        panic!("proc_spawn answers an Int pid, got {v:?}");
    };
    let _stop = Stop(pid);
    let ended = wait(pid, 5000).expect("proc_wait");
    assert!(matches!(ended, Value::Bool(true)), "{ended:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "a child that ended on its own was held until the deadline"
    );
}

// ── (e) Windows: taskkill /T /F stops the process and its tree ────────────

#[cfg(windows)]
#[test]
fn a_deadline_stops_a_ping_on_windows() {
    let _g = lock();
    sandbox();
    let v = spawn(vec![
        s("ping"),
        s("-n"),
        s("30"),
        s("127.0.0.1"),
        timeout(500),
    ])
    .expect("spawn with a deadline");
    let Value::Int(pid) = v else {
        panic!("proc_spawn answers an Int pid, got {v:?}");
    };
    let _stop = Stop(pid);
    let ended = wait(pid, 5000).expect("proc_wait");
    assert!(matches!(ended, Value::Bool(true)), "{ended:?}");
}
