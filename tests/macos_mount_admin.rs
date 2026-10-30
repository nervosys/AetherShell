//! Contracts for fs_mount and fs_unmount on macOS, through hdiutil and umount.
//!
//! Compiled on macOS only. Every test is `#[ignore]`d and returns at once unless
//! AE_DISPOSABLE_HOST=1, which only the disposable macOS runner sets. They attach
//! and detach disk images, so they run as root there:
//!
//!     sudo -E env AE_DISPOSABLE_HOST=1 <test binary> --include-ignored --test-threads=1
//!
//! Each test makes its own image and mount point under a temporary directory. A
//! Drop guard detaches the mount point and removes the directory. The oracle is
//! `/sbin/mount`, run by the test itself and never through the builtin.

#![cfg(target_os = "macos")]

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

// The environment and the mount table are process-wide, so the tests take this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn root() -> bool {
    unsafe { libc::geteuid() == 0 }
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

fn fields(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Record(map) => map,
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

fn flag(map: &BTreeMap<String, Value>, key: &str) -> bool {
    matches!(map.get(key), Some(Value::Bool(true)))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// One line of `/sbin/mount`: `<device> on <mountpoint> (<fstype>, <options>)`.
struct Mounted {
    point: String,
    options: String,
}

fn mount_table() -> Vec<Mounted> {
    let out = Command::new("/sbin/mount")
        .env("LC_ALL", "C")
        .output()
        .expect("mount runs");
    assert!(out.status.success(), "mount failed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (_device, rest) = line.split_once(" on ")?;
            let (point, options) = rest.rsplit_once(" (")?;
            Some(Mounted {
                point: point.to_string(),
                options: options.trim_end_matches(')').to_string(),
            })
        })
        .collect()
}

/// The oracle's entry for a mount point, if one is listed there.
fn mounted_at(path: &Path) -> Option<Mounted> {
    let want = path_text(path);
    mount_table().into_iter().find(|m| m.point == want)
}

fn mount_points() -> Vec<String> {
    mount_table().into_iter().map(|m| m.point).collect()
}

/// A temporary directory for one test, with an image and a mount point in it.
/// The Drop guard detaches the mount point, best effort, and removes everything.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aemnt-{}", uuid::Uuid::new_v4()));
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
        std::env::set_var("IRONSHELL_SESSION_DIR", root.join("session"));
        std::fs::create_dir(root.join("session")).unwrap();
        std::fs::create_dir(root.join("mnt")).unwrap();
        Self { root }
    }

    fn mnt(&self) -> PathBuf {
        self.root.join("mnt")
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::create_dir(&path).unwrap();
        path
    }

    /// A small HFS+ disk image, made by hdiutil here and not by the builtin.
    fn make_image(&self) -> PathBuf {
        let image = self.root.join("img.dmg");
        let volname = format!("AECI{}", std::process::id());
        let out = Command::new("/usr/bin/hdiutil")
            .env("LC_ALL", "C")
            .args([
                "create",
                "-size",
                "4m",
                "-fs",
                "HFS+",
                "-volname",
                volname.as_str(),
                "-layout",
                "NONE",
            ])
            .arg(&image)
            .output()
            .expect("hdiutil runs");
        assert!(
            out.status.success(),
            "hdiutil create failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        image
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-force"])
            .arg(self.mnt())
            .output();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore]
fn a_disk_image_mounts_reads_writes_and_unmounts() {
    if !disposable() {
        return;
    }
    assert!(
        root(),
        "the mount tests run as root on the disposable runner"
    );
    let _lock = lock();
    let scratch = Scratch::new();
    let image = scratch.make_image();
    let mnt = scratch.mnt();
    let mut env = Env::default();

    let mounted = fields(
        call(
            "fs_mount",
            vec![s(&path_text(&image)), s(&path_text(&mnt))],
            &mut env,
        )
        .unwrap(),
    );
    assert_eq!(text(&mounted, "source"), path_text(&image));
    assert_eq!(text(&mounted, "target"), path_text(&mnt));
    assert!(!flag(&mounted, "readonly"));

    // The oracle lists the mount point, with the file system the builtin reported.
    let listed = mounted_at(&mnt).expect("the oracle lists the mount point");
    let fstype = listed.options.split(',').next().unwrap_or("").trim();
    assert_eq!(fstype, text(&mounted, "fstype"), "{}", listed.options);
    assert!(!listed.options.contains("read-only"), "{}", listed.options);

    // Write through the mount, and read the file back.
    std::fs::write(mnt.join("aeci.txt"), "through the mount").unwrap();
    assert_eq!(
        std::fs::read_to_string(mnt.join("aeci.txt")).unwrap(),
        "through the mount"
    );

    let unmounted = fields(call("fs_unmount", vec![s(&path_text(&mnt))], &mut env).unwrap());
    assert!(flag(&unmounted, "unmounted"));
    assert!(
        mounted_at(&mnt).is_none(),
        "the oracle still lists the mount"
    );
    assert_eq!(
        std::fs::read_dir(&mnt).unwrap().count(),
        0,
        "the file was on the image, so the directory is empty again"
    );
}

#[test]
#[ignore]
fn a_read_only_mount_refuses_writes_and_a_lazy_unmount_detaches() {
    if !disposable() {
        return;
    }
    assert!(
        root(),
        "the mount tests run as root on the disposable runner"
    );
    let _lock = lock();
    let scratch = Scratch::new();
    let image = scratch.make_image();
    let mnt = scratch.mnt();
    let mut env = Env::default();

    let mounted = fields(
        call(
            "fs_mount",
            vec![
                s(&path_text(&image)),
                s(&path_text(&mnt)),
                rec(vec![("readonly", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap(),
    );
    assert!(flag(&mounted, "readonly"));
    let listed = mounted_at(&mnt).expect("the oracle lists the mount point");
    assert!(
        listed.options.split(',').any(|o| o.trim() == "read-only"),
        "{}",
        listed.options
    );
    assert!(std::fs::write(mnt.join("refused.txt"), "x").is_err());

    let unmounted = fields(
        call(
            "fs_unmount",
            vec![s(&path_text(&mnt)), rec(vec![("lazy", Value::Bool(true))])],
            &mut env,
        )
        .unwrap(),
    );
    assert!(flag(&unmounted, "unmounted"));
    assert!(
        mounted_at(&mnt).is_none(),
        "the oracle still lists the mount"
    );
}

#[test]
#[ignore]
fn refused_mounts_and_unmounts_change_nothing() {
    if !disposable() {
        return;
    }
    assert!(
        root(),
        "the mount tests run as root on the disposable runner"
    );
    let _lock = lock();
    let scratch = Scratch::new();
    let image = scratch.make_image();
    let mnt = scratch.mnt();
    let busy = scratch.dir("busy");
    std::fs::write(busy.join("kept"), "x").unwrap();
    let plain = scratch.dir("plain");
    let before = mount_points();
    let mut env = Env::default();
    let img = s(&path_text(&image));
    let target = s(&path_text(&mnt));

    // A target that is not empty.
    code(
        call(
            "fs_mount",
            vec![img.clone(), s(&path_text(&busy))],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    // bind, fstype and options have no macOS meaning.
    code(
        call(
            "fs_mount",
            vec![
                img.clone(),
                target.clone(),
                rec(vec![("bind", Value::Bool(true))]),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "fs_mount",
            vec![img.clone(), target.clone(), rec(vec![("fstype", s("hfs"))])],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "fs_mount",
            vec![
                img.clone(),
                target.clone(),
                rec(vec![("options", Value::Array(vec![s("noexec")]))]),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    // A directory and a device node are not disk images.
    code(
        call(
            "fs_mount",
            vec![s(&path_text(&scratch.root)), target.clone()],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call("fs_mount", vec![s("/dev/null"), target.clone()], &mut env),
        "E_BAD_ARG",
    );
    // A protected target, by canonical path.
    code(
        call("fs_mount", vec![img.clone(), s("/Volumes")], &mut env),
        "E_POLICY_DENY",
    );
    // Unmounting a directory that is not a mount point, and the root.
    code(
        call("fs_unmount", vec![s(&path_text(&plain))], &mut env),
        "E_NOT_FOUND",
    );
    code(call("fs_unmount", vec![s("/")], &mut env), "E_POLICY_DENY");

    assert_eq!(
        mount_points(),
        before,
        "a refusal mounted or unmounted something"
    );
    assert!(
        std::fs::read_dir(&mnt).unwrap().next().is_none(),
        "a refused mount left something in the mount point"
    );
}
