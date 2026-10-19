//! Shared plumbing for the diagnostic builtins that wrap a host tool (blkid,
//! lsof, tcpdump, ethtool, perf): argument checks that run before any effect,
//! and a bounded runner that starts a program without a shell, caps what it
//! prints, kills its whole process group at the deadline, and reports how it
//! ended. The runner follows `test_watch::run_once`.
#![cfg_attr(not(unix), allow(dead_code))]

use crate::safety;
use crate::value::Value;
use anyhow::Result;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// Most output kept from one run. Output past it is drained and dropped, and
/// the run is marked `truncated`, which callers turn into a budget error.
pub(crate) const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_ARGV: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
/// How long a finished run may keep its pipes open before the group is killed.
const SETTLE: Duration = Duration::from_secs(2);

/// How a bounded run ended.
pub(crate) struct Run {
    /// The exit code; `None` when killed by a signal or at the deadline.
    pub(crate) code: Option<i32>,
    /// The deadline passed and the process group was killed.
    pub(crate) timed_out: bool,
    /// Output exceeded `MAX_OUTPUT` on either stream.
    pub(crate) truncated: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Refuse pipeline input: these builtins take their whole input as arguments.
pub(crate) fn no_pipeline_input(builtin: &str, input: &Option<Value>) -> Result<()> {
    if input.is_some() {
        return Err(safety::bad_arg(
            builtin,
            "a direct call; pipeline input is not accepted",
            "pipeline input",
        ));
    }
    Ok(())
}

/// A network interface name: 1 to 15 bytes (IFNAMSIZ), no option-like leading
/// `-`, and no characters a shell or a tool's own parser would give meaning.
pub(crate) fn interface_name(builtin: &str, name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.len() <= 15
        && !name.starts_with('-')
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'@' | b'-'));
    if plain {
        Ok(())
    } else {
        Err(safety::bad_arg(
            builtin,
            "a network interface name of 1 to 15 letters, digits, '_', '.', ':', '@' or '-', not starting with '-'",
            name,
        ))
    }
}

/// A block device path under /dev/, with no `..` component.
pub(crate) fn device_path(builtin: &str, path: &str) -> Result<()> {
    let plain = path.len() > 5
        && path.len() <= 256
        && path.starts_with("/dev/")
        && !path.split('/').any(|part| part == "..")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-:+@".contains(&b));
    if plain {
        Ok(())
    } else {
        Err(safety::bad_arg(
            builtin,
            "a device path under /dev/ (letters, digits and / . _ - : + @)",
            path,
        ))
    }
}

/// A perf event list: comma-separated event names such as `cycles` or
/// `cache-misses:u`, with no option-like leading `-` and no whitespace.
pub(crate) fn event_list(builtin: &str, events: &str) -> Result<()> {
    let plain = !events.is_empty()
        && events.len() <= 512
        && !events.starts_with('-')
        && events.split(',').all(|event| {
            !event.is_empty()
                && event.bytes().all(|b| {
                    b.is_ascii_alphanumeric()
                        || matches!(b, b'-' | b'_' | b':' | b'.' | b'=' | b'/')
                })
        });
    if plain {
        Ok(())
    } else {
        Err(safety::bad_arg(
            builtin,
            "a comma-separated list of event names such as \"cycles,instructions\"",
            events,
        ))
    }
}

/// A file path given as a positional argument: non-empty, bounded, no NUL, and
/// not option-like, since a leading `-` would be parsed as a flag by the tool.
pub(crate) fn file_path(builtin: &str, path: &str) -> Result<()> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return Err(safety::arg_err(format!(
            "{builtin}: a file path must be 1 to 4096 bytes without NUL"
        )));
    }
    safety::reject_option_like(builtin, &[path.to_string()])
}

/// The file an output is written to: a [`file_path`] whose directory exists.
pub(crate) fn output_path(builtin: &str, path: &str) -> Result<()> {
    file_path(builtin, path)?;
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        if !parent.is_dir() {
            return Err(safety::not_found(
                builtin,
                "directory",
                &parent.display().to_string(),
            ));
        }
    }
    Ok(())
}

/// An argv as an `Array` of `String`s: the program first, then its arguments.
/// Nothing is joined into a shell string; the program name is not option-like.
pub(crate) fn argv_arg(builtin: &str, value: &Value) -> Result<Vec<String>> {
    let items = match value {
        Value::Array(items) => items,
        other => {
            return Err(safety::bad_arg(
                builtin,
                "an argv Array of Strings, e.g. [\"/usr/bin/true\"]",
                other.type_name(),
            ))
        }
    };
    if items.is_empty() || items.len() > MAX_ARGV {
        return Err(safety::arg_err(format!(
            "{builtin}: argv must hold 1 to {MAX_ARGV} strings"
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        match item {
            Value::Str(s) if !s.is_empty() && s.len() <= MAX_ARG_BYTES && !s.contains('\0') => {
                out.push(s.clone())
            }
            Value::Str(_) => {
                return Err(safety::arg_err(format!(
                    "{builtin}: argv[{i}] must be 1 to {MAX_ARG_BYTES} bytes without NUL"
                )))
            }
            other => {
                return Err(safety::bad_arg(
                    builtin,
                    &format!("a String at argv[{i}]"),
                    other.type_name(),
                ))
            }
        }
    }
    safety::reject_option_like(builtin, &out[..1])?;
    Ok(out)
}

/// Split one column-aligned row into `n` columns. The first `n - 1` are
/// whitespace-separated fields and the last is the rest of the line, so a NAME
/// containing spaces stays whole and runs of padding create no empty columns.
pub(crate) fn columns(line: &str, n: usize) -> Vec<&str> {
    let mut out = Vec::with_capacity(n);
    let mut rest = line.trim_start();
    while out.len() + 1 < n {
        match rest.find(char::is_whitespace) {
            Some(i) => {
                out.push(&rest[..i]);
                rest = rest[i..].trim_start();
            }
            None => {
                out.push(rest);
                rest = "";
            }
        }
    }
    out.push(rest.trim_end());
    out.resize(n, "");
    out
}

/// Fail unless the run exited 0 within its limit and under the output cap.
/// A deadline kill and an overflow are budget errors; any other nonzero exit
/// (or death by signal) is a tool failure carrying the stderr text.
pub(crate) fn require_success(builtin: &str, tool: &str, run: &Run, limit: Duration) -> Result<()> {
    if run.timed_out {
        return Err(safety::budget_exceeded(
            builtin,
            format!(
                "`{tool}` was still running after {}s and was killed",
                limit.as_secs()
            ),
            "use a shorter command or a smaller request; the cap is fixed",
        ));
    }
    if run.truncated {
        return Err(safety::budget_exceeded(
            builtin,
            format!("`{tool}` wrote more than {MAX_OUTPUT} bytes"),
            "narrow what the request asks for",
        ));
    }
    if run.code != Some(0) {
        return Err(safety::tool_failed(builtin, tool, run.code, &run.stderr));
    }
    Ok(())
}

/// Run `program` with `args` directly (no shell), with stdin closed, and wait
/// at most `limit`. On the deadline the whole process group is killed.
pub(crate) fn run_bounded(
    builtin: &str,
    tool: &str,
    program: &str,
    args: &[String],
    limit: Duration,
) -> Result<Run> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| safety::spawn_error(builtin, tool, &e))?;
    let (out, out_done) = drain(child.stdout.take());
    let (err, err_done) = drain(child.stderr.take());

    let mut timed_out = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                return Err(e.into());
            }
        }
        if started.elapsed() >= limit {
            kill_tree(&mut child);
            timed_out = true;
            break None;
        }
        thread::sleep(Duration::from_millis(5));
    };

    // A descendant that outlived the child can hold a pipe open; the group is
    // killed rather than waited for.
    let out_closed = wait_for(&out_done);
    let err_closed = wait_for(&err_done);
    if !(out_closed && err_closed) {
        kill_tree(&mut child);
        wait_for(&out_done);
        wait_for(&err_done);
    }

    let (stdout, out_cut) = snapshot(&out);
    let (stderr, err_cut) = snapshot(&err);
    Ok(Run {
        code,
        timed_out,
        truncated: out_cut || err_cut,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

struct Kept {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Read a pipe to its end on a thread, keeping at most `MAX_OUTPUT` bytes but
/// draining the rest so the child never blocks on a full pipe. The receiver
/// gets one message when the pipe reaches end-of-file.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> (Arc<Mutex<Kept>>, mpsc::Receiver<()>) {
    let kept = Arc::new(Mutex::new(Kept {
        bytes: Vec::new(),
        truncated: false,
    }));
    let (tx, rx) = mpsc::channel();
    match pipe {
        Some(mut pipe) => {
            let shared = Arc::clone(&kept);
            thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            let mut k = shared.lock().unwrap_or_else(PoisonError::into_inner);
                            let room = MAX_OUTPUT.saturating_sub(k.bytes.len());
                            if n > room {
                                k.truncated = true;
                            }
                            let take = n.min(room);
                            k.bytes.extend_from_slice(&chunk[..take]);
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                let _ = tx.send(());
            });
        }
        None => {
            let _ = tx.send(());
        }
    }
    (kept, rx)
}

fn wait_for(done: &mpsc::Receiver<()>) -> bool {
    done.recv_timeout(SETTLE).is_ok()
}

fn snapshot(kept: &Arc<Mutex<Kept>>) -> (Vec<u8>, bool) {
    let k = kept.lock().unwrap_or_else(PoisonError::into_inner);
    (k.bytes.clone(), k.truncated)
}

/// Kill the child and everything in its process group, then reap it.
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
