//! Bounded runs of long-running external tools: `iperf3 -s`, `entr`,
//! `watchexec` and `nodemon`.
//!
//! Each of these never exits on its own, so each call runs its tool for a
//! caller-chosen deadline and then kills it. The child is started from an argv
//! (never a shell), in its own process group, with stdin null or a fixed
//! payload, and its output is kept to the last 64 KiB per stream. On timeout
//! the whole process tree is killed and reaped before the call returns, so no
//! watcher or server is left running behind the result.
use crate::safety;
use crate::value::Value;
use anyhow::Result;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const IPERF3: &str = "iperf3_server";
const ENTR: &str = "entr_watch";
const WATCHEXEC: &str = "watchexec_run";
const NODEMON: &str = "nodemon_run";

const MAX_ARGV: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const MAX_PATHS: usize = 64;
const OUTPUT_KEEP: usize = 65_536;

fn bad(name: &str, message: impl Into<String>) -> anyhow::Error {
    safety::arg_err(format!("{name}: {}", message.into()))
}

fn over_limit(name: &str, message: impl Into<String>, hint: &str) -> anyhow::Error {
    safety::budget_exceeded(name, message.into(), hint)
}

/// Arity and pipeline-input check shared by all four builtins.
fn check_shape(
    name: &str,
    args: &[Value],
    input: &Option<Value>,
    lo: usize,
    hi: usize,
) -> Result<()> {
    if input.is_some() {
        return Err(bad(
            name,
            "direct call only; pipeline input is not accepted",
        ));
    }
    if !(lo..=hi).contains(&args.len()) {
        return Err(bad(
            name,
            format!("expected {lo} to {hi} arguments, got {}", args.len()),
        ));
    }
    Ok(())
}

/// Starting a process that is not rolled back would leave it running after the
/// transaction discards its other effects.
fn refuse_in_transaction(name: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(safety::unimplemented(
            name,
            "running an external tool cannot be rolled back",
            "finish the transaction first",
        ));
    }
    Ok(())
}

/// The options Record, with every key checked against the allowed list.
fn read_options(
    name: &str,
    value: Option<&Value>,
    allowed: &[&str],
) -> Result<BTreeMap<String, Value>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let Value::Record(map) = value else {
        return Err(bad(name, "options must be a Record"));
    };
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(bad(
                name,
                format!("unknown option {key}; expected {}", allowed.join(", ")),
            ));
        }
    }
    Ok(map.clone())
}

fn int_in(name: &str, key: &str, value: &Value, lo: i64, hi: i64) -> Result<i64> {
    let Value::Int(n) = value else {
        return Err(bad(name, format!("option {key} must be an Int")));
    };
    if *n < lo || *n > hi {
        return Err(bad(
            name,
            format!("option {key} must be in {lo}..={hi}, got {n}"),
        ));
    }
    Ok(*n)
}

fn timeout_of(
    name: &str,
    opts: &BTreeMap<String, Value>,
    lo: i64,
    hi: i64,
    default: i64,
) -> Result<Duration> {
    let ms = match opts.get("timeout_ms") {
        Some(v) => int_in(name, "timeout_ms", v, lo, hi)?,
        None => default,
    };
    // Clamped to any evaluation deadline the caller is under.
    safety::bounded_io_timeout(name, Duration::from_millis(ms as u64))
}

/// A command argv: `argv[0]` is an absolute existing file or a plain name with
/// no path separator and no leading `-`. Later elements are the program's own
/// arguments and are passed after `--` or as argv, never through a shell.
fn command(name: &str, value: &Value) -> Result<Vec<String>> {
    let Value::Array(items) = value else {
        return Err(bad(name, "command must be an Array of String"));
    };
    if items.is_empty() {
        return Err(bad(name, "command must name a program"));
    }
    if items.len() > MAX_ARGV {
        return Err(over_limit(
            name,
            format!("command has more than {MAX_ARGV} elements"),
            "pass a shorter argv",
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Str(text) = item else {
            return Err(bad(name, "command elements must be String"));
        };
        if text.contains('\0') {
            return Err(bad(name, "command elements must not contain NUL"));
        }
        if text.len() > MAX_ARG_BYTES {
            return Err(over_limit(
                name,
                format!("a command element exceeds {MAX_ARG_BYTES} bytes"),
                "pass shorter arguments",
            ));
        }
        out.push(text.clone());
    }
    let program = &out[0];
    if program.starts_with('-') {
        return Err(bad(
            name,
            format!("command program must not start with '-': {program}"),
        ));
    }
    let path = Path::new(program);
    if path.is_absolute() {
        if !path.is_file() {
            return Err(bad(
                name,
                format!("command program does not exist: {program}"),
            ));
        }
    } else if program.contains('/') || program.contains('\\') {
        return Err(bad(
            name,
            format!("command program must be an absolute path or a plain name, got {program}"),
        ));
    }
    Ok(out)
}

/// Watched paths, each of which must exist. `as_argv` says whether the paths
/// become argv elements (and so must not look like options) or are written to
/// the tool's stdin (and so must not contain a line break).
fn paths(name: &str, value: &Value, as_argv: bool) -> Result<Vec<String>> {
    let Value::Array(items) = value else {
        return Err(bad(name, "paths must be an Array of String"));
    };
    if items.is_empty() {
        return Err(bad(name, "paths must name at least one file or directory"));
    }
    if items.len() > MAX_PATHS {
        return Err(over_limit(
            name,
            format!("paths has more than {MAX_PATHS} entries"),
            "watch fewer paths",
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Str(text) = item else {
            return Err(bad(name, "paths elements must be String"));
        };
        if text.is_empty() || text.contains('\0') || text.contains('\n') || text.contains('\r') {
            return Err(bad(
                name,
                "paths elements must be non-empty and free of NUL and line breaks",
            ));
        }
        if std::fs::symlink_metadata(text).is_err() {
            return Err(bad(name, format!("path does not exist: {text}")));
        }
        out.push(text.clone());
    }
    if as_argv {
        safety::reject_option_like(name, &out)?;
    }
    Ok(out)
}

/// Captures one pipe on its own thread, keeping only the last `OUTPUT_KEEP`
/// bytes so a chatty tool cannot exhaust memory.
struct Capture {
    buf: Arc<Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
}

fn capture<R: Read + Send + 'static>(pipe: Option<R>) -> Capture {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let (tx, done) = mpsc::channel();
    if let Some(mut pipe) = pipe {
        let shared = Arc::clone(&buf);
        thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut kept = shared.lock().unwrap_or_else(PoisonError::into_inner);
                        kept.extend_from_slice(&chunk[..n]);
                        if kept.len() > OUTPUT_KEEP {
                            let excess = kept.len() - OUTPUT_KEEP;
                            kept.drain(..excess);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = tx.send(());
        });
    }
    Capture { buf, done }
}

impl Capture {
    /// The retained text. A grandchild that holds the pipe open cannot delay
    /// the caller past two seconds; whatever arrived by then is returned.
    fn finish(self) -> String {
        let _ = self.done.recv_timeout(Duration::from_secs(2));
        let kept = self.buf.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&kept).into_owned()
    }
}

/// Kills the child and everything it started, then reaps the child.
///
/// Unix: the child leads its own process group (see `run_bounded`), so the
/// whole group is signalled. Windows: `taskkill /T` walks the tree, which only
/// works while the child is still alive, so it runs before `kill`.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: a negative pid addresses the process group that
        // `process_group(0)` made the child's own; kill has no memory effects.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// After a normal exit, members of the group that the leader started may still
/// be running. Signal the group too (Unix only; on Windows the tree is only
/// walked while the child is alive, so this is a no-op there).
fn reap_group(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: as in `kill_tree`; the group id is the reaped leader's pid,
        // which the kernel keeps reserved while any member of the group lives.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = child;
}

/// What a bounded run observed.
pub(super) struct Finished {
    exit_code: Option<i32>,
    timed_out: bool,
    duration_ms: i64,
    stdout: String,
    stderr: String,
}

impl Finished {
    /// The common record: `exit_code` is Int only for a run that exited on its
    /// own; a run killed at the deadline has Null.
    fn into_fields(self) -> BTreeMap<String, Value> {
        let exit_code = match self.exit_code {
            Some(code) if !self.timed_out => Value::Int(i64::from(code)),
            _ => Value::Null,
        };
        let mut m = BTreeMap::new();
        m.insert("exit_code".to_string(), exit_code);
        m.insert("timed_out".to_string(), Value::Bool(self.timed_out));
        m.insert("duration_ms".to_string(), Value::Int(self.duration_ms));
        m.insert("stdout".to_string(), Value::Str(self.stdout));
        m.insert("stderr".to_string(), Value::Str(self.stderr));
        m
    }
}

/// Runs `argv` for at most `limit`. `stdin` is a fixed payload written once and
/// closed; `None` gives the child a null stdin. The child is killed with its
/// tree at the deadline and is reaped before this returns.
pub(super) fn run_bounded(
    name: &str,
    argv: &[String],
    stdin: Option<String>,
    limit: Duration,
) -> Result<Finished> {
    let started = Instant::now();
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| safety::spawn_error(name, &argv[0], &e))?;
    if let (Some(payload), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // Detached: a child that never reads its stdin blocks this write until
        // the kill below closes the pipe, which then fails the write.
        thread::spawn(move || {
            let _ = pipe.write_all(payload.as_bytes());
        });
    }
    let out = capture(child.stdout.take());
    let err = capture(child.stderr.take());

    let exited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                return Err(e.into());
            }
        }
        if started.elapsed() >= limit {
            break None;
        }
        thread::sleep(Duration::from_millis(5));
    };
    let (exit_code, timed_out) = match exited {
        Some(status) => {
            reap_group(&mut child);
            (status.code(), false)
        }
        None => {
            kill_tree(&mut child);
            (None, true)
        }
    };
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    Ok(Finished {
        exit_code,
        timed_out,
        duration_ms,
        stdout: out.finish(),
        stderr: err.finish(),
    })
}

/// A bounded run that exited on its own with a nonzero status is a tool
/// failure, with the stderr tail. A run that hit its deadline is a result.
fn check_exit(name: &str, label: &str, finished: &Finished) -> Result<()> {
    if finished.timed_out || finished.exit_code == Some(0) {
        return Ok(());
    }
    Err(safety::tool_failed(
        name,
        label,
        finished.exit_code,
        &finished.stderr,
    ))
}

fn record(fields: BTreeMap<String, Value>) -> Value {
    Value::Record(fields)
}

/// `iperf3_server([options])`: one iperf3 test as a server, then exit.
pub(super) fn iperf3_server(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_shape(IPERF3, &args, &input, 0, 1)?;
    let opts = read_options(IPERF3, args.first(), &["bind", "port", "timeout_ms"])?;
    let mut bind: IpAddr = Ipv4Addr::LOCALHOST.into();
    if let Some(v) = opts.get("bind") {
        let Value::Str(text) = v else {
            return Err(bad(IPERF3, "option bind must be a String"));
        };
        bind = text.parse().map_err(|_| {
            bad(
                IPERF3,
                "option bind must be a literal IPv4 or IPv6 address; resolve hostnames separately",
            )
        })?;
    }
    let port = match opts.get("port") {
        Some(v) => int_in(IPERF3, "port", v, 1024, 65535)?,
        None => 5201,
    };
    let timeout = timeout_of(IPERF3, &opts, 1000, 600_000, 60_000)?;
    refuse_in_transaction(IPERF3)?;

    // Policy is enforced by the dispatcher, which guards every Network builtin
    // before it runs, loopback binds included.

    let argv = vec![
        "iperf3".to_string(),
        "-s".to_string(),
        "-1".to_string(),
        "-J".to_string(),
        "-p".to_string(),
        port.to_string(),
        "-B".to_string(),
        bind.to_string(),
    ];
    let finished = run_bounded(IPERF3, &argv, None, timeout)?;
    check_exit(IPERF3, "iperf3 -s -1 -J", &finished)?;

    let timed_out = finished.timed_out;
    let stdout = finished.stdout.clone();
    let mut fields = finished.into_fields();
    if timed_out {
        // No client connected, or the test had not finished: a result, not an
        // error. There is no measurement to report.
        for key in ["received_bytes", "seconds", "bits_per_second"] {
            fields.insert(key.to_string(), Value::Null);
        }
        return Ok(record(fields));
    }
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        safety::tool_failed(
            IPERF3,
            "iperf3 -s -1 -J",
            Some(0),
            &format!("output was not iperf3 JSON ({e})"),
        )
    })?;
    let sum = match json.pointer("/end/sum_received") {
        Some(sum) => sum,
        None => {
            let reason = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("no end.sum_received in the output");
            return Err(safety::tool_failed(
                IPERF3,
                "iperf3 -s -1 -J",
                Some(0),
                reason,
            ));
        }
    };
    let bytes = sum.get("bytes").and_then(|v| v.as_i64());
    let seconds = sum.get("seconds").and_then(|v| v.as_f64());
    let bps = sum.get("bits_per_second").and_then(|v| v.as_f64());
    let (Some(bytes), Some(seconds), Some(bps)) = (bytes, seconds, bps) else {
        return Err(safety::tool_failed(
            IPERF3,
            "iperf3 -s -1 -J",
            Some(0),
            "end.sum_received lacks bytes, seconds or bits_per_second",
        ));
    };
    fields.insert("received_bytes".to_string(), Value::Int(bytes));
    fields.insert("seconds".to_string(), Value::Float(seconds));
    fields.insert("bits_per_second".to_string(), Value::Float(bps));
    Ok(record(fields))
}

/// `entr_watch(paths, command, [options])`.
///
/// Runs `entr -n -- <command>` with `paths` written to its stdin. entr runs the
/// command once at start and again on every change to a listed path, until the
/// deadline kills entr and the command's process tree. `-p` is not passed, since
/// that would postpone the first run until a change.
pub(super) fn entr_watch(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_shape(ENTR, &args, &input, 2, 3)?;
    let watched = paths(ENTR, &args[0], false)?;
    let argv_cmd = command(ENTR, &args[1])?;
    let opts = read_options(ENTR, args.get(2), &["timeout_ms"])?;
    let timeout = timeout_of(ENTR, &opts, 100, 600_000, 30_000)?;
    refuse_in_transaction(ENTR)?;

    let mut argv = vec!["entr".to_string(), "-n".to_string(), "--".to_string()];
    argv.extend(argv_cmd);
    let stdin = format!("{}\n", watched.join("\n"));
    let finished = run_bounded(ENTR, &argv, Some(stdin), timeout)?;
    check_exit(ENTR, "entr", &finished)?;
    Ok(record(finished.into_fields()))
}

/// `watchexec_run(paths, command, [options])`.
///
/// Runs `watchexec --shell=none -w <path>... -- <command>`: the command runs at
/// start and on each change under the given paths, with no shell, until the
/// deadline kills the tree.
pub(super) fn watchexec_run(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_shape(WATCHEXEC, &args, &input, 2, 3)?;
    let watched = paths(WATCHEXEC, &args[0], true)?;
    let argv_cmd = command(WATCHEXEC, &args[1])?;
    let opts = read_options(WATCHEXEC, args.get(2), &["timeout_ms"])?;
    let timeout = timeout_of(WATCHEXEC, &opts, 100, 600_000, 30_000)?;
    refuse_in_transaction(WATCHEXEC)?;

    let mut argv = vec!["watchexec".to_string(), "--shell=none".to_string()];
    for path in &watched {
        argv.push("-w".to_string());
        argv.push(path.clone());
    }
    argv.push("--".to_string());
    argv.extend(argv_cmd);
    let finished = run_bounded(WATCHEXEC, &argv, None, timeout)?;
    check_exit(WATCHEXEC, "watchexec", &finished)?;
    Ok(record(finished.into_fields()))
}

/// `nodemon_run(script, [options])`.
///
/// Runs `nodemon --exitcrash --watch <dir> <script>`, where `dir` is the
/// script's own directory. nodemon runs the script at start and again on each
/// change in `dir`; a crash ends nodemon early. The deadline kills it otherwise.
pub(super) fn nodemon_run(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_shape(NODEMON, &args, &input, 1, 2)?;
    let Value::Str(script) = &args[0] else {
        return Err(bad(NODEMON, "script must be a String"));
    };
    if script.is_empty() || script.contains('\0') {
        return Err(bad(NODEMON, "script must be a non-empty path without NUL"));
    }
    safety::reject_option_like(NODEMON, std::slice::from_ref(script))?;
    let path = Path::new(script);
    if !path.is_file() {
        return Err(bad(
            NODEMON,
            format!("script does not exist as a file: {script}"),
        ));
    }
    let is_js = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("js"));
    if !is_js {
        return Err(bad(NODEMON, "script must be a .js file"));
    }
    let opts = read_options(NODEMON, args.get(1), &["timeout_ms"])?;
    let timeout = timeout_of(NODEMON, &opts, 100, 600_000, 30_000)?;
    refuse_in_transaction(NODEMON)?;

    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());

    let argv = vec![
        "nodemon".to_string(),
        "--exitcrash".to_string(),
        "--watch".to_string(),
        dir,
        script.clone(),
    ];
    let finished = run_bounded(NODEMON, &argv, None, timeout)?;
    check_exit(NODEMON, "nodemon", &finished)?;
    Ok(record(finished.into_fields()))
}
