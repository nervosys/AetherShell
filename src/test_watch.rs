//! Bounded re-run of one argv program when watched files change.
//!
//! Change detection polls: each `poll_ms` the watched paths are rescanned and
//! compared by relative path, length and modified time. No OS notification
//! API is used. Every child process is killed and waited for on timeout and
//! at the overall deadline.
use crate::safety;
use crate::value::Value;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const MAX_ARGV: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const MAX_PATHS: usize = 64;
const MAX_FILES: usize = 10_000;
const OUTPUT_KEEP: usize = 65_536;
const LISTED_CHANGES: usize = 32;

/// `(length, modified time)` per watched file, keyed by its path as given.
type Snapshot = BTreeMap<String, (u64, Option<SystemTime>)>;

struct Options {
    max_runs: usize,
    poll: Duration,
    idle: Duration,
    timeout: Duration,
    total: Duration,
}

struct Finished {
    exit_code: Option<i32>,
    timed_out: bool,
    duration_ms: i64,
    stdout: String,
    stderr: String,
}

impl Finished {
    fn passed(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }

    fn into_value(self, trigger: &str, changed: &[String]) -> Value {
        let listed: Vec<Value> = changed
            .iter()
            .take(LISTED_CHANGES)
            .map(|p| Value::Str(p.clone()))
            .collect();
        let exit_code = match self.exit_code {
            Some(code) if !self.timed_out => Value::Int(i64::from(code)),
            _ => Value::Null,
        };
        let mut m = BTreeMap::new();
        m.insert("trigger".to_string(), Value::Str(trigger.to_string()));
        m.insert("changed".to_string(), Value::Array(listed));
        m.insert(
            "changed_count".to_string(),
            Value::Int(changed.len() as i64),
        );
        m.insert("exit_code".to_string(), exit_code);
        m.insert("timed_out".to_string(), Value::Bool(self.timed_out));
        m.insert("duration_ms".to_string(), Value::Int(self.duration_ms));
        m.insert("stdout".to_string(), Value::Str(self.stdout));
        m.insert("stderr".to_string(), Value::Str(self.stderr));
        Value::Record(m)
    }
}

fn bad(name: &str, message: impl Into<String>) -> anyhow::Error {
    safety::arg_err(format!("{name}: {}", message.into()))
}

fn over_limit(name: &str, message: impl Into<String>, hint: &str) -> anyhow::Error {
    safety::budget_exceeded(name, message.into(), hint)
}

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
    items
        .iter()
        .map(|item| {
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
            Ok(text.clone())
        })
        .collect()
}

fn watched_paths(name: &str, value: &Value) -> Result<Vec<String>> {
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
        if text.contains('\0') {
            return Err(bad(name, "paths elements must not contain NUL"));
        }
        if fs::symlink_metadata(text).is_err() {
            return Err(bad(name, format!("path does not exist: {text}")));
        }
        out.push(text.clone());
    }
    Ok(out)
}

fn int_option(name: &str, key: &str, value: &Value, lo: i64, hi: i64) -> Result<i64> {
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

fn options(name: &str, value: Option<&Value>) -> Result<Options> {
    let mut o = Options {
        max_runs: 2,
        poll: Duration::from_millis(200),
        idle: Duration::from_millis(30_000),
        timeout: Duration::from_millis(300_000),
        total: Duration::from_millis(900_000),
    };
    let Some(value) = value else {
        return Ok(o);
    };
    let Value::Record(map) = value else {
        return Err(bad(name, "options must be a Record"));
    };
    for (key, v) in map {
        match key.as_str() {
            "max_runs" => o.max_runs = int_option(name, key, v, 1, 20)? as usize,
            "poll_ms" => o.poll = Duration::from_millis(int_option(name, key, v, 50, 5000)? as u64),
            "idle_ms" => {
                o.idle = Duration::from_millis(int_option(name, key, v, 100, 600_000)? as u64)
            }
            "timeout_ms" => {
                o.timeout = Duration::from_millis(int_option(name, key, v, 100, 1_800_000)? as u64)
            }
            "total_ms" => {
                o.total = Duration::from_millis(int_option(name, key, v, 100, 3_600_000)? as u64)
            }
            other => {
                return Err(bad(
                    name,
                    format!(
                        "unknown option {other}; expected max_runs, poll_ms, idle_ms, timeout_ms or total_ms"
                    ),
                ))
            }
        }
    }
    Ok(o)
}

fn add_file(name: &str, path: &Path, meta: &fs::Metadata, snap: &mut Snapshot) -> Result<()> {
    let key = path.to_string_lossy().into_owned();
    if !snap.contains_key(&key) && snap.len() >= MAX_FILES {
        return Err(over_limit(
            name,
            format!("watched paths contain more than {MAX_FILES} files"),
            "watch fewer or smaller directories",
        ));
    }
    snap.insert(key, (meta.len(), meta.modified().ok()));
    Ok(())
}

/// Scan a directory recursively. Symlinks are not followed or watched, and
/// entries named `target`, `.git` or `node_modules` are skipped. Entries that
/// vanish or cannot be read mid-scan are left out of this snapshot.
fn walk(name: &str, dir: &Path, snap: &mut Snapshot) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let skip = matches!(
            entry.file_name().to_str(),
            Some("target" | ".git" | "node_modules")
        );
        if skip {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            walk(name, &path, snap)?;
        } else if kind.is_file() {
            if let Ok(meta) = entry.metadata() {
                add_file(name, &path, &meta, snap)?;
            }
        }
    }
    Ok(())
}

fn scan(name: &str, root: &Path, snap: &mut Snapshot) -> Result<()> {
    // symlink_metadata: a symlinked root is recorded as a file, not followed.
    let meta = fs::symlink_metadata(root)
        .map_err(|_| bad(name, format!("path does not exist: {}", root.display())))?;
    if meta.is_dir() {
        fs::read_dir(root).map_err(|e| {
            bad(
                name,
                format!("cannot read directory {}: {e}", root.display()),
            )
        })?;
        walk(name, root, snap)
    } else {
        add_file(name, root, &meta, snap)
    }
}

fn snapshot(name: &str, paths: &[String]) -> Result<Snapshot> {
    let mut snap = Snapshot::new();
    for p in paths {
        scan(name, Path::new(p), &mut snap)?;
    }
    Ok(snap)
}

/// Sorted keys that were added, removed or whose length or mtime changed.
fn changed(old: &Snapshot, new: &Snapshot) -> Vec<String> {
    let mut out: Vec<String> = new
        .iter()
        .filter(|(k, v)| old.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect();
    out.extend(old.keys().filter(|k| !new.contains_key(*k)).cloned());
    out.sort();
    out.dedup();
    out
}

/// Reads a pipe on its own thread, keeping only the last `OUTPUT_KEEP` bytes.
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
    /// The retained text. A grandchild holding the pipe open cannot delay the
    /// caller past two seconds; whatever arrived by then is returned.
    fn finish(self) -> String {
        let _ = self.done.recv_timeout(Duration::from_secs(2));
        let kept = self.buf.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&kept).into_owned()
    }
}

/// Kills the child and every process it started, then reaps the child.
///
/// Unix: the child leads a process group of its own (see `run_once`), so the
/// whole group is signalled. Windows: `taskkill /T` walks the process tree,
/// which only works while the child is still alive, so it runs first.
fn kill_tree(child: &mut std::process::Child) {
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
        // Bounded: the shared helper gives taskkill a short limit.
        super::taskkill_tree(child.id());
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run_once(name: &str, argv: &[String], limit: Duration) -> Result<Finished> {
    let started = Instant::now();
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
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
        .map_err(|e| safety::spawn_error(name, &argv[0], &e))?;
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
        Some(status) => (status.code(), false),
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

/// The per-run limit: the run timeout, cut short by the overall deadline.
fn run_limit(deadline: Instant, timeout: Duration) -> Duration {
    timeout.min(deadline.saturating_duration_since(Instant::now()))
}

pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if input.is_some() {
        return Err(bad(
            name,
            "direct call only; pipeline input is not accepted",
        ));
    }
    if !(2..=3).contains(&args.len()) {
        return Err(bad(
            name,
            "expected command Array, paths Array and an optional options Record",
        ));
    }
    let argv = command(name, &args[0])?;
    let paths = watched_paths(name, &args[1])?;
    let opts = options(name, args.get(2))?;

    let started = Instant::now();
    let deadline = started + opts.total;
    let mut snap = snapshot(name, &paths)?;

    let first = run_once(name, &argv, run_limit(deadline, opts.timeout))?;
    let mut passed = first.passed();
    let mut last_finished = Instant::now();
    let mut runs = vec![first.into_value("initial", &[])];

    let stopped = loop {
        if runs.len() >= opts.max_runs {
            break "max_runs";
        }
        let idle_end = last_finished + opts.idle;
        let stop_at = idle_end.min(deadline);
        let stop_reason = if deadline <= idle_end {
            "deadline"
        } else {
            "idle"
        };

        // Poll until something changes or the stop point is reached.
        let diff = loop {
            let now = Instant::now();
            if now >= stop_at {
                break None;
            }
            thread::sleep(opts.poll.min(stop_at - now));
            if Instant::now() >= stop_at {
                break None;
            }
            let fresh = snapshot(name, &paths)?;
            let diff = changed(&snap, &fresh);
            if !diff.is_empty() {
                snap = fresh;
                break Some(diff);
            }
        };

        let Some(diff) = diff else {
            break stop_reason;
        };
        let f = run_once(name, &argv, run_limit(deadline, opts.timeout))?;
        passed = f.passed();
        last_finished = Instant::now();
        runs.push(f.into_value("change", &diff));
    };

    let mut out = BTreeMap::new();
    out.insert("runs".to_string(), Value::Array(runs));
    out.insert("stopped".to_string(), Value::Str(stopped.to_string()));
    out.insert("passed".to_string(), Value::Bool(passed));
    Ok(Value::Record(out))
}
