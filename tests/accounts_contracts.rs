//! Contracts for the account and group builtins (`user_*`, `group_*`).
//!
//! Ordinary tests run everywhere: every bad argument is refused with a coded
//! E_BAD_ARG before any effect, pipeline input and an active transaction are
//! refused, and off Linux and Windows every call is E_UNIMPLEMENTED with nothing
//! changed.
//!
//! The system-changing tests are `#[ignore]`d and run only in the disposable
//! distro, via scripts/disposable_linux_tests.sh. They create and delete
//! accounts named with this process id, verify every step with getent,
//! passwd -S and /etc/shadow rather than with the builtins' own answers, and
//! remove what they created in a Drop guard. The refusal test points the
//! builtins at root and daemon and checks that nothing about them changed.

#[path = "support/disposable.rs"]
mod disposable;

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Every test takes this lock: the transaction tests hold the process-wide
/// transaction open, and a concurrent call would be refused by it.
static SERIAL: Mutex<()> = Mutex::new(());

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

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call_with_input(name, args, Some(input), &mut env)
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

/// One well-formed call per builtin. The ordinary tests use these only where
/// the builtin must refuse before it reaches a tool.
fn valid_calls() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("user_add", vec![s("aeacct_x")]),
        ("user_del", vec![s("aeacct_x")]),
        ("user_mod", vec![s("aeacct_x"), rec(&[("comment", s("x"))])]),
        ("user_passwd", vec![s("aeacct_x"), s("pw")]),
        ("user_lock", vec![s("aeacct_x")]),
        ("user_unlock", vec![s("aeacct_x")]),
        ("group_add", vec![s("aeacct_x")]),
        ("group_del", vec![s("aeacct_x")]),
        (
            "group_mod",
            vec![s("aeacct_x"), rec(&[("gid", Value::Int(5000))])],
        ),
    ]
}

#[test]
fn aeacct_invalid_arguments_are_refused_before_any_effect() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let long_password = s(&"x".repeat(1025));
    let cases: Vec<(&str, Vec<Value>)> = vec![
        // user_add
        ("user_add", vec![]),
        ("user_add", vec![s("Bad Name")]),
        ("user_add", vec![Value::Int(1)]),
        ("user_add", vec![s("ok"), s("not a record")]),
        ("user_add", vec![s("ok"), rec(&[("home", s("yes"))])]),
        ("user_add", vec![s("ok"), rec(&[("bogus", Value::Int(1))])]),
        ("user_add", vec![s("ok"), rec(&[("shell", s("relative"))])]),
        ("user_add", vec![s("ok"), rec(&[("comment", s("a:b"))])]),
        ("user_add", vec![s("ok"), rec(&[("comment", s("a\nb"))])]),
        (
            "user_add",
            vec![
                s("ok"),
                rec(&[("groups", Value::Array(vec![s("Bad Group")]))]),
            ],
        ),
        (
            "user_add",
            vec![
                s("ok"),
                rec(&[("groups", Value::Array(vec![s("a"), s("a")]))]),
            ],
        ),
        ("user_add", vec![s("ok"), rec(&[("groups", s("a"))])]),
        ("user_add", vec![s("ok"), rec(&[("system", Value::Int(1))])]),
        ("user_add", vec![s("ok"), rec(&[]), s("extra")]),
        // user_del
        ("user_del", vec![]),
        ("user_del", vec![s("Bad Name")]),
        (
            "user_del",
            vec![s("ok"), rec(&[("remove_home", Value::Int(1))])],
        ),
        (
            "user_del",
            vec![s("ok"), rec(&[("bogus", Value::Bool(true))])],
        ),
        // user_mod: options are required, and must name a change
        ("user_mod", vec![s("ok")]),
        ("user_mod", vec![s("ok"), rec(&[])]),
        (
            "user_mod",
            vec![s("ok"), rec(&[("move_home", Value::Bool(true))])],
        ),
        (
            "user_mod",
            vec![
                s("ok"),
                rec(&[
                    ("groups", Value::Array(vec![s("a")])),
                    ("add_groups", Value::Array(vec![s("b")])),
                ]),
            ],
        ),
        (
            "user_mod",
            vec![s("ok"), rec(&[("add_groups", Value::Array(vec![]))])],
        ),
        (
            "user_mod",
            vec![
                s("ok"),
                rec(&[("shell", s("/bin/sh")), ("bogus", Value::Int(1))]),
            ],
        ),
        ("user_mod", vec![s("ok"), rec(&[("home", s("relative"))])]),
        ("user_mod", vec![s("ok"), rec(&[("comment", s("x:y"))])]),
        (
            "user_mod",
            vec![s("Bad Name"), rec(&[("shell", s("/bin/sh"))])],
        ),
        // user_passwd
        ("user_passwd", vec![s("ok")]),
        ("user_passwd", vec![s("ok"), s("")]),
        ("user_passwd", vec![s("ok"), s("bad:pw")]),
        ("user_passwd", vec![s("ok"), s("a\nb")]),
        ("user_passwd", vec![s("ok"), Value::Int(5)]),
        ("user_passwd", vec![s("Bad Name"), s("pw")]),
        ("user_passwd", vec![s("ok"), long_password]),
        // user_lock / user_unlock
        ("user_lock", vec![]),
        ("user_lock", vec![s("Bad Name")]),
        ("user_lock", vec![s("ok"), s("extra")]),
        ("user_unlock", vec![]),
        ("user_unlock", vec![s("Bad Name")]),
        ("user_unlock", vec![s("ok"), s("extra")]),
        // group_add
        ("group_add", vec![]),
        ("group_add", vec![s("Bad Name")]),
        ("group_add", vec![s("ok"), rec(&[("gid", Value::Int(999))])]),
        (
            "group_add",
            vec![s("ok"), rec(&[("gid", Value::Int(60001))])],
        ),
        (
            "group_add",
            vec![
                s("ok"),
                rec(&[("gid", Value::Int(1000)), ("system", Value::Bool(true))]),
            ],
        ),
        ("group_add", vec![s("ok"), rec(&[("gid", s("1000"))])]),
        // group_del
        ("group_del", vec![]),
        ("group_del", vec![s("Bad Name")]),
        ("group_del", vec![s("ok"), s("extra")]),
        // group_mod
        ("group_mod", vec![s("ok")]),
        ("group_mod", vec![s("ok"), rec(&[])]),
        ("group_mod", vec![s("ok"), rec(&[("new_name", s("ok"))])]),
        ("group_mod", vec![s("ok"), rec(&[("gid", Value::Int(5))])]),
        (
            "group_mod",
            vec![s("ok"), rec(&[("new_name", s("Bad Name"))])],
        ),
        (
            "group_mod",
            vec![s("Bad Name"), rec(&[("gid", Value::Int(5000))])],
        ),
    ];
    for (name, args) in cases {
        match call(name, args) {
            Ok(v) => panic!("{name} answered {v:?} for a call it must refuse"),
            Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{name}: {e}"),
        }
    }
}

#[test]
fn aeacct_pipeline_input_is_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (name, args) in valid_calls() {
        expect_code(call_piped(name, args, Value::Int(1)), "E_BAD_ARG");
    }
}

/// An active transaction cannot roll back an account change, so every change
/// is refused inside one. The refusal comes before any tool runs.
#[test]
fn aeacct_transaction_refuses_each_change() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    struct Rollback;
    impl Drop for Rollback {
        fn drop(&mut self) {
            let _ = iron_shell::tx::rollback();
        }
    }
    iron_shell::tx::begin().expect("begin a transaction");
    let _rollback = Rollback;
    for (name, args) in valid_calls() {
        expect_code(call(name, args), "E_POLICY_DENY");
    }
}

/// macOS has a backend (dscl), and every change needs root: as an ordinary
/// user each valid call is refused before any tool runs, and no account or
/// group of that name appears. The changes themselves are verified as root in
/// tests/macos_account_admin.rs on a disposable runner.
#[cfg(target_os = "macos")]
#[test]
fn aeacct_macos_every_valid_call_needs_root_and_changes_nothing() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    if unsafe { libc::geteuid() } == 0 {
        return; // as root these calls would act; the disposable tests cover that
    }
    for (name, args) in valid_calls() {
        match call(name, args) {
            Ok(v) => panic!("{name} answered {v:?} without root"),
            Err(e) => assert_eq!(code_of(&e), "E_OS_DENIED", "{name}: {e}"),
        }
    }
    for kind in ["/Users/aeacct_x", "/Groups/aeacct_x"] {
        let read = std::process::Command::new("dscl")
            .args([".", "-read", kind])
            .output()
            .expect("run dscl");
        assert!(!read.status.success(), "{kind} exists after refused calls");
    }
}

/// Keeps the disposable guard compiled and exercised on every platform: it
/// must refuse outside the disposable distro.
#[test]
fn aeacct_guard_refuses_outside_the_disposable_distro() {
    if std::env::var("IRONSHELL_DISPOSABLE").as_deref() == Ok("1") {
        return; // inside the runner, the ignored tests below are the check
    }
    assert!(std::panic::catch_unwind(disposable::require).is_err());
}

#[cfg(target_os = "linux")]
mod on_disposable_linux {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn getent(database: &str, key: &str) -> Option<String> {
        let out = Command::new("getent").args([database, key]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(str::to_string)
    }

    fn field(line: &str, index: usize) -> String {
        line.split(':').nth(index).unwrap_or("").to_string()
    }

    fn shadow_hash(name: &str) -> String {
        std::fs::read_to_string("/etc/shadow")
            .expect("read /etc/shadow as root")
            .lines()
            .find(|line| line.split(':').next() == Some(name))
            .map(|line| field(line, 1))
            .unwrap_or_default()
    }

    /// The status letter `passwd -S` reports: P usable, L locked, NP none.
    fn passwd_status(name: &str) -> String {
        let out = Command::new("passwd")
            .args(["-S", name])
            .output()
            .expect("run passwd -S");
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string()
    }

    fn members(group: &str) -> Vec<String> {
        getent("group", group)
            .map(|line| field(&line, 3))
            .unwrap_or_default()
            .split(',')
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn free_gid() -> i64 {
        (50000..60000)
            .find(|gid| getent("group", &gid.to_string()).is_none())
            .expect("a free gid in 50000..60000")
    }

    fn call_ok(name: &str, args: Vec<Value>) -> Value {
        call(name, args).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
    }

    fn int(v: &Value, key: &str) -> i64 {
        match v {
            Value::Record(m) => match m.get(key) {
                Some(Value::Int(n)) => *n,
                other => panic!("{key} is not an Int: {other:?}"),
            },
            other => panic!("not a record: {other:?}"),
        }
    }

    fn text(v: &Value, key: &str) -> String {
        match v {
            Value::Record(m) => match m.get(key) {
                Some(Value::Str(s)) => s.clone(),
                other => panic!("{key} is not a String: {other:?}"),
            },
            other => panic!("not a record: {other:?}"),
        }
    }

    fn flag(v: &Value, key: &str) -> bool {
        match v {
            Value::Record(m) => match m.get(key) {
                Some(Value::Bool(b)) => *b,
                other => panic!("{key} is not a Bool: {other:?}"),
            },
            other => panic!("not a record: {other:?}"),
        }
    }

    fn changed(v: &Value) -> Vec<String> {
        match v {
            Value::Record(m) => match m.get("changed") {
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|i| match i {
                        Value::Str(s) => s.clone(),
                        other => panic!("changed entry is not a String: {other:?}"),
                    })
                    .collect(),
                other => panic!("changed is not an Array: {other:?}"),
            },
            other => panic!("not a record: {other:?}"),
        }
    }

    /// Deletes what the test created, even when an assertion failed first.
    #[derive(Default)]
    struct Cleanup {
        users: Vec<String>,
        groups: Vec<String>,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            for user in &self.users {
                let _ = Command::new("userdel").args(["-r", user]).output();
            }
            for group in &self.groups {
                let _ = Command::new("groupdel").arg(group).output();
            }
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeacct_user_lifecycle_verifies_each_step() {
        disposable::require();
        let pid = std::process::id();
        let user = format!("aeacct_u_{pid}");
        let group = format!("aeacct_g_{pid}");
        let mut cleanup = Cleanup::default();
        cleanup.users.push(user.clone());
        cleanup.groups.push(group.clone());

        // add
        let added = call_ok(
            "user_add",
            vec![s(&user), rec(&[("comment", s("aeacct initial"))])],
        );
        let uid = int(&added, "uid");
        assert!(uid >= 1000, "a new user has a uid >= 1000, got {uid}");
        let home = text(&added, "home");
        assert_eq!(home, format!("/home/{user}"));
        assert!(flag(&added, "created_home"));
        let entry = getent("passwd", &user).expect("getent sees the new user");
        assert_eq!(field(&entry, 2), uid.to_string());
        assert_eq!(field(&entry, 4), "aeacct initial");
        assert!(Path::new(&home).is_dir(), "the home directory exists");

        // mod: shell and comment
        let shell = if Path::new("/usr/sbin/nologin").is_file() {
            "/usr/sbin/nologin"
        } else {
            "/bin/false"
        };
        let modded = call_ok(
            "user_mod",
            vec![
                s(&user),
                rec(&[("shell", s(shell)), ("comment", s("aeacct changed"))]),
            ],
        );
        assert_eq!(changed(&modded), ["shell", "comment"]);
        let entry = getent("passwd", &user).unwrap();
        assert_eq!(field(&entry, 6), shell);
        assert_eq!(field(&entry, 4), "aeacct changed");

        // groups: append, then replace with none
        call_ok("group_add", vec![s(&group)]);
        let appended = call_ok(
            "user_mod",
            vec![
                s(&user),
                rec(&[("add_groups", Value::Array(vec![s(&group)]))]),
            ],
        );
        assert_eq!(changed(&appended), ["add_groups"]);
        assert!(members(&group).contains(&user), "appended to the group");
        call_ok(
            "user_mod",
            vec![s(&user), rec(&[("groups", Value::Array(vec![]))])],
        );
        assert!(
            !members(&group).contains(&user),
            "supplementary groups cleared"
        );

        // passwd: the hash changes, and the password is never stored in clear
        let before = shadow_hash(&user);
        let set = call_ok("user_passwd", vec![s(&user), s("aeacct-pw-1")]);
        assert!(flag(&set, "changed"));
        let after = shadow_hash(&user);
        assert!(
            after.starts_with('$'),
            "a crypt hash was written: {after:?}"
        );
        assert_ne!(after, before, "the shadow hash changed");
        assert!(!after.contains("aeacct-pw-1"));

        // lock and unlock, verified with passwd -S
        let locked = call_ok("user_lock", vec![s(&user)]);
        assert!(flag(&locked, "locked"));
        assert_eq!(passwd_status(&user), "L");
        assert!(shadow_hash(&user).starts_with('!'));
        let unlocked = call_ok("user_unlock", vec![s(&user)]);
        assert!(!flag(&unlocked, "locked"));
        assert_eq!(passwd_status(&user), "P");

        // del with the home directory
        let deleted = call_ok(
            "user_del",
            vec![s(&user), rec(&[("remove_home", Value::Bool(true))])],
        );
        assert!(flag(&deleted, "removed_home"));
        assert!(getent("passwd", &user).is_none(), "the account is gone");
        assert!(!Path::new(&home).exists(), "the home directory is gone");

        call_ok("group_del", vec![s(&group)]);
        assert!(getent("group", &group).is_none(), "the group is gone");
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeacct_group_lifecycle_verifies_each_step() {
        disposable::require();
        let pid = std::process::id();
        let first = format!("aeacct_g_{pid}");
        let second = format!("aeacct_h_{pid}");
        let mut cleanup = Cleanup::default();
        cleanup.groups.push(first.clone());
        cleanup.groups.push(second.clone());

        let added = call_ok("group_add", vec![s(&first)]);
        let gid = int(&added, "gid");
        assert!(gid >= 1000, "a new group has a gid >= 1000, got {gid}");
        let entry = getent("group", &first).expect("getent sees the new group");
        assert_eq!(field(&entry, 2), gid.to_string());

        let target = free_gid();
        let renamed = call_ok(
            "group_mod",
            vec![
                s(&first),
                rec(&[("new_name", s(&second)), ("gid", Value::Int(target))]),
            ],
        );
        assert_eq!(text(&renamed, "name"), second);
        assert_eq!(changed(&renamed), ["new_name", "gid"]);
        assert!(getent("group", &first).is_none(), "the old name is gone");
        let entry = getent("group", &second).expect("the renamed group exists");
        assert_eq!(field(&entry, 2), target.to_string());

        let deleted = call_ok("group_del", vec![s(&second)]);
        assert_eq!(text(&deleted, "name"), second);
        assert!(getent("group", &second).is_none(), "the group is gone");
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn aeacct_refusals_leave_system_accounts_unchanged() {
        disposable::require();
        let ghost = format!("aeacct_ghost_{}", std::process::id());
        let mut cleanup = Cleanup::default();
        cleanup.users.push(ghost.clone());

        let before = (
            getent("passwd", "root"),
            getent("passwd", "daemon"),
            getent("group", "root"),
            shadow_hash("root"),
            shadow_hash("daemon"),
        );

        expect_code(call("user_del", vec![s("root")]), "E_POLICY_DENY");
        expect_code(
            call("user_mod", vec![s("root"), rec(&[("shell", s("/bin/sh"))])]),
            "E_POLICY_DENY",
        );
        expect_code(
            call("user_passwd", vec![s("root"), s("aeacct-pw")]),
            "E_POLICY_DENY",
        );
        expect_code(call("user_lock", vec![s("root")]), "E_POLICY_DENY");
        expect_code(call("user_unlock", vec![s("root")]), "E_POLICY_DENY");
        expect_code(call("user_del", vec![s("daemon")]), "E_POLICY_DENY");
        expect_code(
            call("user_mod", vec![s("daemon"), rec(&[("comment", s("x"))])]),
            "E_POLICY_DENY",
        );
        expect_code(
            call("user_passwd", vec![s("daemon"), s("aeacct-pw")]),
            "E_POLICY_DENY",
        );
        expect_code(call("user_lock", vec![s("daemon")]), "E_POLICY_DENY");
        expect_code(call("group_del", vec![s("root")]), "E_POLICY_DENY");
        expect_code(
            call(
                "group_mod",
                vec![s("root"), rec(&[("gid", Value::Int(50000))])],
            ),
            "E_POLICY_DENY",
        );
        // gid 0 is a privileged group: a new user may not join it.
        expect_code(
            call(
                "user_add",
                vec![s(&ghost), rec(&[("groups", Value::Array(vec![s("root")]))])],
            ),
            "E_POLICY_DENY",
        );
        // An existing account is never re-created, and a missing one is named.
        expect_code(call("user_add", vec![s("root")]), "E_BAD_STATE");
        expect_code(call("user_del", vec![s(&ghost)]), "E_NOT_FOUND");
        expect_code(
            call(
                "group_mod",
                vec![s(&ghost), rec(&[("gid", Value::Int(50000))])],
            ),
            "E_NOT_FOUND",
        );

        let after = (
            getent("passwd", "root"),
            getent("passwd", "daemon"),
            getent("group", "root"),
            shadow_hash("root"),
            shadow_hash("daemon"),
        );
        assert_eq!(before, after, "refused calls changed a system account");
        assert!(
            getent("passwd", &ghost).is_none(),
            "no ghost account was made"
        );
    }
}
