//! Contracts for the Windows `fs_mount` and `fs_unmount` backends, which attach
//! a VHD or VHDX file to an empty NTFS folder and detach it again.
//!
//! Compiled on Windows only. Each test builds its own 16 MB VHDX with diskpart
//! in a temp directory, mounts it on an empty folder there, and removes the
//! mount, the image and the directory in a Drop guard. The tests are `#[ignore]`d
//! and also return at once unless AE_DISPOSABLE_HOST=1, which only the
//! disposable Windows runner sets: they attach disks and change the mount table.
//!
//! The builtins are checked against PowerShell's own view of the system, not
//! against their return values alone: the number of partitions that list the
//! folder as an access path, and whether the image is attached. Paths reach
//! PowerShell in environment variables, never in the script text.

#![cfg(windows)]

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

/// Every test takes this lock, so mounts never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

/// Counts the partitions that list AE_DST (the folder, with a trailing `\`) as an access path.
const ACCESS_PATH_COUNT: &str = r#"(Get-Partition | Where-Object { $_.AccessPaths -contains ($env:AE_DST + '\') } | Measure-Object).Count"#;

/// Whether the image at AE_SRC is attached, as Get-DiskImage reports it.
const IMAGE_ATTACHED: &str = r#"(Get-DiskImage -ImagePath $env:AE_SRC).Attached"#;

/// Best-effort cleanup for the Drop guard: remove the folder's access path,
/// then detach the image. Errors are not reported.
const CLEANUP: &str = r#"
Get-Partition | Where-Object { $_.AccessPaths -contains ($env:AE_DST + '\') } | ForEach-Object { $_ | Remove-PartitionAccessPath -AccessPath ($env:AE_DST + '\') }
Dismount-DiskImage -ImagePath $env:AE_SRC -ErrorAction SilentlyContinue | Out-Null
"#;

/// True only on the disposable Windows runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

fn call_ok(name: &str, args: Vec<Value>) -> Value {
    call(name, args).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn expect_code(result: anyhow::Result<Value>, want: &str) {
    match result {
        Ok(v) => panic!("answered {v:?}; expected {want}"),
        Err(e) => assert_eq!(code_of(&e), want, "wrong refusal: {e}"),
    }
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(m) => m.get(key).unwrap_or_else(|| panic!("no {key} in {v:?}")),
        other => panic!("not a record: {other:?}"),
    }
}

fn text(v: &Value, key: &str) -> String {
    match field(v, key) {
        Value::Str(s) => s.clone(),
        other => panic!("{key} is not a String: {other:?}"),
    }
}

fn flag(v: &Value, key: &str) -> bool {
    match field(v, key) {
        Value::Bool(b) => *b,
        other => panic!("{key} is not a Bool: {other:?}"),
    }
}

/// Runs Windows PowerShell with `script`, with each pair of `vars` set as an
/// environment variable. A PSModulePath inherited from PowerShell 7 is removed,
/// because it keeps Windows PowerShell from loading the storage module.
fn run_powershell(script: &str, vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    cmd.env_remove("PSModulePath");
    for (name, value) in vars {
        cmd.env(name, value);
    }
    cmd.output().expect("powershell.exe should start")
}

/// The oracle: PowerShell's output for `script`, which must succeed.
fn powershell(script: &str, vars: &[(&str, &str)]) -> String {
    let out = run_powershell(script, vars);
    assert!(
        out.status.success(),
        "PowerShell oracle failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The number of partitions that list `folder` as an access path.
fn access_paths(folder: &str) -> usize {
    powershell(ACCESS_PATH_COUNT, &[("AE_DST", folder)])
        .parse()
        .expect("the access path count is a number")
}

/// Whether the image is attached.
fn attached(image: &str) -> bool {
    powershell(IMAGE_ATTACHED, &[("AE_SRC", image)]) == "True"
}

/// A path as `canonicalize` gives it, without the `\\?\` prefix. The temp
/// directory can be a short (8.3) name, and the builtins report long names.
fn canon(path: &Path) -> PathBuf {
    let text = std::fs::canonicalize(path)
        .expect("the path should exist")
        .to_string_lossy()
        .into_owned();
    PathBuf::from(text.trim_start_matches(r"\\?\"))
}

/// A temp directory holding a formatted 16 MB VHDX (one NTFS partition, left
/// detached) and an empty folder for the mount. Dropping it removes the mount,
/// detaches the image and deletes the directory, all best effort.
struct Fixture {
    dir: PathBuf,
    image: String,
    folder: String,
}

impl Fixture {
    fn new() -> Self {
        let raw = std::env::temp_dir().join(format!("aemnt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&raw).unwrap();
        let dir = canon(&raw);
        let image = dir.join("img.vhdx");
        let folder = dir.join("mnt");
        std::fs::create_dir(&folder).unwrap();

        // The script is written as the diskpart text, one command per line.
        let script = dir.join("make.txt");
        let commands = format!(
            "create vdisk file=\"{image}\" maximum=16 type=expandable\n\
             select vdisk file=\"{image}\"\n\
             attach vdisk\n\
             create partition primary\n\
             format fs=ntfs quick label=AECI\n\
             detach vdisk\n",
            image = image.display()
        );
        std::fs::write(&script, commands).unwrap();
        let out = Command::new("diskpart")
            .arg("/s")
            .arg(&script)
            .output()
            .expect("diskpart should start");
        assert!(
            out.status.success(),
            "diskpart could not make the image: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        Self {
            image: image.to_string_lossy().into_owned(),
            folder: folder.to_string_lossy().into_owned(),
            dir,
        }
    }

    /// Nothing is mounted at the folder, and the image is detached.
    fn assert_unchanged(&self) {
        assert_eq!(access_paths(&self.folder), 0, "a mount was left behind");
        assert!(!attached(&self.image), "the image was left attached");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = run_powershell(
            CLEANUP,
            &[("AE_DST", &self.folder), ("AE_SRC", &self.image)],
        );
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
#[ignore]
fn aemnt_mount_round_trip_matches_powershell() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fx = Fixture::new();

    // Read-write mount: the record matches the system, and the folder is a mount.
    let made = call_ok("fs_mount", vec![s(&fx.image), s(&fx.folder)]);
    assert_eq!(text(&made, "source"), fx.image);
    assert_eq!(text(&made, "target"), fx.folder);
    assert_eq!(text(&made, "fstype"), "NTFS");
    assert!(!flag(&made, "readonly"));
    assert_eq!(access_paths(&fx.folder), 1);
    assert!(attached(&fx.image));

    // A file written through the folder is on the image's volume.
    let file = Path::new(&fx.folder).join("hello.txt");
    std::fs::write(&file, "round trip").unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "round trip");

    let gone = call_ok("fs_unmount", vec![s(&fx.folder)]);
    assert!(flag(&gone, "unmounted"));
    assert_eq!(text(&gone, "target"), fx.folder);
    assert_eq!(access_paths(&fx.folder), 0);
    assert!(!attached(&fx.image));
    assert_eq!(std::fs::read_dir(&fx.folder).unwrap().count(), 0);

    // Read-only mount: the record says so, and writes through the folder fail.
    let ro = call_ok(
        "fs_mount",
        vec![
            s(&fx.image),
            s(&fx.folder),
            rec(&[("readonly", Value::Bool(true))]),
        ],
    );
    assert!(flag(&ro, "readonly"));
    assert_eq!(access_paths(&fx.folder), 1);
    assert!(std::fs::write(Path::new(&fx.folder).join("denied.txt"), "x").is_err());
    call_ok("fs_unmount", vec![s(&fx.folder)]);
    fx.assert_unchanged();
}

#[test]
#[ignore]
fn aemnt_refusals_change_nothing() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fx = Fixture::new();

    // A target that is not empty.
    let occupied = Path::new(&fx.folder).join("occupied.txt");
    std::fs::write(&occupied, "x").unwrap();
    expect_code(
        call("fs_mount", vec![s(&fx.image), s(&fx.folder)]),
        "E_BAD_ARG",
    );
    fx.assert_unchanged();
    std::fs::remove_file(&occupied).unwrap();

    // bind: there are no bind mounts on Windows.
    expect_code(
        call(
            "fs_mount",
            vec![
                s(&fx.image),
                s(&fx.folder),
                rec(&[("bind", Value::Bool(true))]),
            ],
        ),
        "E_BAD_ARG",
    );
    fx.assert_unchanged();

    // fstype: the file system is read from the image.
    expect_code(
        call(
            "fs_mount",
            vec![s(&fx.image), s(&fx.folder), rec(&[("fstype", s("ntfs"))])],
        ),
        "E_BAD_ARG",
    );
    fx.assert_unchanged();

    // A source that is not a VHD or VHDX file.
    let note = Path::new(&fx.dir).join("note.txt");
    std::fs::write(&note, "not an image").unwrap();
    expect_code(
        call("fs_mount", vec![s(&note.to_string_lossy()), s(&fx.folder)]),
        "E_BAD_ARG",
    );
    fx.assert_unchanged();

    // A protected target, refused before the source is looked at.
    let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    expect_code(
        call("fs_mount", vec![s(&fx.image), s(&system)]),
        "E_POLICY_DENY",
    );
    fx.assert_unchanged();

    // Unmounting a folder that is not a mount point.
    expect_code(call("fs_unmount", vec![s(&fx.folder)]), "E_NOT_FOUND");
    fx.assert_unchanged();

    // lazy has no meaning on Windows; the mount is still there afterwards.
    call_ok("fs_mount", vec![s(&fx.image), s(&fx.folder)]);
    assert_eq!(access_paths(&fx.folder), 1);
    expect_code(
        call(
            "fs_unmount",
            vec![s(&fx.folder), rec(&[("lazy", Value::Bool(true))])],
        ),
        "E_BAD_ARG",
    );
    assert_eq!(
        access_paths(&fx.folder),
        1,
        "the refused unmount changed the mount"
    );
    call_ok("fs_unmount", vec![s(&fx.folder)]);
    fx.assert_unchanged();
}
