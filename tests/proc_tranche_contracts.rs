//! Contracts for the proc_* builtins.
//!
//! Every process these tests act on is one the test itself spawned, a
//! `sleep` or `ping` that is killed in its guard's `Drop`. No test names a pid
//! it did not start, except refusals, which are checked before any effect and
//! use pids that cannot be live (2_000_000_000) or are refused by value (0,
//! -1, this test process's own pid for the mutators).
//!
//! What runs where:
//! - Ordinary tests run on every OS: argument refusals, pipeline-input
//!   refusals, coded not-found answers, and readers checked against the
//!   child's and this process's own facts.
//! - Linux tests that change a process's state (suspend, resume, nice) are
//!   `#[ignore]`d and call `disposable::require()`, run by
//!   scripts/disposable_linux_tests.sh.
//! - Windows suspend and resume are in tests/proc_suspend_windows.rs.
//!   Priority is read back through proc_priority, which reads Get-Process; no
//!   independent Windows check is used for it.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

/// A pid no live process holds on any supported OS.
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

fn run_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call_with_input(name, args, Some(input), &mut env)
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

/// A process this test starts and kills. It sleeps long enough for any
/// assertion, and is killed and reaped when the guard drops.
struct Sleeper(std::process::Child);

impl Sleeper {
    fn start() -> Self {
        Self::start_for(120)
    }

    /// A child that ends by itself after about `secs` seconds.
    fn start_for(secs: u32) -> Self {
        use std::process::{Command, Stdio};
        #[cfg(windows)]
        let child = Command::new("ping")
            .args(["-n", &secs.to_string(), "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("ping starts");
        #[cfg(not(windows))]
        let child = Command::new("sleep")
            .arg(secs.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep starts");
        Sleeper(child)
    }

    fn pid(&mut self) -> i64 {
        i64::from(self.0.id())
    }

    fn value(&mut self) -> Value {
        Value::Int(self.pid())
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Every builtin that takes one pid and reads it. Each answers E_NOT_FOUND for
/// a pid that is not live.
const READERS: &[&str] = &[
    "proc_children",
    "proc_parent",
    "proc_priority",
    "proc_cpu_usage",
    "proc_mem_usage",
    "proc_threads",
    "proc_env",
    "proc_cwd",
    "proc_exe",
    "proc_cmdline",
    "proc_start_time",
];

/// Every builtin that takes a pid, readers and actors alike. Each refuses a
/// pipeline subject and surplus arguments before any effect.
const ALL_PID: &[&str] = &[
    "proc_children",
    "proc_parent",
    "proc_priority",
    "proc_cpu_usage",
    "proc_mem_usage",
    "proc_threads",
    "proc_env",
    "proc_cwd",
    "proc_exe",
    "proc_cmdline",
    "proc_start_time",
    "proc_wait",
    "proc_suspend",
    "proc_resume",
    "proc_set_priority",
];

// ── refusals that never reach the OS ───────────────────────────────────────

#[test]
fn every_pid_builtin_refuses_a_pipeline_subject() {
    for name in ALL_PID {
        let args = match *name {
            "proc_set_priority" => vec![Value::Int(NOT_LIVE), Value::Int(5)],
            _ => vec![Value::Int(NOT_LIVE)],
        };
        let e = run_piped(name, args, Value::Int(1)).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name} took a piped subject: {e}");
    }
    let e = run_piped("proc_list", vec![], Value::Int(1)).expect_err("proc_list");
    assert_eq!(
        code_of(&e),
        "E_BAD_ARG",
        "proc_list took a piped subject: {e}"
    );
    let e = run_piped(
        "proc_spawn",
        vec![Value::Str("hostname".into())],
        Value::Int(1),
    )
    .expect_err("proc_spawn");
    assert_eq!(
        code_of(&e),
        "E_BAD_ARG",
        "proc_spawn took a piped subject: {e}"
    );
}

#[test]
fn every_pid_builtin_refuses_surplus_arguments() {
    for name in ALL_PID {
        let mut args = vec![Value::Int(NOT_LIVE)];
        if *name == "proc_set_priority" {
            args.push(Value::Int(5));
        }
        args.push(Value::Int(1));
        args.push(Value::Int(2));
        let e = run(name, args).expect_err(name);
        assert_eq!(
            code_of(&e),
            "E_BAD_ARG",
            "{name} took surplus arguments: {e}"
        );
    }
    let e = run("proc_list", vec![Value::Int(1)]).expect_err("proc_list");
    assert_eq!(code_of(&e), "E_BAD_ARG", "proc_list took an argument: {e}");
}

#[test]
fn wrongly_typed_pids_are_refused_not_defaulted() {
    let odd = [
        Value::Bool(true),
        Value::Array(vec![]),
        Value::Record(Default::default()),
        Value::Str("firefox".into()),
    ];
    for name in ALL_PID {
        for v in &odd {
            let mut args = vec![v.clone()];
            if *name == "proc_set_priority" {
                args.push(Value::Int(5));
            }
            let e = run(name, args).expect_err(name);
            assert_eq!(code_of(&e), "E_BAD_ARG", "{name}({v:?}): {e}");
        }
        let e = run(name, vec![]).expect_err(name);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}(): {e}");
    }
}

#[test]
fn a_pid_that_is_not_positive_or_does_not_fit_is_refused() {
    for name in ALL_PID {
        for bad in [0_i64, -1, 4_294_967_297] {
            let mut args = vec![Value::Int(bad)];
            if *name == "proc_set_priority" {
                args.push(Value::Int(5));
            }
            let e = run(name, args).expect_err(name);
            assert_eq!(code_of(&e), "E_BAD_ARG", "{name}({bad}): {e}");
        }
    }
}

#[test]
fn a_nice_value_outside_minus_twenty_to_nineteen_is_refused_before_any_effect() {
    for bad in [20_i64, -21, 1_000_000] {
        let e = run(
            "proc_set_priority",
            vec![Value::Int(NOT_LIVE), Value::Int(bad)],
        )
        .expect_err("proc_set_priority");
        assert_eq!(code_of(&e), "E_BAD_ARG", "priority {bad}: {e}");
    }
}

#[test]
fn a_wait_timeout_must_be_a_non_negative_int() {
    let e = run("proc_wait", vec![Value::Int(own_pid()), Value::Int(-5)]).expect_err("wait");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");
    let e = run(
        "proc_wait",
        vec![Value::Int(own_pid()), Value::Str("soon".into())],
    )
    .expect_err("wait");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");
}

#[test]
fn this_shell_refuses_to_suspend_or_resume_itself() {
    expect_code("proc_suspend", vec![Value::Int(own_pid())], "E_BAD_ARG");
    expect_code("proc_resume", vec![Value::Int(own_pid())], "E_BAD_ARG");
}

#[test]
fn spawn_refuses_a_non_string_argument_instead_of_dropping_it() {
    expect_code(
        "proc_spawn",
        vec![Value::Str("hostname".into()), Value::Int(5)],
        "E_BAD_ARG",
    );
    expect_code("proc_spawn", vec![], "E_BAD_ARG");
    expect_code("proc_spawn", vec![Value::Int(5)], "E_BAD_ARG");
    expect_code(
        "proc_spawn",
        vec![Value::Str("hostname".into()), Value::Str("a\0b".into())],
        "E_BAD_ARG",
    );
}

#[test]
fn a_program_that_does_not_exist_is_tool_missing() {
    expect_code(
        "proc_spawn",
        vec![Value::Str("ae-no-such-program-7f3a9c".into())],
        "E_TOOL_MISSING",
    );
}

// ── a pid that is not live is one coded answer, never a value ─────────────

#[test]
fn a_pid_that_is_not_live_is_not_found_for_every_reader() {
    for name in READERS {
        expect_code(name, vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    }
}

#[test]
fn a_pid_that_is_not_live_is_not_found_for_every_actor() {
    expect_code("proc_wait", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code("proc_suspend", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code("proc_resume", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code(
        "proc_set_priority",
        vec![Value::Int(NOT_LIVE), Value::Int(5)],
        "E_NOT_FOUND",
    );
}

#[test]
fn the_declared_refusals_match_the_bodies() {
    // The same answers the declarations state, checked here without the
    // declaration tier, so a drift in either one fails.
    expect_code("proc_priority", vec![Value::Int(0)], "E_BAD_ARG");
    expect_code("proc_env", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
    expect_code("proc_start_time", vec![Value::Int(NOT_LIVE)], "E_NOT_FOUND");
}

// ── readers answer true facts about this process ──────────────────────────

#[test]
fn proc_list_names_this_process() {
    let list = run("proc_list", vec![]).expect("proc_list");
    let Value::Array(rows) = list else {
        panic!("proc_list did not answer an array");
    };
    assert!(!rows.is_empty(), "proc_list answered no processes");
    let found = rows.iter().any(|row| match row {
        Value::Record(r) => r.get("pid") == Some(&Value::Int(own_pid())),
        _ => false,
    });
    assert!(
        found,
        "proc_list does not list this process ({})",
        own_pid()
    );
}

#[test]
fn proc_env_of_this_process_sees_this_process_environment() {
    std::env::set_var("AE_PROC_TRANCHE_MARK", "seen");
    let env = run("proc_env", vec![Value::Int(own_pid())]).expect("proc_env");
    let Value::Record(vars) = env else {
        panic!("proc_env did not answer a record");
    };
    assert_eq!(
        vars.get("AE_PROC_TRANCHE_MARK"),
        Some(&Value::Str("seen".into())),
        "proc_env of this process lacks a variable it set"
    );
}

#[test]
fn proc_cwd_of_this_process_is_its_directory() {
    let cwd = run("proc_cwd", vec![Value::Int(own_pid())]).expect("proc_cwd");
    let here = std::env::current_dir().expect("current dir");
    let Value::Str(got) = cwd else {
        panic!("proc_cwd did not answer a string");
    };
    let got = std::path::PathBuf::from(got);
    // Canonical forms differ in spelling on Windows (the \\?\ prefix, 8.3
    // names), so compare what they name.
    assert_eq!(
        std::fs::canonicalize(&got).ok(),
        std::fs::canonicalize(&here).ok(),
        "proc_cwd {got:?} is not the current directory {here:?}"
    );
}

#[test]
fn proc_start_time_is_utc_iso_8601_to_the_second() {
    let v = run("proc_start_time", vec![Value::Int(own_pid())]).expect("proc_start_time");
    let Value::Str(s) = v else {
        panic!("proc_start_time did not answer a string");
    };
    let b = s.as_bytes();
    let shape = b.len() == 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'Z'
        && s.bytes()
            .enumerate()
            .filter(|(i, _)| ![4, 7, 10, 13, 16, 19].contains(i))
            .all(|(_, c)| c.is_ascii_digit());
    assert!(shape, "proc_start_time {s:?} is not YYYY-MM-DDTHH:MM:SSZ");
    assert!(
        s.starts_with("20"),
        "proc_start_time {s:?} is not a plausible date"
    );
}

#[test]
fn proc_parent_of_this_process_is_an_int_or_null() {
    let v = run("proc_parent", vec![Value::Int(own_pid())]).expect("proc_parent");
    assert!(
        matches!(v, Value::Int(_) | Value::Null),
        "proc_parent answered {v:?}"
    );
}

#[test]
fn proc_priority_of_this_process_is_a_nice_record() {
    let v = run("proc_priority", vec![Value::Int(own_pid())]).expect("proc_priority");
    let Value::Record(r) = v else {
        panic!("proc_priority did not answer a record");
    };
    match r.get("nice") {
        Some(Value::Int(n)) => assert!((-20..=19).contains(n), "nice {n} out of range"),
        other => panic!("nice is {other:?}"),
    }
    assert!(matches!(r.get("class"), Some(Value::Str(c)) if !c.is_empty()));
}

#[test]
fn proc_mem_usage_and_threads_of_this_process_are_positive() {
    let mem = run("proc_mem_usage", vec![Value::Int(own_pid())]).expect("proc_mem_usage");
    assert!(
        matches!(mem, Value::Int(n) if n > 0),
        "proc_mem_usage answered {mem:?}"
    );
    let threads = run("proc_threads", vec![Value::Int(own_pid())]).expect("proc_threads");
    assert!(
        matches!(threads, Value::Int(n) if n >= 1),
        "proc_threads answered {threads:?}"
    );
}

#[test]
fn proc_cpu_usage_of_this_process_is_a_float() {
    let v = run("proc_cpu_usage", vec![Value::Int(own_pid())]).expect("proc_cpu_usage");
    assert!(
        matches!(v, Value::Float(f) if f >= 0.0),
        "proc_cpu_usage answered {v:?}"
    );
}

// ── readers checked against a child this test started ─────────────────────

#[test]
fn a_child_has_this_process_as_its_parent_and_is_listed_as_its_child() {
    let mut child = Sleeper::start();
    let pid = child.pid();
    let parent = run("proc_parent", vec![child.value()]).expect("proc_parent");
    assert_eq!(parent, Value::Int(own_pid()), "parent of child {pid}");
    let kids = run("proc_children", vec![Value::Int(own_pid())]).expect("proc_children");
    let Value::Array(kids) = kids else {
        panic!("proc_children did not answer an array");
    };
    assert!(
        kids.contains(&Value::Int(pid)),
        "child {pid} not listed in {kids:?}"
    );
}

#[test]
fn a_child_with_no_children_answers_an_empty_array() {
    let mut child = Sleeper::start();
    let kids = run("proc_children", vec![child.value()]).expect("proc_children");
    assert_eq!(kids, Value::Array(vec![]), "child {}", child.pid());
}

#[test]
fn a_child_has_its_own_command_line_and_program() {
    let mut child = Sleeper::start();
    let argv = run("proc_cmdline", vec![child.value()]).expect("proc_cmdline");
    let Value::Array(argv) = argv else {
        panic!("proc_cmdline did not answer an array");
    };
    let words: Vec<String> = argv
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            other => panic!("argument {other:?} is not a string"),
        })
        .collect();
    #[cfg(windows)]
    assert_eq!(
        &words[words.len() - 3..],
        &["-n", "120", "127.0.0.1"],
        "ping's arguments: {words:?}"
    );
    #[cfg(not(windows))]
    assert_eq!(words, ["sleep", "120"], "sleep's arguments");

    let exe = run("proc_exe", vec![child.value()]).expect("proc_exe");
    let Value::Str(exe) = exe else {
        panic!("proc_exe did not answer a string");
    };
    #[cfg(windows)]
    let want = "ping.exe";
    #[cfg(not(windows))]
    let want = "sleep";
    assert!(
        exe.to_ascii_lowercase().ends_with(want),
        "proc_exe {exe:?} is not the {want} program"
    );
}

#[test]
fn a_child_has_a_resident_size_a_thread_count_and_a_priority() {
    let mut child = Sleeper::start();
    let mem = run("proc_mem_usage", vec![child.value()]).expect("proc_mem_usage");
    assert!(matches!(mem, Value::Int(n) if n > 0), "child mem {mem:?}");
    let threads = run("proc_threads", vec![child.value()]).expect("proc_threads");
    assert!(
        matches!(threads, Value::Int(n) if n >= 1),
        "child threads {threads:?}"
    );
    let prio = run("proc_priority", vec![child.value()]).expect("proc_priority");
    assert!(matches!(prio, Value::Record(_)), "child priority {prio:?}");
}

#[test]
fn wait_on_a_live_child_is_false_at_timeout_and_true_once_it_ends() {
    // Ends about three seconds after it starts, so it is live for the first
    // two calls and ends during the third.
    let mut child = Sleeper::start_for(3);
    let pid = child.pid();
    let running = run("proc_wait", vec![Value::Int(pid), Value::Int(0)]).expect("proc_wait 0");
    assert_eq!(
        running,
        Value::Bool(false),
        "a live child answered {running:?} at timeout"
    );
    let running = run("proc_wait", vec![Value::Int(pid), Value::Int(150)]).expect("proc_wait");
    assert_eq!(
        running,
        Value::Bool(false),
        "a live child ended within 150 ms"
    );
    let ended = run("proc_wait", vec![Value::Int(pid), Value::Int(20_000)]).expect("proc_wait");
    assert_eq!(
        ended,
        Value::Bool(true),
        "the child did not end within 20 s"
    );
}

#[cfg(unix)]
#[test]
fn spawn_then_wait_reaps_the_child() {
    // A finished child that is not reaped stays a zombie, and kill(pid, 0)
    // still finds it, so wait would time out. Reaped, the pid is gone.
    let pid = run("proc_spawn", vec![Value::Str("true".into())]).expect("proc_spawn");
    let Value::Int(pid) = pid else {
        panic!("proc_spawn did not answer a pid");
    };
    assert!(pid > 0, "proc_spawn answered pid {pid}");
    let ended = run("proc_wait", vec![Value::Int(pid), Value::Int(10_000)]).expect("proc_wait");
    assert_eq!(
        ended,
        Value::Bool(true),
        "a finished child did not end within 10 s"
    );
    // Waited for once: a second wait finds no such child.
    expect_code("proc_wait", vec![Value::Int(pid)], "E_NOT_FOUND");
    // SAFETY: signal 0 sends nothing; it asks whether the pid is still there.
    let still = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    assert!(
        !still,
        "pid {pid} is still there after proc_wait: it was not reaped"
    );
}

#[cfg(windows)]
#[test]
fn spawn_then_wait_on_windows_ends_the_child() {
    let pid = run(
        "proc_spawn",
        vec![
            Value::Str("cmd".into()),
            Value::Str("/c".into()),
            Value::Str("exit".into()),
            Value::Str("0".into()),
        ],
    )
    .expect("proc_spawn");
    let Value::Int(pid) = pid else {
        panic!("proc_spawn did not answer a pid");
    };
    // cmd may finish before this call: a child this shell spawned and that
    // has already ended is still true, once.
    let ended = run("proc_wait", vec![Value::Int(pid), Value::Int(10_000)]).expect("proc_wait");
    assert_eq!(ended, Value::Bool(true), "cmd did not end within 10 s");
    expect_code("proc_wait", vec![Value::Int(pid)], "E_NOT_FOUND");
}

// ── mutators on a child this test started (Windows) ───────────────────────

#[cfg(windows)]
#[test]
fn set_priority_on_a_child_is_read_back_as_its_class() {
    let mut child = Sleeper::start();
    let pid = child.pid();
    // A raised nice needs no privilege; 5 maps to BelowNormal.
    let ok = run("proc_set_priority", vec![Value::Int(pid), Value::Int(5)]).expect("set");
    assert_eq!(ok, Value::Bool(true));
    let prio = run("proc_priority", vec![Value::Int(pid)]).expect("proc_priority");
    let Value::Record(r) = prio else {
        panic!("proc_priority did not answer a record");
    };
    assert_eq!(
        r.get("class"),
        Some(&Value::Str("BelowNormal".into())),
        "class of {pid}"
    );
    assert_eq!(r.get("nice"), Some(&Value::Int(10)), "nice of {pid}");
}

// Windows suspend and resume (NtSuspendProcess / NtResumeProcess) are tested
// in tests/proc_suspend_windows.rs.

// Windows reads another process's environment and directory: see
// tests/proc_env_cwd_windows.rs.

#[cfg(target_os = "macos")]
#[test]
fn another_processes_environment_is_unimplemented_on_macos() {
    let mut child = Sleeper::start();
    expect_code("proc_env", vec![child.value()], "E_UNIMPLEMENTED");
}

// ── Linux: state changes, in the disposable environment only ──────────────

/// The state letter from /proc/<pid>/stat: the field after the command name,
/// which can itself contain spaces and parentheses, so split after the last ')'.
#[cfg(target_os = "linux")]
fn proc_state(pid: i64) -> char {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("read stat");
    let after = &stat[stat.rfind(')').expect("stat has a name") + 1..];
    after.trim_start().chars().next().expect("a state letter")
}

#[cfg(target_os = "linux")]
fn wait_for_state(pid: i64, want: char) -> bool {
    for _ in 0..100 {
        if proc_state(pid) == want {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn suspend_then_resume_a_child_is_visible_in_proc() {
    disposable::require();
    let mut child = Sleeper::start();
    let pid = child.pid();
    assert_eq!(
        run("proc_suspend", vec![Value::Int(pid)]).expect("suspend"),
        Value::Bool(true)
    );
    assert!(
        wait_for_state(pid, 'T'),
        "child {pid} is not stopped (state {})",
        proc_state(pid)
    );
    assert_eq!(
        run("proc_resume", vec![Value::Int(pid)]).expect("resume"),
        Value::Bool(true)
    );
    assert!(
        wait_for_state(pid, 'S') || wait_for_state(pid, 'R'),
        "child {pid} did not run again (state {})",
        proc_state(pid)
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn set_priority_on_a_child_is_its_nice_in_proc() {
    disposable::require();
    let mut child = Sleeper::start();
    let pid = child.pid();
    assert_eq!(
        run("proc_set_priority", vec![Value::Int(pid), Value::Int(5)]).expect("set"),
        Value::Bool(true)
    );
    // Field 19 of /proc/<pid>/stat is the nice value.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("stat");
    let after: Vec<&str> = stat[stat.rfind(')').expect("name") + 1..]
        .split_whitespace()
        .collect();
    let nice: i64 = after[16].parse().expect("nice field");
    assert_eq!(nice, 5, "nice of child {pid} in /proc");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn set_priority_refused_for_a_lower_nice_is_policy_deny_not_a_false() {
    disposable::require();
    // Only meaningful when this process may not lower its own nice value. The
    // disposable runner is root, so this checks the refusal path only when
    // the caller is not root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let mut child = Sleeper::start();
    let e = run("proc_set_priority", vec![child.value(), Value::Int(-5)])
        .expect_err("a non-root raise of priority");
    assert_eq!(code_of(&e), "E_POLICY_DENY", "{e}");
}

/// 2^32 + 1 is no process. `pid as u32` wrapped it to pid 1, so proc_info
/// described init (or whatever held pid 1) for a pid that does not exist.
#[test]
fn a_pid_above_u32_is_refused_not_wrapped() {
    expect_code("proc_info", vec![Value::Int(4_294_967_297)], "E_BAD_ARG");
}
