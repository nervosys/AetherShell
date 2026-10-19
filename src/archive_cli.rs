//! archive_test for bzip2, xz and zstd, and for the tar archives inside them
//! (.tar.bz2, .tar.xz, .tar.zst). No compression crate is linked for these
//! formats, so the standard tools do the decoding.
//!
//! A file is checked against its magic bytes before any tool runs: a name and
//! a content that disagree are a coded refusal. A plain archive is tested with
//! the tool's own test mode (`bzip2 -t`, `xz -t`, `zstd -t`). A tar archive is
//! decompressed with `-dc`, and the decompressed stream is read to its end by
//! the tar crate.
//!
//! Tools run with argv only, never through a shell. Every run is bounded: a
//! deadline kills the whole process tree (the pattern in test_watch.rs), and
//! the decompressed size of a tar archive is capped. Hitting either bound is a
//! coded budget error, not an answer, because `false` would claim corruption
//! that was never found.
//!
//! Corrupt data is a result (`false`), as it is for zip and tar. A tool that
//! exits with any other status, or cannot start, is a coded error.

use anyhow::Result;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::safety;

/// Decompressed bytes a tar archive may produce before the test gives up.
const DECOMPRESSED_CAP: u64 = 4 << 30;
/// Wall-clock limit for one tool run, including the read of a tar stream.
const DEADLINE: Duration = Duration::from_secs(600);
/// Stderr kept for a failure message.
const STDERR_KEEP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    Bzip2,
    Xz,
    Zstd,
}

impl Codec {
    const ALL: [Codec; 3] = [Codec::Bzip2, Codec::Xz, Codec::Zstd];

    fn tool(self) -> &'static str {
        match self {
            Codec::Bzip2 => "bzip2",
            Codec::Xz => "xz",
            Codec::Zstd => "zstd",
        }
    }

    fn magic(self) -> &'static [u8] {
        match self {
            Codec::Bzip2 => b"BZh",
            Codec::Xz => &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00],
            Codec::Zstd => &[0x28, 0xB5, 0x2F, 0xFD],
        }
    }

    fn magic_text(self) -> &'static str {
        match self {
            Codec::Bzip2 => "\"BZh\"",
            Codec::Xz => "FD 37 7A 58 5A 00",
            Codec::Zstd => "28 B5 2F FD",
        }
    }

    /// The exit status that means the data is corrupt. bzip2 uses 2 for that
    /// (1 is an environmental problem). xz and zstd use 1 for every error.
    fn corrupt_exit(self) -> i32 {
        match self {
            Codec::Bzip2 => 2,
            Codec::Xz | Codec::Zstd => 1,
        }
    }
}

/// The codec a file name claims, and whether it wraps a tar archive.
pub(crate) fn named(path: &str) -> Option<(Codec, bool)> {
    let p = path.to_ascii_lowercase();
    Some(if p.ends_with(".tar.bz2") {
        (Codec::Bzip2, true)
    } else if p.ends_with(".tar.xz") {
        (Codec::Xz, true)
    } else if p.ends_with(".tar.zst") {
        (Codec::Zstd, true)
    } else if p.ends_with(".bz2") {
        (Codec::Bzip2, false)
    } else if p.ends_with(".xz") {
        (Codec::Xz, false)
    } else if p.ends_with(".zst") {
        (Codec::Zstd, false)
    } else {
        return None;
    })
}

/// Tests a bzip2, xz or zstd file, or the tar archive inside it.
///
/// `Ok(true)` means the tool accepted every byte. `Ok(false)` means the data
/// is corrupt or truncated. Refusals and tool failures are coded errors.
pub(crate) fn test(builtin: &str, path: &str, codec: Codec, tar: bool) -> Result<bool> {
    safety::reject_option_like(builtin, &[path.to_string()])?;
    let found = sniff(builtin, path)?;
    if found != Some(codec) {
        let got = match found {
            Some(other) => format!("{path}: {} magic bytes", other.magic_text()),
            None => format!("{path}: no bzip2, xz or zstd magic bytes"),
        };
        return Err(safety::bad_arg(
            builtin,
            &format!("a {} archive (magic {})", codec.tool(), codec.magic_text()),
            &got,
        ));
    }
    if tar {
        test_tar(builtin, path, codec)
    } else {
        test_stream(builtin, path, codec)
    }
}

/// The codec whose magic bytes the file starts with, if any.
fn sniff(builtin: &str, path: &str) -> Result<Option<Codec>> {
    let file = File::open(path).map_err(|e| safety::fs_error(builtin, path, &e))?;
    let mut head = Vec::with_capacity(8);
    file.take(8)
        .read_to_end(&mut head)
        .map_err(|e| safety::fs_error(builtin, path, &e))?;
    Ok(Codec::ALL.into_iter().find(|c| head.starts_with(c.magic())))
}

/// `tool -t [-q] -- path`: the tool's own integrity test.
fn test_stream(builtin: &str, path: &str, codec: Codec) -> Result<bool> {
    let mut argv = vec![codec.tool().to_string(), "-t".to_string()];
    if codec == Codec::Zstd {
        argv.push("-q".to_string());
    }
    argv.push("--".to_string());
    argv.push(path.to_string());

    let mut child = spawn(builtin, &argv, Stdio::null(), Stdio::null())?;
    let stderr = capture_stderr(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                return Err(safety::tool_failed(
                    builtin,
                    &argv.join(" "),
                    None,
                    &format!("could not wait for it: {e}"),
                ));
            }
        }
        if started.elapsed() >= DEADLINE {
            kill_tree(&mut child);
            return Err(overran(builtin, codec));
        }
        thread::sleep(Duration::from_millis(10));
    };
    verdict(builtin, codec, &argv, status, &stderr_text(stderr))
}

/// `tool -dc` with the file on stdin, the decompressed stream read to its end
/// as a tar archive. A deadline and a byte cap bound the read; a watchdog
/// thread kills the decompressor if the deadline passes while the read blocks.
fn test_tar(builtin: &str, path: &str, codec: Codec) -> Result<bool> {
    let argv = vec![codec.tool().to_string(), "-dc".to_string()];
    let input = File::open(path).map_err(|e| safety::fs_error(builtin, path, &e))?;
    let mut child = spawn(builtin, &argv, Stdio::from(input), Stdio::piped())?;
    let stderr = capture_stderr(child.stderr.take());
    let Some(stdout) = child.stdout.take() else {
        kill_tree(&mut child);
        return Err(safety::tool_failed(
            builtin,
            &argv.join(" "),
            None,
            "its stdout was not captured",
        ));
    };

    let slot = Arc::new(Mutex::new(Slot {
        child,
        finished: false,
        timed_out: false,
    }));
    let cap_hit = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = mpsc::channel::<()>();
    {
        let slot = Arc::clone(&slot);
        thread::spawn(move || {
            // Disconnected means the read finished and the sender was dropped.
            if let Err(mpsc::RecvTimeoutError::Timeout) = done_rx.recv_timeout(DEADLINE) {
                let mut slot = lock(&slot);
                if !slot.finished {
                    slot.timed_out = true;
                    kill_only(&mut slot.child);
                }
            }
        });
    }

    let read = read_tar(stdout, &cap_hit);
    let (status, timed_out) = {
        let mut slot = lock(&slot);
        slot.finished = true;
        if read.is_err() {
            // The read stopped early; the decompressor must not block on a
            // pipe nobody reads.
            kill_only(&mut slot.child);
        }
        (slot.child.wait(), slot.timed_out)
    };
    drop(done_tx);

    if timed_out {
        return Err(overran(builtin, codec));
    }
    if cap_hit.load(Ordering::SeqCst) {
        return Err(safety::budget_exceeded(
            builtin,
            format!(
                "the decompressed {} stream passed {} GiB",
                codec.tool(),
                DECOMPRESSED_CAP >> 30
            ),
            "the archive is too large to test in-process; it is neither confirmed nor refuted",
        ));
    }
    if read.is_err() {
        // The tar stream is malformed or truncated: corrupt data, a result.
        return Ok(false);
    }
    let status = status.map_err(|e| {
        safety::tool_failed(
            builtin,
            &argv.join(" "),
            None,
            &format!("could not wait for it: {e}"),
        )
    })?;
    verdict(builtin, codec, &argv, status, &stderr_text(stderr))
}

/// Reads a tar stream to its end. Entry contents are read and discarded; the
/// bytes after the end-of-archive marker are drained so the decompressor
/// finishes and its exit status means something.
fn read_tar(stdout: impl Read, cap_hit: &AtomicBool) -> std::io::Result<()> {
    let mut capped = Capped {
        inner: stdout,
        left: DECOMPRESSED_CAP,
        hit: cap_hit,
    };
    {
        let mut ar = tar::Archive::new(&mut capped);
        for entry in ar.entries()? {
            let mut entry = entry?;
            std::io::copy(&mut entry, &mut std::io::sink())?;
        }
    }
    std::io::copy(&mut capped, &mut std::io::sink())?;
    Ok(())
}

/// Passes at most `left` bytes through. One byte past the cap raises `hit`
/// and fails the read, so the caller can tell the cap from corruption.
struct Capped<'a, R> {
    inner: R,
    left: u64,
    hit: &'a AtomicBool,
}

impl<R: Read> Read for Capped<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            let mut probe = [0u8; 1];
            if self.inner.read(&mut probe)? == 0 {
                return Ok(0);
            }
            self.hit.store(true, Ordering::SeqCst);
            return Err(std::io::Error::new(
                ErrorKind::Other,
                "decompressed size cap reached",
            ));
        }
        let max = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..max])?;
        self.left -= n as u64;
        Ok(n)
    }
}

/// The child and the state the watchdog and the reader share. `finished` is
/// set under the lock before the child is reaped, so the watchdog never kills
/// a process that has already been waited for.
struct Slot {
    child: Child,
    finished: bool,
    timed_out: bool,
}

fn lock(slot: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

fn verdict(
    builtin: &str,
    codec: Codec,
    argv: &[String],
    status: ExitStatus,
    stderr: &str,
) -> Result<bool> {
    match status.code() {
        Some(0) => Ok(true),
        Some(code) if code == codec.corrupt_exit() => Ok(false),
        code => Err(safety::tool_failed(builtin, &argv.join(" "), code, stderr)),
    }
}

fn overran(builtin: &str, codec: Codec) -> anyhow::Error {
    safety::budget_exceeded(
        builtin,
        format!(
            "`{}` did not finish within {}s and was killed",
            codec.tool(),
            DEADLINE.as_secs()
        ),
        "the archive is neither confirmed nor refuted; test a smaller archive or test it with the tool directly",
    )
}

/// Spawns a tool from argv. The program is one of three fixed names; the only
/// caller value is a path, which the caller has already run through
/// reject_option_like and which follows a `--` where the tool takes one.
fn spawn(builtin: &str, argv: &[String], stdin: Stdio, stdout: Stdio) -> Result<Child> {
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(stdin)
        .stdout(stdout)
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|e| safety::spawn_error(builtin, &argv[0], &e))
}

/// Reads stderr on its own thread, keeping only the last `STDERR_KEEP` bytes.
fn capture_stderr(pipe: Option<ChildStderr>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 4096];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        kept.extend_from_slice(&chunk[..n]);
                        if kept.len() > STDERR_KEEP {
                            let excess = kept.len() - STDERR_KEEP;
                            kept.drain(..excess);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&kept).into_owned());
    });
    rx
}

/// The captured stderr. A grandchild holding the pipe open cannot delay the
/// caller past two seconds; whatever arrived by then is used.
fn stderr_text(rx: mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
}

/// Signals the child's whole process tree without reaping it. Unix: the child
/// leads its own process group (see `spawn`). Windows: `taskkill /T` walks
/// the tree, which only works while the child is still alive.
fn kill_only(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: a negative pid addresses the process group that `spawn`
        // made the child's own; kill has no memory effects.
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
}

/// Kills the process tree and reaps the child.
fn kill_tree(child: &mut Child) {
    kill_only(child);
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_codecs_and_tar_wrapping() {
        assert_eq!(named("a.tar.bz2"), Some((Codec::Bzip2, true)));
        assert_eq!(named("A.TAR.XZ"), Some((Codec::Xz, true)));
        assert_eq!(named("a.tar.zst"), Some((Codec::Zstd, true)));
        assert_eq!(named("a.bz2"), Some((Codec::Bzip2, false)));
        assert_eq!(named("a.xz"), Some((Codec::Xz, false)));
        assert_eq!(named("a.zst"), Some((Codec::Zstd, false)));
        assert_eq!(named("a.zip"), None);
        assert_eq!(named("a.tgz"), None);
    }
}
