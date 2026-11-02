//! GNU-compatibility of in-process `touch` and `xargs`, through the real binary.
//!
//! `touch` on an existing file must set its times to now, through the gate, as
//! GNU does; it used to leave them alone. `xargs` must print GNU's message for a
//! program it cannot start, and tell a program killed by a signal from one that
//! exits with a status of 128 or more. The first two cases run on every host;
//! the rest need Unix signals and permission bits.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

struct Dirs {
    root: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
}

impl Dirs {
    fn new(tag: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("ae-posix-gnu-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = Dirs {
            workspace: root.join("ws"),
            session: root.join("session"),
            root,
        };
        for p in [&d.workspace, &d.session] {
            std::fs::create_dir_all(p).unwrap();
        }
        d
    }

    /// `ish --bash -c SCRIPT` from the workspace, in `mode`.
    fn ish(&self, mode: &str, script: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ish"))
            .args(["--bash", "-c", script])
            .current_dir(&self.workspace)
            .env("NO_COLOR", "1")
            .env("IRONSHELL_MODE", mode)
            .env("IRONSHELL_WORKSPACE", &self.workspace)
            .env("IRONSHELL_SESSION_DIR", &self.session)
            .env("IRONSHELL_ALLOW_SH", "")
            .output()
            .expect("run ish")
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Sets `p`'s access and modification times to 2001, a long time ago.
fn set_old_times(p: &Path) {
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    std::fs::OpenOptions::new()
        .write(true)
        .open(p)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(old)
                .set_modified(old),
        )
        .unwrap();
}

/// How long ago `p` was last modified, negative when it is in the future.
fn age_secs(p: &Path) -> i64 {
    let mtime = std::fs::metadata(p).unwrap().modified().unwrap();
    match SystemTime::now().duration_since(mtime) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// GNU: `touch f` on an existing file sets its mtime to now and keeps its
/// contents. The file is made old first, so a no-op cannot pass.
#[test]
fn touch_sets_an_existing_files_mtime_to_now() {
    let d = Dirs::new("touch-existing");
    let f = d.workspace.join("f.txt");
    std::fs::write(&f, "kept").unwrap();
    set_old_times(&f);
    assert!(age_secs(&f) > 365 * 24 * 3600, "the fixture is old");

    let o = d.ish("agent", "touch f.txt");
    assert_eq!(o.status.code(), Some(0), "stderr: {}", stderr(&o));
    assert!(age_secs(&f).abs() < 120, "touch left the mtime old");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "kept");
}

/// GNU: `touch` on an absent name creates it, empty.
#[test]
fn touch_creates_a_missing_file() {
    let d = Dirs::new("touch-new");
    let o = d.ish("agent", "touch new.txt");
    assert_eq!(o.status.code(), Some(0), "stderr: {}", stderr(&o));
    assert_eq!(std::fs::read(d.workspace.join("new.txt")).unwrap(), b"");
}

/// GNU on WSL: `printf 'a\nb\n' | xargs -n1 NAME` prints one line,
/// `xargs: NAME: No such file or directory`, and exits 127 without running
/// the second word.
#[test]
fn xargs_names_a_program_it_cannot_start_as_gnu_does() {
    let d = Dirs::new("xargs-missing");
    let o = d.ish(
        "human",
        "printf 'a\\nb\\n' | xargs -n1 ae_no_such_program_zz",
    );
    assert_eq!(o.status.code(), Some(127));
    assert_eq!(
        stderr(&o),
        "xargs: ae_no_such_program_zz: No such file or directory\n"
    );
    assert!(o.stdout.is_empty());
}

/// GNU on WSL: a child killed by TERM is `xargs: sh: terminated by signal 15`,
/// and the runs stop with 125.
#[cfg(unix)]
#[test]
fn xargs_reports_a_child_killed_by_a_signal() {
    let d = Dirs::new("xargs-signal");
    let o = d.ish(
        "human",
        "printf 'a\\nb\\n' | xargs -n1 sh -c 'kill -TERM $$'",
    );
    assert_eq!(o.status.code(), Some(125));
    assert_eq!(stderr(&o), "xargs: sh: terminated by signal 15\n");
}

/// GNU on WSL: a child that exits 128 is an exit code, not a signal. Each run
/// fails, and the status is 123 with no message.
#[cfg(unix)]
#[test]
fn xargs_takes_an_exit_of_128_as_an_exit_code() {
    let d = Dirs::new("xargs-128");
    let o = d.ish("human", "printf 'a\\nb\\n' | xargs -n1 sh -c 'exit 128'");
    assert_eq!(o.status.code(), Some(123));
    assert_eq!(stderr(&o), "");
}

/// GNU on WSL: a file that is not executable is `xargs: ./NAME: Permission
/// denied`, and the status is 126.
#[cfg(unix)]
#[test]
fn xargs_reports_a_file_that_is_not_executable() {
    use std::os::unix::fs::PermissionsExt;
    let d = Dirs::new("xargs-noexec");
    let f = d.workspace.join("noexec");
    std::fs::write(&f, "x\n").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
    let o = d.ish("human", "printf 'a\\n' | xargs ./noexec");
    assert_eq!(o.status.code(), Some(126));
    assert_eq!(stderr(&o), "xargs: ./noexec: Permission denied\n");
}
