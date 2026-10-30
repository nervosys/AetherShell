//! A repository's own files must not let an agent run a program nobody approved.
//!
//! `core.fsmonitor` in `.git/config` names a program that `git status` and
//! `git diff` run, and `.git/hooks/*` run on checkout, commit and merge. The
//! git builtins are ReadLocal or approved by their verb, so an agent that
//! wrote `.git/config` and then called `git_status` ran code with no approval.
//! Two things now stand in the way: the guard refuses agent writes under any
//! `.git` (`safety::is_git_control_path`), and every git builtin runs with
//! `core.fsmonitor=false`, for a repository that arrived already planted.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

const VARS: &[&str] = &[
    "IRONSHELL_MODE",
    "IRONSHELL_WORKSPACE",
    "IRONSHELL_AUDIT_LOG",
    "IRONSHELL_RBAC_CONFIG",
    "IRONSHELL_AUDIT_KEY_FILE",
    "IRONSHELL_AUDIT_SINK",
    "IRONSHELL_SESSION_DIR",
];

struct Ws {
    root: PathBuf,
    outside: PathBuf,
}

impl Ws {
    fn new(mode: Option<&str>) -> Self {
        let id = uuid::Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("ish-gitctl-ws-{id}"));
        let outside = std::env::temp_dir().join(format!("ish-gitctl-out-{id}"));
        std::fs::create_dir_all(root.join(".git").join("hooks")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let root = root.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();
        for var in VARS {
            std::env::remove_var(var);
        }
        if let Some(mode) = mode {
            std::env::set_var("IRONSHELL_MODE", mode);
        }
        std::env::set_var("IRONSHELL_WORKSPACE", &root);
        std::env::set_var("IRONSHELL_AUDIT_LOG", outside.join("audit.log"));
        std::env::set_var("IRONSHELL_SESSION_DIR", outside.join("session"));
        Self { root, outside }
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Ws {
    fn drop(&mut self) {
        for var in VARS {
            std::env::remove_var(var);
        }
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.outside);
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn assert_refused(name: &str, args: Vec<Value>) {
    let shown = format!("{name} {args:?}");
    let e = call(name, args).expect_err(&format!("{shown} must be refused"));
    let text = e.to_string();
    assert!(text.contains("E_OUTSIDE_WORKSPACE"), "{shown}: {text}");
    assert!(text.contains("inside a .git directory"), "{shown}: {text}");
}

#[test]
fn an_agent_cannot_write_git_config_or_hooks() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new(Some("agent"));
    let config = ws.path(".git/config");
    std::fs::write(&config, "[core]\n").unwrap();

    assert_refused(
        "file_write",
        vec![s(&config), s("[core]\n\tfsmonitor = ./evil\n")],
    );
    assert_refused("file_append", vec![s(&config), s("\tfsmonitor = ./evil\n")]);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "[core]\n");

    let hook = ws.path(".git/hooks/post-checkout");
    assert_refused("file_write", vec![s(&hook), s("#!/bin/sh\n")]);
    assert!(!Path::new(&hook).exists());

    // A nested repository, the `.git` entry itself, and a move into place.
    std::fs::create_dir_all(ws.root.join("vendor").join("lib")).unwrap();
    assert_refused(
        "file_write",
        vec![s(&ws.path("vendor/lib/.git/config")), s("x")],
    );
    assert_refused(
        "file_write",
        vec![s(&ws.path("vendor/lib/.git")), s("gitdir: /x\n")],
    );
    std::fs::write(ws.root.join("planted"), "x").unwrap();
    // A move is Destructive, so the approval gate may answer first; either way
    // it must not land.
    assert!(call("file_move", vec![s(&ws.path("planted")), s(&config)]).is_err());
    assert_refused("rm", vec![s(&config)]);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "[core]\n");

    // Names that only look alike are ordinary files.
    for ok in [
        ".gitignore",
        ".github/workflows/ci.yml",
        "notes.git",
        "x.git/y",
    ] {
        let p = ws.root.join(ok);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        assert!(
            call("file_write", vec![s(&p.to_string_lossy()), s("ok")]).is_ok(),
            "{ok} was refused"
        );
    }
}

#[test]
fn a_human_may_still_edit_git_config() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new(None);
    let config = ws.path(".git/config");
    assert!(call("file_write", vec![s(&config), s("[core]\n")]).is_ok());
}

/// git itself, if present on PATH.
fn git(dir: &Path, args: &[&str]) -> Option<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()
}

#[test]
fn git_status_does_not_run_a_planted_fsmonitor() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new(Some("agent"));
    std::fs::remove_dir_all(ws.root.join(".git")).unwrap();
    if git(&ws.root, &["init", "-q"]).is_none_or(|o| !o.status.success()) {
        eprintln!("skipped: git is not available");
        return;
    }
    std::fs::write(ws.root.join("a.txt"), "a\n").unwrap();
    assert!(git(&ws.root, &["add", "a.txt"]).unwrap().status.success());

    // Planted the way a cloned repository or an earlier session could have
    // left it. git runs the value through a shell and appends its own args.
    let marker = ws.outside.join("fsmonitor-ran");
    let marker_sh = marker.to_string_lossy().replace('\\', "/");
    assert!(git(
        &ws.root,
        &[
            "config",
            "core.fsmonitor",
            &format!("echo ran >> '{marker_sh}'")
        ]
    )
    .unwrap()
    .status
    .success());

    let root = ws.root.to_string_lossy().into_owned();
    call("git_status", vec![s(&root)]).expect("git_status");
    call("git_diff", vec![s(&root)]).expect("git_diff");
    assert!(
        !marker.exists(),
        "a ReadLocal git builtin ran the repository's fsmonitor"
    );
}
