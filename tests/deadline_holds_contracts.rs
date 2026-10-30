//! The deadlines in four call sites hold: a child that stays silent past its
//! deadline is killed and the call returns a coded budget error near the
//! deadline, and a grandchild that holds the output pipe after its child has
//! exited does not hold the call.
//!
//! The sites: `agent::__test_execute_step` (the agent's step runner),
//! `safety::output_with_timeout` (the io-returning runner many builtins use),
//! `builtins::__test_kubectl_run` (kubectl), and
//! `ExternalToolRegistry::execute` (external tool capabilities).
//!
//! Each call runs on its own thread under a hard bound of [`HARD_BOUND`], so a
//! regression fails the test instead of hanging the suite. Deadlines are 2 to
//! 10 seconds. The stand-in programs are `sh` on unix and PowerShell or `cmd`
//! on Windows.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// No call may take longer than this before the test gives up on it.
const HARD_BOUND: Duration = Duration::from_secs(60);
/// A call refused at a deadline must return well inside this.
const NEAR_DEADLINE: Duration = Duration::from_secs(20);

/// Runs `f` on a thread and returns what it returns with how long it took, or
/// fails the test if it has not returned within [`HARD_BOUND`].
fn bounded<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> (T, Duration) {
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(HARD_BOUND) {
        Ok(value) => (value, started.elapsed()),
        Err(_) => panic!("{what} did not return within {HARD_BOUND:?}: a deadline does not hold"),
    }
}

/// The coded error name in a message, such as `E_BUDGET_EXCEEDED`.
fn code_of(text: &str) -> String {
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

/// A stand-in program: its bare name (found on the search path), and the argv
/// that follows it.
struct Stand {
    program: &'static str,
    argv: Vec<String>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A child that prints nothing and stays alive for 60 s.
fn silent_child() -> Stand {
    #[cfg(unix)]
    let stand = Stand {
        program: "sh",
        argv: strings(&["-c", "sleep 60"]),
    };
    #[cfg(windows)]
    let stand = Stand {
        program: "powershell",
        argv: strings(&["-NoProfile", "-Command", "Start-Sleep 60"]),
    };
    stand
}

/// A child that prints `ok` and exits 0, leaving a grandchild that holds the
/// pipes open for about 30 s.
fn grandchild_child() -> Stand {
    #[cfg(unix)]
    let stand = Stand {
        program: "sh",
        argv: strings(&["-c", "sleep 30 & echo ok"]),
    };
    #[cfg(windows)]
    let stand = Stand {
        program: "cmd",
        argv: strings(&["/c", "start /b ping -n 30 127.0.0.1 >nul & echo ok"]),
    };
    stand
}

/// A fresh directory for one test's manifest.
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("ae-deadline-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create a scratch directory");
    dir
}

/// The absolute path of a stand-in program, for a manifest's `executable`.
fn absolute_path(program: &str) -> String {
    #[cfg(unix)]
    {
        format!("/bin/{program}")
    }
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        match program {
            "cmd" => format!(r"{root}\System32\cmd.exe"),
            _ => format!(r"{root}\System32\WindowsPowerShell\v1.0\{program}.exe"),
        }
    }
}

/// Writes a manifest whose one capability runs the stand-in with its argv, and
/// returns the directory holding it. The first argv element is the capability's
/// `command`, the rest are its template `args`.
fn write_tool(tag: &str, stand: &Stand, timeout_secs: u64) -> PathBuf {
    let dir = scratch_dir(tag);
    let (command, rest) = stand.argv.split_first().expect("a non-empty argv");
    let quoted: Vec<String> = rest
        .iter()
        .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    let manifest = format!(
        "[tool]\nname = \"deadline_probe_{tag}\"\ndescription = \"deadline test stand-in\"\n\
         version = \"0.0.1\"\nexecutable = '{}'\n\n[agent]\ntimeout_secs = {timeout_secs}\n\n\
         [[capabilities]]\nname = \"run\"\ndescription = \"run a stand-in\"\n\
         command = \"{}\"\nargs = [{}]\n",
        absolute_path(stand.program),
        command.replace('"', "\\\""),
        quoted.join(", ")
    );
    std::fs::write(dir.join("tool.toml"), manifest).expect("write the manifest");
    dir
}

// ─────────────────────────────────────────────────────────────────────────
// safety::output_with_timeout
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn output_with_timeout_kills_a_silent_child_at_its_deadline() {
    let stand = silent_child();
    let (result, elapsed) = bounded("output_with_timeout", move || {
        let mut cmd = std::process::Command::new(stand.program);
        cmd.args(&stand.argv);
        iron_shell::safety::output_with_timeout(cmd, 2)
    });
    let err = match result {
        Ok(out) => panic!(
            "a silent child should time out, it finished with {:?}",
            out.status
        ),
        Err(e) => e,
    };
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "error: {err}");
    assert!(
        elapsed < NEAR_DEADLINE,
        "the refusal came after {elapsed:?}, not near the 2 s deadline"
    );
}

#[test]
fn output_with_timeout_is_not_held_by_a_grandchild_on_its_pipes() {
    let stand = grandchild_child();
    let (result, elapsed) = bounded("output_with_timeout", move || {
        let mut cmd = std::process::Command::new(stand.program);
        cmd.args(&stand.argv);
        iron_shell::safety::output_with_timeout(cmd, 10)
    });
    let out = match result {
        Ok(out) => out,
        Err(e) => panic!("the child exited 0, the call should finish: {e}"),
    };
    assert!(out.status.success(), "status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ok"),
        "the output the child printed is kept, got {stdout:?}"
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the call waited {elapsed:?} on a pipe that a grandchild held"
    );
}

/// Output past the cap is a coded budget error, never a cut-short success.
#[cfg(unix)]
#[test]
fn output_with_timeout_refuses_output_past_its_cap() {
    let (result, _) = bounded("output_with_timeout", || {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "head -c 70000000 /dev/zero"]);
        iron_shell::safety::output_with_timeout(cmd, 60)
    });
    let err = match result {
        Ok(out) => panic!(
            "70 MB of output should be refused, got {} bytes",
            out.stdout.len()
        ),
        Err(e) => e,
    };
    let coded = iron_shell::safety::io_budget_error(&err)
        .unwrap_or_else(|| panic!("the refusal should carry a coded error: {err}"));
    assert_eq!(code_of(&coded.to_string()), "E_BUDGET_EXCEEDED", "{coded}");
}

// ─────────────────────────────────────────────────────────────────────────
// agent step runner
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_agent_step_that_stays_silent_is_killed_with_a_budget_error() {
    let stand = silent_child();
    let (result, elapsed) = bounded("the agent step runner", move || {
        let argv: Vec<&str> = stand.argv.iter().map(String::as_str).collect();
        iron_shell::agent::__test_execute_step(stand.program, &argv, 2)
    });
    let err = match result {
        Ok(()) => panic!("a silent step should time out"),
        Err(e) => e,
    };
    assert_eq!(
        code_of(&format!("{err:#}")),
        "E_BUDGET_EXCEEDED",
        "message: {err:#}"
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the refusal came after {elapsed:?}, not near the 2 s deadline"
    );
}

#[test]
fn an_agent_step_is_not_held_by_a_grandchild_on_its_pipes() {
    let stand = grandchild_child();
    let (result, elapsed) = bounded("the agent step runner", move || {
        let argv: Vec<&str> = stand.argv.iter().map(String::as_str).collect();
        iron_shell::agent::__test_execute_step(stand.program, &argv, 10)
    });
    if let Err(e) = result {
        panic!("the child exited 0, the step should finish: {e:#}");
    }
    assert!(
        elapsed < NEAR_DEADLINE,
        "the step waited {elapsed:?} on a pipe that a grandchild held"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// kubectl
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn kubectl_is_killed_at_its_deadline_with_a_budget_error() {
    let stand = silent_child();
    let (result, elapsed) = bounded("kubectl", move || {
        let argv: Vec<&str> = stand.argv.iter().map(String::as_str).collect();
        iron_shell::builtins::__test_kubectl_run(stand.program, &argv, 1024 * 1024, 2)
    });
    let err = match result {
        Ok(text) => panic!("a silent tool should time out, it printed {text:?}"),
        Err(e) => e,
    };
    assert_eq!(
        code_of(&format!("{err:#}")),
        "E_BUDGET_EXCEEDED",
        "message: {err:#}"
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the refusal came after {elapsed:?}, not near the 2 s deadline"
    );
}

#[test]
fn kubectl_is_not_held_by_a_grandchild_on_its_pipes() {
    let stand = grandchild_child();
    let (result, elapsed) = bounded("kubectl", move || {
        let argv: Vec<&str> = stand.argv.iter().map(String::as_str).collect();
        iron_shell::builtins::__test_kubectl_run(stand.program, &argv, 1024 * 1024, 10)
    });
    let text = match result {
        Ok(text) => text,
        Err(e) => panic!("the child exited 0, the call should finish: {e:#}"),
    };
    assert!(
        text.contains("ok"),
        "stdout the child printed is kept, got {text:?}"
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the call waited {elapsed:?} on a pipe that a grandchild held"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// external tools
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_external_tool_that_stays_silent_is_killed_with_a_budget_error() {
    let stand = silent_child();
    let dir = write_tool("silent", &stand, 2);
    let tool_dir = dir.clone();
    let (result, elapsed) = bounded("the external tool runner", move || {
        let registry = iron_shell::external_tools::ExternalToolRegistry::new();
        let name = registry
            .register_from_path(&tool_dir)
            .expect("the manifest registers");
        registry.execute(&name, "run", &serde_json::json!({}))
    });
    let _ = std::fs::remove_dir_all(&dir);
    let err = match result {
        Ok(r) => panic!("a silent tool should time out, it returned {r:?}"),
        Err(e) => e,
    };
    assert_eq!(
        code_of(&format!("{err:#}")),
        "E_BUDGET_EXCEEDED",
        "message: {err:#}"
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the refusal came after {elapsed:?}, not near the 2 s deadline"
    );
}

#[test]
fn an_external_tool_is_not_held_by_a_grandchild_on_its_pipes() {
    let stand = grandchild_child();
    let dir = write_tool("grandchild", &stand, 10);
    let tool_dir = dir.clone();
    let (result, elapsed) = bounded("the external tool runner", move || {
        let registry = iron_shell::external_tools::ExternalToolRegistry::new();
        let name = registry
            .register_from_path(&tool_dir)
            .expect("the manifest registers");
        registry.execute(&name, "run", &serde_json::json!({}))
    });
    let _ = std::fs::remove_dir_all(&dir);
    let result = match result {
        Ok(r) => r,
        Err(e) => panic!("the child exited 0, the call should finish: {e:#}"),
    };
    assert!(result.success, "the child exited 0: {:?}", result.error);
    let text = result
        .output
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        text.contains("ok"),
        "stdout the child printed is kept, got {:?}",
        result.output
    );
    assert!(
        elapsed < NEAR_DEADLINE,
        "the call waited {elapsed:?} on a pipe that a grandchild held"
    );
}
