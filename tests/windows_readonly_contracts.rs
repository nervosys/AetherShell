#![cfg(windows)]
//! Windows contracts for the file builtins against the read-only attribute,
//! and for `fs_symlink` without the symbolic-link privilege.
//!
//! Each expectation follows the builtin's doc in src/signature.rs and was read
//! against its body in src/builtins.rs. Refusals are read from the coded error
//! text, so a refusal that is not coded fails the test.
//!
//! The read-only attribute is set and cleared with `Permissions::set_readonly`.
//! The attribute is always cleared again before the fixture is removed, since
//! a read-only file blocks cleanup.

use iron_shell::{builtins::call, env::Env, value::Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Mutex,
};

static LOCK: Mutex<()> = Mutex::new(());

struct Fixture {
    root: PathBuf,
    vars: Vec<(&'static str, Option<OsString>)>,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("ae-readonly-contract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let keys = [
            "NO_COLOR",
            "IRONSHELL_MODE",
            "IRONSHELL_AGENT",
            "IRONSHELL_WORKSPACE",
            "IRONSHELL_POLICY",
            "IRONSHELL_SESSION_DIR",
            "IRONSHELL_AUDIT_LOG",
            "IRONSHELL_AUDIT_KEY",
            "IRONSHELL_AUDIT_KEY_FILE",
            "IRONSHELL_AUDIT_SINK",
        ];
        let fixture = Self {
            root,
            vars: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        };
        for key in keys {
            std::env::remove_var(key);
        }
        let session = fixture.root.join("session");
        std::fs::create_dir(&session).unwrap();
        std::env::set_var("IRONSHELL_WORKSPACE", &fixture.root);
        std::env::set_var("IRONSHELL_SESSION_DIR", &session);
        std::env::set_var("IRONSHELL_AUDIT_LOG", fixture.root.join("audit.log"));
        std::env::set_var("IRONSHELL_AUDIT_KEY", "11".repeat(32));
        std::env::set_var("NO_COLOR", "1");
        fixture
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        while iron_shell::tx::rollback().is_ok() {}
        for (key, value) in &self.vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        // Only recursively remove the exact absolute directory we exclusively created.
        if self
            .root
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.root)
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// Sets or clears the read-only attribute.
fn set_read_only(path: &Path, read_only: bool) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions).unwrap();
}

/// Clears the read-only attribute when dropped, so the fixture can be removed
/// even when an assertion fails. Declared after the fixture, it drops first.
struct Locked(PathBuf);

impl Drop for Locked {
    fn drop(&mut self) {
        if self.0.exists() {
            set_read_only(&self.0, false);
        }
    }
}

fn is_read_only(path: &Path) -> bool {
    std::fs::metadata(path).unwrap().permissions().readonly()
}

/// Names of `.ironshell_tmp_*` files left in a directory.
fn leftover_temporaries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".ironshell_tmp_"))
        .collect()
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// The `code` field of a coded refusal. Panics when the error carries none.
fn error_code(result: anyhow::Result<Value>) -> String {
    let text = match result {
        Ok(value) => panic!("expected a coded refusal, got {value:?}"),
        Err(error) => error.to_string(),
    };
    let rest = text
        .split("\"code\":\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no coded refusal in {text}"));
    rest.split('"').next().unwrap().to_string()
}

#[test]
fn file_write_over_read_only_file_is_refused_and_leaves_no_temporary() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // The temporary is created beside the target, given the target's read-only
    // attribute, and the rename over the target is refused by the operating
    // system: E_OS_DENIED (io::ErrorKind::PermissionDenied), not E_POLICY_DENY.
    let code = error_code(call(
        "file_write",
        vec![s(&fixture.path("locked.txt")), s("new")],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(is_read_only(&target));
    assert!(leftover_temporaries(&fixture.root).is_empty());
}

#[test]
fn file_append_to_read_only_file_is_refused_and_content_unchanged() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // Opening for append needs write access, which the read-only attribute denies.
    let code = error_code(call(
        "file_append",
        vec![s(&fixture.path("locked.txt")), s(" more")],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(is_read_only(&target));
}

#[test]
fn file_patch_of_read_only_file_is_refused_and_content_unchanged() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // The search matches, so the body reaches the atomic write-back and is refused there.
    let patch = record(vec![("find", s("old")), ("replace", s("new"))]);
    let code = error_code(call(
        "file_patch",
        vec![s(&fixture.path("locked.txt")), patch],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(is_read_only(&target));
    assert!(leftover_temporaries(&fixture.root).is_empty());
}

#[test]
fn file_replace_of_read_only_file_is_refused_and_content_unchanged() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // file_replace delegates to file_patch, so the refusal is the same.
    let code = error_code(call(
        "file_replace",
        vec![s(&fixture.path("locked.txt")), s("old"), s("new")],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(is_read_only(&target));
}

#[test]
fn rm_of_read_only_file_removes_it() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // The doc codes a locked file or a permission refusal as E_OS_DENIED. A
    // read-only attribute is neither: std::fs::remove_file on Windows retries a
    // refused DeleteFileW as a POSIX-semantics delete, which ignores the
    // attribute, so the file is removed and the call answers success.
    let result = call("rm", vec![s(&fixture.path("locked.txt"))], &mut env).unwrap();
    match result {
        Value::Str(text) => assert!(text.starts_with("removed: "), "{text}"),
        other => panic!("rm should answer a String, got {other:?}"),
    }
    assert!(!target.exists());
}

#[test]
fn fs_chmod_round_trips_the_read_only_attribute_on_windows() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("mode.txt");
    std::fs::write(&target, "x").unwrap();
    let _locked = Locked(target.clone());
    let path = fixture.path("mode.txt");

    // Only the owner-write bit is mapped: 0o444 clears it, 0o644 sets it.
    assert!(matches!(
        call("fs_chmod", vec![s(&path), s("0444")], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(is_read_only(&target));

    assert!(matches!(
        call("fs_chmod", vec![s(&path), Value::Int(0o644)], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(!is_read_only(&target));

    // The octal String and Int forms agree.
    assert!(matches!(
        call("fs_chmod", vec![s(&path), s("0o400")], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(is_read_only(&target));
    assert!(matches!(
        call("fs_chmod", vec![s(&path), Value::Int(0o600)], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(!is_read_only(&target));
}

#[test]
fn fs_chmod_accepts_unix_only_bits_and_maps_only_owner_write() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("mode.txt");
    std::fs::write(&target, "x").unwrap();
    let _locked = Locked(target.clone());
    let path = fixture.path("mode.txt");

    // NOTE: the premise that a Unix-only mode is coded does not hold. The doc
    // says "on Windows set the read-only attribute from the owner-write bit
    // only" and "return true or a coded failure". Setuid, setgid and sticky
    // bits are valid octal modes (0..=07777), so they are accepted and ignored,
    // and the result is true. Setting or clearing the owner-write bit still applies.
    assert!(matches!(
        call("fs_chmod", vec![s(&path), s("04444")], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(is_read_only(&target));
    assert!(matches!(
        call("fs_chmod", vec![s(&path), s("06644")], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(!is_read_only(&target));

    // A string that is not octal is coded.
    let code = error_code(call("fs_chmod", vec![s(&path), s("0o9")], &mut env));
    assert_eq!(code, "E_BAD_ARG");
    assert!(!is_read_only(&target));
}

#[test]
fn touch_of_read_only_file_is_refused_and_content_unchanged() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("locked.txt");
    std::fs::write(&target, "old").unwrap();
    set_read_only(&target, true);
    let _locked = Locked(target.clone());

    // The doc promises only that touch sets the modification time. The body
    // opens the existing file for append to do that, which the read-only
    // attribute refuses, so the refusal is coded E_OS_DENIED and the time is
    // not changed. The doc is silent about read-only files.
    let code = error_code(call(
        "touch",
        vec![s(&fixture.path("locked.txt"))],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
    assert!(is_read_only(&target));
}

#[test]
fn file_copy_onto_read_only_destination_is_refused_and_destination_unchanged() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.root.join("source.txt");
    let destination = fixture.root.join("locked.txt");
    std::fs::write(&source, "new").unwrap();
    std::fs::write(&destination, "old").unwrap();
    set_read_only(&destination, true);
    let _locked = Locked(destination.clone());

    // The copy goes through a temporary and an atomic rename over the
    // read-only destination, which the operating system refuses.
    let code = error_code(call(
        "file_copy",
        vec![
            s(&fixture.path("source.txt")),
            s(&fixture.path("locked.txt")),
        ],
        &mut env,
    ));
    assert_eq!(code, "E_OS_DENIED");
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "old");
    assert!(is_read_only(&destination));
    assert!(leftover_temporaries(&fixture.root).is_empty());
}

#[test]
fn fs_symlink_without_symlink_privilege_is_created_or_coded() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let target = fixture.root.join("target.txt");
    std::fs::write(&target, "hi").unwrap();
    let link = fixture.root.join("link.txt");

    // Creating a symbolic link needs SeCreateSymbolicLinkPrivilege or Developer
    // Mode. With either, the call succeeds and the link keeps the relative
    // spelling of its target. Without either, the OS error 1314
    // (ERROR_PRIVILEGE_NOT_HELD) is refused with a code.
    match call(
        "fs_symlink",
        vec![s("target.txt"), s(&fixture.path("link.txt"))],
        &mut env,
    ) {
        Ok(value) => {
            assert!(matches!(value, Value::Bool(true)));
            assert!(std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(
                std::fs::read_link(&link).unwrap(),
                PathBuf::from("target.txt")
            );
            assert_eq!(std::fs::read_to_string(&link).unwrap(), "hi");
        }
        Err(error) => {
            // ERROR_PRIVILEGE_NOT_HELD is the operating system refusing, so
            // fs_error codes it E_OS_DENIED (std does not count it as
            // PermissionDenied, which once let it fall through to E_IO).
            assert_eq!(error_code(Err(error)), "E_OS_DENIED");
            assert!(std::fs::symlink_metadata(&link).is_err());
        }
    }
}
