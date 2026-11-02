//! The Linux host side of the in-process `uname`, `whoami`, `ps`, `kill` and
//! `chmod`, run through the real binary.
//!
//! The crate's own tests drive these utilities over a fake `Os`, so the code
//! that reads `/proc`, calls `kill(2)` and sets mode bits is compiled out of
//! them, and on a Windows host it is not compiled at all. These run it.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Dirs {
    root: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
}

impl Dirs {
    fn new(tag: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("ae-posix-host-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = Dirs {
            workspace: root.join("ws"),
            session: root.join("session"),
            root,
        };
        std::fs::create_dir_all(&d.workspace).unwrap();
        std::fs::create_dir_all(&d.session).unwrap();
        d
    }

    /// `ish --bash -c CMD` from the workspace, in `mode`.
    fn run(&self, mode: &str, cmd: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ish"))
            .args(["--bash", "-c", cmd])
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

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn mode_of(p: &std::path::Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o7777
}

#[test]
fn uname_reads_the_kernel_the_way_uname_does() {
    let d = Dirs::new("uname");
    let o = d.run("human", "uname -s");
    assert_eq!(stdout(&o), "Linux\n", "{o:?}");
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap();
    let o = d.run("human", "uname -r");
    assert_eq!(stdout(&o), format!("{}\n", release.trim()), "{o:?}");
}

#[test]
fn whoami_names_the_effective_user() {
    let d = Dirs::new("whoami");
    let want = Command::new("id").arg("-un").output().expect("id -un");
    let o = d.run("human", "whoami");
    assert_eq!(stdout(&o), String::from_utf8_lossy(&want.stdout), "{o:?}");
}

#[test]
fn ps_lists_this_test_process() {
    let d = Dirs::new("ps");
    let o = d.run("human", "ps -e -o pid=,comm=");
    let me = std::process::id().to_string();
    assert!(
        stdout(&o)
            .lines()
            .any(|l| l.split_whitespace().next() == Some(me.as_str())),
        "pid {me} missing from:\n{}",
        stdout(&o)
    );
}

#[test]
fn kill_zero_probes_a_process_in_human_mode_and_is_refused_to_an_agent() {
    let d = Dirs::new("kill");
    let me = std::process::id();
    let o = d.run("human", &format!("kill -0 {me}"));
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let o = d.run("agent", &format!("kill -0 {me}"));
    assert_ne!(
        o.status.code(),
        Some(0),
        "an agent signalled a process: {o:?}"
    );
}

#[test]
fn chmod_sets_modes_and_an_agent_cannot_reach_outside_the_workspace() {
    let d = Dirs::new("chmod");
    let f = d.workspace.join("f");
    std::fs::write(&f, b"x").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();

    let o = d.run("human", "chmod 600 f");
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert_eq!(mode_of(&f), 0o600);
    let o = d.run("human", "chmod u+x,g+r f");
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert_eq!(mode_of(&f), 0o740);

    let outside = d.root.join("outside");
    std::fs::write(&outside, b"x").unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();
    let o = d.run("agent", &format!("chmod 777 {}", outside.display()));
    assert_ne!(o.status.code(), Some(0), "{o:?}");
    assert_eq!(
        mode_of(&outside),
        0o644,
        "an agent changed a mode outside the workspace"
    );
}
