//! The agent-mode workspace root is fixed at startup, so `cd` does not move it.
//!
//! The root used to be `IRONSHELL_WORKSPACE` or else the live working
//! directory. A `cd` therefore moved the jail, and with it the audit log: an
//! agent could `cd` out, and the guard then approved writes there, and the
//! next audit entry started a fresh chain in the new directory with no marker.
//!
//! `ish` now pins the root once, after the mode is decided. These drive the
//! real binary, because the pin is in `main`, below anything a library call
//! reaches. The last test covers library callers that never pin.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Dirs {
    root: PathBuf,
    ws: PathBuf,
    outside: PathBuf,
    session: PathBuf,
}

impl Dirs {
    fn new(tag: &str) -> Dirs {
        let root = std::env::temp_dir().join(format!("ae-ws-pinned-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = Dirs {
            ws: root.join("ws"),
            outside: root.join("outside"),
            session: root.join("session"),
            root,
        };
        for p in [&d.ws, &d.outside, &d.session] {
            std::fs::create_dir_all(p).unwrap();
        }
        d
    }

    /// Runs `ish` in agent mode from the workspace, with no `IRONSHELL_WORKSPACE`.
    fn ish(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ish"))
            .args(args)
            .current_dir(&self.ws)
            .env_remove("IRONSHELL_WORKSPACE")
            .env_remove("IRONSHELL_PINNED_ROOT")
            // A test below sets this in this process, and a child would inherit it.
            .env_remove("IRONSHELL_AUDIT_LOG")
            .env("NO_COLOR", "1")
            .env("IRONSHELL_MODE", "agent")
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

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A path spelled for the POSIX parser, where a backslash is an escape. A
/// Windows path with its separators eaten fails the `cd`, and the test then
/// proves nothing.
fn p(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn an_absolute_write_after_cd_outside_is_refused() {
    let d = Dirs::new("abs");
    let script = format!("cd {}; echo x > {}/f", p(&d.outside), p(&d.outside));
    let out = d.ish(&["-b", "-c", &script]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("E_OUTSIDE_WORKSPACE"), "{}", text(&out));
    assert!(!d.outside.join("f").exists());
}

#[test]
fn a_relative_write_after_cd_outside_is_refused() {
    let d = Dirs::new("rel");
    let script = format!("cd {}; echo x > f", p(&d.outside));
    let out = d.ish(&["-b", "-c", &script]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("E_OUTSIDE_WORKSPACE"), "{}", text(&out));
    assert!(!d.outside.join("f").exists());
    assert!(!d.ws.join("f").exists());
}

#[test]
fn a_relative_write_after_cd_inside_follows_the_working_directory() {
    let d = Dirs::new("inside");
    std::fs::create_dir_all(d.ws.join("sub")).unwrap();
    let out = d.ish(&["-b", "-c", "cd sub; echo x > f"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(d.ws.join("sub").join("f").exists(), "{}", text(&out));
    assert!(!d.ws.join("f").exists());
}

#[test]
fn builtin_writes_after_cd_keep_the_audit_log_in_the_workspace() {
    let d = Dirs::new("audit");
    let script =
        "mkdir(\"sub\")\ncd(\"sub\")\nfile_write(\"b.txt\", \"2\")\nfile_write(\"c.txt\", \"3\")\n";
    let out = d.ish(&["-c", script]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        !d.ws.join("sub").join(".ironshell").exists(),
        "a cd created an audit directory inside the subdirectory"
    );
    let log = std::fs::read_to_string(d.ws.join(".ironshell").join("audit.log"))
        .expect("the audit log is in the workspace");
    assert!(log.contains("c.txt"), "{log}");
    assert!(
        d.ws.join("sub").join("c.txt").exists(),
        "a relative path follows the cd"
    );
}

#[test]
fn cd_above_the_workspace_through_a_builtin_is_refused() {
    let d = Dirs::new("up");
    let out = d.ish(&["-c", "cd(\"..\")"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("E_OUTSIDE_WORKSPACE"), "{}", text(&out));
}

/// A library caller never pins. Moving the audit log inside one process must
/// leave a marker naming the old log, not start a silent new chain.
#[test]
fn a_moved_audit_log_in_one_process_starts_with_a_marker_naming_the_old_one() {
    use iron_shell::safety::{self, Effect};

    let d = Dirs::new("lib");
    let first = d.root.join("first").join("audit.log");
    let second = d.root.join("second").join("audit.log");
    let keys = [
        "IRONSHELL_MODE",
        "IRONSHELL_AUDIT_LOG",
        "IRONSHELL_SESSION_DIR",
    ];
    let before: Vec<_> = keys.iter().map(|k| std::env::var_os(k)).collect();

    std::env::set_var("IRONSHELL_MODE", "agent");
    std::env::set_var("IRONSHELL_SESSION_DIR", &d.session);
    std::env::set_var("IRONSHELL_AUDIT_LOG", &first);
    safety::audit(
        "probe",
        Effect::ReadLocal,
        "allow",
        "one",
        serde_json::json!({}),
    )
    .unwrap();
    std::env::set_var("IRONSHELL_AUDIT_LOG", &second);
    safety::audit(
        "probe",
        Effect::ReadLocal,
        "allow",
        "two",
        serde_json::json!({}),
    )
    .unwrap();

    for (k, v) in keys.iter().zip(before) {
        match v {
            Some(v) => std::env::set_var(k, v),
            None => std::env::remove_var(k),
        }
    }

    let log = std::fs::read_to_string(&second).expect("second log written");
    let first_line = log.lines().next().unwrap_or_default();
    assert!(first_line.contains("log-path-changed"), "{log}");
    assert!(first_line.contains("first"), "{log}");
    assert!(safety::verify_audit(&second).is_ok(), "{log}");
}
