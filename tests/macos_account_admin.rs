//! Contracts for the macOS account and group builtins (`user_*`, `group_*`).
//!
//! Compiled on macOS only. The ordinary tests change nothing: each call is
//! refused before any write (a bad name, an option with no macOS meaning, a
//! reserved name, or a missing root). The system-changing tests are `#[ignore]`d,
//! and they also return at once unless AE_DISPOSABLE_HOST=1, which only the
//! disposable macOS runner sets. They run as root (sudo), create accounts and
//! groups named aeci<pid>, check each step with dscl, id and dseditgroup rather
//! than with the builtins, and remove what they made in a Drop guard.

#![cfg(target_os = "macos")]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

/// Every test takes this lock, so account changes never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

/// True only on the disposable macOS runner, which sets AE_DISPOSABLE_HOST=1.
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
    aethershell::builtins::call(name, args, &mut env)
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

fn changed(v: &Value) -> Vec<String> {
    match field(v, "changed") {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Str(s) => s.clone(),
                other => panic!("changed entry is not a String: {other:?}"),
            })
            .collect(),
        other => panic!("changed is not an Array: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Oracles: dscl, id, dseditgroup and pwpolicy, never the builtins
// ---------------------------------------------------------------------------

fn output(program: &str, args: &[&str]) -> std::process::Output {
    Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {program}: {e}"))
}

fn succeeds(program: &str, args: &[&str]) -> bool {
    output(program, args).status.success()
}

fn stdout_of(program: &str, args: &[&str]) -> String {
    String::from_utf8_lossy(&output(program, args).stdout).into_owned()
}

/// True when the directory lists a record with exactly this name.
fn listed(node: &str, name: &str) -> bool {
    stdout_of("dscl", &[".", "-list", node])
        .lines()
        .any(|line| line.trim() == name)
}

fn user_exists(name: &str) -> bool {
    listed("/Users", name)
}

fn group_exists(name: &str) -> bool {
    listed("/Groups", name)
}

/// The account's groups as `id -Gn` reports them, read by the system itself.
fn groups_of(user: &str) -> Vec<String> {
    stdout_of("id", &["-Gn", user])
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// True when AuthenticationAuthority carries the DisabledUser marker that
/// pwpolicy -disableuser writes.
fn user_disabled(name: &str) -> bool {
    stdout_of(
        "dscl",
        &[
            ".",
            "-read",
            &format!("/Users/{name}"),
            "AuthenticationAuthority",
        ],
    )
    .contains("DisabledUser")
}

/// True when the directory checks this password for this user.
fn auth_ok(user: &str, password: &str) -> bool {
    succeeds("dscl", &[".", "-authonly", user, password])
}

/// The lowest gid from 1000 that no group record uses.
fn free_gid() -> i64 {
    let used: Vec<i64> = stdout_of("dscl", &[".", "-list", "/Groups", "PrimaryGroupID"])
        .lines()
        .filter_map(|line| {
            let (_, value) = line.trim_end().rsplit_once(char::is_whitespace)?;
            value.parse::<i64>().ok()
        })
        .collect();
    (1000..60000)
        .find(|gid| !used.contains(gid))
        .expect("a free gid")
}

fn is_root() -> bool {
    stdout_of("id", &["-u"]).trim() == "0"
}

/// A random password with upper and lower case, digits and a symbol. Never
/// printed.
fn strong_password() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::from("Ae");
    for _ in 0..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out.push('!');
    out
}

/// Deletes what a test created, even when an assertion failed first.
#[derive(Default)]
struct Cleanup {
    users: Vec<String>,
    groups: Vec<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for user in &self.users {
            let _ = Command::new("dscl")
                .args([".", "-delete", &format!("/Users/{user}")])
                .output();
            let _ = std::fs::remove_dir_all(format!("/Users/{user}"));
        }
        for group in &self.groups {
            let _ = Command::new("dscl")
                .args([".", "-delete", &format!("/Groups/{group}")])
                .output();
        }
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: each call is refused before any write
// ---------------------------------------------------------------------------

#[test]
fn aeci_bad_names_and_types_are_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    expect_code(call("user_add", vec![s("Bad Name")]), "E_BAD_ARG");
    expect_code(
        call(
            "user_add",
            vec![s("aeci_probe"), rec(&[("comment", Value::Int(5))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(call("user_del", vec![s("Bad Name")]), "E_BAD_ARG");
    expect_code(
        call("user_passwd", vec![s("Bad Name"), s("pw")]),
        "E_BAD_ARG",
    );
    expect_code(call("group_add", vec![s("Bad Name")]), "E_BAD_ARG");
    expect_code(
        call(
            "group_mod",
            vec![s("Bad Name"), rec(&[("new_name", s("aeci_x"))])],
        ),
        "E_BAD_ARG",
    );
}

/// Options with no macOS meaning are refused by name, before any change.
#[test]
fn aeci_options_without_a_macos_meaning_are_refused() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    expect_code(
        call(
            "user_add",
            vec![s("aeci_probe"), rec(&[("home", Value::Bool(false))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_add",
            vec![s("aeci_probe"), rec(&[("system", Value::Bool(true))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_mod",
            vec![
                s("aeci_probe"),
                rec(&[
                    ("home", s("/Users/aeci_probe")),
                    ("move_home", Value::Bool(true)),
                ]),
            ],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_mod",
            vec![
                s("aeci_probe"),
                rec(&[("groups", Value::Array(vec![s("staff")]))]),
            ],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_mod",
            vec![s("aeci_probe"), rec(&[("groups", Value::Array(vec![]))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "group_add",
            vec![s("aeci_probe"), rec(&[("system", Value::Bool(true))])],
        ),
        "E_BAD_ARG",
    );
}

/// Reserved names are refused before the root check and before any write.
#[test]
fn aeci_reserved_names_are_refused_before_any_write() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    expect_code(call("user_add", vec![s("_aeci_probe")]), "E_POLICY_DENY");
    expect_code(call("group_add", vec![s("_aeci_probe")]), "E_POLICY_DENY");
    assert!(!user_exists("_aeci_probe"));
    assert!(!group_exists("_aeci_probe"));
}

/// Without root every change is refused as E_OS_DENIED before it starts. Run
/// as root, this test has nothing to check and returns.
#[test]
fn aeci_without_root_is_refused_before_any_change() {
    if is_root() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    expect_code(call("user_add", vec![s("aeci_probe")]), "E_OS_DENIED");
    expect_code(call("user_del", vec![s("aeci_probe")]), "E_OS_DENIED");
    expect_code(call("group_add", vec![s("aeci_probe")]), "E_OS_DENIED");
    assert!(!user_exists("aeci_probe"));
}

// ---------------------------------------------------------------------------
// System-changing tests: disposable macOS runner only, as root
// ---------------------------------------------------------------------------

/// Built-in and system records, and the account sudo ran for, are refused
/// before any change. Disposable only, although it should change nothing: if
/// the guard were ever broken these calls would delete, lock or re-password
/// real system accounts.
#[test]
#[ignore = "aims at real system accounts: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_system_accounts_and_groups_are_refused_before_any_change() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    expect_code(call("user_del", vec![s("root")]), "E_POLICY_DENY");
    expect_code(call("user_lock", vec![s("root")]), "E_POLICY_DENY");
    expect_code(call("user_del", vec![s("_www")]), "E_POLICY_DENY");
    expect_code(call("user_lock", vec![s("_www")]), "E_POLICY_DENY");
    expect_code(
        call("user_passwd", vec![s("daemon"), s("Aeci-Probe-1")]),
        "E_POLICY_DENY",
    );
    expect_code(call("group_del", vec![s("admin")]), "E_POLICY_DENY");
    expect_code(
        call(
            "group_mod",
            vec![s("staff"), rec(&[("new_name", s("aeci_probe_g"))])],
        ),
        "E_POLICY_DENY",
    );
    // The account sudo ran for is refused too, whatever its uid.
    let me = std::env::var("SUDO_USER").unwrap_or_default();
    assert!(!me.is_empty(), "SUDO_USER is not set: run through sudo");
    expect_code(call("user_lock", vec![s(&me)]), "E_POLICY_DENY");
}

/// Disposable only: it reaches the real account records.
#[test]
#[ignore = "calls dscl on real records: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_missing_names_are_not_found() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let missing = format!("aeci_none{}", std::process::id());
    expect_code(call("user_del", vec![s(&missing)]), "E_NOT_FOUND");
    expect_code(call("user_lock", vec![s(&missing)]), "E_NOT_FOUND");
    expect_code(call("group_del", vec![s(&missing)]), "E_NOT_FOUND");
}

#[test]
#[ignore = "changes accounts: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_user_lifecycle_verifies_each_step() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let pid = std::process::id();
    let user = format!("aeci{pid}");
    let group = format!("aeci{pid}g");
    let mut cleanup = Cleanup::default();
    cleanup.users.push(user.clone());
    cleanup.groups.push(group.clone());

    // create: a record with a uid of at least 501, disabled, with no password
    let added = call_ok(
        "user_add",
        vec![s(&user), rec(&[("comment", s("aeci initial"))])],
    );
    assert!(user_exists(&user), "dscl lists the account");
    assert!(
        succeeds("id", &["-u", user.as_str()]),
        "id finds the account"
    );
    assert!(user_disabled(&user), "a new account starts disabled");
    assert!(
        !auth_ok(&user, "anything-at-all"),
        "a new account cannot log on"
    );
    assert_eq!(text(&added, "name"), user);
    let uid = match field(&added, "uid") {
        Value::Int(n) => *n,
        other => panic!("uid is not an Int: {other:?}"),
    };
    assert!(uid >= 501, "a person's uid, got {uid}");
    assert_eq!(
        stdout_of(
            "dscl",
            &[".", "-read", &format!("/Users/{user}"), "UniqueID"]
        )
        .trim(),
        format!("UniqueID: {uid}")
    );
    expect_code(call("user_add", vec![s(&user)]), "E_BAD_STATE");

    // comment and shell
    let modded = call_ok(
        "user_mod",
        vec![
            s(&user),
            rec(&[("comment", s("aeci changed")), ("shell", s("/bin/sh"))]),
        ],
    );
    assert_eq!(changed(&modded), ["shell", "comment"]);
    assert!(
        stdout_of(
            "dscl",
            &[".", "-read", &format!("/Users/{user}"), "RealName"]
        )
        .contains("aeci changed"),
        "dscl shows the new comment"
    );
    assert!(
        stdout_of(
            "dscl",
            &[".", "-read", &format!("/Users/{user}"), "UserShell"]
        )
        .contains("/bin/sh"),
        "dscl shows the new shell"
    );

    // password: set, unlock, and a directory check with it succeeds
    let password = strong_password();
    let set = call_ok("user_passwd", vec![s(&user), s(&password)]);
    assert!(flag(&set, "changed"));
    call_ok("user_unlock", vec![s(&user)]);
    assert!(!user_disabled(&user), "unlock enables the account");
    assert!(auth_ok(&user, &password), "the new password checks out");

    // lock: the account is disabled and the same password no longer checks
    let locked = call_ok("user_lock", vec![s(&user)]);
    assert!(flag(&locked, "locked"));
    assert!(user_disabled(&user), "lock disables the account");
    assert!(
        !auth_ok(&user, &password),
        "a disabled account does not log on"
    );
    call_ok("user_unlock", vec![s(&user)]);
    assert!(!user_disabled(&user), "unlock clears the disabled marker");

    // groups: create one, add the account, see it through id
    call_ok("group_add", vec![s(&group)]);
    assert!(group_exists(&group), "dscl lists the group");
    let appended = call_ok(
        "user_mod",
        vec![
            s(&user),
            rec(&[("add_groups", Value::Array(vec![s(&group)]))]),
        ],
    );
    assert_eq!(changed(&appended), ["add_groups"]);
    assert!(
        groups_of(&user).contains(&group),
        "id lists the account in the group"
    );
    // adding again is not an error
    call_ok(
        "user_mod",
        vec![
            s(&user),
            rec(&[("add_groups", Value::Array(vec![s(&group)]))]),
        ],
    );
    // remove the membership with dseditgroup: no builtin does that on macOS
    assert!(succeeds(
        "dseditgroup",
        &[
            "-o",
            "edit",
            "-d",
            user.as_str(),
            "-t",
            "user",
            group.as_str()
        ]
    ));
    assert!(
        !groups_of(&user).contains(&group),
        "id no longer lists the group"
    );

    // delete: the account is gone, and so is the group
    let deleted = call_ok("user_del", vec![s(&user)]);
    assert!(!flag(&deleted, "removed_home"));
    assert!(!user_exists(&user), "dscl no longer lists the account");
    assert!(
        !succeeds("id", &["-u", user.as_str()]),
        "id no longer finds it"
    );
    call_ok("group_del", vec![s(&group)]);
    assert!(!group_exists(&group), "the group is gone");
}

#[test]
#[ignore = "changes groups: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_group_lifecycle_verifies_each_step() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let pid = std::process::id();
    let first = format!("aeci{pid}g");
    let second = format!("aeci{pid}h");
    let mut cleanup = Cleanup::default();
    cleanup.groups.push(first.clone());
    cleanup.groups.push(second.clone());

    let added = call_ok("group_add", vec![s(&first)]);
    let gid = match field(&added, "gid") {
        Value::Int(n) => *n,
        other => panic!("gid is not an Int: {other:?}"),
    };
    assert!(gid >= 501, "a person's gid, got {gid}");
    assert!(group_exists(&first), "dscl lists the group");
    expect_code(call("group_add", vec![s(&first)]), "E_BAD_STATE");

    let renamed = call_ok(
        "group_mod",
        vec![s(&first), rec(&[("new_name", s(&second))])],
    );
    assert_eq!(text(&renamed, "name"), second);
    assert_eq!(changed(&renamed), ["new_name"]);
    assert!(!group_exists(&first), "the old name is gone");
    assert!(group_exists(&second), "the new name exists");

    // renumber to a gid no record uses; the new value is read back with dscl
    let target = free_gid();
    let renumbered = call_ok(
        "group_mod",
        vec![s(&second), rec(&[("gid", Value::Int(target))])],
    );
    assert_eq!(changed(&renumbered), ["gid"]);
    assert_eq!(
        stdout_of(
            "dscl",
            &[".", "-read", &format!("/Groups/{second}"), "PrimaryGroupID"]
        )
        .trim(),
        format!("PrimaryGroupID: {target}")
    );

    let deleted = call_ok("group_del", vec![s(&second)]);
    assert_eq!(text(&deleted, "name"), second);
    assert!(!group_exists(&second), "the group is gone");
}

/// remove_home removes the standard folder /Users/<name>, and nothing else.
/// macOS creates no folder at add time, so this test makes one first.
#[test]
#[ignore = "changes accounts and folders: disposable macOS runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_remove_home_removes_only_the_standard_folder() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let user = format!("aeci{}h", std::process::id());
    let mut cleanup = Cleanup::default();
    cleanup.users.push(user.clone());

    call_ok("user_add", vec![s(&user)]);
    let home = format!("/Users/{user}");
    std::fs::create_dir_all(&home).expect("make the test folder");
    std::fs::write(format!("{home}/probe.txt"), "aeci").expect("write the test file");

    let deleted = call_ok(
        "user_del",
        vec![s(&user), rec(&[("remove_home", Value::Bool(true))])],
    );
    assert!(flag(&deleted, "removed_home"));
    assert!(!user_exists(&user), "the account is gone");
    assert!(
        !std::path::Path::new(&home).exists(),
        "the standard folder is gone"
    );
}
