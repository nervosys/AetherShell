//! Contracts for the Windows account and group builtins (`user_*`, `group_*`).
//!
//! Compiled on Windows only. The ordinary tests change nothing: each one is
//! refused before any NetAPI write call. The system-changing tests are
//! `#[ignore]`d, and they also return at once unless AE_DISPOSABLE_HOST=1,
//! which only the disposable Windows runner sets. They create accounts and
//! groups named aeci<pid>, check each step through the Windows APIs themselves
//! (flags read with NetUserGetInfo, a network logon with LogonUserW, and
//! `net user` and `net localgroup` for existence), and remove what they made
//! in a Drop guard.

#![cfg(windows)]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::hash::{BuildHasher, Hasher};
use std::process::Command;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

/// Every test takes this lock, so account changes never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

const UF_ACCOUNTDISABLE: u32 = 0x0002;

#[link(name = "netapi32")]
extern "system" {
    fn NetUserGetInfo(server: *const u16, user: *const u16, level: u32, buf: *mut *mut u8) -> u32;
    fn NetApiBufferFree(buf: *mut u8) -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    fn LogonUserW(
        user: *const u16,
        domain: *const u16,
        password: *const u16,
        logon_type: u32,
        provider: u32,
        token: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CloseHandle(handle: *mut c_void) -> i32;
}

/// The start of USER_INFO_1. Only the flags are read here.
#[repr(C)]
#[allow(dead_code)]
struct UserInfo1 {
    name: *const u16,
    password: *const u16,
    password_age: u32,
    privilege: u32,
    home_dir: *const u16,
    comment: *const u16,
    flags: u32,
    script_path: *const u16,
}

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

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The flags of an account read straight from NetUserGetInfo, or None when
/// the account does not exist.
fn account_flags(name: &str) -> Option<u32> {
    let account = wide(name);
    let mut buf: *mut u8 = null_mut();
    // SAFETY: `account` is NUL-terminated and lives across the call; on success
    // `buf` holds a USER_INFO_1 that is freed below.
    let status = unsafe { NetUserGetInfo(null(), account.as_ptr(), 1, &mut buf) };
    if status != 0 {
        return None;
    }
    // SAFETY: the successful call left a USER_INFO_1 at `buf`.
    let flags = unsafe { (*(buf as *const UserInfo1)).flags };
    // SAFETY: `buf` came from NetUserGetInfo and is freed once.
    unsafe { NetApiBufferFree(buf) };
    Some(flags)
}

fn account_disabled(name: &str) -> bool {
    account_flags(name).is_some_and(|flags| (flags & UF_ACCOUNTDISABLE) != 0)
}

/// True when a network logon with this password succeeds. A disabled account
/// fails whatever the password, so callers unlock first.
fn logon_ok(user: &str, password: &str) -> bool {
    let (name, domain, secret) = (wide(user), wide("."), wide(password));
    let mut token: *mut c_void = null_mut();
    // SAFETY: the three strings are NUL-terminated and live across the call.
    // LOGON32_LOGON_NETWORK is 3 and LOGON32_PROVIDER_DEFAULT is 0.
    let granted = unsafe {
        LogonUserW(
            name.as_ptr(),
            domain.as_ptr(),
            secret.as_ptr(),
            3,
            0,
            &mut token,
        )
    } != 0;
    if granted {
        // SAFETY: the granted logon returned a token handle, closed once here.
        unsafe { CloseHandle(token) };
    }
    granted
}

fn net_ok(args: &[&str]) -> bool {
    Command::new("net")
        .args(args)
        .output()
        .is_ok_and(|out| out.status.success())
}

fn net_output(args: &[&str]) -> String {
    Command::new("net")
        .args(args)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// A local account or group's name, looked up by SID, since a runner image may
/// rename the built-in Administrator account. Empty when nothing matches.
fn name_by_sid(kind: &str, sid_like: &str) -> String {
    let script = format!(
        "(Get-Local{kind} | Where-Object {{ $_.SID.Value -like '{sid_like}' }} | Select-Object -First 1).Name"
    );
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// A random password with upper and lower case, digits and a symbol. Never
/// printed.
fn strong_password() -> String {
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
            let _ = Command::new("net")
                .args(["user", user.as_str(), "/delete"])
                .output();
        }
        for group in &self.groups {
            let _ = Command::new("net")
                .args(["localgroup", group.as_str(), "/delete"])
                .output();
        }
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: each call is refused before any NetAPI write
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

/// Options with no Windows meaning are refused by name, before any change.
#[test]
fn aeci_options_without_a_windows_meaning_are_refused() {
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
            vec![s("aeci_probe"), rec(&[("shell", s("/bin/sh"))])],
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
            "user_add",
            vec![
                s("aeci_probe"),
                rec(&[("groups", Value::Array(vec![s("aeci_g")]))]),
            ],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_del",
            vec![s("aeci_probe"), rec(&[("remove_home", Value::Bool(true))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "user_mod",
            vec![s("aeci_probe"), rec(&[("shell", s("/bin/sh"))])],
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
            vec![s("aeci_probe"), rec(&[("gid", Value::Int(5000))])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "group_mod",
            vec![s("aeci_probe"), rec(&[("gid", Value::Int(5000))])],
        ),
        "E_BAD_ARG",
    );
}

/// The accounts are found by SID, not by name: the built-in Administrator
/// (RID 500) may be renamed, as it is on GitHub's Windows images. Names are
/// matched case-insensitively by Windows, so the lowercased names still reach
/// the SID guard, which refuses them before any write.
///
/// Disposable only, although it should change nothing: if the guard were ever
/// broken, these calls would delete, lock or re-password the real
/// Administrator account and rename the real Users group on whatever machine
/// ran them, elevated or not depending on how the shell was started.
#[test]
#[ignore = "aims at real built-in accounts: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_built_in_accounts_and_groups_are_refused_before_any_change() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let admin = name_by_sid("User", "S-1-5-21-*-500").to_lowercase();
    let admins = name_by_sid("Group", "S-1-5-32-544").to_lowercase();
    let users = name_by_sid("Group", "S-1-5-32-545").to_lowercase();
    assert!(
        !admin.is_empty() && !admins.is_empty() && !users.is_empty(),
        "built-in accounts not found: {admin:?} {admins:?} {users:?}"
    );
    expect_code(call("user_del", vec![s(&admin)]), "E_POLICY_DENY");
    expect_code(call("user_lock", vec![s(&admin)]), "E_POLICY_DENY");
    expect_code(
        call("user_passwd", vec![s(&admin), s("Aeci-Probe-1")]),
        "E_POLICY_DENY",
    );
    expect_code(call("group_del", vec![s(&admins)]), "E_POLICY_DENY");
    expect_code(
        call(
            "group_mod",
            vec![s(&users), rec(&[("new_name", s("aeci_probe_g"))])],
        ),
        "E_POLICY_DENY",
    );
    // The account the tests run as is refused too, whatever its RID.
    let me = std::env::var("USERNAME").unwrap_or_default();
    assert!(!me.is_empty(), "USERNAME is not set");
    expect_code(call("user_lock", vec![s(&me)]), "E_POLICY_DENY");
}

/// Disposable only: it reaches the real account APIs, which is the point, so it
/// is not run where a mistake would land on a real machine.
#[test]
#[ignore = "calls the account APIs: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn aeci_missing_names_are_not_found() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let missing = format!("aeci_none{}", std::process::id());
    expect_code(call("user_del", vec![s(&missing)]), "E_NOT_FOUND");
    expect_code(call("group_del", vec![s(&missing)]), "E_NOT_FOUND");
}

// ---------------------------------------------------------------------------
// System-changing tests: disposable Windows runner only
// ---------------------------------------------------------------------------

#[test]
#[ignore = "changes accounts: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
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

    // create: disabled, with no password and the comment set
    let added = call_ok(
        "user_add",
        vec![s(&user), rec(&[("comment", s("aeci initial"))])],
    );
    assert!(
        net_ok(&["user", user.as_str()]),
        "net user sees the account"
    );
    assert!(flag(&added, "locked"), "a new account starts disabled");
    assert!(
        account_disabled(&user),
        "NetUserGetInfo sees the disabled flag"
    );
    assert_eq!(text(&added, "name"), user);
    assert!(
        text(&added, "sid").starts_with("S-1-5-21-"),
        "a local account SID"
    );
    expect_code(call("user_add", vec![s(&user)]), "E_BAD_STATE");

    // comment
    let modded = call_ok(
        "user_mod",
        vec![s(&user), rec(&[("comment", s("aeci changed"))])],
    );
    assert_eq!(changed(&modded), ["comment"]);
    assert!(
        net_output(&["user", user.as_str()]).contains("aeci changed"),
        "net user shows the new comment"
    );

    // password: set, unlock, and a network logon with it succeeds
    let password = strong_password();
    let set = call_ok("user_passwd", vec![s(&user), s(&password)]);
    assert!(flag(&set, "changed"));
    call_ok("user_unlock", vec![s(&user)]);
    assert!(!account_disabled(&user), "unlock enables the account");
    assert!(logon_ok(&user, &password), "the new password logs on");

    // lock: the account is disabled and a logon with the same password fails
    let locked = call_ok("user_lock", vec![s(&user)]);
    assert!(flag(&locked, "locked"));
    assert!(account_disabled(&user), "lock disables the account");
    assert!(
        !logon_ok(&user, &password),
        "a disabled account does not log on"
    );
    call_ok("user_unlock", vec![s(&user)]);
    assert!(!account_disabled(&user), "unlock clears the disabled flag");

    // groups: create one, append the account, and see it listed there
    call_ok("group_add", vec![s(&group)]);
    assert!(net_ok(&["localgroup", group.as_str()]));
    let appended = call_ok(
        "user_mod",
        vec![
            s(&user),
            rec(&[("add_groups", Value::Array(vec![s(&group)]))]),
        ],
    );
    assert_eq!(changed(&appended), ["add_groups"]);
    assert!(
        net_output(&["localgroup", group.as_str()]).contains(user.as_str()),
        "the account is a member of the group"
    );

    // delete: the account is gone, and so is the group
    let deleted = call_ok("user_del", vec![s(&user)]);
    assert!(!flag(&deleted, "removed_home"));
    assert!(
        !net_ok(&["user", user.as_str()]),
        "net user no longer finds it"
    );
    assert!(
        account_flags(&user).is_none(),
        "NetUserGetInfo no longer finds it"
    );
    call_ok("group_del", vec![s(&group)]);
    assert!(!net_ok(&["localgroup", group.as_str()]));
}

#[test]
#[ignore = "changes groups: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
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
    assert!(
        text(&added, "sid").starts_with("S-1-5-21-"),
        "a local group SID"
    );
    assert!(
        net_ok(&["localgroup", first.as_str()]),
        "net localgroup sees it"
    );
    expect_code(call("group_add", vec![s(&first)]), "E_BAD_STATE");

    let renamed = call_ok(
        "group_mod",
        vec![s(&first), rec(&[("new_name", s(&second))])],
    );
    assert_eq!(text(&renamed, "name"), second);
    assert_eq!(changed(&renamed), ["new_name"]);
    assert!(
        !net_ok(&["localgroup", first.as_str()]),
        "the old name is gone"
    );
    assert!(
        net_ok(&["localgroup", second.as_str()]),
        "the new name exists"
    );

    let deleted = call_ok("group_del", vec![s(&second)]);
    assert_eq!(text(&deleted, "name"), second);
    assert!(
        !net_ok(&["localgroup", second.as_str()]),
        "the group is gone"
    );
}
