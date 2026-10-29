//! Contracts for `acl_get`, `acl_set`, `fs_mount` and `fs_unmount`.
//!
//! The ordinary tests here run everywhere: they check argument refusals,
//! pipeline-input refusals, transaction refusals, and the coded E_UNIMPLEMENTED
//! refusal of the mount builtins off Linux and Windows (and of the ACL builtins
//! off Linux, Windows and macOS). Linux also runs a read-only ACL test on a file this
//! test owns. The tests that change the system (ACLs on owned files, loop and
//! bind mounts, unmount refusals) are `#[ignore]`d and run only as root in the
//! disposable distro: scripts/disposable_linux_tests.sh.
#[path = "support/disposable.rs"]
#[allow(dead_code)]
mod disposable;

use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::{path::PathBuf, sync::Mutex};

static LOCK: Mutex<()> = Mutex::new(());

const SANDBOX_KEYS: &[&str] = &[
    "AETHER_MODE",
    "AETHER_AGENT",
    "AETHER_WORKSPACE",
    "AETHER_POLICY",
    "AETHER_AUDIT_LOG",
    "AETHER_AUDIT_KEY",
    "AETHER_AUDIT_KEY_FILE",
];

/// Keeps the audit log out of the workspace, and removes any jail or mode
/// setting another test might have left behind. The environment is restored on drop.
struct Sandbox {
    root: PathBuf,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aeaclm-sandbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let saved = SANDBOX_KEYS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        for key in SANDBOX_KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("AETHER_AUDIT_LOG", root.join("audit.log"));
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        Self { root, saved }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        while aethershell::tx::rollback().is_ok() {}
        for (key, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn rec(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    match result {
        Ok(v) => panic!("expected {expected}, got success {v:?}"),
        Err(e) => assert!(e.to_string().contains(expected), "expected {expected}: {e}"),
    }
}

#[test]
fn bad_arguments_are_refused_before_any_effect() {
    let _lock = LOCK.lock().unwrap();
    let _sandbox = Sandbox::new();
    let mut env = Env::default();
    let cases: Vec<(&str, Vec<Value>, &str)> = vec![
        ("acl_get", vec![], "E_BAD_ARG"),
        ("acl_get", vec![Value::Int(1)], "E_BAD_ARG"),
        ("acl_get", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("acl_get", vec![s("")], "E_BAD_ARG"),
        ("acl_get", vec![s("-rf")], "E_BAD_ARG"),
        ("acl_set", vec![], "E_BAD_ARG"),
        ("acl_set", vec![s("/x")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("bogus")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("u:r--")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("u:nobody:rw")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("u:nobody:rwz")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("u:Nobody:r--")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("u:1000:r--")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("m:someone:rw-")], "E_BAD_ARG"),
        ("acl_set", vec![s("/x"), s("o:someone:r--")], "E_BAD_ARG"),
        (
            "acl_set",
            vec![s("/x"), s("u::rw-"), Value::Int(1)],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![s("/x"), s("u::rw-"), rec(vec![("default", Value::Int(1))])],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![
                s("/x"),
                s("u::rw-"),
                rec(vec![("bogus", Value::Bool(true))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![
                s("/x"),
                s("u:nobody:r--"),
                rec(vec![("remove", Value::Bool(true))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![s("/x"), s("m:x"), rec(vec![("remove", Value::Bool(true))])],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![s("/x"), s("o:"), rec(vec![("remove", Value::Bool(true))])],
            "E_BAD_ARG",
        ),
        (
            "acl_set",
            vec![s("/x"), s("u:"), rec(vec![("remove", Value::Bool(true))])],
            "E_BAD_ARG",
        ),
        ("acl_set", vec![s("-x"), s("u::rw-")], "E_BAD_ARG"),
        ("fs_mount", vec![], "E_BAD_ARG"),
        ("fs_mount", vec![s("/a")], "E_BAD_ARG"),
        ("fs_mount", vec![s("relative"), s("/b")], "E_BAD_ARG"),
        ("fs_mount", vec![s("/a"), s("relative")], "E_BAD_ARG"),
        (
            "fs_mount",
            vec![s("/a"), s("/b"), Value::Int(1)],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![s("/a"), s("/b"), rec(vec![("fstype", s("EXT4"))])],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![s("/a"), s("/b"), rec(vec![("readonly", s("yes"))])],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![Value::Int(1)]))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![s("a b")]))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![s("a,,b")]))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![s("remount")]))]),
            ],
            "E_POLICY_DENY",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![s("ro,remount=ro")]))]),
            ],
            "E_POLICY_DENY",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("options", Value::Array(vec![s("bind")]))]),
            ],
            "E_POLICY_DENY",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![
                    ("bind", Value::Bool(true)),
                    ("readonly", Value::Bool(true)),
                ]),
            ],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![
                s("/a"),
                s("/b"),
                rec(vec![("bind", Value::Bool(true)), ("fstype", s("ext4"))]),
            ],
            "E_BAD_ARG",
        ),
        (
            "fs_mount",
            vec![s("/a"), s("/b"), rec(vec![("device", s("/dev/sda"))])],
            "E_BAD_ARG",
        ),
        ("fs_unmount", vec![], "E_BAD_ARG"),
        ("fs_unmount", vec![s("relative")], "E_BAD_ARG"),
        ("fs_unmount", vec![Value::Int(1)], "E_BAD_ARG"),
        ("fs_unmount", vec![s("/x"), Value::Int(1)], "E_BAD_ARG"),
        (
            "fs_unmount",
            vec![s("/x"), rec(vec![("lazy", Value::Int(1))])],
            "E_BAD_ARG",
        ),
        (
            "fs_unmount",
            vec![s("/x"), rec(vec![("force", Value::Bool(true))])],
            "E_BAD_ARG",
        ),
    ];
    for (name, args, expected) in cases {
        code(call(name, args.clone(), &mut env), expected);
    }
}

#[test]
fn pipeline_input_is_refused_for_every_builtin() {
    let _lock = LOCK.lock().unwrap();
    let _sandbox = Sandbox::new();
    let mut env = Env::default();
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("acl_get", vec![s("/x")]),
        ("acl_set", vec![s("/x"), s("u::rw-")]),
        ("fs_mount", vec![s("/a"), s("/b")]),
        ("fs_unmount", vec![s("/a")]),
    ];
    for (name, args) in cases {
        code(
            call_with_input(name, args, Some(s("piped")), &mut env),
            "E_BAD_ARG",
        );
    }
}

/// A well-formed entry for this platform's acl_set grammar.
fn acl_entry_for_this_os() -> &'static str {
    if cfg!(windows) {
        "Users:F"
    } else if cfg!(target_os = "macos") {
        "user:nobody allow read"
    } else {
        "u::rw-"
    }
}

#[test]
fn every_builtin_refuses_inside_a_transaction_without_touching_anything() {
    let _lock = LOCK.lock().unwrap();
    let _sandbox = Sandbox::new();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    code(call("acl_get", vec![s("/")], &mut env), "E_POLICY_DENY");
    code(
        call(
            "acl_set",
            vec![s("/"), s(acl_entry_for_this_os())],
            &mut env,
        ),
        "E_POLICY_DENY",
    );
    code(
        call("fs_mount", vec![s("/a"), s("/b")], &mut env),
        "E_POLICY_DENY",
    );
    code(call("fs_unmount", vec![s("/a")], &mut env), "E_POLICY_DENY");
    aethershell::tx::rollback().unwrap();
}

#[test]
fn declared_examples_are_refusals() {
    for name in ["acl_get", "acl_set", "fs_mount", "fs_unmount"] {
        let signature = aethershell::signature::signature_of(name).unwrap();
        assert!(!signature.examples.is_empty(), "{name} has no examples");
        assert!(
            !signature.doc.contains("Unsupported placeholder"),
            "{name} still describes itself as a placeholder"
        );
        for (source, expected) in signature.examples {
            assert_eq!(*expected, "E_BAD_ARG", "{name} example {source}");
        }
    }
}

/// Off Linux, Windows and macOS, the ACL and mount builtins are unimplemented.
/// macOS runs ls, chmod, hdiutil and umount for them, which macos_acl_admin and
/// macos_mount_admin cover.
#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
#[test]
fn off_linux_every_valid_call_is_unimplemented_and_changes_nothing() {
    let _lock = LOCK.lock().unwrap();
    let sandbox = Sandbox::new();
    let mut env = Env::default();
    let file = sandbox.root.join("owned");
    std::fs::write(&file, "unchanged").unwrap();
    let file = file.to_string_lossy().into_owned();
    code(call("acl_get", vec![s(&file)], &mut env), "E_UNIMPLEMENTED");
    code(
        call("acl_set", vec![s(&file), s("u:nobody:r--")], &mut env),
        "E_UNIMPLEMENTED",
    );
    code(
        call("fs_mount", vec![s("/dev/null"), s("/mnt/aeaclm")], &mut env),
        "E_UNIMPLEMENTED",
    );
    code(
        call("fs_unmount", vec![s("/")], &mut env),
        "E_UNIMPLEMENTED",
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unchanged");
}

#[cfg(windows)]
#[test]
fn windows_mount_builtins_stay_unimplemented_and_linux_entries_are_refused() {
    let _lock = LOCK.lock().unwrap();
    let sandbox = Sandbox::new();
    let mut env = Env::default();
    let file = sandbox.root.join("owned");
    std::fs::write(&file, "unchanged").unwrap();
    let file = file.to_string_lossy().into_owned();
    code(
        call("acl_set", vec![s(&file), s("u:nobody:r--")], &mut env),
        "E_BAD_ARG",
    );
    code(
        call("fs_mount", vec![s("/dev/null"), s("/mnt/aeaclm")], &mut env),
        "E_UNIMPLEMENTED",
    );
    code(
        call("fs_unmount", vec![s("/")], &mut env),
        "E_UNIMPLEMENTED",
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unchanged");
}

#[cfg(target_os = "linux")]
#[test]
fn acl_get_reads_an_owned_file_without_changing_it() {
    let _lock = LOCK.lock().unwrap();
    let sandbox = Sandbox::new();
    let mut env = Env::default();
    let file = sandbox.root.join("owned");
    std::fs::write(&file, "x").unwrap();
    let path = file.to_string_lossy().into_owned();
    let Value::Record(fields) = call("acl_get", vec![s(&path)], &mut env).unwrap() else {
        panic!("acl_get should return a record");
    };
    assert_eq!(fields.get("path"), Some(&s(&path)));
    assert!(matches!(fields.get("owner"), Some(Value::Str(_))));
    assert!(matches!(fields.get("group"), Some(Value::Str(_))));
    let Some(Value::Array(entries)) = fields.get("entries") else {
        panic!("entries should be an array");
    };
    let has = |tag: &str| {
        entries.iter().any(|e| match e {
            Value::Record(r) => {
                r.get("tag") == Some(&s(tag)) && r.get("qualifier") == Some(&Value::Null)
            }
            _ => false,
        })
    };
    assert!(has("user") && has("group") && has("other"), "{entries:?}");
    code(
        call("acl_get", vec![s(&format!("{path}-missing"))], &mut env),
        "E_NOT_FOUND",
    );
}

#[cfg(target_os = "linux")]
mod disposable_tests {
    use super::{code, rec, s, Sandbox, LOCK};
    use aethershell::{builtins::call, env::Env, value::Value};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    /// Every scratch path lives under one unique directory. On drop, any mount
    /// this test made is detached and the directory is removed.
    struct Scratch {
        root: PathBuf,
        mounts: Vec<PathBuf>,
    }

    impl Scratch {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("aeaclm-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self {
                root: root.canonicalize().unwrap(),
                mounts: Vec::new(),
            }
        }
        fn path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }
        fn dir(&self, name: &str) -> PathBuf {
            let path = self.path(name);
            fs::create_dir(&path).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            for mount in self.mounts.iter().rev() {
                if is_mounted_at(mount) {
                    let _ = Command::new("umount").arg("-l").arg(mount).status();
                }
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// The mount-table line for `path`, read by the test itself rather than
    /// through the builtin.
    fn mount_line(path: &Path) -> Option<String> {
        let table = fs::read_to_string("/proc/self/mounts").unwrap();
        let wanted = path.to_string_lossy();
        table
            .lines()
            .find(|line| line.split_whitespace().nth(1) == Some(wanted.as_ref()))
            .map(str::to_string)
    }

    fn is_mounted_at(path: &Path) -> bool {
        mount_line(path).is_some()
    }

    fn getfacl(path: &Path) -> String {
        let output = Command::new("getfacl")
            .args(["-p", "--"])
            .arg(path)
            .output()
            .expect("getfacl runs in the disposable distro");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
        match value {
            Value::Record(map) => map.get(key).unwrap_or_else(|| panic!("no field {key}")),
            other => panic!("not a record: {other:?}"),
        }
    }

    fn entries(value: &Value) -> Vec<Value> {
        match field(value, "entries") {
            Value::Array(items) => items.clone(),
            other => panic!("entries not an array: {other:?}"),
        }
    }

    fn entry_matches(
        entry: &Value,
        tag: &str,
        qualifier: Option<&str>,
        perms: &str,
        default: bool,
    ) -> bool {
        field(entry, "tag") == &s(tag)
            && match qualifier {
                Some(q) => field(entry, "qualifier") == &s(q),
                None => field(entry, "qualifier") == &Value::Null,
            }
            && field(entry, "perms") == &s(perms)
            && field(entry, "default") == &Value::Bool(default)
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeaclm_acl_set_get_and_remove_a_named_user() {
        crate::disposable::require();
        let _lock = LOCK.lock().unwrap();
        let _sandbox = Sandbox::new();
        let mut env = Env::default();
        let mut scratch = Scratch::new();
        let file = scratch.path("file");
        fs::write(&file, "x").unwrap();
        let path = file.to_string_lossy().into_owned();

        let set = call("acl_set", vec![s(&path), s("u:nobody:r--")], &mut env).unwrap();
        assert_eq!(field(&set, "entry"), &s("u:nobody:r--"));
        assert_eq!(field(&set, "applied"), &Value::Bool(true));
        assert!(
            getfacl(&file).contains("user:nobody:r--"),
            "{}",
            getfacl(&file)
        );

        let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
        assert_eq!(field(&got, "path"), &s(&path));
        assert!(entries(&got).iter().any(|e| entry_matches(
            e,
            "user",
            Some("nobody"),
            "r--",
            false
        )));

        call(
            "acl_set",
            vec![
                s(&path),
                s("u:nobody"),
                rec(vec![("remove", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap();
        assert!(
            !getfacl(&file).contains("user:nobody"),
            "{}",
            getfacl(&file)
        );
        let got = call("acl_get", vec![s(&path)], &mut env).unwrap();
        assert!(!entries(&got).iter().any(|e| entry_matches(
            e,
            "user",
            Some("nobody"),
            "r--",
            false
        )));
        scratch.mounts.clear();
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeaclm_default_acl_applies_to_directories_only() {
        crate::disposable::require();
        let _lock = LOCK.lock().unwrap();
        let _sandbox = Sandbox::new();
        let mut env = Env::default();
        let scratch = Scratch::new();
        let dir = scratch.dir("dir");
        let dir_text = dir.to_string_lossy().into_owned();
        let file = scratch.path("file");
        fs::write(&file, "x").unwrap();
        let file_text = file.to_string_lossy().into_owned();

        code(
            call(
                "acl_set",
                vec![
                    s(&file_text),
                    s("g:nogroup:rwx"),
                    rec(vec![("default", Value::Bool(true))]),
                ],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        call(
            "acl_set",
            vec![
                s(&dir_text),
                s("g:nogroup:rwx"),
                rec(vec![("default", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap();
        assert!(
            getfacl(&dir).contains("default:group:nogroup:rwx"),
            "{}",
            getfacl(&dir)
        );
        let got = call("acl_get", vec![s(&dir_text)], &mut env).unwrap();
        assert!(entries(&got).iter().any(|e| entry_matches(
            e,
            "group",
            Some("nogroup"),
            "rwx",
            true
        )));

        call(
            "acl_set",
            vec![
                s(&dir_text),
                s("g:nogroup"),
                rec(vec![
                    ("default", Value::Bool(true)),
                    ("remove", Value::Bool(true)),
                ]),
            ],
            &mut env,
        )
        .unwrap();
        assert!(
            !getfacl(&dir).contains("default:group:nogroup"),
            "{}",
            getfacl(&dir)
        );
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeaclm_loop_mount_is_read_only_then_read_write() {
        crate::disposable::require();
        let _lock = LOCK.lock().unwrap();
        let _sandbox = Sandbox::new();
        let mut env = Env::default();
        let mut scratch = Scratch::new();
        let image = scratch.path("disk.img");
        fs::File::create(&image)
            .unwrap()
            .set_len(16 * 1024 * 1024)
            .unwrap();
        let mkfs = Command::new("mkfs.ext4")
            .args(["-q", "-F"])
            .arg(&image)
            .status()
            .expect("mkfs.ext4 runs in the disposable distro");
        assert!(mkfs.success());
        let mnt = scratch.dir("mnt");
        let image_text = image.to_string_lossy().into_owned();
        let mnt_text = mnt.to_string_lossy().into_owned();

        let ro = call(
            "fs_mount",
            vec![
                s(&image_text),
                s(&mnt_text),
                rec(vec![("fstype", s("ext4")), ("readonly", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap();
        scratch.mounts.push(mnt.clone());
        assert_eq!(field(&ro, "fstype"), &s("ext4"));
        assert_eq!(field(&ro, "readonly"), &Value::Bool(true));
        let line = mount_line(&mnt).expect("listed in /proc/self/mounts");
        assert!(line.contains(" ext4 "), "{line}");
        assert!(
            line.split_whitespace()
                .nth(3)
                .unwrap()
                .split(',')
                .any(|o| o == "ro"),
            "{line}"
        );
        assert!(
            fs::write(mnt.join("probe"), "x").is_err(),
            "a read-only mount accepted a write"
        );

        call("fs_unmount", vec![s(&mnt_text)], &mut env).unwrap();
        assert!(!is_mounted_at(&mnt));

        let rw = call(
            "fs_mount",
            vec![
                s(&image_text),
                s(&mnt_text),
                rec(vec![("fstype", s("ext4"))]),
            ],
            &mut env,
        )
        .unwrap();
        scratch.mounts.push(mnt.clone());
        assert_eq!(field(&rw, "readonly"), &Value::Bool(false));
        fs::write(mnt.join("probe"), "written").unwrap();
        assert_eq!(fs::read_to_string(mnt.join("probe")).unwrap(), "written");

        let un = call(
            "fs_unmount",
            vec![s(&mnt_text), rec(vec![("lazy", Value::Bool(true))])],
            &mut env,
        )
        .unwrap();
        assert_eq!(field(&un, "unmounted"), &Value::Bool(true));
        assert!(!is_mounted_at(&mnt));
        scratch.mounts.clear();
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeaclm_bind_mount_shows_the_source_directory() {
        crate::disposable::require();
        let _lock = LOCK.lock().unwrap();
        let _sandbox = Sandbox::new();
        let mut env = Env::default();
        let mut scratch = Scratch::new();
        let src = scratch.dir("src");
        fs::write(src.join("marker"), "bound").unwrap();
        let target = scratch.dir("bind");
        let src_text = src.to_string_lossy().into_owned();
        let target_text = target.to_string_lossy().into_owned();

        call(
            "fs_mount",
            vec![
                s(&src_text),
                s(&target_text),
                rec(vec![("bind", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap();
        scratch.mounts.push(target.clone());
        assert_eq!(fs::read_to_string(target.join("marker")).unwrap(), "bound");
        assert!(is_mounted_at(&target));

        call("fs_unmount", vec![s(&target_text)], &mut env).unwrap();
        assert!(!is_mounted_at(&target));
        scratch.mounts.clear();
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeaclm_unmount_refuses_protected_paths_and_non_mounts() {
        crate::disposable::require();
        let _lock = LOCK.lock().unwrap();
        let _sandbox = Sandbox::new();
        let mut env = Env::default();
        let scratch = Scratch::new();

        code(call("fs_unmount", vec![s("/")], &mut env), "E_POLICY_DENY");
        code(
            call("fs_unmount", vec![s("/proc")], &mut env),
            "E_POLICY_DENY",
        );
        code(
            call("fs_unmount", vec![s("/proc/")], &mut env),
            "E_POLICY_DENY",
        );
        let plain = scratch.dir("plain");
        code(
            call("fs_unmount", vec![s(&plain.to_string_lossy())], &mut env),
            "E_NOT_FOUND",
        );

        // A protected target is refused before the source is even examined.
        let file = scratch.path("file");
        fs::write(&file, "not a filesystem").unwrap();
        code(
            call(
                "fs_mount",
                vec![s(&file.to_string_lossy()), s("/proc")],
                &mut env,
            ),
            "E_POLICY_DENY",
        );
        // A directory source needs bind: true.
        let target = scratch.dir("target");
        code(
            call(
                "fs_mount",
                vec![s(&plain.to_string_lossy()), s(&target.to_string_lossy())],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        // A regular file is loop-mounted, and this one holds no filesystem, so mount refuses.
        code(
            call(
                "fs_mount",
                vec![s(&file.to_string_lossy()), s(&target.to_string_lossy())],
                &mut env,
            ),
            "E_TOOL_FAILED",
        );
        assert!(!is_mounted_at(&target));
    }
}
