//! The host backend for the POSIX subset: `std` for files, processes and the
//! shell's streams, chrono for time, libc for locales and account names, and
//! the shell's own policy (`safety` and the journal) as the [`Gate`].
//!
//! The trait and its plain-data types live in `ironshell-posix`, which builds
//! without `std`. Everything here is the one place that subset meets `std`.
//!
//! The host reports what the subset reported when it called `std` directly:
//! the same error kinds, and the same message text ([`OsError::message`] is
//! `std`'s own), so the utilities print the same bytes.

use std::io::{BufRead, Read, Write};

pub use ironshell_posix::os::{
    Captured, CopyStep, DirEntry, ErrorKind, FileKind, Gate, Meta, Os, OsError, Proc, Refusal,
    Spawn, SpawnError, Stream, Timestamp, Uname, WriteError,
};
use ironshell_posix::{copy_steps, remove_tree};

use crate::journal;
use crate::safety;
use crate::value::Value;

/// The backend on the Rust standard library, and the shell's own policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostOs;

/// The error for an operation this host does not support.
fn unsupported_here() -> OsError {
    OsError {
        kind: ErrorKind::Other,
        errno: None,
        message: String::from("not supported by this host"),
    }
}

/// The trimmed text of a file, or `None` when it cannot be read or is empty.
fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// One `/proc/PID/stat` line. The command name sits between the first `(` and
/// the last `)`, since it may hold either; the parent's id follows the state.
fn parse_proc_stat(pid: u32, text: &str) -> Option<Proc> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_string();
    let mut rest = text.get(close + 1..)?.split_whitespace();
    let _state = rest.next()?;
    let ppid = rest.next()?.parse().ok()?;
    Some(Proc { pid, ppid, comm })
}

/// The host error for a failed `std` call.
fn os_error(e: &std::io::Error) -> OsError {
    use std::io::ErrorKind as K;
    let kind = match e.kind() {
        K::NotFound => ErrorKind::NotFound,
        K::PermissionDenied => ErrorKind::PermissionDenied,
        K::IsADirectory => ErrorKind::IsADirectory,
        K::NotADirectory => ErrorKind::NotADirectory,
        K::AlreadyExists => ErrorKind::AlreadyExists,
        _ => ErrorKind::Other,
    };
    OsError {
        kind,
        errno: e.raw_os_error(),
        message: e.to_string(),
    }
}

fn kind_of(ft: std::fs::FileType) -> FileKind {
    if ft.is_dir() {
        FileKind::Dir
    } else if ft.is_symlink() {
        FileKind::Symlink
    } else if ft.is_file() {
        FileKind::File
    } else {
        special_kind(ft)
    }
}

#[cfg(unix)]
fn special_kind(ft: std::fs::FileType) -> FileKind {
    use std::os::unix::fs::FileTypeExt;
    if ft.is_block_device() {
        FileKind::BlockDevice
    } else if ft.is_char_device() {
        FileKind::CharDevice
    } else if ft.is_fifo() {
        FileKind::Fifo
    } else if ft.is_socket() {
        FileKind::Socket
    } else {
        FileKind::Other
    }
}

#[cfg(not(unix))]
fn special_kind(_ft: std::fs::FileType) -> FileKind {
    FileKind::Other
}

fn meta_of(m: &std::fs::Metadata) -> Meta {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Meta {
            kind: kind_of(m.file_type()),
            len: m.len(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            nlink: m.nlink(),
            blocks: m.blocks(),
            mtime: m.mtime(),
        }
    }
    #[cfg(not(unix))]
    {
        Meta {
            kind: kind_of(m.file_type()),
            len: m.len(),
            mode: 0,
            uid: 0,
            gid: 0,
            nlink: 0,
            blocks: 0,
            mtime: 0,
        }
    }
}

impl Os for HostOs {
    fn metadata(&self, path: &str) -> Result<Meta, OsError> {
        std::fs::metadata(path)
            .map(|m| meta_of(&m))
            .map_err(|e| os_error(&e))
    }

    fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError> {
        std::fs::symlink_metadata(path)
            .map(|m| meta_of(&m))
            .map_err(|e| os_error(&e))
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError> {
        let rd = std::fs::read_dir(path).map_err(|e| os_error(&e))?;
        Ok(rd
            .filter_map(|e| e.ok())
            .map(|e| DirEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                kind: e.file_type().ok().map(kind_of),
            })
            .collect())
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError> {
        std::fs::read(path).map_err(|e| os_error(&e))
    }

    fn write_file(&self, path: &str, data: &[u8], append: bool) -> Result<(), OsError> {
        let res = if append {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(data))
        } else {
            std::fs::write(path, data)
        };
        res.map_err(|e| os_error(&e))
    }

    fn remove(&self, path: &str, dir: bool) -> Result<(), OsError> {
        if dir {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        }
        .map_err(|e| os_error(&e))
    }

    fn create_dir(&self, path: &str) -> Result<(), OsError> {
        std::fs::create_dir(path).map_err(|e| os_error(&e))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), OsError> {
        std::fs::rename(from, to).map_err(|e| os_error(&e))
    }

    fn read_link(&self, path: &str) -> Result<String, OsError> {
        std::fs::read_link(path)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| os_error(&e))
    }

    fn canonicalize(&self, path: &str) -> Result<String, OsError> {
        std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| os_error(&e))
    }

    fn current_dir(&self) -> Result<String, OsError> {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| os_error(&e))
    }

    fn set_current_dir(&self, path: &str) -> Result<(), OsError> {
        std::env::set_current_dir(path).map_err(|e| os_error(&e))
    }

    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// Sorted, so that `env` prints the same order every run. Lossy UTF-8:
    /// `std::env::vars` would panic on a value that is not Unicode.
    /// The environment a child of this shell receives, which is what `env`
    /// prints: the shell's own `IRONSHELL_*` settings, and in agent mode its
    /// credentials, are removed by the same [`safety::strip_ironshell_env`] the
    /// child's spawn applies. Listing the shell's environment instead showed a
    /// variable no child would ever see.
    fn env_vars(&self) -> Vec<(String, String)> {
        let mut probe = std::process::Command::new("env");
        safety::strip_ironshell_env(&mut probe);
        let mut vars: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> =
            std::env::vars_os().collect();
        for (name, value) in probe.get_envs() {
            match value {
                Some(v) => {
                    vars.insert(name.to_os_string(), v.to_os_string());
                }
                None => {
                    vars.remove(name);
                }
            }
        }
        vars.into_iter()
            .map(|(n, v)| {
                (
                    n.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    fn process_id(&self) -> u32 {
        std::process::id()
    }

    fn run_program(&self, spec: Spawn) -> Result<Captured, SpawnError> {
        use std::process::{Command, Stdio};
        let mut cmd = Command::new(&spec.argv[0]);
        cmd.args(&spec.argv[1..]);
        for (n, v) in &spec.env {
            cmd.env(n, v);
        }
        safety::strip_ironshell_env(&mut cmd);
        cmd.stdout(Stdio::piped());
        cmd.stderr(match spec.stderr {
            Stream::Inherit => Stdio::inherit(),
            Stream::Null => Stdio::null(),
            Stream::Piped => Stdio::piped(),
        });
        cmd.stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
        let mut child = cmd.spawn().map_err(|e| SpawnError::Start(os_error(&e)))?;
        // Feeding stdin on a thread: a child that fills its stdout before it
        // reads all of its input would otherwise block both processes.
        let writer = match spec.stdin {
            Some(data) => child.stdin.take().map(|mut w| {
                std::thread::spawn(move || {
                    let _ = w.write_all(&data);
                })
            }),
            None => None,
        };
        let out = child
            .wait_with_output()
            .map_err(|e| SpawnError::Wait(os_error(&e)))?;
        if let Some(w) = writer {
            let _ = w.join();
        }
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&out.status);
        #[cfg(not(unix))]
        let signal = None;
        Ok(Captured {
            status: out.status.code(),
            signal,
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    fn write_stdout(&self, bytes: &[u8]) {
        let _ = std::io::stdout().write_all(bytes);
        let _ = std::io::stdout().flush();
    }

    fn write_stderr(&self, bytes: &[u8]) {
        let _ = std::io::stderr().write_all(bytes);
    }

    fn read_stdin_line(&self, limit: usize) -> Option<Vec<u8>> {
        let mut chunk = Vec::new();
        match std::io::stdin()
            .lock()
            .take(limit as u64)
            .read_until(b'\n', &mut chunk)
        {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(chunk),
        }
    }

    fn read_stdin_to_end(&self, out: &mut Vec<u8>) {
        let _ = std::io::stdin().read_to_end(out);
    }

    fn now(&self) -> Timestamp {
        let now = chrono::Local::now();
        Timestamp {
            secs: now.timestamp(),
            nanos: now.timestamp_subsec_nanos(),
        }
    }

    fn format_local(&self, at: Timestamp, format: &str) -> Option<String> {
        let t = chrono::DateTime::from_timestamp(at.secs, at.nanos)?.with_timezone(&chrono::Local);
        // chrono panics on a format it cannot parse; ask it first.
        use chrono::format::{Item, StrftimeItems};
        if StrftimeItems::new(format).any(|i| matches!(i, Item::Error)) {
            return None;
        }
        Some(t.format(format).to_string())
    }

    fn locale_available(&self, name: &str) -> bool {
        locale_loads(name)
    }

    fn has_unix_metadata(&self) -> bool {
        cfg!(unix)
    }

    #[cfg(unix)]
    fn user_name(&self, uid: u32) -> Option<String> {
        let mut buf = vec![0 as libc::c_char; 4096];
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: getpwuid_r writes into the caller's buffers and sets `result`
        // to `&pwd` or null; `pw_name` points into `buf`, which outlives the read.
        let rc =
            unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result) };
        if rc == 0 && !result.is_null() {
            // SAFETY: as above; the name is a NUL-terminated string within `buf`.
            let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
            return Some(name.to_string_lossy().into_owned());
        }
        None
    }

    #[cfg(unix)]
    fn group_name(&self, gid: u32) -> Option<String> {
        let mut buf = vec![0 as libc::c_char; 4096];
        let mut grp: libc::group = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::group = std::ptr::null_mut();
        // SAFETY: getgrgid_r writes into the caller's buffers and sets `result`
        // to `&grp` or null; `gr_name` points into `buf`, which outlives the read.
        let rc =
            unsafe { libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr(), buf.len(), &mut result) };
        if rc == 0 && !result.is_null() {
            // SAFETY: as above.
            let name = unsafe { std::ffi::CStr::from_ptr(grp.gr_name) };
            return Some(name.to_string_lossy().into_owned());
        }
        None
    }

    /// Pauses this thread. `sleep` checks the length before it asks.
    fn sleep(&self, d: std::time::Duration) -> Result<(), OsError> {
        std::thread::sleep(d);
        Ok(())
    }

    /// The name the host gives itself, from `std` alone. The system name comes
    /// from the target OS, the machine from its architecture, and the node name
    /// from `COMPUTERNAME` on Windows or `/proc/sys/kernel/hostname` on Linux.
    /// Only Linux reports a release (`/proc/sys/kernel/osrelease`); elsewhere
    /// it is `None`, and `uname -r` and `-a` decline.
    fn uname(&self) -> Result<Uname, OsError> {
        let sysname = match std::env::consts::OS {
            "linux" | "android" => "Linux",
            "macos" => "Darwin",
            "windows" => "Windows_NT",
            "freebsd" => "FreeBSD",
            "openbsd" => "OpenBSD",
            "netbsd" => "NetBSD",
            _ => return Err(unsupported_here()),
        };
        let machine = match std::env::consts::ARCH {
            "x86" => "i686",
            other => other,
        };
        let node = if cfg!(windows) {
            std::env::var("COMPUTERNAME").ok().filter(|s| !s.is_empty())
        } else {
            read_trim("/proc/sys/kernel/hostname")
        };
        let Some(nodename) = node else {
            return Err(unsupported_here());
        };
        let release = if cfg!(target_os = "linux") {
            read_trim("/proc/sys/kernel/osrelease")
        } else {
            None
        };
        Ok(Uname {
            sysname: sysname.into(),
            nodename,
            release,
            machine: machine.into(),
        })
    }

    /// The account entry for the effective user, as GNU `whoami` reports it, on
    /// Unix; `USER` or `LOGNAME` only when the uid has no entry, since after
    /// `su` they still name the previous user. `USERNAME` on Windows.
    #[cfg(unix)]
    fn user_name_current(&self) -> Option<String> {
        // SAFETY: geteuid has no preconditions and cannot fail.
        let uid = unsafe { libc::geteuid() };
        self.user_name(uid).or_else(|| {
            ["USER", "LOGNAME"]
                .into_iter()
                .find_map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()))
        })
    }

    #[cfg(not(unix))]
    fn user_name_current(&self) -> Option<String> {
        std::env::var("USERNAME").ok().filter(|v| !v.is_empty())
    }

    fn can_signal(&self) -> bool {
        cfg!(unix)
    }

    /// Sends a signal with `kill(2)`, through libc, which this crate already
    /// depends on for account names. No program is run.
    #[cfg(unix)]
    fn kill(&self, pid: u32, signal: i32) -> Result<(), OsError> {
        let Ok(p) = i32::try_from(pid) else {
            return Err(OsError {
                kind: ErrorKind::NotFound,
                errno: Some(3),
                message: String::from("No such process"),
            });
        };
        // SAFETY: kill takes a pid and a signal number, and reads no memory of ours.
        if unsafe { libc::kill(p, signal) } == 0 {
            Ok(())
        } else {
            Err(os_error(&std::io::Error::last_os_error()))
        }
    }

    #[cfg(not(unix))]
    fn kill(&self, _pid: u32, _signal: i32) -> Result<(), OsError> {
        Err(unsupported_here())
    }

    /// Every process under `/proc` whose `stat` reads, on Linux. A process that
    /// exits between the listing and the read is left out. Elsewhere, unsupported.
    fn processes(&self) -> Result<Vec<Proc>, OsError> {
        if !cfg!(target_os = "linux") {
            return Err(unsupported_here());
        }
        let rd = std::fs::read_dir("/proc").map_err(|e| os_error(&e))?;
        let mut out = Vec::new();
        for entry in rd.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(p) = parse_proc_stat(pid, &text) {
                    out.push(p);
                }
            }
        }
        Ok(out)
    }

    #[cfg(unix)]
    fn set_mode(&self, path: &str, mode: u32) -> Result<(), OsError> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| os_error(&e))
    }

    #[cfg(not(unix))]
    fn set_mode(&self, _path: &str, _mode: u32) -> Result<(), OsError> {
        Err(unsupported_here())
    }

    fn set_times(&self, path: &str, at: Timestamp) -> Result<(), OsError> {
        set_file_times(path, at)
    }

    fn gate(&self) -> &dyn Gate {
        self
    }
}

/// Sets the access and modification times of `path` to `at`, as `utimensat`
/// does for GNU `touch`. The file is opened only for its attributes, so a
/// read-only file can have its times set. On Unix it is opened without
/// blocking, so a FIFO is not waited for; on Windows a directory is opened
/// with backup semantics, so a directory can be touched too.
#[cfg(unix)]
fn set_file_times(path: &str, at: Timestamp) -> Result<(), OsError> {
    use std::os::unix::fs::OpenOptionsExt;
    let when = system_time(at)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| os_error(&e))?;
    file.set_times(
        std::fs::FileTimes::new()
            .set_accessed(when)
            .set_modified(when),
    )
    .map_err(|e| os_error(&e))
}

#[cfg(windows)]
fn set_file_times(path: &str, at: Timestamp) -> Result<(), OsError> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES, and FILE_FLAG_BACKUP_SEMANTICS
    // so that a directory opens.
    const ATTRIBUTES: u32 = 0x0080 | 0x0100;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let when = system_time(at)?;
    let file = std::fs::OpenOptions::new()
        .access_mode(ATTRIBUTES)
        .custom_flags(BACKUP_SEMANTICS)
        .open(path)
        .map_err(|e| os_error(&e))?;
    file.set_times(
        std::fs::FileTimes::new()
            .set_accessed(when)
            .set_modified(when),
    )
    .map_err(|e| os_error(&e))
}

#[cfg(not(any(unix, windows)))]
fn set_file_times(_path: &str, _at: Timestamp) -> Result<(), OsError> {
    Err(unsupported_here())
}

/// `at` as a `SystemTime`. A time before the epoch is refused: the shell's
/// own clock never gives one, and a `touch` to one is not a thing GNU does.
#[cfg(any(unix, windows))]
fn system_time(at: Timestamp) -> Result<std::time::SystemTime, OsError> {
    let secs = u64::try_from(at.secs).map_err(|_| OsError {
        kind: ErrorKind::Other,
        errno: None,
        message: "time before 1970-01-01 cannot be set".into(),
    })?;
    Ok(std::time::UNIX_EPOCH + std::time::Duration::new(secs, at.nanos))
}

/// The guard and the journal for a change one of the posix file utilities is
/// about to make. Returns the journal mark, to roll back to if the host fails.
///
/// The effect is `Destructive` for all four utilities, and it is deliberate.
/// `rm`, `cp`, `mv` and `mkdir` ran as external programs under `guard_exec`,
/// which approves them in agent mode. The typed builtins classify `cp` and
/// `mkdir` as `WriteLocal`, which agent mode allows outright, so using that
/// class here would let an agent create directories and copy files without
/// asking. `Destructive` keeps the approval. The honest class is a policy
/// decision, and it is the owner's to make.
///
/// The workspace jail is not applied, for the same reason: the external
/// program was not jailed, and jailing it would deny a path outside the
/// workspace that agent mode approves today. The typed builtins jail; the
/// posix path does not, and that difference predates this change.
fn admit_change(
    builtin: &str,
    what: &str,
    guard_targets: Vec<String>,
    journal_paths: &[String],
    blast_radius: serde_json::Value,
) -> Result<usize, WriteError> {
    safety::guard(safety::GuardCtx {
        builtin,
        effect: safety::Effect::Destructive,
        what,
        targets: guard_targets,
        blast_radius,
        reversible: false,
        fs_paths: false,
    })
    .map_err(|e| {
        WriteError::Refused(Refusal {
            message: e.to_string(),
            code: e.code.exit_code(),
        })
    })?;
    let mark = journal::mark();
    let args: Vec<Value> = journal_paths.iter().cloned().map(Value::Str).collect();
    journal::record_before(builtin, safety::Effect::Destructive, &args);
    Ok(mark)
}

/// The host's result for a change made after [`admit_change`]: a failure
/// takes back the journal entries recorded for it.
fn settle<T>(mark: usize, res: Result<T, OsError>) -> Result<T, WriteError> {
    res.map_err(|e| {
        journal::rollback_to(mark);
        WriteError::Io(e)
    })
}

impl Gate for HostOs {
    fn agent_mode(&self) -> bool {
        safety::current_mode() == safety::Mode::Agent
    }

    /// The paths are as the command gave them, not resolved against the
    /// workspace: the external program this replaces resolved them against the
    /// working directory, and so does the `tx` snapshot.
    fn guarded_remove(&self, os: &dyn Os, path: &str, recursive: bool) -> Result<(), WriteError> {
        let target = path.to_string();
        let mark = admit_change(
            "rm",
            "delete",
            vec![target.clone()],
            &[target],
            serde_json::json!({ "recursive": recursive }),
        )?;
        crate::tx::snapshot(path);
        settle(mark, remove_tree(os, path, recursive))
    }

    fn guarded_copy(&self, os: &dyn Os, plan: &[CopyStep]) -> Result<(), WriteError> {
        let dests: Vec<String> = plan.iter().map(|s| s.to.clone()).collect();
        let top = dests.first().cloned().unwrap_or_default();
        let mark = admit_change(
            "cp",
            "copy",
            vec![top],
            &dests,
            serde_json::json!({ "entries": plan.len() }),
        )?;
        for s in plan {
            // A directory that is there already is merged into, and not
            // snapshotted: a snapshot would journal everything in it.
            let merged = s.dir && os.metadata(&s.to).is_ok_and(|m| m.kind == FileKind::Dir);
            if !merged {
                crate::tx::snapshot(&s.to);
            }
        }
        settle(mark, copy_steps(os, plan))
    }

    fn guarded_create_dirs(&self, os: &dyn Os, dirs: &[String]) -> Result<(), WriteError> {
        let Some(top) = dirs.first() else {
            return Ok(());
        };
        let mark = admit_change(
            "mkdir",
            "create directory",
            vec![top.clone()],
            dirs,
            serde_json::json!({ "dirs": dirs.len() }),
        )?;
        // The highest directory is recorded as absent, so a rollback removes
        // every directory this call made.
        crate::tx::snapshot(top);
        settle(mark, dirs.iter().try_for_each(|d| os.create_dir(d)))
    }

    fn guarded_rename(&self, os: &dyn Os, from: &str, to: &str) -> Result<(), WriteError> {
        let both = [from.to_string(), to.to_string()];
        let mark = admit_change(
            "mv",
            "move",
            both.to_vec(),
            &both,
            serde_json::json!({ "from": from, "to": to }),
        )?;
        crate::tx::snapshot(from);
        crate::tx::snapshot(to);
        settle(mark, os.rename(from, to))
    }

    /// A signal reaches another process, so an agent may not send one. Nothing
    /// is journalled: a signal cannot be taken back.
    fn guarded_kill(&self, os: &dyn Os, pid: u32, signal: i32) -> Result<(), WriteError> {
        if self.agent_mode() {
            let refusal = safety::policy_deny(
                "posix",
                format!("kill {pid}: an agent may not signal a process"),
                "signals reach other processes; the owner approves them",
            );
            return Err(WriteError::Refused(Refusal {
                message: refusal.to_string(),
                code: 126,
            }));
        }
        os.kill(pid, signal).map_err(WriteError::Io)
    }

    /// A mode change goes through the same path check as a write, as a
    /// destructive change: agent mode asks the owner about it. It is not
    /// journalled, since the prior mode is not recorded, so it cannot be undone.
    fn guarded_chmod(&self, os: &dyn Os, path: &str, mode: u32) -> Result<(), WriteError> {
        let resolved = safety::resolve_path_str(path);
        safety::guard(safety::GuardCtx {
            builtin: "chmod",
            effect: safety::Effect::Destructive,
            what: "change mode",
            targets: vec![resolved.clone()],
            blast_radius: serde_json::json!({ "path": resolved, "mode": format!("{mode:o}") }),
            reversible: false,
            fs_paths: true,
        })
        .map_err(|e| {
            WriteError::Refused(Refusal {
                message: e.to_string(),
                code: e.code.exit_code(),
            })
        })?;
        os.set_mode(&resolved, mode).map_err(WriteError::Io)
    }

    /// Set the times of `path` to now for `touch`, through the guard as a
    /// write is. The old times cannot be journalled back, so the change is
    /// not reversible, and the guard is told so.
    fn guarded_touch(&self, os: &dyn Os, path: &str) -> Result<(), WriteError> {
        let resolved = safety::resolve_path_str(path);
        safety::guard(safety::GuardCtx {
            builtin: "posix",
            effect: safety::Effect::WriteLocal,
            what: "set times",
            targets: vec![resolved.clone()],
            blast_radius: serde_json::json!({ "path": resolved }),
            reversible: false,
            fs_paths: true,
        })
        .map_err(|e| {
            WriteError::Refused(Refusal {
                message: e.to_string(),
                code: e.code.exit_code(),
            })
        })?;
        os.set_times(&resolved, os.now()).map_err(WriteError::Io)
    }

    fn admit_exec(&self, argv: &str) -> Result<(), Refusal> {
        safety::guard_exec("posix", argv).map_err(|e| Refusal {
            message: e.to_string(),
            code: e.code.exit_code(),
        })
    }

    fn refuse_gate_variable(&self, name: &str) -> Refusal {
        let refusal = safety::policy_deny(
            "posix",
            format!("{name}: an agent may not pass an IRONSHELL_* variable to a child process"),
            "IRONSHELL_* variables are set by whoever launches the agent",
        );
        Refusal {
            message: refusal.to_string(),
            code: 126,
        }
    }

    /// The write goes to the path the guard checked. It went to `path` as
    /// given, which the OS resolved against the working directory while the
    /// guard, in agent mode, had resolved it against the workspace: with the
    /// two apart, `echo x > f` was approved as `<workspace>/f` and wrote
    /// `<cwd>/f`, outside the jail. The prior contents are journalled first, as
    /// the dispatcher does for `file.write`, so the `reversible` the guard is
    /// told is true.
    fn guarded_write(
        &self,
        os: &dyn Os,
        path: &str,
        data: &[u8],
        append: bool,
    ) -> Result<(), WriteError> {
        let resolved = safety::resolve_path_str(path);
        safety::guard(safety::GuardCtx {
            builtin: "posix",
            effect: safety::Effect::WriteLocal,
            what: "write",
            targets: vec![resolved.clone()],
            blast_radius: serde_json::json!({ "path": resolved }),
            reversible: true,
            fs_paths: true,
        })
        .map_err(|e| {
            WriteError::Refused(Refusal {
                message: e.to_string(),
                code: e.code.exit_code(),
            })
        })?;
        let mark = journal::mark();
        journal::record_before(
            "posix",
            safety::Effect::WriteLocal,
            &[Value::Str(resolved.clone())],
        );
        let res = os.write_file(&resolved, data, append);
        if res.is_err() {
            journal::rollback_to(mark);
        }
        res.map_err(WriteError::Io)
    }
}

/// Whether glibc can load the collation the environment names. When it cannot
/// (the locale was never generated), `ls` and `sort` fall back to bytes, so
/// output meant to match theirs must too. Cached per name: a sort asks once
/// per comparison.
#[cfg(target_os = "linux")]
fn locale_loads(name: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static SEEN: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&ok) = seen.as_ref().and_then(|m| m.get(name)) {
        return ok;
    }
    // `newlocale` with "" resolves the same variables `setlocale` would,
    // without touching the process locale.
    let ok = unsafe {
        let l = libc::newlocale(libc::LC_COLLATE_MASK, c"".as_ptr(), std::ptr::null_mut());
        if l.is_null() {
            false
        } else {
            libc::freelocale(l);
            true
        }
    };
    seen.get_or_insert_with(HashMap::new)
        .insert(name.to_string(), ok);
    ok
}

#[cfg(not(target_os = "linux"))]
fn locale_loads(_name: &str) -> bool {
    true
}
