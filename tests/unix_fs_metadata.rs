#![cfg(unix)]
//! Unix ownership, mode, execute-bit, symlink and dangling-link behaviour of the
//! file builtins. Every test works in its own scratch directory under the system
//! temp dir, with the workspace jail pointed at it, and removes it on the way out.

use aethershell::{builtins::call, env::Env, safety::SafetyError, value::Value};
use std::collections::BTreeMap;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

struct Scratch {
    root: PathBuf,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-unix-meta-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("session");
        std::fs::create_dir_all(&session).unwrap();
        let keys = [
            "AETHER_WORKSPACE",
            "AETHER_SESSION_DIR",
            "AETHER_AUDIT_LOG",
            "AETHER_AUDIT_KEY",
            "NO_COLOR",
        ];
        let saved = keys
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect::<Vec<_>>();
        std::env::set_var("AETHER_WORKSPACE", &root);
        std::env::set_var("AETHER_SESSION_DIR", &session);
        std::env::set_var("AETHER_AUDIT_LOG", root.join("audit.log"));
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        std::env::set_var("NO_COLOR", "1");
        Self { root, saved }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        for (key, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn p(path: &Path) -> Value {
    Value::Str(path.to_string_lossy().into_owned())
}

fn err_code(result: anyhow::Result<Value>) -> &'static str {
    match result {
        Ok(_) => panic!("expected a coded error, got a value"),
        Err(e) => e
            .downcast_ref::<SafetyError>()
            .expect("a SafetyError")
            .code
            .as_str(),
    }
}

fn record(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Record(m) => m,
        other => panic!("expected Record, got {}", other.type_name()),
    }
}

fn field<'a>(m: &'a BTreeMap<String, Value>, key: &str) -> &'a Value {
    m.get(key).unwrap_or_else(|| panic!("missing field {key}"))
}

fn as_int(value: &Value) -> i64 {
    match value {
        Value::Int(n) => *n,
        other => panic!("expected Int, got {}", other.type_name()),
    }
}

fn as_bool(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        other => panic!("expected Bool, got {}", other.type_name()),
    }
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[test]
fn fs_stat_reports_mode_and_follows_a_link_to_its_target() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("data.txt");
    let link = scratch.path("link");
    run("file_write", vec![p(&file), s("hi")]).unwrap();
    set_mode(&file, 0o640);
    symlink(&file, &link).unwrap();

    let direct = record(run("fs_stat", vec![p(&file)]).unwrap());
    assert_eq!(as_int(field(&direct, "mode")) & 0o777, 0o640);
    assert!(as_bool(field(&direct, "is_file")));
    assert!(!as_bool(field(&direct, "is_symlink")));
    assert_eq!(as_int(field(&direct, "size")), 2);

    // stat follows the link, so the record describes the target, not the link.
    let via_link = record(run("fs_stat", vec![p(&link)]).unwrap());
    assert!(!as_bool(field(&via_link, "is_symlink")));
    assert!(as_bool(field(&via_link, "is_file")));
    assert_eq!(as_int(field(&via_link, "mode")) & 0o777, 0o640);

    // fs_lstat describes the link itself.
    let lstat = record(run("fs_lstat", vec![p(&link)]).unwrap());
    assert!(as_bool(field(&lstat, "is_symlink")));
    assert!(!as_bool(field(&lstat, "is_file")));
}

#[test]
fn fs_stat_of_a_dangling_link_is_null_and_lstat_still_sees_the_link() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let link = scratch.path("dangling");
    symlink(scratch.path("missing"), &link).unwrap();

    // The doc says Null when the path does not exist. stat follows the link,
    // and the target is absent, so the result is Null.
    assert!(matches!(
        run("fs_stat", vec![p(&link)]).unwrap(),
        Value::Null
    ));
    let lstat = record(run("fs_lstat", vec![p(&link)]).unwrap());
    assert!(as_bool(field(&lstat, "is_symlink")));
}

#[test]
fn ls_lists_executable_and_link_entries_without_mode_fields() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let dir = scratch.path("dir");
    std::fs::create_dir(&dir).unwrap();
    let script = dir.join("run.sh");
    std::fs::write(&script, "#!/bin/sh\n").unwrap();
    set_mode(&script, 0o755);
    let data = dir.join("data.txt");
    std::fs::write(&data, "abc").unwrap();
    symlink(&data, dir.join("link")).unwrap();

    let entries = match run("ls", vec![p(&dir)]).unwrap() {
        Value::Array(items) => items,
        other => panic!("expected Array, got {}", other.type_name()),
    };
    assert_eq!(entries.len(), 3);
    let by_name: BTreeMap<String, BTreeMap<String, Value>> = entries
        .into_iter()
        .map(|v| {
            let m = record(v);
            let name = match field(&m, "name") {
                Value::Str(n) => n.clone(),
                other => panic!("name is {}", other.type_name()),
            };
            (name, m)
        })
        .collect();

    // The doc lists name, path, size and modified. No mode or owner fields.
    for entry in by_name.values() {
        assert!(!entry.contains_key("mode"));
        assert!(!entry.contains_key("uid"));
    }
    let run_sh = &by_name["run.sh"];
    assert!(!as_bool(field(run_sh, "is_dir")));
    assert_eq!(as_int(field(run_sh, "size")), 10);
    assert_eq!(
        match field(run_sh, "ext") {
            Value::Str(e) => e.as_str(),
            other => panic!("ext is {}", other.type_name()),
        },
        ".sh"
    );
    // ls follows links: the link reports its target's size and kind.
    let link = &by_name["link"];
    assert!(!as_bool(field(link, "is_dir")));
    assert_eq!(as_int(field(link, "size")), 3);
    assert_eq!(mode_of(&script), 0o755);
}

#[test]
fn ls_lists_a_dangling_link_as_itself() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let dir = scratch.path("dir");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("ok.txt"), "x").unwrap();
    symlink(scratch.path("missing"), dir.join("broken")).unwrap();

    // ls(1) lists a dangling link; one used to fail the whole listing with
    // E_NOT_FOUND. With no target to follow, the entry describes the link.
    let Value::Array(entries) = run("ls", vec![p(&dir)]).unwrap() else {
        panic!("ls did not answer an array");
    };
    let names: Vec<String> = entries
        .into_iter()
        .map(|v| match field(&record(v), "name") {
            Value::Str(n) => n.clone(),
            other => panic!("name is {}", other.type_name()),
        })
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"broken".to_string()), "{names:?}");
    assert!(names.contains(&"ok.txt".to_string()), "{names:?}");
}

#[test]
fn file_copy_keeps_the_execute_bit() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let src = scratch.path("tool");
    let dst = scratch.path("tool-copy");
    std::fs::write(&src, "#!/bin/sh\necho hi\n").unwrap();
    set_mode(&src, 0o755);

    let result = record(run("file_copy", vec![p(&src), p(&dst)]).unwrap());
    assert!(as_bool(field(&result, "success")));
    assert_eq!(as_int(field(&result, "bytes")), 18);
    assert_eq!(mode_of(&dst), 0o755);
}

#[test]
fn file_copy_refuses_a_symlink_source_whether_or_not_it_dangles() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let target = scratch.path("target");
    std::fs::write(&target, "content").unwrap();
    let live = scratch.path("live-link");
    symlink(&target, &live).unwrap();
    let dangling = scratch.path("dangling-link");
    symlink(scratch.path("missing"), &dangling).unwrap();

    // The doc says "regular file or directory tree without symlink entries", so
    // a symlink is refused rather than copied as a link or as its content.
    let dst_live = scratch.path("copy-live");
    assert_eq!(
        err_code(run("file_copy", vec![p(&live), p(&dst_live)])),
        "E_BAD_ARG"
    );
    let dst_dangling = scratch.path("copy-dangling");
    assert_eq!(
        err_code(run("file_copy", vec![p(&dangling), p(&dst_dangling)])),
        "E_BAD_ARG"
    );
    assert!(std::fs::symlink_metadata(&dst_live).is_err());
    assert!(std::fs::symlink_metadata(&dst_dangling).is_err());
}

#[test]
fn file_write_over_an_0o755_file_keeps_the_mode() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("script.sh");
    std::fs::write(&file, "old").unwrap();
    set_mode(&file, 0o755);

    let result = record(run("file_write", vec![p(&file), s("new content")]).unwrap());
    assert!(as_bool(field(&result, "success")));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
    assert_eq!(mode_of(&file), 0o755);
}

#[test]
fn file_write_through_a_symlink_updates_the_target_and_keeps_the_link() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let target = scratch.path("target.txt");
    std::fs::write(&target, "old").unwrap();
    let link = scratch.path("link.txt");
    symlink(&target, &link).unwrap();

    run("file_write", vec![p(&link), s("written via link")]).unwrap();

    assert!(is_link(&link));
    assert_eq!(std::fs::read_link(&link).unwrap(), target);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "written via link"
    );
}

#[test]
fn fs_chmod_sets_the_exact_mode_from_octal_string_and_int() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("f");
    run("file_write", vec![p(&file), s("hi")]).unwrap();

    assert!(as_bool(
        &run("fs_chmod", vec![p(&file), s("0640")]).unwrap()
    ));
    assert_eq!(mode_of(&file), 0o640);

    assert!(as_bool(
        &run("fs_chmod", vec![p(&file), s("0o600")]).unwrap()
    ));
    assert_eq!(mode_of(&file), 0o600);

    // chmod is the alias. 493 is 0o755 written as an Int.
    assert!(as_bool(
        &run("chmod", vec![p(&file), Value::Int(493)]).unwrap()
    ));
    assert_eq!(mode_of(&file), 0o755);
}

#[test]
fn fs_chmod_with_a_bad_mode_is_bad_arg_and_changes_nothing() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("f");
    run("file_write", vec![p(&file), s("hi")]).unwrap();
    set_mode(&file, 0o644);

    for bad in ["0x755", "0888", "", "0o", "7777x"] {
        assert_eq!(
            err_code(run("fs_chmod", vec![p(&file), s(bad)])),
            "E_BAD_ARG",
            "mode {bad:?}"
        );
    }
    assert_eq!(
        err_code(run("fs_chmod", vec![p(&file), Value::Int(0o10000)])),
        "E_BAD_ARG"
    );
    assert_eq!(mode_of(&file), 0o644);
}

#[test]
fn rm_on_a_symlink_removes_the_link_and_not_the_target() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let target = scratch.path("keep.txt");
    std::fs::write(&target, "keep").unwrap();
    let link = scratch.path("link");
    symlink(&target, &link).unwrap();

    let out = run("rm", vec![p(&link)]).unwrap();
    assert!(matches!(&out, Value::Str(s) if s.starts_with("removed:")));
    assert!(!is_link(&link));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
}

#[test]
fn rm_on_a_dangling_link_removes_the_link() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let link = scratch.path("dangling");
    symlink(scratch.path("missing"), &link).unwrap();

    // rm uses remove_file, which deletes the link itself without needing the
    // target to exist. Pinned as success.
    let out = run("rm", vec![p(&link)]).unwrap();
    assert!(matches!(&out, Value::Str(s) if s.starts_with("removed:")));
    assert!(!is_link(&link));
}

#[test]
fn read_text_of_a_dangling_link_is_not_found() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let link = scratch.path("dangling");
    symlink(scratch.path("missing"), &link).unwrap();

    // file_read is not a registered builtin. read_text is the file reader.
    assert_eq!(err_code(run("read_text", vec![p(&link)])), "E_NOT_FOUND");
}

#[test]
fn fs_chown_to_the_files_own_uid_succeeds() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("f");
    run("file_write", vec![p(&file), s("hi")]).unwrap();
    let uid = std::fs::metadata(&file).unwrap().uid();

    assert!(as_bool(
        &run("fs_chown", vec![p(&file), Value::Int(uid as i64)]).unwrap()
    ));
}

#[test]
fn fs_chown_to_root_as_a_non_root_user_is_os_denied() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let file = scratch.path("f");
    run("file_write", vec![p(&file), s("hi")]).unwrap();

    assert_eq!(
        err_code(run("fs_chown", vec![p(&file), Value::Int(0)])),
        "E_OS_DENIED"
    );
}

#[test]
fn fs_chown_of_a_dangling_link_is_not_found() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = Scratch::new();
    let link = scratch.path("dangling");
    symlink(scratch.path("missing"), &link).unwrap();

    // fs_chown resolves the link before changing ownership, so the absent
    // target is reported as E_NOT_FOUND.
    assert_eq!(
        err_code(run("fs_chown", vec![p(&link), Value::Int(0)])),
        "E_NOT_FOUND"
    );
}
