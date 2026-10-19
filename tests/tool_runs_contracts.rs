//! Contracts for the bounded tool runners: `iperf3_server`, `entr_watch`,
//! `watchexec_run` and `nodemon_run`.
//!
//! Ordinary tests run everywhere: bad arguments, pipeline input and surplus
//! arguments are refused with a coded E_BAD_ARG before any process starts.
//!
//! The real runs are `#[ignore]`d and run only in the disposable distro, via
//! scripts/disposable_linux_tests.sh. Each one checks its effect from outside
//! the builtin (a log file, a marker file, pgrep) and checks that no process of
//! the tool is left behind. Margins are generous: a deadline kill is allowed to
//! take a moment to be reflected in the process table.

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::Mutex;

/// The real runs use fixed tool names in the process table, so they must not
/// overlap.
#[cfg(target_os = "linux")]
static SERIAL: Mutex<()> = Mutex::new(());

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn strs(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|t| s(t)).collect())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

#[cfg(target_os = "linux")]
fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(map) => map
            .get(key)
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn call_builtin(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn refused_with(name: &str, args: Vec<Value>) -> String {
    match call_builtin(name, args) {
        Ok(v) => panic!("{name} should refuse, but returned {v:?}"),
        Err(e) => code_of(&e),
    }
}

/// A fresh directory unique to this process and name, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ae_tool_runs_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- refusals

#[test]
fn iperf3_refuses_a_port_outside_the_range() {
    assert_eq!(
        refused_with("iperf3_server", vec![rec(&[("port", Value::Int(80))])]),
        "E_BAD_ARG"
    );
}

#[test]
fn iperf3_refuses_unknown_and_hostname_options() {
    assert_eq!(
        refused_with("iperf3_server", vec![rec(&[("bogus", Value::Int(1))])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("iperf3_server", vec![rec(&[("bind", s("localhost"))])]),
        "E_BAD_ARG"
    );
}

#[test]
fn iperf3_refuses_a_timeout_outside_the_range() {
    assert_eq!(
        refused_with(
            "iperf3_server",
            vec![rec(&[("timeout_ms", Value::Int(10))])]
        ),
        "E_BAD_ARG"
    );
}

#[test]
fn iperf3_refuses_pipeline_input_and_surplus_arguments() {
    let piped = call_with_input(
        "iperf3_server",
        vec![],
        Some(Value::Int(1)),
        &mut Env::new(),
    );
    assert!(piped.is_err(), "pipeline input must be refused");
    assert_eq!(
        refused_with("iperf3_server", vec![Value::Null, Value::Null]),
        "E_BAD_ARG"
    );
}

#[test]
fn entr_refuses_missing_paths_and_bad_commands() {
    assert_eq!(
        refused_with(
            "entr_watch",
            vec![strs(&["/nonexistent-ae-tool-runs/x"]), strs(&["/bin/true"])]
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("entr_watch", vec![strs(&[]), strs(&["/bin/true"])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("entr_watch", vec![strs(&["/"]), strs(&[])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("entr_watch", vec![strs(&["/"]), strs(&["bin/true"])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("entr_watch", vec![strs(&["/"]), strs(&["-rf"])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with(
            "entr_watch",
            vec![strs(&["/"]), strs(&["/nonexistent-ae-tool-runs/prog"])]
        ),
        "E_BAD_ARG"
    );
}

#[test]
fn entr_refuses_a_timeout_outside_the_range_and_pipeline_input() {
    assert_eq!(
        refused_with(
            "entr_watch",
            vec![
                strs(&["/"]),
                strs(&["/bin/true"]),
                rec(&[("timeout_ms", Value::Int(5))]),
            ]
        ),
        "E_BAD_ARG"
    );
    let piped = call_with_input(
        "entr_watch",
        vec![strs(&["/"]), strs(&["/bin/true"])],
        Some(Value::Int(1)),
        &mut Env::new(),
    );
    assert!(piped.is_err(), "pipeline input must be refused");
}

#[test]
fn watchexec_refuses_missing_paths_and_bad_commands() {
    assert_eq!(
        refused_with(
            "watchexec_run",
            vec![strs(&["/nonexistent-ae-tool-runs/x"]), strs(&["/bin/true"])]
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("watchexec_run", vec![strs(&["/"]), strs(&["./true"])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("watchexec_run", vec![strs(&["/"]), strs(&["--help"])]),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with(
            "watchexec_run",
            vec![
                strs(&["/"]),
                strs(&["/bin/true"]),
                rec(&[("timeout_ms", Value::Int(1))])
            ]
        ),
        "E_BAD_ARG"
    );
}

#[test]
fn nodemon_refuses_a_missing_or_non_js_script() {
    assert_eq!(
        refused_with("nodemon_run", vec![s("/nonexistent-ae-tool-runs/app.js")]),
        "E_BAD_ARG"
    );
    let scratch = Scratch::new("nodemon_refusal");
    let text = scratch.0.join("app.txt");
    std::fs::write(&text, "not javascript").expect("write fixture");
    assert_eq!(
        refused_with("nodemon_run", vec![s(&text.display().to_string())]),
        "E_BAD_ARG"
    );
}

#[test]
fn nodemon_refuses_an_option_like_script_and_a_bad_timeout() {
    assert_eq!(
        refused_with("nodemon_run", vec![s("--exec=evil.js")]),
        "E_BAD_ARG"
    );
    let scratch = Scratch::new("nodemon_timeout");
    let js = scratch.0.join("app.js");
    std::fs::write(&js, "").expect("write fixture");
    assert_eq!(
        refused_with(
            "nodemon_run",
            vec![
                s(&js.display().to_string()),
                rec(&[("timeout_ms", Value::Int(1))])
            ]
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_with("nodemon_run", vec![Value::Int(3)]),
        "E_BAD_ARG"
    );
}

// ------------------------------------------------- real runs, disposable only

/// Waits up to `limit` for `pgrep` to find nothing, then returns what it found.
#[cfg(target_os = "linux")]
fn lingering(args: &[&str], limit: std::time::Duration) -> String {
    let until = std::time::Instant::now() + limit;
    loop {
        let out = std::process::Command::new("pgrep")
            .args(args)
            .output()
            .expect("pgrep runs on the disposable distro");
        let found = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if found.is_empty() || std::time::Instant::now() >= until {
            return found;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(target_os = "linux")]
fn free_port() -> i64 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    i64::from(port)
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
fn path_list(path: &std::path::Path) -> Value {
    Value::Array(vec![s(&path.display().to_string())])
}

#[cfg(target_os = "linux")]
fn duration_millis(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("duration_ms is not an Int: {other:?}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn iperf3_server_receives_one_client_stream() {
    disposable::require();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let port = free_port();

    // The client retries until the server is listening, so the order of the
    // two does not matter.
    let port_text = port.to_string();
    let client = std::thread::spawn(move || {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let out = std::process::Command::new("iperf3")
                .args(["-c", "127.0.0.1", "-p", port_text.as_str(), "-t", "1"])
                .output()
                .expect("iperf3 client runs");
            if out.status.success() {
                return;
            }
            if std::time::Instant::now() >= until {
                panic!("iperf3 client never connected to the server");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let result = call_builtin(
        "iperf3_server",
        vec![rec(&[
            ("port", Value::Int(port)),
            ("timeout_ms", Value::Int(30_000)),
        ])],
    )
    .expect("iperf3_server runs");
    client.join().expect("client thread finished");

    assert_eq!(field(&result, "timed_out"), &Value::Bool(false));
    match field(&result, "received_bytes") {
        Value::Int(n) => assert!(*n > 0, "received_bytes was {n}"),
        other => panic!("received_bytes is not an Int: {other:?}"),
    }
    assert!(matches!(field(&result, "bits_per_second"), Value::Float(f) if *f > 0.0));
    assert!(lingering(&["iperf3 -s"], std::time::Duration::from_secs(2)).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn iperf3_server_times_out_without_a_client_and_leaves_nothing() {
    disposable::require();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let port = free_port();

    let started = std::time::Instant::now();
    let result = call_builtin(
        "iperf3_server",
        vec![rec(&[
            ("port", Value::Int(port)),
            ("timeout_ms", Value::Int(2_000)),
        ])],
    )
    .expect("a run without a client is a result, not an error");
    let elapsed = started.elapsed();

    assert_eq!(field(&result, "timed_out"), &Value::Bool(true));
    assert_eq!(field(&result, "exit_code"), &Value::Null);
    assert_eq!(field(&result, "received_bytes"), &Value::Null);
    assert!(elapsed.as_millis() >= 1_500, "returned after {elapsed:?}");
    assert!(elapsed.as_secs() < 30, "returned after {elapsed:?}");
    assert!(lingering(&["iperf3 -s"], std::time::Duration::from_secs(5)).is_empty());
}

/// An argv that appends one line to `log` each time it runs.
#[cfg(target_os = "linux")]
fn append_line_command(log: &std::path::Path) -> Value {
    Value::Array(vec![
        s("/bin/sh"),
        s("-c"),
        s("echo run >> \"$1\""),
        s("sh"),
        s(&log.display().to_string()),
    ])
}

#[cfg(target_os = "linux")]
fn line_count(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

/// Touches `watched` after `delay`, so the tool sees a change during the run.
#[cfg(target_os = "linux")]
fn touch_after(watched: PathBuf, delay: std::time::Duration) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&watched)
            .expect("open the watched file");
        file.write_all(b"changed\n").expect("write the change");
    })
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn entr_watch_reruns_on_change_and_is_killed_at_the_deadline() {
    disposable::require();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("entr");
    let watched = scratch.0.join("watched.txt");
    let log = scratch.0.join("runs.log");
    std::fs::write(&watched, "start\n").expect("write watched file");

    let touch = touch_after(watched.clone(), std::time::Duration::from_millis(2_000));
    let result = call_builtin(
        "entr_watch",
        vec![
            path_list(&watched),
            append_line_command(&log),
            rec(&[("timeout_ms", Value::Int(6_000))]),
        ],
    )
    .expect("entr_watch runs");
    touch.join().expect("touch thread finished");

    assert_eq!(field(&result, "timed_out"), &Value::Bool(true));
    assert_eq!(field(&result, "exit_code"), &Value::Null);
    assert!(duration_millis(field(&result, "duration_ms")) >= 5_500);
    assert!(
        line_count(&log) >= 2,
        "expected the initial run and a re-run, log has {} line(s)",
        line_count(&log)
    );
    assert!(lingering(&["-x", "entr"], std::time::Duration::from_secs(3)).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn watchexec_run_reruns_on_change_and_is_killed_at_the_deadline() {
    disposable::require();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("watchexec");
    let watched = scratch.0.join("watched.txt");
    let log = scratch.0.join("runs.log");
    std::fs::write(&watched, "start\n").expect("write watched file");

    let touch = touch_after(watched.clone(), std::time::Duration::from_millis(2_000));
    let result = call_builtin(
        "watchexec_run",
        vec![
            path_list(&watched),
            append_line_command(&log),
            rec(&[("timeout_ms", Value::Int(6_000))]),
        ],
    )
    .expect("watchexec_run runs");
    touch.join().expect("touch thread finished");

    assert_eq!(field(&result, "timed_out"), &Value::Bool(true));
    assert_eq!(field(&result, "exit_code"), &Value::Null);
    assert!(
        line_count(&log) >= 2,
        "expected the initial run and a re-run, log has {} line(s)",
        line_count(&log)
    );
    assert!(lingering(&["-x", "watchexec"], std::time::Duration::from_secs(3)).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn nodemon_run_runs_the_script_and_is_killed_at_the_deadline() {
    disposable::require();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new("nodemon");
    let marker = scratch.0.join("marker.txt");
    let script = scratch.0.join("app.js");
    let js_marker = serde_json::to_string(&marker.display().to_string()).expect("quote path");
    std::fs::write(
        &script,
        format!("require('fs').writeFileSync({js_marker}, 'ran');\n"),
    )
    .expect("write script");

    let result = call_builtin(
        "nodemon_run",
        vec![
            s(&script.display().to_string()),
            rec(&[("timeout_ms", Value::Int(6_000))]),
        ],
    )
    .expect("nodemon_run runs");

    assert!(marker.is_file(), "the script did not run");
    assert_eq!(field(&result, "timed_out"), &Value::Bool(true));
    assert_eq!(field(&result, "exit_code"), &Value::Null);
    let script_text = script.display().to_string();
    assert!(
        lingering(
            &["-f", script_text.as_str()],
            std::time::Duration::from_secs(3)
        )
        .is_empty(),
        "a node process for the script is still running"
    );
}
