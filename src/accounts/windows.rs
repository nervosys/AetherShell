//! Windows account and group administration: the `user_*` and `group_*`
//! builtins through the NetAPI (netapi32) and the local SAM.
//!
//! The entry points take the arguments the Linux backend takes, run the same
//! validation (the parse structs and option helpers in the parent module),
//! refuse inside a filesystem transaction, and return the same record shapes
//! where Windows has a counterpart. An option with no Windows meaning is
//! E_BAD_ARG naming the option; it is never ignored.
//!
//! Protection is by SID, as uid and gid below 1000 are on Linux. A principal
//! whose RID is below 1000 (Administrator 500, Guest 501, DefaultAccount 503,
//! WDAGUtilityAccount 504) and every BUILTIN group are E_POLICY_DENY, and so
//! is the account running this process.
//!
//! A password travels in one UTF-16 buffer that is handed to NetUserSetInfo
//! and then overwritten. It never reaches a command line, a log or an error.

use anyhow::Result;
use std::ffi::c_void;
use std::ptr::{from_ref, null, null_mut};

use super::{
    refuse_in_transaction, record, strings, GroupAdd, GroupDel, GroupMod, UserAdd, UserDel,
    UserMod, UserOnly, UserPasswd,
};
use crate::safety::{arg_err, bad_state, not_found, os_denied, policy_deny, tool_failed};
use crate::value::Value;

const NERR_SUCCESS: u32 = 0;
const NERR_GROUP_NOT_FOUND: u32 = 2220;
const NERR_USER_NOT_FOUND: u32 = 2221;
const NERR_GROUP_EXISTS: u32 = 2223;
const NERR_USER_EXISTS: u32 = 2224;
const NERR_PASSWORD_TOO_SHORT: u32 = 2245;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_PASSWORD: u32 = 1324;
const ERROR_MEMBER_IN_ALIAS: u32 = 1378;
const ERROR_ALIAS_EXISTS: u32 = 1379;

const UF_SCRIPT: u32 = 0x0001;
const UF_ACCOUNTDISABLE: u32 = 0x0002;
const UF_NORMAL_ACCOUNT: u32 = 0x0200;
const USER_PRIV_USER: u32 = 1;

/// RIDs below this belong to built-in principals.
const BUILTIN_RID_LIMIT: u32 = 1000;
/// The subauthority that starts every BUILTIN domain SID (S-1-5-32-*).
const BUILTIN_DOMAIN: u32 = 32;

#[link(name = "netapi32")]
extern "system" {
    fn NetUserAdd(server: *const u16, level: u32, buf: *const u8, parm_err: *mut u32) -> u32;
    fn NetUserDel(server: *const u16, user: *const u16) -> u32;
    fn NetUserGetInfo(server: *const u16, user: *const u16, level: u32, buf: *mut *mut u8) -> u32;
    fn NetUserSetInfo(
        server: *const u16,
        user: *const u16,
        level: u32,
        buf: *const u8,
        parm_err: *mut u32,
    ) -> u32;
    fn NetLocalGroupAdd(server: *const u16, level: u32, buf: *const u8, parm_err: *mut u32)
        -> u32;
    fn NetLocalGroupDel(server: *const u16, group: *const u16) -> u32;
    fn NetLocalGroupGetInfo(
        server: *const u16,
        group: *const u16,
        level: u32,
        buf: *mut *mut u8,
    ) -> u32;
    fn NetLocalGroupSetInfo(
        server: *const u16,
        group: *const u16,
        level: u32,
        buf: *const u8,
        parm_err: *mut u32,
    ) -> u32;
    fn NetLocalGroupAddMembers(
        server: *const u16,
        group: *const u16,
        level: u32,
        buf: *const u8,
        total: u32,
    ) -> u32;
    fn NetApiBufferFree(buf: *mut u8) -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertSidToStringSidW(sid: *const c_void, text: *mut *mut u16) -> i32;
    fn GetUserNameW(buf: *mut u16, len: *mut u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

// The structures below mirror the NetAPI layouts. Not every field is read on
// the Rust side: the API reads the ones it needs.

/// USER_INFO_1. Read back for the flags and the comment; written to create an
/// account.
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

#[repr(C)]
#[allow(dead_code)]
struct UserInfo1003 {
    password: *const u16,
}

#[repr(C)]
#[allow(dead_code)]
struct UserInfo1007 {
    comment: *const u16,
}

#[repr(C)]
#[allow(dead_code)]
struct UserInfo1008 {
    flags: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct LocalGroupInfo0 {
    name: *const u16,
}

#[repr(C)]
#[allow(dead_code)]
struct LocalGroupInfo1 {
    name: *const u16,
    comment: *const u16,
}

#[repr(C)]
#[allow(dead_code)]
struct LocalGroupMemberInfo0 {
    sid: *const c_void,
}

/// What this backend reads back from an account.
struct UserState {
    flags: u32,
    comment: String,
}

// ---------------------------------------------------------------------------
// Public entry points, one per Linux builtin
// ---------------------------------------------------------------------------

pub fn user_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserAdd::parse(&args, &input)?;
    refuse_in_transaction("user_add")?;
    if matches!(args.get(1), Some(Value::Record(rec)) if rec.contains_key("home")) {
        return Err(no_windows_meaning(
            "user_add",
            "home",
            "Windows makes a profile at first logon, not when the account is created",
        ));
    }
    if plan.shell.is_some() {
        return Err(no_windows_meaning(
            "user_add",
            "shell",
            "Windows accounts have no login shell",
        ));
    }
    if plan.system {
        return Err(no_windows_meaning(
            "user_add",
            "system",
            "Windows has no system-account class; the RID is assigned by Windows",
        ));
    }
    if !plan.groups.is_empty() {
        return Err(no_windows_meaning(
            "user_add",
            "groups",
            "add the account to groups with user_mod add_groups once it exists",
        ));
    }
    if user_state("user_add", &plan.name)?.is_some() {
        return Err(bad_state(
            "user_add",
            &format!("user {} already exists", plan.name),
            "a different name; the existing account is not changed",
        ));
    }

    let account = wide(&plan.name);
    let comment = wide(plan.comment.as_deref().unwrap_or(""));
    // Created disabled and with no password, so it cannot log on until
    // user_passwd sets a password and user_unlock enables it.
    let info = UserInfo1 {
        name: account.as_ptr(),
        password: null(),
        password_age: 0,
        privilege: USER_PRIV_USER,
        home_dir: null(),
        comment: comment.as_ptr(),
        flags: UF_SCRIPT | UF_NORMAL_ACCOUNT | UF_ACCOUNTDISABLE,
        script_path: null(),
    };
    // SAFETY: `info` and the strings it points at outlive the call.
    let status = unsafe { NetUserAdd(null(), 1, from_ref(&info).cast(), null_mut()) };
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "user_add",
            "NetUserAdd",
            status,
            &plan.name,
            "user",
        ));
    }
    let Some(state) = user_state("user_add", &plan.name)? else {
        return Err(tool_failed(
            "user_add",
            "NetUserGetInfo",
            Some(0),
            "the account was missing after NetUserAdd reported success",
        ));
    };
    let sid = crate::builtins::windows_account_sid("user_add", &plan.name)?;
    Ok(record(vec![
        ("name", Value::Str(plan.name.clone())),
        ("sid", Value::Str(sid_text("user_add", &sid)?)),
        ("comment", Value::Str(state.comment)),
        (
            "locked",
            Value::Bool((state.flags & UF_ACCOUNTDISABLE) != 0),
        ),
    ]))
}

pub fn user_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserDel::parse(&args, &input)?;
    refuse_in_transaction("user_del")?;
    if plan.remove_home {
        return Err(no_windows_meaning(
            "user_del",
            "remove_home",
            "Windows leaves the profile folder in place when an account is deleted; remove it yourself after checking what it holds",
        ));
    }
    if user_state("user_del", &plan.name)?.is_none() {
        return Err(not_found("user_del", "user", &plan.name));
    }
    guard_user("user_del", &plan.name)?;
    let account = wide(&plan.name);
    // SAFETY: `account` is NUL-terminated and lives across the call.
    let status = unsafe { NetUserDel(null(), account.as_ptr()) };
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "user_del",
            "NetUserDel",
            status,
            &plan.name,
            "user",
        ));
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("removed_home", Value::Bool(false)),
    ]))
}

pub fn user_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserMod::parse(&args, &input)?;
    refuse_in_transaction("user_mod")?;
    if plan.move_home {
        return Err(no_windows_meaning(
            "user_mod",
            "move_home",
            "Windows does not move a profile when an account changes",
        ));
    }
    if plan.shell.is_some() {
        return Err(no_windows_meaning(
            "user_mod",
            "shell",
            "Windows accounts have no login shell",
        ));
    }
    if plan.home.is_some() {
        return Err(no_windows_meaning(
            "user_mod",
            "home",
            "Windows does not relocate a profile when an account changes",
        ));
    }
    if plan.groups.is_some() {
        return Err(no_windows_meaning(
            "user_mod",
            "groups",
            "replacing the groups would have to remove built-in memberships; use add_groups to append",
        ));
    }
    if user_state("user_mod", &plan.name)?.is_none() {
        return Err(not_found("user_mod", "user", &plan.name));
    }
    guard_user("user_mod", &plan.name)?;
    let groups: Vec<String> = plan.add_groups.clone().unwrap_or_default();
    for group in &groups {
        if !local_group_exists("user_mod", group)? {
            return Err(not_found("user_mod", "group", group));
        }
        guard_builtin("user_mod", group, "group")?;
    }

    let mut changed: Vec<&str> = Vec::new();
    if let Some(comment) = &plan.comment {
        set_user_comment("user_mod", &plan.name, comment)?;
        changed.push("comment");
    }
    if !groups.is_empty() {
        let sid = crate::builtins::windows_account_sid("user_mod", &plan.name)?;
        for group in &groups {
            add_member("user_mod", group, &sid)?;
        }
        changed.push("add_groups");
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("changed", strings(&changed)),
    ]))
}

pub fn user_passwd(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserPasswd::parse(&args, &input)?;
    refuse_in_transaction("user_passwd")?;
    if user_state("user_passwd", &plan.name)?.is_none() {
        return Err(not_found("user_passwd", "user", &plan.name));
    }
    guard_user("user_passwd", &plan.name)?;
    let account = wide(&plan.name);
    let mut password = wide(&plan.password);
    let info = UserInfo1003 {
        password: password.as_ptr(),
    };
    // SAFETY: `info` points at `password`, and both outlive the call.
    let status = unsafe {
        NetUserSetInfo(
            null(),
            account.as_ptr(),
            1003,
            from_ref(&info).cast(),
            null_mut(),
        )
    };
    wipe(&mut password);
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "user_passwd",
            "NetUserSetInfo",
            status,
            &plan.name,
            "user",
        ));
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("changed", Value::Bool(true)),
    ]))
}

pub fn user_lock(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    set_lock("user_lock", args, input, true)
}

pub fn user_unlock(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    set_lock("user_unlock", args, input, false)
}

pub fn group_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = GroupAdd::parse(&args, &input)?;
    refuse_in_transaction("group_add")?;
    if plan.gid.is_some() {
        return Err(no_windows_meaning(
            "group_add",
            "gid",
            "Windows assigns each group its identifier",
        ));
    }
    if plan.system {
        return Err(no_windows_meaning(
            "group_add",
            "system",
            "Windows has no system-group class",
        ));
    }
    if local_group_exists("group_add", &plan.name)? {
        return Err(bad_state(
            "group_add",
            &format!("group {} already exists", plan.name),
            "a different name; the existing group is not changed",
        ));
    }
    let group = wide(&plan.name);
    let info = LocalGroupInfo1 {
        name: group.as_ptr(),
        comment: null(),
    };
    // SAFETY: `info` and `group` outlive the call.
    let status = unsafe { NetLocalGroupAdd(null(), 1, from_ref(&info).cast(), null_mut()) };
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "group_add",
            "NetLocalGroupAdd",
            status,
            &plan.name,
            "group",
        ));
    }
    let sid = crate::builtins::windows_account_sid("group_add", &plan.name)?;
    Ok(record(vec![
        ("name", Value::Str(plan.name.clone())),
        ("sid", Value::Str(sid_text("group_add", &sid)?)),
    ]))
}

pub fn group_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = GroupDel::parse(&args, &input)?;
    refuse_in_transaction("group_del")?;
    if !local_group_exists("group_del", &plan.name)? {
        return Err(not_found("group_del", "group", &plan.name));
    }
    guard_builtin("group_del", &plan.name, "group")?;
    let group = wide(&plan.name);
    // SAFETY: `group` is NUL-terminated and lives across the call.
    let status = unsafe { NetLocalGroupDel(null(), group.as_ptr()) };
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "group_del",
            "NetLocalGroupDel",
            status,
            &plan.name,
            "group",
        ));
    }
    Ok(record(vec![("name", Value::Str(plan.name))]))
}

pub fn group_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = GroupMod::parse(&args, &input)?;
    refuse_in_transaction("group_mod")?;
    if plan.gid.is_some() {
        return Err(no_windows_meaning(
            "group_mod",
            "gid",
            "Windows assigns each group its identifier, and groups are not renumbered",
        ));
    }
    if !local_group_exists("group_mod", &plan.name)? {
        return Err(not_found("group_mod", "group", &plan.name));
    }
    guard_builtin("group_mod", &plan.name, "group")?;
    let new_name = plan
        .new_name
        .clone()
        .ok_or_else(|| arg_err("group_mod needs new_name"))?;
    if local_group_exists("group_mod", &new_name)? {
        return Err(bad_state(
            "group_mod",
            &format!("group {new_name} already exists"),
            "a different new_name; the existing group is not changed",
        ));
    }
    let old = wide(&plan.name);
    let new = wide(&new_name);
    let info = LocalGroupInfo0 { name: new.as_ptr() };
    // SAFETY: `old`, `new` and `info` outlive the call.
    let status = unsafe {
        NetLocalGroupSetInfo(
            null(),
            old.as_ptr(),
            0,
            from_ref(&info).cast(),
            null_mut(),
        )
    };
    if status != NERR_SUCCESS {
        return Err(net_failure(
            "group_mod",
            "NetLocalGroupSetInfo",
            status,
            &plan.name,
            "group",
        ));
    }
    Ok(record(vec![
        ("name", Value::Str(new_name)),
        ("changed", strings(&["new_name"])),
    ]))
}

// ---------------------------------------------------------------------------
// Operations shared by the entry points
// ---------------------------------------------------------------------------

fn set_lock(builtin: &str, args: Vec<Value>, input: Option<Value>, lock: bool) -> Result<Value> {
    let name = UserOnly::parse(builtin, &args, &input)?.name;
    refuse_in_transaction(builtin)?;
    let Some(state) = user_state(builtin, &name)? else {
        return Err(not_found(builtin, "user", &name));
    };
    guard_user(builtin, &name)?;
    // Locking disables the account, which stops every logon method; unlocking
    // enables it. The password itself is not touched.
    let flags = if lock {
        state.flags | UF_ACCOUNTDISABLE
    } else {
        state.flags & !UF_ACCOUNTDISABLE
    };
    let account = wide(&name);
    let info = UserInfo1008 { flags };
    // SAFETY: `account` and `info` outlive the call.
    let status = unsafe {
        NetUserSetInfo(
            null(),
            account.as_ptr(),
            1008,
            from_ref(&info).cast(),
            null_mut(),
        )
    };
    if status != NERR_SUCCESS {
        return Err(net_failure(builtin, "NetUserSetInfo", status, &name, "user"));
    }
    Ok(record(vec![
        ("name", Value::Str(name)),
        ("locked", Value::Bool(lock)),
    ]))
}

fn set_user_comment(builtin: &str, name: &str, comment: &str) -> Result<()> {
    let account = wide(name);
    let text = wide(comment);
    let info = UserInfo1007 {
        comment: text.as_ptr(),
    };
    // SAFETY: `account`, `text` and `info` outlive the call.
    let status = unsafe {
        NetUserSetInfo(
            null(),
            account.as_ptr(),
            1007,
            from_ref(&info).cast(),
            null_mut(),
        )
    };
    if status == NERR_SUCCESS {
        Ok(())
    } else {
        Err(net_failure(builtin, "NetUserSetInfo", status, name, "user"))
    }
}

/// Adds the account with `user_sid` to a local group. A user who is already a
/// member is not an error, as with usermod -a.
fn add_member(builtin: &str, group: &str, user_sid: &[u32]) -> Result<()> {
    let name = wide(group);
    let member = LocalGroupMemberInfo0 {
        sid: user_sid.as_ptr().cast(),
    };
    // SAFETY: `name` and `member` outlive the call, and `member` points at the
    // SID buffer, which lives across it too.
    let status = unsafe {
        NetLocalGroupAddMembers(null(), name.as_ptr(), 0, from_ref(&member).cast(), 1)
    };
    match status {
        NERR_SUCCESS | ERROR_MEMBER_IN_ALIAS => Ok(()),
        other => Err(net_failure(
            builtin,
            "NetLocalGroupAddMembers",
            other,
            group,
            "group",
        )),
    }
}

/// The account's flags and comment, or None when no such account exists.
fn user_state(builtin: &str, name: &str) -> Result<Option<UserState>> {
    let account = wide(name);
    let mut buf: *mut u8 = null_mut();
    // SAFETY: `account` is NUL-terminated and lives across the call; on success
    // `buf` receives a USER_INFO_1 that is released below.
    let status = unsafe { NetUserGetInfo(null(), account.as_ptr(), 1, &mut buf) };
    if status == NERR_USER_NOT_FOUND {
        return Ok(None);
    }
    if status != NERR_SUCCESS {
        return Err(net_failure(builtin, "NetUserGetInfo", status, name, "user"));
    }
    // SAFETY: the successful call left a USER_INFO_1 at `buf`, valid until it is
    // freed on the next line.
    let state = unsafe {
        let info = &*(buf as *const UserInfo1);
        UserState {
            flags: info.flags,
            comment: wide_to_string(info.comment),
        }
    };
    // SAFETY: `buf` came from NetUserGetInfo and is freed once.
    unsafe { NetApiBufferFree(buf) };
    Ok(Some(state))
}

fn local_group_exists(builtin: &str, name: &str) -> Result<bool> {
    let group = wide(name);
    let mut buf: *mut u8 = null_mut();
    // SAFETY: `group` is NUL-terminated and lives across the call; on success
    // `buf` receives a buffer that is released below.
    let status = unsafe { NetLocalGroupGetInfo(null(), group.as_ptr(), 0, &mut buf) };
    match status {
        NERR_SUCCESS => {
            // SAFETY: `buf` came from NetLocalGroupGetInfo and is freed once.
            unsafe { NetApiBufferFree(buf) };
            Ok(true)
        }
        NERR_GROUP_NOT_FOUND => Ok(false),
        other => Err(net_failure(
            builtin,
            "NetLocalGroupGetInfo",
            other,
            name,
            "group",
        )),
    }
}

// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

/// Refuses a built-in principal, by SID. The name must exist: the caller has
/// already looked it up.
fn guard_builtin(builtin: &str, name: &str, kind: &str) -> Result<()> {
    let sid = crate::builtins::windows_account_sid(builtin, name)?;
    if is_protected_sid(&sid) {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: {name} is a built-in {kind} (a BUILTIN group, or a RID below {BUILTIN_RID_LIMIT}); this builtin never changes it"
            ),
            "built-in accounts and groups are managed with Windows' own tools",
        ));
    }
    Ok(())
}

fn guard_user(builtin: &str, name: &str) -> Result<()> {
    guard_builtin(builtin, name, "account")?;
    if current_account(builtin)?.eq_ignore_ascii_case(name) {
        return Err(policy_deny(
            builtin,
            format!("{builtin}: {name} is the account running this process"),
            "changing it could lock this machine out; use another tool",
        ));
    }
    Ok(())
}

/// True for a SID that names a BUILTIN principal (S-1-5-32-*), or one whose
/// RID is below BUILTIN_RID_LIMIT. A SID that cannot be read is protected.
fn is_protected_sid(sid: &[u32]) -> bool {
    let Some(subs) = sid_subauthorities(sid) else {
        return true;
    };
    let Some(&rid) = subs.last() else {
        return true;
    };
    subs.first() == Some(&BUILTIN_DOMAIN) || rid < BUILTIN_RID_LIMIT
}

/// The subauthorities of a SID, read from its bytes in memory order.
fn sid_subauthorities(sid: &[u32]) -> Option<Vec<u32>> {
    let bytes: Vec<u8> = sid.iter().flat_map(|word| word.to_ne_bytes()).collect();
    let count = usize::from(*bytes.get(1)?);
    (0..count)
        .map(|i| {
            let at = 8 + 4 * i;
            Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
        })
        .collect()
}

/// The name of the account this process runs as.
fn current_account(builtin: &str) -> Result<String> {
    let mut buf = vec![0u16; 257];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` UTF-16 units; on success GetUserNameW writes a
    // NUL-terminated name into it.
    let ok = unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) };
    if ok == 0 {
        return Err(bad_state(
            builtin,
            &format!("GetUserNameW failed: {}", std::io::Error::last_os_error()),
            "a process that can read its own account name",
        ));
    }
    let end = buf.iter().position(|&unit| unit == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..end]))
}

// ---------------------------------------------------------------------------
// Errors and text
// ---------------------------------------------------------------------------

fn no_windows_meaning(builtin: &str, option: &str, why: &str) -> anyhow::Error {
    arg_err(format!(
        "{builtin}: option {option} has no Windows meaning; {why}; NOTHING WAS CHANGED"
    ))
}

/// Maps a NetAPI status onto the builtin error taxonomy.
fn net_failure(builtin: &str, api: &str, status: u32, name: &str, kind: &str) -> anyhow::Error {
    match status {
        NERR_USER_NOT_FOUND | NERR_GROUP_NOT_FOUND => not_found(builtin, kind, name),
        NERR_USER_EXISTS | NERR_GROUP_EXISTS | ERROR_ALIAS_EXISTS => bad_state(
            builtin,
            &format!("{kind} {name} already exists"),
            &format!("a different name; the existing {kind} is not changed"),
        ),
        ERROR_ACCESS_DENIED => os_denied(
            builtin,
            format!("{builtin}: Windows refused {api} for {name}: access denied"),
            "run this from an elevated (Administrator) process",
        ),
        NERR_PASSWORD_TOO_SHORT | ERROR_INVALID_PASSWORD => arg_err(format!(
            "{builtin}: the password does not meet this machine's password policy"
        )),
        other => tool_failed(
            builtin,
            api,
            Some(other as i32),
            &std::io::Error::from_raw_os_error(other as i32).to_string(),
        ),
    }
}

/// A UTF-16 buffer with a terminating NUL.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Overwrites a password buffer with zeros in a way the optimizer keeps.
fn wipe(buf: &mut [u16]) {
    for unit in buf.iter_mut() {
        // SAFETY: `unit` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(unit, 0) };
    }
}

/// The text at a NUL-terminated UTF-16 pointer; null gives "".
///
/// # Safety
///
/// `ptr` is null or points at a NUL-terminated UTF-16 string that stays valid
/// for the call.
unsafe fn wide_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: the caller's contract puts the terminating NUL within reach.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the `len` units before the NUL are readable.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// The S-1-5-... text of a SID.
fn sid_text(builtin: &str, sid: &[u32]) -> Result<String> {
    let mut text: *mut u16 = null_mut();
    // SAFETY: `sid` is a complete SID. On success `text` receives a LocalAlloc'd
    // NUL-terminated string, which is freed below.
    let ok = unsafe { ConvertSidToStringSidW(sid.as_ptr().cast(), &mut text) };
    if ok == 0 {
        return Err(tool_failed(
            builtin,
            "ConvertSidToStringSidW",
            None,
            &std::io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: `text` is the string the successful call allocated.
    let out = unsafe { wide_to_string(text) };
    // SAFETY: `text` came from ConvertSidToStringSidW and is freed once.
    unsafe { LocalFree(text.cast()) };
    Ok(out)
}
