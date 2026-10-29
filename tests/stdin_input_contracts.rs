//! Contracts for the shared stdin reader and the builtins built on it:
//! `input_timeout`, `input_progress`, and the `prompt` path through
//! `read_answer`.
//!
//! Every case drives the real `ae` binary with `-c`. Each child has a hard
//! deadline: it is killed if it runs past 30 seconds, so no test waits
//! unbounded on stdin.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

const DEADLINE: Duration = Duration::from_secs(30);

/// What the child finds on stdin.
enum Input {
    /// These bytes, then end of input.
    Piped(Vec<u8>),
    /// End of input at once.
    Eof,
    /// Open and silent. The handle stays with the test until the child exits.
    OpenEmpty,
}

struct Run {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    elapsed: Duration,
    /// Largest resident size seen while polling. Sampled, so a short spike
    /// between samples can be missed.
    peak_rss: u64,
}

fn session_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_stdin_contracts_{}_{tag}", std::process::id()));
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

/// Peak resident size in bytes, from VmHWM in /proc/<pid>/status.
#[cfg(target_os = "linux")]
fn vm_hwm(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

fn run_ae(expr: &str, input: Input, tag: &str) -> Run {
    run_args(&["-c", expr], input, tag)
}

fn run_args(args: &[&str], input: Input, tag: &str) -> Run {
    let expr = args.join(" ");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ae"));
    cmd.args(args)
        .env("AETHER_SESSION_DIR", session_dir(tag))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(match input {
            Input::Eof => Stdio::null(),
            Input::Piped(_) | Input::OpenEmpty => Stdio::piped(),
        });
    let start = Instant::now();
    let mut child = cmd.spawn().expect("spawn ae");
    let pid = Pid::from_u32(child.id());

    // Piped bytes are written from a thread, then stdin is dropped so the
    // child sees end of input. The write may fail with a broken pipe if the
    // child exits before reading everything; that is not a test failure.
    let mut kept_open = None;
    let writer = match input {
        Input::Piped(data) => {
            let mut stdin = child.stdin.take().expect("piped stdin");
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(&data);
            }))
        }
        Input::OpenEmpty => {
            kept_open = child.stdin.take();
            None
        }
        Input::Eof => None,
    };
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());

    let mut sys = System::new();
    let mut peak_rss = 0u64;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if start.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae ran past {DEADLINE:?} on `{expr}`");
        }
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if let Some(process) = sys.process(pid) {
            peak_rss = peak_rss.max(process.memory());
        }
        // Linux keeps the high-water mark itself, so a buffer freed between
        // samples still counts.
        #[cfg(target_os = "linux")]
        if let Some(hwm) = vm_hwm(child.id()) {
            peak_rss = peak_rss.max(hwm);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let elapsed = start.elapsed();
    drop(kept_open);
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Run {
        status,
        stdout: out.join().expect("stdout reader"),
        stderr: err.join().expect("stderr reader"),
        elapsed,
        peak_rss,
    }
}

#[test]
fn input_timeout_answers_the_piped_line_without_its_terminator() {
    let r = run_ae(
        r#"input_timeout("", 5000)"#,
        Input::Piped(b"hello there\r\n".to_vec()),
        "piped",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "hello there");
}

#[test]
fn input_timeout_with_open_silent_stdin_answers_null_after_the_timeout() {
    let r = run_ae(
        r#"input_timeout("", 300) == null"#,
        Input::OpenEmpty,
        "open",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "true", "the answer was not null");
    assert!(
        r.elapsed >= Duration::from_millis(250),
        "returned after {:?}, before the 300 ms timeout",
        r.elapsed
    );
}

#[test]
fn input_timeout_with_closed_stdin_is_no_ui() {
    let r = run_ae(r#"input_timeout("Q? ", 5000)"#, Input::Eof, "eof");
    assert!(!r.status.success(), "closed stdin must not answer");
    assert!(r.stderr.contains("E_NO_UI"), "stderr: {}", r.stderr);
}

#[test]
fn an_over_long_line_is_budget_exceeded_and_the_next_line_still_arrives() {
    let mut data = vec![b'a'; 70 * 1024];
    data.extend_from_slice(b"\nnext\n");
    let r = run_ae(
        r#"(try { input_timeout("", 5000) } catch e { e.error.code }) + " " + input_timeout("", 5000)"#,
        Input::Piped(data),
        "long",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "E_BUDGET_EXCEEDED next");
}

#[test]
fn a_long_line_does_not_grow_the_process() {
    // Eight mebibytes on one line, against a control that reads a short line.
    // The reader discards bytes past the cap, so the peak should not track
    // the line's size. The allowance covers allocator slack and sampling.
    let big = {
        let mut data = vec![b'b'; 8 * 1024 * 1024];
        data.extend_from_slice(b"\n");
        data
    };
    // The child stays alive half a second after the read, so the sampler sees
    // it even when the read itself is quicker than one sample.
    const HELD: &str =
        r#"r = (try { input_timeout("", 5000) } catch e { e.error.code }); sleep(0.5); r"#;
    let small = run_ae(HELD, Input::Piped(b"short\n".to_vec()), "rss_small");
    let large = run_ae(HELD, Input::Piped(big), "rss_large");
    assert!(
        large.stdout.contains("E_BUDGET_EXCEEDED"),
        "stdout: {}",
        large.stdout
    );
    assert!(
        small.peak_rss > 0 && large.peak_rss > 0,
        "no resident size was sampled; the memory check did not run"
    );
    // 6 MiB: a run on a GitHub Linux runner sampled 4.6 MiB of noise, and an
    // 8 MiB line held whole would still exceed it.
    let allowance = 6 * 1024 * 1024;
    assert!(
        large.peak_rss < small.peak_rss + allowance,
        "peak {} bytes for an 8 MiB line against {} bytes for a short one",
        large.peak_rss,
        small.peak_rss
    );
}

#[test]
fn prompt_reads_a_piped_line_through_the_shared_reader() {
    let r = run_ae(
        r#"prompt("")"#,
        Input::Piped(b"typed answer\n".to_vec()),
        "prompt",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "typed answer");
}

#[test]
fn input_progress_answers_the_record_and_draws_the_bar() {
    let r = run_ae(
        r#"json.stringify(input_progress(1, 2))"#,
        Input::Eof,
        "progress",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(
        r.stdout.trim_end(),
        r#"{"current":1,"percent":50,"total":2}"#
    );
    assert!(
        r.stderr.contains("[###############---------------] 50%"),
        "stderr: {:?}",
        r.stderr
    );
    assert!(
        !r.stderr.contains('\n'),
        "a partial bar must not end the line: {:?}",
        r.stderr
    );
}

#[test]
fn input_progress_draws_the_label_and_a_newline_at_completion() {
    let r = run_ae(
        r#"input_progress(3, 4, "copy").percent"#,
        Input::Eof,
        "progress_label",
    );
    assert!(r.status.success(), "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim_end(), "75");
    assert!(
        r.stderr
            .contains("copy [######################--------] 75%"),
        "stderr: {:?}",
        r.stderr
    );
    let done = run_ae(
        r#"input_progress(4, 4).percent"#,
        Input::Eof,
        "progress_done",
    );
    assert!(done.stderr.ends_with("100%\n"), "stderr: {:?}", done.stderr);
}

#[test]
fn refused_arguments_are_coded_and_never_read_stdin() {
    // Stdin is at end of input, so a read would fail with E_NO_UI instead of
    // E_BAD_ARG; any E_BAD_ARG here proves the refusal came first.
    let cases = [
        r#"(try { input_timeout("x", 0) } catch e { e.error.code })"#,
        r#"(try { input_timeout("x", 3600001) } catch e { e.error.code })"#,
        r#"(try { input_timeout(5, 100) } catch e { e.error.code })"#,
        r#"(try { input_progress(3, 2) } catch e { e.error.code })"#,
        r#"(try { input_progress(1, 0) } catch e { e.error.code })"#,
    ];
    for expr in cases {
        let r = run_ae(expr, Input::Eof, "refusals");
        assert!(r.status.success(), "`{expr}` stderr: {}", r.stderr);
        assert_eq!(r.stdout.trim_end(), "E_BAD_ARG", "`{expr}`");
    }
}

/// The piped REPL reads its own lines from stdin. A `prompt` answered in the
/// middle must take exactly its one line and leave stdin free, or the REPL
/// blocks on the stdin lock (or loses the line read ahead) and never runs
/// the rest.
#[test]
fn the_piped_repl_reads_on_after_a_prompt() {
    let script = "x = prompt(\"\")\nanswered\nprint(\"got \" + x)\nprint(\"after\")\n";
    let r = run_args(&[], Input::Piped(script.as_bytes().to_vec()), "repl");
    assert!(
        r.stdout.contains("got answered") && r.stdout.contains("after"),
        "stdout: {}\nstderr: {}",
        r.stdout,
        r.stderr
    );
}
