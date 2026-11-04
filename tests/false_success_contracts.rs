//! No builtin may report success it did not achieve.
//!
//! Covered here: `syntax_add` over a `~/.ironshell/syntax_kb.json` that does not
//! load, `audit_log` when a required audit write fails, and
//! `a2ui_agent_completed` given a non-Bool success flag. `rbac_logout` is not
//! covered: it needs a live auth backend with a login session, which the
//! in-process harness here does not set up.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::safety;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn code(e: &anyhow::Error) -> String {
    safety::code_of(e).as_str().to_string()
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_false_success_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Points the audit log at `log` with auditing required, and clears both
/// variables again when dropped, even if the test panics.
struct AuditEnv;

impl AuditEnv {
    fn set(log: &std::path::Path) -> Self {
        std::env::set_var("IRONSHELL_AUDIT_LOG", log);
        std::env::set_var("IRONSHELL_AUDIT_REQUIRED", "1");
        AuditEnv
    }
}

impl Drop for AuditEnv {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_AUDIT_LOG");
        std::env::remove_var("IRONSHELL_AUDIT_REQUIRED");
    }
}

#[test]
fn syntax_add_refuses_an_unloadable_kb_file_and_leaves_it_alone() {
    let _g = lock();
    // The knowledge base is loaded once per process, from HOME (then
    // USERPROFILE). This binary touches it only here, so the redirect lands
    // before the first load on every platform.
    let home = scratch("syntax_home");
    let dir = home.join(".ironshell");
    std::fs::create_dir_all(&dir).expect("kb dir");
    let file = dir.join("syntax_kb.json");
    let corrupt = b"{ this is not json".to_vec();
    std::fs::write(&file, &corrupt).expect("corrupt kb");
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);

    let mut entry = BTreeMap::new();
    entry.insert("id".to_string(), Value::Str("probe_entry".into()));
    entry.insert("name".to_string(), Value::Str("Probe".into()));
    entry.insert("category".to_string(), Value::Str("protocol".into()));
    entry.insert("specification".to_string(), Value::Str("a probe".into()));

    let err = call("syntax_add", vec![Value::Record(entry)])
        .expect_err("an entry that was not saved must not be reported as added");
    assert_eq!(code(&err), "E_BAD_STATE", "{err:#}");
    let text = format!("{err:#}");
    assert!(
        text.contains("syntax_kb.json"),
        "the error must name the file: {text}"
    );
    assert!(
        text.contains("not saved"),
        "the error must say the entry was not saved: {text}"
    );

    assert_eq!(
        std::fs::read(&file).expect("kb still there"),
        corrupt,
        "the unreadable file must be left exactly as it was"
    );

    // Queries still answer from the in-memory knowledge base.
    let ab = call("syntax_get", vec![Value::Str("ab".into())]);
    assert!(ab.is_ok(), "syntax_get must still work: {ab:?}");
}

#[test]
fn audit_log_is_an_error_when_the_required_write_cannot_be_made() {
    let _g = lock();
    let dir = scratch("audit_required");
    // A regular file where the log's directory should be: the append cannot succeed.
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, "a file, so nothing can sit beneath it").expect("blocker");
    let log = blocker.join("audit.log");
    let _env = AuditEnv::set(&log);

    let err = call(
        "audit_log",
        vec![Value::Str("probe".into()), Value::Str("resource".into())],
    )
    .expect_err("a required audit write that failed must not be reported as logged");
    assert_eq!(code(&err), "E_IO", "{err:#}");
    assert!(
        format!("{err:#}").contains("audit"),
        "the error must say what was not recorded: {err:#}"
    );
}

#[test]
fn a2ui_agent_completed_refuses_a_success_flag_that_is_not_a_bool() {
    let _g = lock();
    for flag in [Value::Str("false".into()), Value::Int(0)] {
        let shown = format!("{flag:?}");
        let err = call("a2ui_agent_completed", vec![Value::Str("x".into()), flag])
            .expect_err("a non-Bool success flag must be refused, not read as success");
        assert_eq!(code(&err), "E_BAD_ARG", "flag {shown}: {err:#}");
    }
}
