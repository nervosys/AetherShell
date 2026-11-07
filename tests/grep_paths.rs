//! `grep` reads its path the way `cat` does: a glob searches every matching
//! file, a directory is refused as a bad argument (not as E_OS_DENIED), and
//! every expanded path goes through the workspace jail.
//!
//! Test safety: every call runs with the working directory set to a fresh temp
//! directory, which is also the workspace for the call. Nothing outside the
//! temp directories is read, and nothing is written outside them.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

/// The working directory and the workspace variable are process-global, so
/// every call in this file takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_grep_paths_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
}

/// A call in human mode run from `dir`, which is also the workspace.
fn call_here(dir: &Path, name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let before = std::env::current_dir().expect("the working directory");
    std::env::set_current_dir(dir).expect("enter the temp directory");
    std::env::set_var("IRONSHELL_WORKSPACE", dir);
    let mut env = Env::new();
    let out = builtins::call(name, args, &mut env);
    std::env::remove_var("IRONSHELL_WORKSPACE");
    std::env::set_current_dir(&before).expect("restore the working directory");
    out
}

/// A piped call run from `dir`, for `cat(glob) | grep(pattern)`.
fn piped_here(dir: &Path, name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let before = std::env::current_dir().expect("the working directory");
    std::env::set_current_dir(dir).expect("enter the temp directory");
    std::env::set_var("IRONSHELL_WORKSPACE", dir);
    let mut env = Env::new();
    let out = builtins::call_with_input(name, args, Some(input), &mut env);
    std::env::remove_var("IRONSHELL_WORKSPACE");
    std::env::set_current_dir(&before).expect("restore the working directory");
    out
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn lines(xs: &[&str]) -> Value {
    Value::Array(xs.iter().map(|s| Value::Str(s.to_string())).collect())
}

/// The first `E_...` code in an error's rendered text.
fn code_of(r: &anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => panic!("expected a coded refusal, got {v:?}"),
        Err(e) => {
            let msg = format!("{e:#}");
            let at = msg
                .find("E_")
                .unwrap_or_else(|| panic!("no code in: {msg}"));
            msg[at..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect()
        }
    }
}

/// A fresh directory under the system temp dir, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!(
            "ae_grep_paths_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).expect("create temp dir");
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `src/a.rs` and `src/b.rs` with a nested `src/deep/c.rs`, under `root`.
fn make_src_tree(root: &Path) {
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    std::fs::write(root.join("src/a.rs"), "fn alpha()\nlet x = 1\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "fn beta()\n").unwrap();
    std::fs::write(root.join("src/deep/c.rs"), "fn gamma()\n").unwrap();
    std::fs::write(root.join("src/notes.txt"), "fn in_a_text_file()\n").unwrap();
}

#[test]
fn grep_on_a_directory_is_a_bad_argument_with_a_hint() {
    let ws = TempDir::new("dir");
    make_src_tree(ws.path());
    let r = call_here(ws.path(), "grep", vec![text("fn "), text("src")]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "a directory is not E_OS_DENIED");
    let msg = format!("{:#}", r.unwrap_err());
    assert!(msg.contains("is a directory"), "message: {msg}");
    assert!(msg.contains("src/**/*.rs"), "hint names a glob: {msg}");
}

#[test]
fn grep_on_a_glob_searches_every_matching_file_in_order() {
    let ws = TempDir::new("glob");
    make_src_tree(ws.path());
    let r = call_here(ws.path(), "grep", vec![text("^fn "), text("src/*.rs")]);
    // The two top-level .rs files, in order. The nested file and the .txt
    // file are not matched by `src/*.rs`.
    assert_eq!(r.unwrap(), lines(&["fn alpha()", "fn beta()"]));
}

#[test]
fn grep_on_a_recursive_glob_reaches_nested_files() {
    let ws = TempDir::new("rglob");
    make_src_tree(ws.path());
    let r = call_here(ws.path(), "grep", vec![text("^fn "), text("src/**/*.rs")]);
    assert_eq!(
        r.unwrap(),
        lines(&["fn alpha()", "fn beta()", "fn gamma()"])
    );
}

#[test]
fn grep_on_a_glob_answers_what_cat_then_grep_answers() {
    let ws = TempDir::new("pipe");
    make_src_tree(ws.path());
    let via_grep = call_here(ws.path(), "grep", vec![text("fn "), text("src/*.rs")]).unwrap();
    let cat = call_here(ws.path(), "cat", vec![text("src/*.rs")]).unwrap();
    let via_pipe = piped_here(ws.path(), "grep", vec![text("fn ")], cat).unwrap();
    assert_eq!(via_grep, via_pipe);
}

#[test]
fn grep_on_a_glob_with_no_match_is_not_found() {
    let ws = TempDir::new("nomatch");
    make_src_tree(ws.path());
    let r = call_here(ws.path(), "grep", vec![text("fn "), text("nope/*.rs")]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");
}

#[test]
fn grep_on_a_single_named_file_is_unchanged() {
    let ws = TempDir::new("single");
    make_src_tree(ws.path());
    let r = call_here(ws.path(), "grep", vec![text("^let"), text("src/a.rs")]);
    assert_eq!(r.unwrap(), lines(&["let x = 1"]));
}

#[test]
fn grep_on_a_missing_named_file_is_not_found() {
    let ws = TempDir::new("missing");
    let r = call_here(ws.path(), "grep", vec![text("x"), text("absent.txt")]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");
}

#[test]
fn a_glob_cannot_grep_outside_the_workspace() {
    // workspace = <root>/ws; the file sits in <root>/outside. A relative glob
    // that climbs out must be refused, not read.
    let root = TempDir::new("jail");
    let ws = root.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir_all(root.path().join("outside")).unwrap();
    std::fs::write(root.path().join("outside/secret.txt"), "fn secret()\n").unwrap();
    let r = call_here(&ws, "grep", vec![text("fn "), text("../outside/*.txt")]);
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");
}
