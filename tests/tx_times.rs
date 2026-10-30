//! A rollback puts back a file's modification time as well as its mode. `touch`
//! changes only the time, and `fs_chmod` changes the mode of a file whose time
//! must also come back. Like `tx_coverage`, these run in their own process:
//! a transaction is process-wide state.

use iron_shell::value::Value;
use std::time::{Duration, UNIX_EPOCH};

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

/// Transactions are process-wide state, so these tests take one at a time.
static TX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Ends whatever transaction a failing assertion left open.
struct TxCleanup;

impl Drop for TxCleanup {
    fn drop(&mut self) {
        while iron_shell::tx::rollback().is_ok() {}
    }
}

/// The workspace these tests run in; the journal lives under `<workspace>/.ironshell/tx`.
fn workspace() -> std::path::PathBuf {
    let w = std::env::temp_dir().join(format!("ae-txt-ws-{}", std::process::id()));
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("IRONSHELL_WORKSPACE", &w);
    w
}

fn path_arg(p: &std::path::Path) -> Value {
    Value::Str(p.to_string_lossy().into_owned())
}

/// A fixed, old time, well away from both the epoch and "now".
const OLD_SECS: u64 = 1_000_000_000;

fn set_old_mtime(p: &std::path::Path) {
    let old = UNIX_EPOCH + Duration::from_secs(OLD_SECS);
    std::fs::OpenOptions::new()
        .write(true)
        .open(p)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
}

/// Modified time in whole seconds since the epoch, so filesystem precision
/// does not matter.
fn mtime_secs(p: &std::path::Path) -> u64 {
    std::fs::metadata(p)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn touch_is_undone_by_rollback() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let p = workspace().join("touch_times.txt");
    std::fs::write(&p, "keep\n").unwrap();
    set_old_mtime(&p);
    assert_eq!(mtime_secs(&p), OLD_SECS);

    iron_shell::tx::begin().unwrap();
    call("touch", vec![path_arg(&p)]).unwrap();
    assert_ne!(mtime_secs(&p), OLD_SECS, "touch changed the time");
    iron_shell::tx::rollback().unwrap();
    assert_eq!(mtime_secs(&p), OLD_SECS, "rollback restored the time");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "keep\n");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn chmod_read_only_is_undone_with_its_time() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let p = workspace().join("chmod_times.txt");
    std::fs::write(&p, "keep\n").unwrap();
    set_old_mtime(&p);

    iron_shell::tx::begin().unwrap();
    call("fs_chmod", vec![path_arg(&p), Value::Int(0o444)]).unwrap();
    assert!(
        std::fs::metadata(&p).unwrap().permissions().readonly(),
        "chmod made the file read-only"
    );
    iron_shell::tx::rollback().unwrap();
    let md = std::fs::metadata(&p).unwrap();
    assert!(!md.permissions().readonly(), "rollback restored the mode");
    assert_eq!(mtime_secs(&p), OLD_SECS, "rollback restored the time");
    let _ = std::fs::remove_file(&p);
}

/// Opens a directory so its times can be set: read-only on Unix, with backup
/// semantics and attribute access on Windows.
fn open_dir_for_times(p: &std::path::Path) -> std::fs::File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .access_mode(0x100)
            .custom_flags(0x0200_0000)
            .open(p)
            .unwrap()
    }
    #[cfg(not(windows))]
    std::fs::File::open(p).unwrap()
}

#[test]
fn touch_of_a_directory_changes_its_time_and_rollback_restores_it() {
    let _lock = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cleanup = TxCleanup;
    let d = workspace().join("touch_dir");
    std::fs::create_dir_all(&d).unwrap();
    let old = UNIX_EPOCH + Duration::from_secs(OLD_SECS);
    open_dir_for_times(&d)
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    assert_eq!(mtime_secs(&d), OLD_SECS);

    iron_shell::tx::begin().unwrap();
    call("touch", vec![path_arg(&d)]).expect("touch of a directory");
    assert_ne!(
        mtime_secs(&d),
        OLD_SECS,
        "touch changed the directory's time"
    );
    iron_shell::tx::rollback().unwrap();
    assert_eq!(
        mtime_secs(&d),
        OLD_SECS,
        "rollback restored the directory's time"
    );
    let _ = std::fs::remove_dir_all(&d);
}
