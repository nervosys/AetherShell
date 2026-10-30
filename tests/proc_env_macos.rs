//! proc_env on macOS, against a child this test starts.
//!
//! The child is a `sleep` with a unique environment variable. It is killed in
//! a guard's `Drop`, so a failed assertion does not leave it running. launchd
//! (pid 1) runs as root, so an ordinary user may not read its environment:
//! the answer must be a coded refusal, not an empty record.

#![cfg(target_os = "macos")]

use iron_shell::builtins::call;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::process::{Child, Command, Stdio};

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

/// A pid no live process holds.
const NOT_LIVE: i64 = 2_000_000_000;

/// A unique value for the marker variable, so a stale process cannot match.
fn unique_mark() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("procmac-{}-{nanos:x}", std::process::id())
}

/// A `sleep` that carries AE_PROCMAC_MARK. Killed on drop.
struct Marked(Child);

impl Marked {
    fn start(mark: &str) -> Self {
        let child = Command::new("/bin/sleep")
            .arg("60")
            .env("AE_PROCMAC_MARK", mark)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep starts");
        // KERN_PROCARGS2 holds the new image's arguments only once exec has
        // finished; a moment is enough for /bin/sleep.
        std::thread::sleep(std::time::Duration::from_millis(300));
        Marked(child)
    }
}

impl Drop for Marked {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn proc_env_of_a_child_has_the_marker_it_was_started_with() {
    let mark = unique_mark();
    let child = Marked::start(&mark);
    let pid = i64::from(child.0.id());
    let env = run("proc_env", vec![Value::Int(pid)]).expect("proc_env of child");
    let Value::Record(r) = env else {
        panic!("proc_env did not answer a record");
    };
    assert_eq!(
        r.get("AE_PROCMAC_MARK"),
        Some(&Value::Str(mark)),
        "marker of child {pid}: {r:?}"
    );
}

#[test]
fn proc_env_of_launchd_is_refused_for_an_ordinary_user() {
    if is_root() {
        return;
    }
    match run("proc_env", vec![Value::Int(1)]) {
        Ok(v) => panic!("proc_env(1) answered {v:?}; it should be E_OS_DENIED"),
        Err(e) => assert_eq!(code_of(&e), "E_OS_DENIED", "{e}"),
    }
}

#[test]
fn proc_env_of_a_pid_that_is_not_live_is_not_found() {
    match run("proc_env", vec![Value::Int(NOT_LIVE)]) {
        Ok(v) => panic!("proc_env answered {v:?} for a dead pid"),
        Err(e) => assert_eq!(code_of(&e), "E_NOT_FOUND", "{e}"),
    }
}
