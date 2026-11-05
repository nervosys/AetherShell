//! A write through a symbolic link is judged where the OS puts it.
//!
//! The jail removed `..` lexically before it followed a link, so
//! `<ws>/lnk/../f` was checked as `<ws>/f`. The OS follows `lnk` first and then
//! takes the parent of the link's target, so with `lnk` pointing outside the
//! workspace the write landed outside it. A redirection in `ish -b` did the same,
//! and a redirection to a dangling link outside the workspace created its target.
//!
//! These drive the real binary in agent mode, from a directory outside the
//! workspace, so the defect is seen where a user would see it. The symbolic
//! links are made inside temporary directories only. On Windows the link cases
//! skip quietly when the process may not create symbolic links, and the junction
//! case skips when `mklink /J` fails.

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
        let root =
            std::env::temp_dir().join(format!("ae-symlink-jail-{tag}-{}", std::process::id()));
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

    /// Runs `ish` in agent mode, from the directory outside the workspace.
    fn ish(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ish"))
            .args(args)
            .current_dir(&self.outside)
            .env("NO_COLOR", "1")
            .env("IRONSHELL_MODE", "agent")
            .env("IRONSHELL_WORKSPACE", &self.workspace)
            .env("IRONSHELL_SESSION_DIR", &self.session)
            .env("IRONSHELL_JOURNAL", "on")
            .env("IRONSHELL_ALLOW_SH", "")
            .output()
            .expect("run ish")
    }

    /// Writes `body` as a script outside the workspace and runs it with `ish`.
    fn script(&self, name: &str, body: &str) -> Output {
        let path = self.outside.join(name);
        std::fs::write(&path, body).unwrap();
        self.ish(&[path.to_str().unwrap()])
    }

    /// `rel` inside the workspace, spelled with `/` so it can sit in a quoted
    /// IronShell string or a shell single-quoted word.
    fn ws(&self, rel: &str) -> String {
        slash(&self.workspace.join(rel))
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn slash(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// Whether anything is at `p`, a dangling link included.
fn present(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Makes `link` point at `target`. Returns false when the platform refused,
/// which on Windows is the normal state of a process without the privilege.
#[cfg(unix)]
fn link(target: &Path, link: &Path, _dir: bool) -> bool {
    std::os::unix::fs::symlink(target, link).expect("symlink");
    true
}

#[cfg(windows)]
fn link(target: &Path, link: &Path, dir: bool) -> bool {
    let made = if dir {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    };
    match made {
        Ok(()) => true,
        Err(e) => {
            eprintln!("skipped: symbolic link creation refused: {e}");
            false
        }
    }
}

/// The write must be refused: non-zero exit, the jail's code, and nothing written
/// at the place the OS would have put it.
fn assert_refused(out: &Output, what: &str) {
    let shown = text(out);
    assert!(!out.status.success(), "{what} was not refused: {shown}");
    assert!(
        shown.contains("E_OUTSIDE_WORKSPACE"),
        "{what} refused without the jail's code: {shown}"
    );
}

#[test]
fn file_write_through_a_link_out_of_the_workspace_is_refused() {
    let d = Dirs::new("fw-out");
    std::fs::create_dir_all(d.outside.join("dir")).unwrap();
    if !link(&d.outside.join("dir"), &d.workspace.join("lnk"), true) {
        return;
    }
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("lnk/../f"))]);
    assert_refused(&out, "file_write through a link");
    assert!(
        !present(&d.outside.join("f")),
        "landed in outside/f: {}",
        text(&out)
    );
    assert!(!present(&d.outside.join("dir").join("f")));
    assert!(!present(&d.workspace.join("f")));
}

#[test]
fn redirection_through_a_link_out_of_the_workspace_is_refused() {
    let d = Dirs::new("redir-out");
    std::fs::create_dir_all(d.outside.join("dir")).unwrap();
    if !link(&d.outside.join("dir"), &d.workspace.join("lnk"), true) {
        return;
    }
    let out = d.script("redir.sh", "echo x > lnk/../f\n");
    assert!(
        !out.status.success(),
        "redirection was not refused: {}",
        text(&out)
    );
    assert!(
        !present(&d.outside.join("f")),
        "landed in outside/f: {}",
        text(&out)
    );
    assert!(!present(&d.workspace.join("f")));
}

#[test]
fn redirection_to_a_dangling_link_outside_the_workspace_creates_nothing_outside() {
    let d = Dirs::new("redir-dangling");
    let absent = d.outside.join("absent");
    if !link(&absent, &d.workspace.join("dang"), false) {
        return;
    }
    let out = d.script("dangling.sh", "echo x > dang\n");
    assert!(
        !out.status.success(),
        "redirection was not refused: {}",
        text(&out)
    );
    assert!(
        !present(&absent),
        "created the link's target outside: {}",
        text(&out)
    );
}

#[test]
fn file_write_to_a_dangling_link_outside_the_workspace_creates_nothing_outside() {
    let d = Dirs::new("fw-dangling");
    let absent = d.outside.join("absent");
    if !link(&absent, &d.workspace.join("dang"), false) {
        return;
    }
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("dang"))]);
    assert_refused(&out, "file_write to a dangling link");
    assert!(
        !present(&absent),
        "created the link's target outside: {}",
        text(&out)
    );
}

#[test]
fn a_real_parent_directory_still_resolves_inside_the_workspace() {
    let d = Dirs::new("real-parent");
    std::fs::create_dir_all(d.workspace.join("real")).unwrap();
    let out = d.ish(&[
        "-c",
        &format!(r#"file_write("{}", "x")"#, d.ws("real/../f")),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(d.workspace.join("f")).unwrap(), "x");
    assert!(!present(&d.outside.join("f")));
}

#[test]
fn a_link_inside_the_workspace_lands_where_the_os_puts_it() {
    let d = Dirs::new("inside");
    std::fs::create_dir_all(d.workspace.join("a").join("b")).unwrap();
    if !link(Path::new("a/b"), &d.workspace.join("lnk"), true) {
        return;
    }
    // The OS goes to a/b, then to its parent a, so the file is ws/a/f, not ws/f.
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("lnk/../f"))]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        std::fs::read_to_string(d.workspace.join("a").join("f")).unwrap(),
        "x"
    );
    assert!(
        !present(&d.workspace.join("f")),
        "landed at ws/f, not where the OS puts it"
    );
    assert!(!present(&d.outside.join("f")));
}

#[test]
fn a_link_loop_is_refused() {
    let d = Dirs::new("loop");
    if !link(Path::new("l2"), &d.workspace.join("l1"), true) {
        return;
    }
    if !link(Path::new("l1"), &d.workspace.join("l2"), true) {
        return;
    }
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("l1/x"))]);
    assert_refused(&out, "a write through a link loop");
}

#[test]
fn a_nested_link_chain_leaving_the_workspace_is_refused() {
    let d = Dirs::new("chain");
    // c1 -> c2 -> c3 -> ../outside, each relative to the directory holding it.
    if !link(Path::new("c2"), &d.workspace.join("c1"), true) {
        return;
    }
    if !link(Path::new("c3"), &d.workspace.join("c2"), true) {
        return;
    }
    if !link(Path::new("../outside"), &d.workspace.join("c3"), true) {
        return;
    }
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("c1/x"))]);
    assert_refused(&out, "a write through a nested chain");
    assert!(
        !present(&d.outside.join("x")),
        "landed in outside/x: {}",
        text(&out)
    );
}

#[cfg(windows)]
#[test]
fn a_junction_out_of_the_workspace_is_refused() {
    let d = Dirs::new("junction");
    std::fs::create_dir_all(d.outside.join("dir")).unwrap();
    let made = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(d.workspace.join("j"))
        .arg(d.outside.join("dir"))
        .output();
    match made {
        Ok(o) if o.status.success() => {}
        _ => {
            eprintln!("skipped: mklink /J refused");
            return;
        }
    }
    let out = d.ish(&["-c", &format!(r#"file_write("{}", "x")"#, d.ws("j/../f"))]);
    assert_refused(&out, "file_write through a junction");
    assert!(
        !present(&d.outside.join("f")),
        "landed in outside/f: {}",
        text(&out)
    );
}
