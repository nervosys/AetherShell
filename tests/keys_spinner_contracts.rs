//! Contracts for `input_read_char`, `input_spinner` and `lazygit_open`.
//!
//! Every child process here runs with stdin and stdout or stderr piped or
//! closed, never a terminal, so none can block on a human. Each child has a
//! hard deadline of 30 seconds. Nothing requires lazygit to be installed:
//! every lazygit case either refuses before the program would run, or is
//! refused for want of a terminal.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

struct Run {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    elapsed: Duration,
}

fn session_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_keys_contracts_{}_{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create session dir");
    dir
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Runs `ish -c <expr>`. `piped` is written to stdin and then closed; `None`
/// gives stdin at end of input, which is never a terminal.
fn run_ae(expr: &str, piped: Option<&[u8]>, tag: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ish"))
        .args(["-c", expr])
        .env("IRONSHELL_SESSION_DIR", session_dir(tag))
        .stdin(if piped.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ae");
    let start = Instant::now();
    let writer = piped.map(|bytes| {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let data = bytes.to_vec();
        std::thread::spawn(move || {
            // A broken pipe means the child exited first; that is not a failure.
            let _ = stdin.write_all(&data);
        })
    });
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if start.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae ran past {DEADLINE:?} on `{expr}`");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let elapsed = start.elapsed();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Run {
        status,
        stdout: out.join().expect("stdout reader"),
        stderr: err.join().expect("stderr reader"),
        elapsed,
    }
}

/// The error code of a call that is expected to be refused, as the script
/// sees it through `try`.
fn refused_code(call_expr: &str, tag: &str) -> String {
    let expr = format!("(try {{ {call_expr} }} catch e {{ e.error.code }})");
    let r = run_ae(&expr, None, tag);
    assert!(r.status.success(), "`{expr}` stderr: {}", r.stderr);
    r.stdout.trim_end().trim_matches('"').to_string()
}

fn in_process(name: &str, args: Vec<Value>, input: Option<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    match input {
        Some(v) => call_with_input(name, args, Some(v), &mut env),
        None => call(name, args, &mut env),
    }
}

// ── input_read_char ─────────────────────────────────────────────────────

#[test]
fn read_char_with_piped_stdin_is_no_ui_and_leaves_the_bytes_for_the_next_read() {
    // `input_read_char` must refuse before reading, so the piped line is still
    // there for `prompt`, which takes exactly one line.
    // `-c` prints only its last value, so both results go into one array.
    let expr = "[(try { input_read_char() } catch e { e.error.code }), prompt(\"\")]";
    let r = run_ae(expr, Some(b"typed answer\n"), "read_char_piped");
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert!(
        r.stdout.contains("E_NO_UI"),
        "read_char did not refuse with E_NO_UI: {}",
        r.stdout
    );
    assert!(
        r.stdout.contains("typed answer"),
        "the piped line was consumed by input_read_char: {}",
        r.stdout
    );
}

#[test]
fn read_char_with_a_timeout_and_piped_stdin_is_no_ui_not_a_wait() {
    let r = run_ae(
        "try { input_read_char(5000) } catch e { e.error.code }",
        Some(b"unread\n"),
        "read_char_timeout",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "E_NO_UI");
    assert!(
        r.elapsed < Duration::from_secs(20),
        "the refusal waited {:?}",
        r.elapsed
    );
}

#[test]
fn read_char_refuses_bad_timeouts_and_surplus_arguments_before_anything() {
    // Closed stdin: any read would be E_NO_UI, so an E_BAD_ARG proves the
    // refusal came first.
    let cases = [
        "input_read_char(0)",
        "input_read_char(3600001)",
        "input_read_char(-5)",
        "input_read_char(\"5\")",
        "input_read_char(1, 2)",
        "\"x\" | input_read_char()",
    ];
    for call_expr in cases {
        assert_eq!(
            refused_code(call_expr, "read_char_refusals"),
            "E_BAD_ARG",
            "`{call_expr}`"
        );
    }
}

#[test]
fn read_char_refusals_hold_in_process_too() {
    assert!(in_process("input_read_char", vec![Value::Int(0)], None).is_err());
    assert!(in_process("input_read_char", vec![], Some(Value::Str("x".into()))).is_err());
}

// ── input_spinner ───────────────────────────────────────────────────────

#[test]
fn spinner_answers_its_record_after_roughly_its_duration() {
    let r = run_ae(
        "input_spinner(\"working\", 400).duration_ms",
        None,
        "spinner_time",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "400");
    assert!(
        r.elapsed >= Duration::from_millis(400),
        "returned after {:?}, before 400 ms",
        r.elapsed
    );
    assert!(
        r.elapsed < Duration::from_secs(25),
        "returned after {:?}",
        r.elapsed
    );
}

#[test]
fn spinner_with_piped_stderr_writes_no_carriage_returns_or_escapes() {
    // The child's stderr is a pipe, not a terminal, so nothing may be drawn.
    let r = run_ae(
        "input_spinner(\"working\", 150).label",
        None,
        "spinner_quiet",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "working");
    assert!(
        !r.stderr.contains('\r') && !r.stderr.contains('\u{1b}'),
        "a piped stderr received spinner bytes: {:?}",
        r.stderr
    );
}

#[test]
fn spinner_refuses_bad_durations_long_labels_and_wrong_shapes() {
    let long_label = format!("input_spinner(\"{}\", 1)", "a".repeat(1025));
    let cases = [
        "input_spinner(\"working\", 0)".to_string(),
        "input_spinner(\"working\", 600001)".to_string(),
        "input_spinner(\"working\", \"5\")".to_string(),
        "input_spinner(5, 100)".to_string(),
        "input_spinner(\"working\")".to_string(),
        "input_spinner(\"working\", 1, 2)".to_string(),
        "\"x\" | input_spinner(\"working\", 1)".to_string(),
        long_label,
    ];
    for call_expr in &cases {
        assert_eq!(
            refused_code(call_expr, "spinner_refusals"),
            "E_BAD_ARG",
            "`{call_expr}`"
        );
    }
}

#[test]
fn spinner_accepts_a_label_of_exactly_1_kib() {
    let expr = format!("input_spinner(\"{}\", 1).duration_ms", "a".repeat(1024));
    let r = run_ae(&expr, None, "spinner_1kib");
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "1");
}

#[test]
fn spinner_refusals_hold_in_process_too() {
    let label = Value::Str("working".into());
    assert!(in_process("input_spinner", vec![label.clone(), Value::Int(0)], None).is_err());
    assert!(in_process(
        "input_spinner",
        vec![label.clone(), Value::Int(1)],
        Some(Value::Str("x".into()))
    )
    .is_err());
}

// ── lazygit_open ────────────────────────────────────────────────────────

#[test]
fn lazygit_open_under_piped_stdio_is_no_ui() {
    // The path is valid (the child's working directory exists), so the only
    // refusal left is the missing terminal. lazygit is never started.
    let expr = "try { lazygit_open(\".\") } catch e { e.error.code }";
    let r = run_ae(expr, None, "lazygit_no_ui");
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "E_NO_UI");
}

#[test]
fn lazygit_open_refuses_a_missing_path_before_the_terminal_check() {
    let r = refused_code(
        "lazygit_open(\"definitely/not/a/real/dir_xyz\")",
        "lazygit_missing",
    );
    assert_eq!(r, "E_NOT_FOUND");
}

#[test]
fn lazygit_open_refuses_option_like_and_non_directory_paths() {
    assert_eq!(
        refused_code("lazygit_open(\"--version\")", "lazygit_opt"),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_code("lazygit_open(\"-p\")", "lazygit_opt2"),
        "E_BAD_ARG"
    );
    let manifest = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    let file = format!("lazygit_open(\"{manifest}/Cargo.toml\")");
    assert_eq!(refused_code(&file, "lazygit_file"), "E_BAD_ARG");
}

#[test]
fn lazygit_open_refuses_pipeline_input_and_surplus_arguments() {
    assert_eq!(
        refused_code("\".\" | lazygit_open()", "lazygit_pipe"),
        "E_BAD_ARG"
    );
    assert_eq!(
        refused_code("lazygit_open(\".\", \".\")", "lazygit_surplus"),
        "E_BAD_ARG"
    );
}

#[test]
fn lazygit_open_refusals_hold_in_process_too() {
    assert!(in_process("lazygit_open", vec![Value::Str("-x".into())], None).is_err());
    assert!(in_process(
        "lazygit_open",
        vec![Value::Str("definitely/not/a/real/dir_xyz".into())],
        None
    )
    .is_err());
    assert!(in_process("lazygit_open", vec![], Some(Value::Str(".".into()))).is_err());
}
