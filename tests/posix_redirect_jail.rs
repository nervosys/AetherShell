//! A redirection in `ae -b` writes where the guard looked, and can be undone.
//!
//! In agent mode the guard resolved a relative redirection target against the
//! workspace, but the write handed the bare name to the OS, which resolved it
//! against the working directory. With the two apart the jail checked one file
//! and the shell wrote another:
//!
//! ```text
//! $ cd /tmp/outside && AETHER_MODE=agent AETHER_WORKSPACE=/ws ae t.sh   # echo x > f.txt
//! # /tmp/outside/f.txt written; /ws/f.txt untouched
//! ```
//!
//! The guard was also told the write was reversible while nothing recorded the
//! prior contents, so `undo()` could not put a redirected-over file back.
//!
//! These drive the real binary, because the defect sat between the POSIX
//! interpreter and the guard, below anything a library call reaches.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Dirs {
    root: PathBuf,
    workspace: PathBuf,
    outside: PathBuf,
    session: PathBuf,
}

impl Dirs {
    fn new(tag: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("ae-posix-jail-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = Dirs {
            workspace: root.join("ws"),
            outside: root.join("outside"),
            session: root.join("session"),
            root,
        };
        for p in [&d.workspace, &d.outside, &d.session] {
            std::fs::create_dir_all(p).unwrap();
        }
        d
    }

    /// Runs `ae` in agent mode from the directory outside the workspace.
    fn ae(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ae"))
            .args(args)
            .current_dir(&self.outside)
            .env("NO_COLOR", "1")
            .env("AETHER_MODE", "agent")
            .env("AETHER_WORKSPACE", &self.workspace)
            .env("AETHER_SESSION_DIR", &self.session)
            .env("AETHER_JOURNAL", "on")
            .env("AETHER_ALLOW_SH", "")
            .output()
            .expect("run ae")
    }

    /// Writes `body` as a script outside the workspace and runs it with `ae`.
    fn script(&self, name: &str, body: &str) -> Output {
        let path = self.outside.join(name);
        std::fs::write(&path, body).unwrap();
        self.ae(&[path.to_str().unwrap()])
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn a_relative_redirection_lands_in_the_workspace_not_the_working_directory() {
    let d = Dirs::new("rel");
    let out = d.script("rel.sh", "echo inside > f.txt\necho more >> f.txt\n");
    assert!(out.status.success(), "{out:?}");
    assert!(
        !d.outside.join("f.txt").exists(),
        "the redirection wrote outside the workspace"
    );
    assert_eq!(
        read(&d.workspace.join("f.txt")).lines().collect::<Vec<_>>(),
        ["inside", "more"]
    );
}

#[test]
fn an_absolute_redirection_outside_the_workspace_is_refused() {
    let d = Dirs::new("abs");
    let target = d.root.join("escaped.txt");
    let body = format!("echo x > '{}'\n", target.display());
    let out = d.script("abs.sh", &body);
    assert!(!out.status.success(), "{out:?}");
    assert!(!target.exists(), "a refused redirection must write nothing");
}

#[test]
fn a_redirected_over_file_is_restored_by_undo() {
    let d = Dirs::new("undo");
    let target = d.workspace.join("g.txt");
    std::fs::write(&target, "before\n").unwrap();
    let out = d.script("undo.sh", "echo after > g.txt\n");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(read(&target), "after\n");

    let undone = d.ae(&["-c", "undo()"]);
    assert!(undone.status.success(), "{undone:?}");
    assert_eq!(read(&target), "before\n", "{undone:?}");
}
