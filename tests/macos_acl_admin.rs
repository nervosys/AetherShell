//! Contracts for acl_get and acl_set on macOS, through `ls -led` and `chmod`.
//!
//! Changing the ACL of a file or directory you own needs no root, so these are
//! ordinary tests rather than ignored ones. They still return early unless
//! AE_DISPOSABLE_HOST=1, which only the disposable CI runner sets, the same
//! convention as the other admin tests. Each test works on a temporary file or
//! directory of its own, removed on drop. The oracle is `/bin/ls -led` run by
//! the test itself, never through the builtin.

#![cfg(target_os = "macos")]

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

// The environment is process-wide, so the tests take this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn rec(pairs: Vec<(&str, Value)>) -> Value {
    Value::Record(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn remove_option() -> Value {
    rec(vec![("remove", Value::Bool(true))])
}

fn call(name: &str, args: Vec<Value>, env: &mut Env) -> anyhow::Result<Value> {
    iron_shell::builtins::call(name, args, env)
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.expect_err("the call should have been refused");
    assert!(
        error.to_string().contains(expected),
        "expected {expected}: {error}"
    );
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    match value {
        Value::Record(map) => map.get(key).unwrap_or_else(|| panic!("no field {key}")),
        other => panic!("not a record: {other:?}"),
    }
}

fn text(value: &Value, key: &str) -> String {
    match field(value, key) {
        Value::Str(text) => text.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

fn entries(value: &Value) -> Vec<Value> {
    match field(value, "entries") {
        Value::Array(items) => items.clone(),
        other => panic!("entries: expected an Array, got {other:?}"),
    }
}

fn words(value: &Value, key: &str) -> Vec<String> {
    match field(value, key) {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Str(word) => word.clone(),
                other => panic!("{key} holds {other:?}"),
            })
            .collect(),
        other => panic!("{key}: expected an Array, got {other:?}"),
    }
}

/// The ACL lines `/bin/ls -led` prints for `path`, read by the test itself:
/// every line after the listing, with the ` N: ` index taken off.
fn oracle(path: &Path) -> Vec<String> {
    let output = Command::new("/bin/ls")
        .env("LC_ALL", "C")
        .args(["-led", "--"])
        .arg(path)
        .output()
        .expect("ls runs");
    assert!(
        output.status.success(),
        "ls -led failed on {}",
        path.display()
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (index, rest) = line
                .trim()
                .split_once(": ")
                .unwrap_or_else(|| panic!("not an ACL line: {line:?}"));
            assert!(
                index.bytes().all(|b| b.is_ascii_digit()),
                "bad index in {line:?}"
            );
            rest.to_string()
        })
        .collect()
}

/// A temporary directory for one test, removed on drop. It also points the
/// audit log and its key into itself, so no test writes into the workspace.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aemacl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        for key in [
            "IRONSHELL_MODE",
            "IRONSHELL_AGENT",
            "IRONSHELL_WORKSPACE",
            "IRONSHELL_POLICY",
        ] {
            std::env::remove_var(key);
        }
        std::env::set_var("IRONSHELL_AUDIT_LOG", root.join("audit.log"));
        std::env::set_var("IRONSHELL_AUDIT_KEY", "11".repeat(32));
        Self { root }
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, "x").unwrap();
        path
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::create_dir(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_file_acl_is_set_listed_and_removed_one_entry_at_a_time() {
    if !disposable() {
        return;
    }
    let _lock = lock();
    let scratch = Scratch::new();
    let mut env = Env::default();
    let file = scratch.file("file");
    let path = file.to_string_lossy().into_owned();
    assert!(oracle(&file).is_empty(), "{:?}", oracle(&file));

    let set = call(
        "acl_set",
        vec![s(&path), s("user:nobody allow read,write")],
        &mut env,
    )
    .unwrap();
    assert_eq!(text(&set, "path"), path);
    assert_eq!(text(&set, "entry"), "user:nobody allow read,write");
    assert_eq!(field(&set, "applied"), &Value::Bool(true));
    assert_eq!(oracle(&file), vec!["user:nobody allow read,write"]);

    let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
    assert_eq!(text(&got, "path"), path);
    let aces = entries(&got);
    assert_eq!(aces.len(), 1, "{aces:?}");
    assert!(matches!(field(&aces[0], "index"), Value::Int(0)));
    assert_eq!(text(&aces[0], "tag"), "user");
    assert_eq!(text(&aces[0], "name"), "nobody");
    assert_eq!(text(&aces[0], "kind"), "allow");
    assert_eq!(words(&aces[0], "perms"), vec!["read", "write"]);
    assert_eq!(field(&aces[0], "inherited"), &Value::Bool(false));

    call(
        "acl_set",
        vec![s(&path), s("group:staff deny delete")],
        &mut env,
    )
    .unwrap();
    assert!(
        oracle(&file).contains(&"group:staff deny delete".to_string()),
        "{:?}",
        oracle(&file)
    );
    let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
    let aces = entries(&got);
    assert_eq!(aces.len(), 2, "{aces:?}");
    // macOS keeps deny entries ahead of allow entries (canonical order), so
    // the new entry is found by its tag, not by where it was added.
    let deny = aces
        .iter()
        .find(|ace| text(ace, "tag") == "group")
        .unwrap_or_else(|| panic!("no group entry in {aces:?}"));
    assert_eq!(text(deny, "name"), "staff");
    assert_eq!(text(deny, "kind"), "deny");
    assert_eq!(words(deny, "perms"), vec!["delete"]);

    // Removing the user entry leaves the group entry alone.
    let removed = call(
        "acl_set",
        vec![s(&path), s("user:nobody allow read,write"), remove_option()],
        &mut env,
    )
    .unwrap();
    assert_eq!(field(&removed, "applied"), &Value::Bool(true));
    let after = oracle(&file);
    assert!(
        !after.iter().any(|line| line.starts_with("user:nobody")),
        "{after:?}"
    );
    assert_eq!(after, vec!["group:staff deny delete"]);
    let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
    let aces = entries(&got);
    assert_eq!(aces.len(), 1, "{aces:?}");
    assert_eq!(text(&aces[0], "name"), "staff");

    call(
        "acl_set",
        vec![s(&path), s("group:staff deny delete"), remove_option()],
        &mut env,
    )
    .unwrap();
    assert!(oracle(&file).is_empty(), "{:?}", oracle(&file));
    let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
    assert!(entries(&got).is_empty());
}

#[test]
fn a_directory_acl_grants_list_and_search_and_is_removed() {
    if !disposable() {
        return;
    }
    let _lock = lock();
    let scratch = Scratch::new();
    let mut env = Env::default();
    let dir = scratch.dir("dir");
    let path = dir.to_string_lossy().into_owned();
    assert!(oracle(&dir).is_empty(), "{:?}", oracle(&dir));

    call(
        "acl_set",
        vec![s(&path), s("user:nobody allow list,search")],
        &mut env,
    )
    .unwrap();
    assert_eq!(oracle(&dir), vec!["user:nobody allow list,search"]);
    let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
    let aces = entries(&got);
    assert_eq!(aces.len(), 1, "{aces:?}");
    assert_eq!(words(&aces[0], "perms"), vec!["list", "search"]);

    call(
        "acl_set",
        vec![
            s(&path),
            s("user:nobody allow list,search"),
            remove_option(),
        ],
        &mut env,
    )
    .unwrap();
    assert!(oracle(&dir).is_empty(), "{:?}", oracle(&dir));
}

#[test]
fn refused_calls_change_nothing_and_say_why() {
    if !disposable() {
        return;
    }
    let _lock = lock();
    let scratch = Scratch::new();
    let mut env = Env::default();
    let file = scratch.file("file");
    let path = file.to_string_lossy().into_owned();
    let dir = scratch.dir("dir");
    let dir_path = dir.to_string_lossy().into_owned();
    let missing = scratch.root.join("missing").to_string_lossy().into_owned();
    let before = oracle(&file);
    assert!(before.is_empty());

    // The Linux form is not a macOS entry.
    code(
        call("acl_set", vec![s(&path), s("u::rw-")], &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(oracle(&file), before);

    // A permission name ls -le does not print.
    code(
        call(
            "acl_set",
            vec![s(&path), s("user:nobody allow fly")],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(oracle(&file), before);

    // Inheritance flags are not taken.
    code(
        call(
            "acl_set",
            vec![
                s(&dir_path),
                s("user:nobody allow list"),
                rec(vec![("default", Value::Bool(true))]),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(oracle(&dir).is_empty());

    // An option the builtin does not have.
    code(
        call(
            "acl_set",
            vec![
                s(&path),
                s("user:nobody allow read"),
                rec(vec![("recursive", Value::Bool(true))]),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(oracle(&file), before);

    // A missing path is not found, for a change and for a read.
    code(
        call(
            "acl_set",
            vec![s(&missing), s("user:nobody allow read")],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    code(call("acl_get", vec![s(&missing)], &mut env), "E_NOT_FOUND");
    assert_eq!(oracle(&file), before);

    // Removing an entry that is not there is a coded error, and changes nothing.
    code(
        call(
            "acl_set",
            vec![s(&path), s("user:nobody allow read"), remove_option()],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    assert_eq!(oracle(&file), before);
}
