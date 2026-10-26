#![cfg(windows)]
//! A hard link is the same file under another name, and NTFS makes one
//! without any privilege. The audit log and the RBAC config must be refused
//! through a hard link, as `audit_alias_contracts.rs` and
//! `governance_file_contracts.rs` require for symbolic links on Unix.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

const VARS: &[&str] = &[
    "AETHER_MODE",
    "AETHER_WORKSPACE",
    "AETHER_AUDIT_LOG",
    "AETHER_RBAC_CONFIG",
    "AETHER_AUDIT_KEY_FILE",
    "AETHER_AUDIT_SINK",
    "AETHER_SESSION_DIR",
];

struct Ws {
    root: PathBuf,
    outside: PathBuf,
}

impl Ws {
    /// A jailed workspace. Its audit log starts outside the workspace; a test
    /// that wants the log inside it moves `AETHER_AUDIT_LOG` itself.
    fn new() -> Self {
        let id = uuid::Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("ae-hardlink-ws-{id}"));
        let outside = std::env::temp_dir().join(format!("ae-hardlink-out-{id}"));
        std::fs::create_dir_all(root.join(".ae")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let root = root.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();
        for var in VARS {
            std::env::remove_var(var);
        }
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", &root);
        std::env::set_var("AETHER_AUDIT_LOG", outside.join("audit.log"));
        std::env::set_var("AETHER_SESSION_DIR", outside.join("session"));
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

fn assert_refused(name: &str, args: Vec<Value>, reason: &str) {
    let shown = format!("{name} {args:?}");
    let e = call(name, args).expect_err(&format!("{shown} must be refused"));
    let text = e.to_string();
    assert!(text.contains("E_OUTSIDE_WORKSPACE"), "{shown}: {text}");
    assert!(text.contains(reason), "{shown}: {text}");
}

#[test]
fn a_hard_link_to_the_audit_log_cannot_be_written() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    std::fs::create_dir_all(ws.root.join("logs")).unwrap();
    let log = ws.path("logs/audit.log");
    std::env::set_var("AETHER_AUDIT_LOG", &log);
    std::fs::write(&log, "entry\n").unwrap();
    let alias = ws.path("innocent.txt");
    std::fs::hard_link(&log, &alias).unwrap();

    assert_refused(
        "file_write",
        vec![s(&alias), s("forged\n")],
        "audit log or its directory",
    );
    assert_refused(
        "file_append",
        vec![s(&alias), s("forged\n")],
        "audit log or its directory",
    );
    // The refusals themselves are recorded in the log, so only the forged
    // content is checked for absence and the original entry for presence.
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.starts_with("entry\n"), "{text}");
    assert!(!text.contains("forged"), "{text}");
}

#[test]
fn a_hard_link_to_the_rbac_config_cannot_be_written() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    let config = ws.path(".ae/rbac.toml");
    std::fs::write(&config, "seed\n").unwrap();
    let alias = ws.path("innocent.txt");
    std::fs::hard_link(&config, &alias).unwrap();

    assert_refused(
        "file_write",
        vec![s(&alias), s("[roles]\n")],
        "access control or audit key",
    );
    assert_refused(
        "file_append",
        vec![s(&alias), s("more\n")],
        "access control or audit key",
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "seed\n");
}

#[test]
fn an_ordinary_file_in_the_workspace_is_still_writable() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    let ordinary = ws.path("ordinary.txt");
    assert!(call("file_write", vec![s(&ordinary), s("ok")]).is_ok());
    assert_eq!(std::fs::read_to_string(&ordinary).unwrap(), "ok");
    assert!(Path::new(&ordinary).exists());
}
