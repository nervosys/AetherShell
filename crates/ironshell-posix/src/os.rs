//! The operating system, as the POSIX subset sees it.
//!
//! [`Os`] is everything the subset asks of the host: reading and writing
//! files, listing directories, metadata, the working directory, one
//! environment variable at a time, the process id, the shell's own standard
//! streams, the clock, the locale and the account names, and running a
//! program to completion with its output captured. Its types are plain data
//! (no `std` types in any signature), so a backend without `std` can
//! implement it.
//!
//! [`Gate`] is the policy the shell consults before it acts: whether a
//! program may run, whether a redirection may write a file, and the mode the
//! shell is in. It is reached through [`Os::gate`], and its defaults fail
//! closed. A backend that supplies no gate refuses every program and every
//! file write, in every mode, rather than allowing them. The host backend in
//! iron-shell supplies the shell's own gate (`safety` and the journal), so
//! its behaviour is unchanged.
//!
//! Contract for a backend that spawns programs: it must apply the same
//! `IRONSHELL_*` stripping that `safety::strip_ironshell_env` applies to a
//! `std` child. The subset does not do that itself, so a backend that skips
//! it would hand a child the gate variables the shell removes.

use alloc::string::String;
use alloc::vec::Vec;

/// What kind of thing a path names. Mirrors the distinctions `ls -l` and
/// `find -type` draw; anything else is [`FileKind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Dir,
    Symlink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
    Other,
}

/// Metadata for one path. The Unix fields are zero on a host without them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub kind: FileKind,
    /// Size in bytes.
    pub len: u64,
    /// Permission and type bits, as `st_mode`.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    /// 512-byte blocks allocated.
    pub blocks: u64,
    /// Modification time, in seconds since the Unix epoch.
    pub mtime: i64,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    /// The entry's kind, not following symbolic links; `None` when the host
    /// could not say.
    pub kind: Option<FileKind>,
}

/// The errno-like category of a failed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    IsADirectory,
    NotADirectory,
    AlreadyExists,
    Other,
}

/// A failed operation: its category, the raw errno if the host gave one, and
/// the host's own text for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsError {
    pub kind: ErrorKind,
    pub errno: Option<i32>,
    /// The host's message. The printed reason strips a trailing
    /// `" (os error N)"` suffix, which `std` appends.
    pub message: String,
}

/// Where a child's standard error goes. Its standard output is always
/// captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// The shell's own standard error.
    Inherit,
    Null,
    /// Captured, and returned in [`Captured::stderr`].
    Piped,
}

/// A program to run to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    /// `argv[0]` is the program; no shell is involved.
    pub argv: Vec<String>,
    /// Variables to set on the child, applied in order, on top of the
    /// inherited environment.
    pub env: Vec<(String, String)>,
    pub stderr: Stream,
    /// `Some(bytes)` pipes them to the child's standard input and closes it;
    /// `None` gives the child the shell's standard input.
    pub stdin: Option<Vec<u8>>,
}

/// A child that ran to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    /// The exit code, or `None` when the child ended on a signal.
    pub status: Option<i32>,
    /// The signal that ended the child, when `status` is `None` and the host
    /// can say which one (a Unix host does; a host without signals says none).
    pub signal: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why [`Os::run_program`] did not produce a [`Captured`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnError {
    /// The child could not be started.
    Start(OsError),
    /// The child started but could not be waited for.
    Wait(OsError),
}

/// A point in time: whole seconds since the Unix epoch, and the nanoseconds
/// within that second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub secs: i64,
    pub nanos: u32,
}

/// A refusal from the [`Gate`]: the text to show and the exit status to
/// return. The gate builds it, so the subset never looks at why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub message: String,
    pub code: i32,
}

/// Why a gated write did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The gate did not admit the write.
    Refused(Refusal),
    /// The gate admitted it, and the host failed to write it.
    Io(OsError),
}

/// The policy the shell asks before it acts. Every method defaults to
/// refusing, in every mode: a backend that does not override this has no
/// policy, and with no policy nothing runs and nothing is written.
pub trait Gate {
    /// Whether the shell is in agent mode, the mode in which an agent's
    /// environment is restricted. Defaults to `true`, the stricter answer.
    fn agent_mode(&self) -> bool {
        true
    }

    /// Admit running `argv` (joined with single spaces) as a program. A
    /// refusal is printed and its code becomes the command's status.
    fn admit_exec(&self, argv: &str) -> Result<(), Refusal> {
        let _ = argv;
        Err(unconfigured())
    }

    /// The refusal for passing an `IRONSHELL_*` variable to a child in agent
    /// mode. `name` is the variable.
    fn refuse_gate_variable(&self, name: &str) -> Refusal {
        let _ = name;
        unconfigured()
    }

    /// Write `data` to `path`, or append it, for a redirection: admit the
    /// write, journal what it will change, write through `os`, and undo the
    /// journal entry if the host fails. Nothing is written unless this
    /// returns `Ok`.
    fn guarded_write(
        &self,
        os: &dyn Os,
        path: &str,
        data: &[u8],
        append: bool,
    ) -> Result<(), WriteError> {
        let _ = (os, path, data, append);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Remove `path` for `rm`: a file, or with `recursive` a whole tree. The
    /// gate admits the operand once, and performs the removal with
    /// [`crate::remove_tree`], so a tree is one decision, not one per entry.
    fn guarded_remove(&self, os: &dyn Os, path: &str, recursive: bool) -> Result<(), WriteError> {
        let _ = (os, path, recursive);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Copy for `cp`: admit the whole `plan` once, and carry it out with
    /// [`crate::copy_steps`]. The plan is read before this is called, so a
    /// gate sees every destination it is asked to write.
    fn guarded_copy(&self, os: &dyn Os, plan: &[CopyStep]) -> Result<(), WriteError> {
        let _ = (os, plan);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Create each of `dirs`, in order, for `mkdir`. The first is the
    /// highest directory the call creates.
    fn guarded_create_dirs(&self, os: &dyn Os, dirs: &[String]) -> Result<(), WriteError> {
        let _ = (os, dirs);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Rename `from` to `to` for `mv`.
    fn guarded_rename(&self, os: &dyn Os, from: &str, to: &str) -> Result<(), WriteError> {
        let _ = (os, from, to);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Send `signal` to process `pid` for `kill`. Signals reach other
    /// processes, so this is gated: the host decides whether it may.
    fn guarded_kill(&self, os: &dyn Os, pid: u32, signal: i32) -> Result<(), WriteError> {
        let _ = (os, pid, signal);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Set the permission bits of `path` to `mode` for `chmod`.
    fn guarded_chmod(&self, os: &dyn Os, path: &str, mode: u32) -> Result<(), WriteError> {
        let _ = (os, path, mode);
        Err(WriteError::Refused(unconfigured()))
    }

    /// Set the access and modification times of an existing `path` to now,
    /// for `touch`.
    fn guarded_touch(&self, os: &dyn Os, path: &str) -> Result<(), WriteError> {
        let _ = (os, path);
        Err(WriteError::Refused(unconfigured()))
    }
}

/// The refusal a backend without a gate gets: fail closed, with the status
/// the shell uses for a refused program.
fn unconfigured() -> Refusal {
    Refusal {
        message: String::from("ish: no policy gate on this host: refusing"),
        code: 126,
    }
}

/// One step of a copy: a directory to create, or a file to copy. Paths are
/// as the command named them, joined onto the destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyStep {
    pub from: String,
    pub to: String,
    /// A directory (`from` is one too, and `to` is created), or a file.
    pub dir: bool,
}

/// What `uname` reports about the host. `release` is `None` when the host
/// cannot say (its `std` has no kernel release), and `uname -r` and `-a`
/// decline on that host rather than print a made-up release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uname {
    pub sysname: String,
    pub nodename: String,
    pub release: Option<String>,
    pub machine: String,
}

/// One process, for `ps`: its id, its parent's id, and its command name as
/// the host records it (truncated, as `ps` truncates it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub comm: String,
}

/// What a host without the operation says: an honest "not supported", never a
/// silent success.
fn unsupported() -> OsError {
    OsError {
        kind: ErrorKind::Other,
        errno: None,
        message: String::from("not supported by this host"),
    }
}

/// The gate a backend gets when it supplies none.
struct NoGate;
impl Gate for NoGate {}
static NO_GATE: NoGate = NoGate;

/// The host operations the POSIX subset uses. Paths are `&str`, as the shell
/// takes them from argv; the results are lossy UTF-8 where a host path is not.
pub trait Os {
    /// Metadata, following symbolic links.
    fn metadata(&self, path: &str) -> Result<Meta, OsError>;
    /// Metadata of the link itself when `path` is one.
    fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError>;
    /// The entries of a directory, in the host's order, with entries that
    /// fail to read left out.
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError>;
    /// Creates or truncates `path` and writes `data`; with `append`, creates
    /// it if needed and writes at its end. Not gated: the subset reaches a
    /// redirection's write through [`Gate::guarded_write`].
    fn write_file(&self, path: &str, data: &[u8], append: bool) -> Result<(), OsError>;
    /// Removes one entry: a file or a link, or with `dir` an empty directory.
    /// Not gated: the subset reaches a removal through [`Gate::guarded_remove`].
    /// A host that does not support it says so, rather than succeeding.
    fn remove(&self, path: &str, dir: bool) -> Result<(), OsError> {
        let _ = (path, dir);
        Err(unsupported())
    }
    /// Creates one directory. Not gated: see [`Gate::guarded_create_dirs`].
    fn create_dir(&self, path: &str) -> Result<(), OsError> {
        let _ = path;
        Err(unsupported())
    }
    /// Renames `from` to `to`. Not gated: see [`Gate::guarded_rename`].
    fn rename(&self, from: &str, to: &str) -> Result<(), OsError> {
        let _ = (from, to);
        Err(unsupported())
    }
    fn read_link(&self, path: &str) -> Result<String, OsError>;
    fn canonicalize(&self, path: &str) -> Result<String, OsError>;
    fn current_dir(&self) -> Result<String, OsError>;
    fn set_current_dir(&self, path: &str) -> Result<(), OsError>;
    /// The value of an environment variable; `None` when unset or not UTF-8.
    fn env_var(&self, name: &str) -> Option<String>;
    /// Every environment variable the host has, for `env` with no command.
    /// A host that cannot list them says none, and `env` prints none.
    fn env_vars(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn process_id(&self) -> u32;
    /// Runs a program to completion. Not gated: the subset calls
    /// [`Gate::admit_exec`] first.
    fn run_program(&self, spec: Spawn) -> Result<Captured, SpawnError>;

    /// The shell's own standard output. Written and flushed.
    fn write_stdout(&self, bytes: &[u8]);
    /// The shell's own standard error. Written and flushed.
    fn write_stderr(&self, bytes: &[u8]);
    /// The next chunk of the shell's standard input, up to and including a
    /// newline and at most `limit` bytes. `None` at end of input or on a
    /// read error.
    fn read_stdin_line(&self, limit: usize) -> Option<Vec<u8>>;
    /// Appends the rest of the shell's standard input to `out`.
    fn read_stdin_to_end(&self, out: &mut Vec<u8>);

    /// The current time.
    fn now(&self) -> Timestamp;
    /// `at` in the host's local time, rendered with a `strftime`-style
    /// format. `None` when the time is out of the host's range or the host
    /// does not accept the format.
    fn format_local(&self, at: Timestamp, format: &str) -> Option<String>;

    /// Whether the host can load the named collation locale. A backend that
    /// cannot tell says `false`, and then sorts by bytes.
    fn locale_available(&self, name: &str) -> bool {
        let _ = name;
        false
    }
    /// The account name for a user id, or `None` when the host has none.
    fn user_name(&self, uid: u32) -> Option<String> {
        let _ = uid;
        None
    }
    /// The group name for a group id, or `None` when the host has none.
    fn group_name(&self, gid: u32) -> Option<String> {
        let _ = gid;
        None
    }

    /// Whether [`Meta`]'s owner, mode and link fields are real, so `ls -l`
    /// can print them. A host without them says `false`, and `ls -l` prints
    /// nothing, as it always has on a host without Unix metadata.
    fn has_unix_metadata(&self) -> bool {
        true
    }

    /// Pauses the calling thread for `d`. Not gated: a pause changes nothing.
    fn sleep(&self, d: core::time::Duration) -> Result<(), OsError> {
        let _ = d;
        Err(unsupported())
    }
    /// The host's name, kernel and machine, for `uname`.
    fn uname(&self) -> Result<Uname, OsError> {
        Err(unsupported())
    }
    /// The name of the account the shell runs as, or `None` when the host has
    /// none. Used by `whoami`.
    fn user_name_current(&self) -> Option<String> {
        None
    }
    /// Whether this host can deliver signals at all. `kill` declines on a host
    /// that cannot, rather than failing each signal.
    fn can_signal(&self) -> bool {
        false
    }
    /// Sends `signal` to process `pid`. Not gated here: the subset reaches a
    /// signal through [`Gate::guarded_kill`].
    fn kill(&self, pid: u32, signal: i32) -> Result<(), OsError> {
        let _ = (pid, signal);
        Err(unsupported())
    }
    /// Every process the host lists, for `ps`. A host that cannot list them
    /// says so, and `ps` declines.
    fn processes(&self) -> Result<Vec<Proc>, OsError> {
        Err(unsupported())
    }
    /// Sets the permission bits of `path` to `mode`. Not gated: the subset
    /// reaches a mode change through [`Gate::guarded_chmod`].
    fn set_mode(&self, path: &str, mode: u32) -> Result<(), OsError> {
        let _ = (path, mode);
        Err(unsupported())
    }
    /// Sets the access and modification times of `path` to `at`, for `touch`.
    /// Not gated here: the subset reaches a time change through
    /// [`Gate::guarded_touch`].
    fn set_times(&self, path: &str, at: Timestamp) -> Result<(), OsError> {
        let _ = (path, at);
        Err(unsupported())
    }

    /// The policy this host applies. Defaults to a gate that refuses
    /// everything.
    fn gate(&self) -> &dyn Gate {
        &NO_GATE
    }
}
