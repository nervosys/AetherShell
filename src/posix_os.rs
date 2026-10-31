//! The operating system, as the POSIX subset sees it.
//!
//! [`Os`] is everything `posix.rs` asks of the host: reading and writing
//! files, listing directories, metadata, the working directory, one
//! environment variable at a time, the process id, and running a program to
//! completion with its output captured. Its types are plain data (no
//! `std::fs::Metadata`, no `std::process` types in any signature), so a
//! backend that is not `std` can implement it.
//!
//! [`HostOs`] is the one backend in use, and it is `std`. It reports what
//! `posix.rs` reported when it called `std` directly: the same error kinds,
//! and the same message text ([`OsError::message`] is `std`'s own), so the
//! utilities print the same bytes.
//!
//! Deliberately not on this trait: the policy gates (`safety`, the journal),
//! which `posix.rs` calls itself, and the shell's own stdin, stdout and
//! stderr, which it writes through `std::io`.
//!
//! Contract for a backend that spawns programs: it must apply the same
//! `IRONSHELL_*` stripping that `safety::strip_ironshell_env` applies to a
//! `std` child. `posix.rs` no longer does that itself, so a backend that
//! skips it would hand a child the gate variables the shell removes.

use std::io::Write;

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
    /// The host's message. For `std` this is `io::Error`'s `Display`, which
    /// may end in `" (os error N)"`; the printed reason strips that suffix.
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
    /// it if needed and writes at its end.
    fn write_file(&self, path: &str, data: &[u8], append: bool) -> Result<(), OsError>;
    fn read_link(&self, path: &str) -> Result<String, OsError>;
    fn canonicalize(&self, path: &str) -> Result<String, OsError>;
    fn current_dir(&self) -> Result<String, OsError>;
    fn set_current_dir(&self, path: &str) -> Result<(), OsError>;
    /// The value of an environment variable; `None` when unset or not UTF-8.
    fn env_var(&self, name: &str) -> Option<String>;
    fn process_id(&self) -> u32;
    fn run_program(&self, spec: Spawn) -> Result<Captured, SpawnError>;
}

/// The backend on the Rust standard library.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostOs;

impl OsError {
    fn from_io(e: &std::io::Error) -> Self {
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
            .map_err(|e| OsError::from_io(&e))
    }

    fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError> {
        std::fs::symlink_metadata(path)
            .map(|m| meta_of(&m))
            .map_err(|e| OsError::from_io(&e))
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError> {
        let rd = std::fs::read_dir(path).map_err(|e| OsError::from_io(&e))?;
        Ok(rd
            .filter_map(|e| e.ok())
            .map(|e| DirEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                kind: e.file_type().ok().map(kind_of),
            })
            .collect())
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError> {
        std::fs::read(path).map_err(|e| OsError::from_io(&e))
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
        res.map_err(|e| OsError::from_io(&e))
    }

    fn read_link(&self, path: &str) -> Result<String, OsError> {
        std::fs::read_link(path)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| OsError::from_io(&e))
    }

    fn canonicalize(&self, path: &str) -> Result<String, OsError> {
        std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| OsError::from_io(&e))
    }

    fn current_dir(&self) -> Result<String, OsError> {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| OsError::from_io(&e))
    }

    fn set_current_dir(&self, path: &str) -> Result<(), OsError> {
        std::env::set_current_dir(path).map_err(|e| OsError::from_io(&e))
    }

    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
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
        crate::safety::strip_ironshell_env(&mut cmd);
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
        let mut child = cmd
            .spawn()
            .map_err(|e| SpawnError::Start(OsError::from_io(&e)))?;
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
            .map_err(|e| SpawnError::Wait(OsError::from_io(&e)))?;
        if let Some(w) = writer {
            let _ = w.join();
        }
        Ok(Captured {
            status: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }
}
