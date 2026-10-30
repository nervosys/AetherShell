//! Contracts for `proc_kill`.
//!
//! Every test here signals a child process this test spawned and owns. The
//! child is killed and reaped by a Drop guard whatever the test did, so a
//! failed assertion cannot leave a `sleep` or `ping` running. Refusals are
//! checked to send nothing: the child is still alive after each one.
//!
//! Human mode allows the Process effect, so these calls need no approval. The
//! audit log is kept out of the workspace, and any agent-mode setting another
//! test left behind is removed.
//!
//! The tree tests also start a child under a parent this test owns. That child
//! is not a `Child`, so it is killed by pid through a guard as well.

use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::{
    collections::BTreeMap,
    process::{Child, Command},
    sync::Mutex,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the environment this file needs and restores nothing: every test
/// sets it the same way, and the audit log lives in the temp directory.
fn sandbox() {
    for key in ["AETHER_MODE", "AETHER_AGENT", "AETHER_POLICY"] {
        std::env::remove_var(key);
    }
    let log = std::env::temp_dir().join(format!("proc_kill_audit_{}.log", std::process::id()));
    std::env::set_var("AETHER_AUDIT_LOG", log);
    std::env::set_var("AETHER_AUDIT_KEY", "22".repeat(32));
}

fn kill(args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call("proc_kill", args, &mut env)
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

/// A child this test owns. Killed and reaped on drop, however the test ends.
struct Owned(Child);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Owned {
    fn pid(&self) -> i64 {
        i64::from(self.0.id())
    }

    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
}

/// A process that runs for about thirty seconds unless signalled.
fn sleeper() -> Owned {
    #[cfg(windows)]
    let child = Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(not(windows))]
    let child = Command::new("sleep").arg("30").spawn();
    Owned(child.expect("spawn a child this test owns"))
}

/// Polls `done` every 50 ms for at most ten seconds.
fn within_ten_seconds(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    done()
}

fn record_field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Record(map) => map.get(key),
        _ => None,
    }
}

#[test]
fn kill_signal_returns_a_record_and_the_child_exits() {
    let _g = lock();
    sandbox();
    let mut child = sleeper();
    let pid = child.pid();
    let v = kill(vec![Value::Int(pid), Value::Str("KILL".into())]).expect("KILL the child");
    assert_eq!(record_field(&v, "pid"), Some(&Value::Int(pid)), "{v:?}");
    assert_eq!(
        record_field(&v, "signal"),
        Some(&Value::Str("KILL".into())),
        "{v:?}"
    );
    assert_eq!(record_field(&v, "sent"), Some(&Value::Bool(true)), "{v:?}");
    assert!(
        within_ten_seconds(|| !child.alive()),
        "the child was still running after KILL"
    );
}

/// Unix: TERM ends a sleep. Windows: TERM is taskkill's graceful close, which
/// a console process refuses, so it must be a coded failure, not a `false`.
#[test]
fn term_ends_a_unix_child_and_is_a_coded_failure_for_a_windows_console_one() {
    let _g = lock();
    sandbox();
    let mut child = sleeper();
    let pid = child.pid();
    let result = kill(vec![Value::Int(pid)]);
    #[cfg(unix)]
    {
        let v = result.expect("TERM the child");
        assert_eq!(
            record_field(&v, "signal"),
            Some(&Value::Str("TERM".into())),
            "{v:?}"
        );
        assert!(
            within_ten_seconds(|| !child.alive()),
            "the child was still running after TERM"
        );
    }
    #[cfg(windows)]
    {
        let e = result.expect_err("a console process must refuse a graceful TERM");
        assert_eq!(code_of(&e), "E_TOOL_FAILED", "{e}");
        assert!(
            child.alive(),
            "a refused TERM must not have ended the child"
        );
    }
}

#[test]
fn killing_an_exited_child_is_not_found() {
    let _g = lock();
    sandbox();
    #[cfg(windows)]
    let mut exited = Command::new("cmd")
        .args(["/C", "exit 0"])
        .spawn()
        .expect("spawn a short child");
    #[cfg(not(windows))]
    let mut exited = Command::new("true").spawn().expect("spawn a short child");
    let pid = i64::from(exited.id());
    exited.wait().expect("reap the short child");
    let e = kill(vec![Value::Int(pid), Value::Str("KILL".into())])
        .expect_err("an exited, reaped child has no process to signal");
    assert_eq!(code_of(&e), "E_NOT_FOUND", "{e}");
}

#[test]
fn refusals_send_nothing() {
    let _g = lock();
    sandbox();
    let mut child = sleeper();
    let pid = child.pid();
    let own = i64::from(std::process::id());

    for (what, args) in [
        ("pid 0", vec![Value::Int(0)]),
        ("a negative pid", vec![Value::Int(-1)]),
        ("a pid past the OS pid type", vec![Value::Int(i64::MAX)]),
        ("a name for a pid", vec![Value::Str("sleep".into())]),
        ("this shell's own pid", vec![Value::Int(own)]),
        (
            "a bad signal name",
            vec![Value::Int(pid), Value::Str("BOGUS".into())],
        ),
        (
            "a bool for the signal",
            vec![Value::Int(pid), Value::Bool(true)],
        ),
        (
            "surplus arguments",
            vec![Value::Int(pid), Value::Str("KILL".into()), Value::Int(1)],
        ),
    ] {
        let e = kill(args).expect_err(what);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{what}: {e}");
    }

    let mut env = Env::new();
    let e = call_with_input(
        "proc_kill",
        vec![Value::Int(pid)],
        Some(Value::Int(1)),
        &mut env,
    )
    .expect_err("pipeline input");
    assert_eq!(code_of(&e), "E_BAD_ARG", "pipeline input: {e}");

    assert!(child.alive(), "a refused call must not signal the child");
}

#[cfg(target_os = "linux")]
fn proc_state(pid: i64) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is parenthesised and may contain spaces or ')', so the
    // state is the first field after the last ')'.
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().next()?.chars().next()
}

/// STOP stops a Linux child (state 'T'), and CONT resumes it.
#[cfg(target_os = "linux")]
#[test]
fn stop_then_cont_changes_the_child_state() {
    let _g = lock();
    sandbox();
    let mut child = sleeper();
    let pid = child.pid();

    kill(vec![Value::Int(pid), Value::Str("STOP".into())]).expect("STOP the child");
    assert!(
        within_ten_seconds(|| proc_state(pid) == Some('T')),
        "STOP did not stop the child"
    );

    kill(vec![Value::Int(pid), Value::Str("CONT".into())]).expect("CONT the child");
    assert!(
        within_ten_seconds(|| matches!(proc_state(pid), Some(s) if s != 'T')),
        "CONT did not resume the child"
    );
    assert!(child.alive());
}

/// A parent this test owns, which starts one long-sleeping child and waits for
/// it. Unix: `sh -c 'sleep 30 & wait'`. Windows: `cmd /C ping ...`, whose ping
/// is the child cmd starts.
fn parent_with_child() -> Owned {
    #[cfg(windows)]
    let child = Command::new("cmd")
        .args(["/C", "ping", "-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(not(windows))]
    let child = Command::new("sh")
        .args(["-c", "sleep 30 & wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Owned(child.expect("spawn a parent this test owns"))
}

/// The pid of a process whose parent is `parent`, polled for up to five seconds.
///
/// A process that started before `parent` is not its child, whatever parent
/// pid it records: an older process can still name a parent pid that has
/// since been reused for `parent`. proc_kill applies the same rule, so a test
/// that took such a process (at once, before the real child existed) would
/// find it missing from `killed`; a windows-latest run failed that way once.
fn child_of(parent: u32) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let ppid = Pid::from_u32(parent);
    loop {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let born = sys.process(ppid).map(|p| p.start_time());
        let found = sys
            .processes()
            .iter()
            .find(|(_, p)| p.parent() == Some(ppid) && born.is_some_and(|b| p.start_time() >= b))
            .map(|(pid, _)| pid.as_u32());
        if found.is_some() || Instant::now() >= deadline {
            return found;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// True once the process has gone, or is a zombie that has already exited.
fn gone(pid: u32) -> bool {
    let mut sys = System::new();
    let spid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[spid]), true);
    match sys.process(spid) {
        None => true,
        Some(p) => p.status() == ProcessStatus::Zombie,
    }
}

/// A child that a parent this test owns started. Not a `Child`, so it is
/// killed by pid through sysinfo, on drop if the test has not done so.
struct ChildOfParent(u32);

impl ChildOfParent {
    fn kill_now(&self) -> bool {
        let mut sys = System::new();
        let spid = Pid::from_u32(self.0);
        sys.refresh_processes(ProcessesToUpdate::Some(&[spid]), true);
        sys.process(spid).is_some_and(|p| p.kill())
    }
}

impl Drop for ChildOfParent {
    fn drop(&mut self) {
        let _ = self.kill_now();
    }
}

fn tree_option(tree: Value) -> Value {
    let mut opts = BTreeMap::new();
    opts.insert("tree".to_string(), tree);
    Value::Record(opts)
}

#[test]
fn tree_must_be_a_bool_and_nothing_is_killed() {
    let _g = lock();
    sandbox();
    let mut child = sleeper();
    let pid = child.pid();

    for (what, opts) in [
        ("a string", tree_option(Value::Str("yes".into()))),
        ("an int", tree_option(Value::Int(1))),
        ("an unknown key", {
            let mut m = BTreeMap::new();
            m.insert("tre".to_string(), Value::Bool(true));
            Value::Record(m)
        }),
    ] {
        let e = kill(vec![Value::Int(pid), Value::Str("KILL".into()), opts]).expect_err(what);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{what}: {e}");
        assert!(
            child.alive(),
            "{what}: a refused call must not signal the child"
        );
    }
}

#[test]
fn tree_true_kills_the_parent_and_its_child() {
    let _g = lock();
    sandbox();
    let mut parent = parent_with_child();
    let ppid = parent.pid() as u32;
    let cpid = child_of(ppid).expect("the parent started a child");
    let grandchild = ChildOfParent(cpid);

    let v = kill(vec![
        Value::Int(i64::from(ppid)),
        Value::Str("KILL".into()),
        tree_option(Value::Bool(true)),
    ])
    .expect("KILL the tree");
    assert_eq!(record_field(&v, "sent"), Some(&Value::Bool(true)), "{v:?}");
    match record_field(&v, "killed") {
        Some(Value::Array(items)) => assert!(
            items.contains(&Value::Int(i64::from(cpid))),
            "killed does not name the child {cpid}: {v:?}"
        ),
        other => panic!("tree: true must name what it killed, got {other:?}"),
    }
    assert!(
        within_ten_seconds(|| !parent.alive()),
        "the parent was still running after the tree KILL"
    );
    assert!(
        within_ten_seconds(|| gone(cpid)),
        "the child was still running after the tree KILL"
    );
    drop(grandchild);
}

#[test]
fn without_tree_the_child_survives_and_is_cleaned_up_here() {
    let _g = lock();
    sandbox();
    let mut parent = parent_with_child();
    let ppid = parent.pid() as u32;
    let cpid = child_of(ppid).expect("the parent started a child");
    let grandchild = ChildOfParent(cpid);

    let v = kill(vec![Value::Int(i64::from(ppid)), Value::Str("KILL".into())])
        .expect("KILL the parent alone");
    assert_eq!(record_field(&v, "killed"), None, "{v:?}");
    assert!(
        within_ten_seconds(|| !parent.alive()),
        "the parent was still running after KILL"
    );

    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !gone(cpid),
        "without tree the child must still be running after its parent is killed"
    );

    assert!(grandchild.kill_now(), "clean up the child by pid");
    assert!(
        within_ten_seconds(|| gone(cpid)),
        "the child did not end after its own kill"
    );
}
