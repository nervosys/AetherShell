//! The audited party may not edit the files that govern it.
//!
//! `main` loads the RBAC config at startup (`IRONSHELL_RBAC_CONFIG`, else
//! `<workspace>/.ironshell/rbac.toml`), and the audit chain key may come from
//! `IRONSHELL_AUDIT_KEY_FILE`. The Python SDK starts a process per call, so a
//! jailed write to either would govern the next call. The guard protected
//! `.ironshell` only while the audit log lived in it; with the log elsewhere, an
//! agent could write itself a new role table. `safety::is_governance_file`
//! now refuses those writes, aliases included.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
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
    /// A jailed workspace whose audit log lives outside it, so `.ironshell` is not
    /// covered by the audit check and only the governance check can refuse.
    fn new() -> Self {
        let id = uuid::Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("ae-governance-ws-{id}"));
        let outside = std::env::temp_dir().join(format!("ae-governance-out-{id}"));
        std::fs::create_dir_all(root.join(".ironshell")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let root = root.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();
        for var in VARS {
            std::env::remove_var(var);
        }
        std::env::set_var("IRONSHELL_MODE", "agent");
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
    assert!(
        text.contains("access control or audit key"),
        "{shown}: {text}"
    );
}

#[test]
fn the_default_rbac_config_and_its_directory_cannot_be_written() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    let config = ws.path(".ironshell/rbac.toml");

    // It does not exist yet: creating it is the attack.
    assert_refused("file_write", vec![s(&config), s("[roles]\n")]);
    assert!(!Path::new(&config).exists());

    std::fs::write(&config, "seed\n").unwrap();
    assert_refused("file_append", vec![s(&config), s("more\n")]);
    assert_refused("rm", vec![s(&config)]);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "seed\n");

    // The directory too, so it cannot be moved aside and recreated.
    assert_refused(
        "file_write",
        vec![s(&ws.path(".ironshell/other.toml")), s("x")],
    );
    assert_refused(
        "file_move",
        vec![s(&ws.path(".ironshell")), s(&ws.path("moved"))],
    );
    assert!(Path::new(&config).exists());

    // The rest of the workspace is still writable.
    assert!(call("file_write", vec![s(&ws.path("ordinary.txt")), s("ok")]).is_ok());
}

#[test]
fn an_explicit_rbac_config_is_protected_and_only_it() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    std::fs::create_dir_all(ws.root.join("policy")).unwrap();
    let config = ws.path("policy/rbac.toml");
    std::env::set_var("IRONSHELL_RBAC_CONFIG", &config);

    assert_refused("file_write", vec![s(&config), s("[roles]\n")]);
    assert!(!Path::new(&config).exists());

    // With the config named elsewhere and the log outside, `.ironshell` is ordinary.
    assert!(call(
        "file_write",
        vec![s(&ws.path(".ironshell/notes.txt")), s("ok")]
    )
    .is_ok());
    assert!(call("file_write", vec![s(&ws.path("policy/other.txt")), s("ok")]).is_ok());
}

#[test]
fn the_audit_key_file_cannot_be_written() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    let key = ws.path("audit.key");
    std::fs::write(&key, "11".repeat(32)).unwrap();
    std::env::set_var("IRONSHELL_AUDIT_KEY_FILE", &key);

    assert_refused("file_write", vec![s(&key), s(&"22".repeat(32))]);
    assert_refused("file_append", vec![s(&key), s("00")]);
    assert_eq!(std::fs::read_to_string(&key).unwrap(), "11".repeat(32));
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_rbac_config_is_refused_too() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ws = Ws::new();
    let config = ws.root.join(".ironshell").join("rbac.toml");
    std::fs::write(&config, "seed\n").unwrap();
    let alias = ws.root.join("innocent.toml");
    std::os::unix::fs::symlink(&config, &alias).unwrap();

    assert_refused(
        "file_write",
        vec![s(&alias.to_string_lossy()), s("[roles]\n")],
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "seed\n");
}
