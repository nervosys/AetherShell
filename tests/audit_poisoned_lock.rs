//! A required audit entry that cannot be written is an error, including when the
//! reason is a poisoned lock: that path returned success with no record made.
//!
//! One test in its own binary: poisoning the audit lock lasts for the process.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::safety;
use iron_shell::value::Value;

fn audit_log() -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(
        "audit_log",
        vec![Value::Str("probe".into()), Value::Str("resource".into())],
        &mut env,
    )
}

#[test]
fn a_poisoned_audit_lock_fails_a_required_entry_and_only_a_required_one() {
    let dir = std::env::temp_dir().join(format!("ish_audit_poison_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let log = dir.join("audit.log");
    std::env::set_var("IRONSHELL_AUDIT_LOG", &log);
    std::env::set_var("IRONSHELL_AUDIT_REQUIRED", "1");

    safety::__test_poison_audit_lock();

    let err = audit_log().expect_err("a required entry was not written");
    assert_eq!(safety::code_of(&err).as_str(), "E_IO", "{err:#}");
    assert!(format!("{err:#}").contains("panicked"), "{err:#}");
    assert!(
        !log.exists(),
        "nothing may be written behind a poisoned lock"
    );

    // Best-effort auditing stays best-effort.
    std::env::remove_var("IRONSHELL_AUDIT_REQUIRED");
    let logged = audit_log().expect("an optional entry does not block the call");
    match logged {
        Value::Record(r) => assert_eq!(r.get("status"), Some(&Value::Str("logged".into()))),
        other => panic!("expected a record, got {other:?}"),
    }
    assert!(!log.exists());

    std::env::remove_var("IRONSHELL_AUDIT_LOG");
    let _ = std::fs::remove_dir_all(&dir);
}
