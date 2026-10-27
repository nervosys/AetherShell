//! tmux_new, tmux_send and tmux_kill against a real tmux, where one is
//! installed (the test passes without doing anything otherwise).
//!
//! Test safety: every call runs with `TMUX_TMPDIR` pointed at a fresh temp
//! directory and `TMUX` unset, so tmux starts a private server and never
//! reaches a session the person has open. The server is killed when the test
//! ends, pass or fail. No terminal is attached.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn have_tmux() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A private tmux server for the length of one test.
struct Server {
    dir: PathBuf,
}

impl Server {
    fn start(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ae_tmux_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TMUX_TMPDIR", &dir);
        std::env::remove_var("TMUX");
        std::env::remove_var("AETHER_MODE");
        std::env::remove_var("AETHER_AGENT");
        std::env::set_var("AETHER_SESSION_DIR", dir.join("session"));
        Server { dir }
    }

    fn sessions(&self) -> Vec<String> {
        let out = std::process::Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .env("TMUX_TMPDIR", &self.dir)
            .output()
            .unwrap();
        let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        names.sort();
        names
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::process::Command::new("tmux")
            .arg("kill-server")
            .env("TMUX_TMPDIR", &self.dir)
            .output();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The first `E_*` code in the error's text.
fn code(e: &anyhow::Error) -> String {
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

#[test]
fn kill_ends_the_named_session_and_only_that_one() {
    if !have_tmux() {
        eprintln!("tmux not installed; skipped");
        return;
    }
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = Server::start("kill");
    call("tmux_new", vec![s("workshop")]).unwrap();
    call("tmux_new", vec![s("other")]).unwrap();
    assert_eq!(server.sessions(), ["other", "workshop"]);

    // No session is called "work": tmux would match "workshop" by prefix.
    let e = call("tmux_kill", vec![s("work")]).unwrap_err();
    assert_eq!(code(&e), "E_TOOL_FAILED", "{e}");
    assert_eq!(server.sessions(), ["other", "workshop"]);

    let r = call("tmux_kill", vec![s("workshop")]).unwrap();
    let Value::Record(r) = r else { panic!("{r:?}") };
    assert_eq!(r.get("success"), Some(&Value::Bool(true)));
    assert_eq!(r.get("session"), Some(&s("workshop")));
    assert_eq!(server.sessions(), ["other"]);

    let e = call("tmux_kill", vec![s("workshop")]).unwrap_err();
    assert_eq!(code(&e), "E_TOOL_FAILED", "{e}");
}

#[test]
fn send_does_not_reach_a_session_by_prefix() {
    if !have_tmux() {
        eprintln!("tmux not installed; skipped");
        return;
    }
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = Server::start("send");
    call("tmux_new", vec![s("workshop")]).unwrap();
    let e = call("tmux_send", vec![s("work"), s("true")]).unwrap_err();
    assert_eq!(code(&e), "E_TOOL_FAILED", "{e}");
    call("tmux_send", vec![s("workshop"), s("true")]).unwrap();
    call("tmux_send", vec![s("workshop:0"), s("true")]).unwrap();
    assert_eq!(server.sessions(), ["workshop"]);
}
