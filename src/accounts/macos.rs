//! macOS account and group administration: the `user_*` and `group_*`
//! builtins through Directory Service. Records are written with `dscl .`,
//! group membership with `dseditgroup`, and the disabled flag as the
//! `;DisabledUser;` value of AuthenticationAuthority, written with `dscl`.
//!
//! The entry points take the arguments the Linux backend takes, run the same
//! validation (the parse structs and option helpers in the parent module),
//! refuse inside a filesystem transaction, and require root (E_OS_DENIED
//! otherwise, as on Windows). Before any write, E_POLICY_DENY refuses:
//! - accounts and groups below uid/gid 501, the first id of a person on macOS
//!   (system and built-in records such as root, daemon, nobody, admin, staff);
//! - names starting with '_', which macOS reserves for service accounts;
//! - the account running this process, and the account `sudo` ran for
//!   (SUDO_USER or SUDO_UID), since the process itself runs as root.
//!
//! New accounts take the first free uid from 501, the primary group staff (20)
//! and a login shell of /bin/zsh unless one is given. They are created with
//! Password "*" and then disabled, so they cannot log on until
//! user_passwd sets a password and user_unlock enables them, as on Windows.
//! The home folder is /Users/<name>; macOS makes that folder at first login.
//! New groups take the first free gid from 501.
//!
//! Options with no macOS meaning are E_BAD_ARG naming the option: home (macOS
//! makes the folder at first login), system (no system accounts are created),
//! move_home (the folder is not moved), groups (replacing them would remove
//! memberships; use add_groups). remove_home removes only the standard folder
//! /Users/<name>.
//!
//! Every tool runs by absolute path with argv only, never through a shell,
//! under a deadline, and each exit status is checked. The one secret is the
//! password for `dscl -passwd`, which has no stdin form this code can rely on:
//! it is on argv, so other local users can see it in the process list while
//! dscl runs. It is blanked from any stderr before it can reach an error.

use anyhow::Result;
use std::path::Path;
use std::process::{Command, Output};

use super::{
    record, refuse_in_transaction, strings, GroupAdd, GroupDel, GroupMod, UserAdd, UserDel,
    UserMod, UserOnly, UserPasswd,
};
use crate::safety::{arg_err, bad_state, not_found, os_denied, policy_deny, tool_failed};
use crate::value::Value;

/// The first uid and gid of a person on macOS. Below it are system records.
const HUMAN_ID_MIN: i64 = 501;
/// The primary group macOS gives a standard account: staff.
const DEFAULT_GID: i64 = 20;
const DEFAULT_SHELL: &str = "/bin/zsh";
const HOME_ROOT: &str = "/Users";
/// Deadline for one dscl or dseditgroup call.
const TOOL_DEADLINE_SECS: u64 = 60;

/// What a user_add has written so far, so a failure can take it back.
#[derive(Default)]
struct Made {
    record: bool,
    groups: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry points, one per Linux builtin
// ---------------------------------------------------------------------------

pub fn user_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "user_add";
    let plan = UserAdd::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    if matches!(args.get(1), Some(Value::Record(rec)) if rec.contains_key("home")) {
        return Err(no_macos_meaning(
            B,
            "home",
            "macOS makes a home folder at the user's first login, not when the account is created",
        ));
    }
    if plan.system {
        return Err(no_macos_meaning(
            B,
            "system",
            "this builtin creates accounts at uid 501 and above; macOS system accounts are below 501",
        ));
    }
    if plan.name.starts_with('_') {
        return Err(policy_deny(
            B,
            format!("{B}: names starting with '_' are reserved for service accounts"),
            "choose a name without a leading underscore",
        ));
    }
    require_root(B)?;
    if lookup_user(B, &plan.name)?.is_some() {
        return Err(bad_state(
            B,
            &format!("user {} already exists", plan.name),
            "a different name; the existing account is not changed",
        ));
    }
    for group in &plan.groups {
        let Some(gid) = lookup_group(B, group)? else {
            return Err(not_found(B, "group", group));
        };
        guard_group(B, group, gid)?;
    }
    if let Some(shell) = &plan.shell {
        if !Path::new(shell).is_file() {
            return Err(arg_err(format!(
                "user_add: shell {shell} is not an existing file"
            )));
        }
    }

    let used: Vec<i64> = records(B, "/Users", "UniqueID")?
        .iter()
        .map(|(_, id)| *id)
        .collect();
    let uid = first_free(&used, HUMAN_ID_MIN);
    let home = format!("{HOME_ROOT}/{}", plan.name);
    let shell = plan
        .shell
        .clone()
        .unwrap_or_else(|| DEFAULT_SHELL.to_string());

    let mut made = Made::default();
    if let Err(e) = create_user(&plan, uid, &home, &shell, &mut made) {
        rollback_user(&plan.name, &made);
        return Err(e);
    }
    if lookup_user(B, &plan.name)?.is_none() {
        return Err(tool_failed(
            B,
            "dscl",
            Some(0),
            "the account was missing after dscl reported success",
        ));
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("uid", Value::Int(uid)),
        ("gid", Value::Int(DEFAULT_GID)),
        ("home", Value::Str(home)),
        ("shell", Value::Str(shell)),
        // This builtin never makes the folder; macOS does it at first login.
        ("created_home", Value::Bool(false)),
    ]))
}

pub fn user_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "user_del";
    let plan = UserDel::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    require_root(B)?;
    let Some(account) = lookup_user(B, &plan.name)? else {
        return Err(not_found(B, "user", &plan.name));
    };
    guard_user(B, &plan.name, account.uid)?;
    let standard_home = user_home(&plan.name);
    if plan.remove_home && account.home != standard_home {
        return Err(policy_deny(
            B,
            format!(
                "{B}: refusing to remove home {}; remove_home removes only the standard folder {standard_home}",
                account.home
            ),
            "remove the folder yourself after checking what it holds",
        ));
    }
    let path = user_path(&plan.name);
    run(B, "dscl", &[".", "-delete", path.as_str()], None)?;

    let mut removed_home = false;
    if plan.remove_home && Path::new(&account.home).exists() {
        std::fs::remove_dir_all(&account.home).map_err(|e| {
            bad_state(
                B,
                &format!(
                    "account {} is deleted, but {} could not be removed: {e}",
                    plan.name, account.home
                ),
                "remove that folder yourself",
            )
        })?;
        removed_home = !Path::new(&account.home).exists();
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("removed_home", Value::Bool(removed_home)),
    ]))
}

pub fn user_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "user_mod";
    let plan = UserMod::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    if plan.move_home {
        return Err(no_macos_meaning(
            B,
            "move_home",
            "macOS does not move a home folder when an account changes",
        ));
    }
    if plan.groups.is_some() {
        return Err(no_macos_meaning(
            B,
            "groups",
            "replacing the groups would have to remove memberships; use add_groups to append",
        ));
    }
    require_root(B)?;
    let Some(account) = lookup_user(B, &plan.name)? else {
        return Err(not_found(B, "user", &plan.name));
    };
    guard_user(B, &plan.name, account.uid)?;
    let groups: Vec<String> = plan.add_groups.clone().unwrap_or_default();
    for group in &groups {
        let Some(gid) = lookup_group(B, group)? else {
            return Err(not_found(B, "group", group));
        };
        guard_group(B, group, gid)?;
    }
    if let Some(shell) = &plan.shell {
        if !Path::new(shell).is_file() {
            return Err(arg_err(format!(
                "user_mod: shell {shell} is not an existing file"
            )));
        }
    }

    let path = user_path(&plan.name);
    let mut changed: Vec<&str> = Vec::new();
    if let Some(shell) = &plan.shell {
        run(
            B,
            "dscl",
            &[".", "-create", path.as_str(), "UserShell", shell.as_str()],
            None,
        )?;
        changed.push("shell");
    }
    if let Some(comment) = &plan.comment {
        run(
            B,
            "dscl",
            &[".", "-create", path.as_str(), "RealName", comment.as_str()],
            None,
        )?;
        changed.push("comment");
    }
    if let Some(home) = &plan.home {
        run(
            B,
            "dscl",
            &[
                ".",
                "-create",
                path.as_str(),
                "NFSHomeDirectory",
                home.as_str(),
            ],
            None,
        )?;
        changed.push("home");
    }
    if !groups.is_empty() {
        for group in &groups {
            add_member(B, group, &plan.name)?;
        }
        changed.push("add_groups");
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("changed", strings(&changed)),
    ]))
}

pub fn user_passwd(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "user_passwd";
    let plan = UserPasswd::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    if plan.password.starts_with('-') {
        return Err(arg_err(format!(
            "{B}: password must not start with '-'; dscl would read it as an option"
        )));
    }
    require_root(B)?;
    let Some(account) = lookup_user(B, &plan.name)? else {
        return Err(not_found(B, "user", &plan.name));
    };
    guard_user(B, &plan.name, account.uid)?;
    let path = user_path(&plan.name);
    // `dscl -passwd` rewrites AuthenticationAuthority, so a disabled account
    // is disabled again afterwards: a new password does not enable it.
    let was_disabled = is_disabled(B, &plan.name)?;
    run(
        B,
        "dscl",
        &[".", "-passwd", path.as_str(), plan.password.as_str()],
        Some(plan.password.as_str()),
    )?;
    if was_disabled {
        set_disabled(B, &plan.name, true)?;
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
    const B: &str = "group_add";
    let plan = GroupAdd::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    if plan.system {
        return Err(no_macos_meaning(
            B,
            "system",
            "this builtin creates groups at gid 501 and above; macOS system groups are below 501",
        ));
    }
    if plan.name.starts_with('_') {
        return Err(policy_deny(
            B,
            format!("{B}: names starting with '_' are reserved for service groups"),
            "choose a name without a leading underscore",
        ));
    }
    require_root(B)?;
    if lookup_group(B, &plan.name)?.is_some() {
        return Err(bad_state(
            B,
            &format!("group {} already exists", plan.name),
            "a different name; the existing group is not changed",
        ));
    }
    let used: Vec<i64> = records(B, "/Groups", "PrimaryGroupID")?
        .iter()
        .map(|(_, id)| *id)
        .collect();
    let gid = match plan.gid {
        Some(gid) if used.contains(&gid) => {
            return Err(bad_state(
                B,
                &format!("gid {gid} is already used by another group"),
                "a different gid; the existing group is not changed",
            ));
        }
        Some(gid) => gid,
        None => first_free(&used, HUMAN_ID_MIN),
    };
    let path = group_path(&plan.name);
    let gid_text = gid.to_string();
    run(B, "dscl", &[".", "-create", path.as_str()], None)?;
    if let Err(e) = run(
        B,
        "dscl",
        &[
            ".",
            "-create",
            path.as_str(),
            "PrimaryGroupID",
            gid_text.as_str(),
        ],
        None,
    ) {
        let _ = run(B, "dscl", &[".", "-delete", path.as_str()], None);
        return Err(e);
    }
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("gid", Value::Int(gid)),
    ]))
}

pub fn group_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "group_del";
    let plan = GroupDel::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    require_root(B)?;
    let Some(gid) = lookup_group(B, &plan.name)? else {
        return Err(not_found(B, "group", &plan.name));
    };
    guard_group(B, &plan.name, gid)?;
    // macOS would leave an account without a primary group, so refuse that.
    let primaries = records(B, "/Users", "PrimaryGroupID")?;
    if let Some((user, _)) = primaries.iter().find(|(_, g)| *g == gid) {
        return Err(bad_state(
            B,
            &format!("group {} is the primary group of account {user}", plan.name),
            "change that account's primary group first, or delete the account",
        ));
    }
    let path = group_path(&plan.name);
    run(B, "dscl", &[".", "-delete", path.as_str()], None)?;
    Ok(record(vec![("name", Value::Str(plan.name))]))
}

pub fn group_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "group_mod";
    let plan = GroupMod::parse(&args, &input)?;
    refuse_in_transaction(B)?;
    require_root(B)?;
    let Some(gid) = lookup_group(B, &plan.name)? else {
        return Err(not_found(B, "group", &plan.name));
    };
    guard_group(B, &plan.name, gid)?;
    let groups = records(B, "/Groups", "PrimaryGroupID")?;
    if let Some(new_name) = &plan.new_name {
        if groups
            .iter()
            .any(|(name, _)| name.as_str() == new_name.as_str())
        {
            return Err(bad_state(
                B,
                &format!("group {new_name} already exists"),
                "a different new_name; the existing group is not changed",
            ));
        }
    }
    if let Some(new_gid) = plan.gid {
        if groups
            .iter()
            .any(|(name, id)| *id == new_gid && name.as_str() != plan.name.as_str())
        {
            return Err(bad_state(
                B,
                &format!("gid {new_gid} is already used by another group"),
                "a different gid; the group is not renumbered",
            ));
        }
    }

    let old_path = group_path(&plan.name);
    // The gid goes first, while the record still has its old name.
    if let Some(new_gid) = plan.gid {
        let gid_text = new_gid.to_string();
        run(
            B,
            "dscl",
            &[
                ".",
                "-create",
                old_path.as_str(),
                "PrimaryGroupID",
                gid_text.as_str(),
            ],
            None,
        )?;
    }
    let mut final_name = plan.name.clone();
    if let Some(new_name) = &plan.new_name {
        run(
            B,
            "dscl",
            &[
                ".",
                "-change",
                old_path.as_str(),
                "RecordName",
                plan.name.as_str(),
                new_name.as_str(),
            ],
            None,
        )?;
        final_name = new_name.clone();
    }
    let mut changed: Vec<&str> = Vec::new();
    if plan.new_name.is_some() {
        changed.push("new_name");
    }
    if plan.gid.is_some() {
        changed.push("gid");
    }
    Ok(record(vec![
        ("name", Value::Str(final_name)),
        ("changed", strings(&changed)),
    ]))
}

// ---------------------------------------------------------------------------
// Operations shared by the entry points
// ---------------------------------------------------------------------------

fn set_lock(builtin: &str, args: Vec<Value>, input: Option<Value>, lock: bool) -> Result<Value> {
    let name = UserOnly::parse(builtin, &args, &input)?.name;
    refuse_in_transaction(builtin)?;
    require_root(builtin)?;
    let Some(account) = lookup_user(builtin, &name)? else {
        return Err(not_found(builtin, "user", &name));
    };
    guard_user(builtin, &name, account.uid)?;
    set_disabled(builtin, &name, lock)?;
    Ok(record(vec![
        ("name", Value::Str(name)),
        ("locked", Value::Bool(lock)),
    ]))
}

/// Writes a new account record, disables it, and adds it to its groups. Each
/// write that succeeds is recorded in `made` before the next one runs.
fn create_user(plan: &UserAdd, uid: i64, home: &str, shell: &str, made: &mut Made) -> Result<()> {
    const B: &str = "user_add";
    let path = user_path(&plan.name);
    let uid_text = uid.to_string();
    let gid_text = DEFAULT_GID.to_string();
    run(B, "dscl", &[".", "-create", path.as_str()], None)?;
    made.record = true;
    let mut attrs: Vec<(&str, &str)> = vec![
        ("UniqueID", uid_text.as_str()),
        ("PrimaryGroupID", gid_text.as_str()),
        ("NFSHomeDirectory", home),
        ("UserShell", shell),
        // "*" is macOS's marker for an account that cannot log on.
        ("Password", "*"),
    ];
    if let Some(comment) = &plan.comment {
        attrs.push(("RealName", comment.as_str()));
    }
    for (attr, value) in attrs {
        run(
            B,
            "dscl",
            &[".", "-create", path.as_str(), attr, value],
            None,
        )?;
    }
    set_disabled(B, &plan.name, true)?;
    for group in &plan.groups {
        add_member(B, group, &plan.name)?;
        made.groups.push(group.clone());
    }
    Ok(())
}

/// The tag Directory Service reads as "this account may not authenticate".
/// `pwpolicy -disableuser` exits 0 on current macOS without writing it or any
/// policy for an account with no password hash, so it is written directly.
const DISABLED_TAG: &str = ";DisabledUser;";

/// True when AuthenticationAuthority carries the disabled tag.
fn is_disabled(builtin: &str, name: &str) -> Result<bool> {
    let path = user_path(name);
    let out = spawn(
        builtin,
        "dscl",
        &[".", "-read", path.as_str(), "AuthenticationAuthority"],
    )?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.split_whitespace().any(|v| v == DISABLED_TAG) {
        return Ok(true);
    }
    // An account without the attribute exits nonzero with "No such key";
    // that is an enabled account, any other failure is not.
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() || stderr.contains("No such key") {
        return Ok(false);
    }
    Err(tool_failed(builtin, "dscl", out.status.code(), &stderr))
}

/// Adds or removes the disabled tag, then reads it back: a write the
/// directory did not keep is E_BAD_STATE, not a success.
fn set_disabled(builtin: &str, name: &str, disable: bool) -> Result<()> {
    if is_disabled(builtin, name)? != disable {
        let path = user_path(name);
        let verb = if disable { "-append" } else { "-delete" };
        run(
            builtin,
            "dscl",
            &[
                ".",
                verb,
                path.as_str(),
                "AuthenticationAuthority",
                DISABLED_TAG,
            ],
            None,
        )?;
    }
    if is_disabled(builtin, name)? != disable {
        return Err(bad_state(
            builtin,
            &format!(
                "dscl reported success but {name} is still {}",
                if disable { "enabled" } else { "disabled" }
            ),
            "check the account's AuthenticationAuthority with dscl . -read",
        ));
    }
    Ok(())
}

/// Best-effort undo of a failed user_add: memberships first, then the record.
fn rollback_user(name: &str, made: &Made) {
    for group in &made.groups {
        let _ = remove_member("user_add", group, name);
    }
    if made.record {
        let path = user_path(name);
        let _ = run("user_add", "dscl", &[".", "-delete", path.as_str()], None);
    }
}

/// Adds the user to a group unless it is already a member.
fn add_member(builtin: &str, group: &str, user: &str) -> Result<()> {
    if is_member(builtin, group, user)? {
        return Ok(());
    }
    run(
        builtin,
        "dseditgroup",
        &["-o", "edit", "-a", user, "-t", "user", group],
        None,
    )
}

fn remove_member(builtin: &str, group: &str, user: &str) -> Result<()> {
    run(
        builtin,
        "dseditgroup",
        &["-o", "edit", "-d", user, "-t", "user", group],
        None,
    )
}

/// True when the group's GroupMembership lists the user. A group with no
/// membership attribute yet makes dscl fail, which reads here as no members.
fn is_member(builtin: &str, group: &str, user: &str) -> Result<bool> {
    let path = group_path(group);
    let out = spawn(
        builtin,
        "dscl",
        &[".", "-read", path.as_str(), "GroupMembership"],
    )?;
    if !out.status.success() {
        return Ok(false);
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .any(|token| token == user))
}

// ---------------------------------------------------------------------------
// Guards and privilege
// ---------------------------------------------------------------------------

/// Refuses a system account, a reserved name, or the account running this
/// process or the one sudo ran for. Called with the account already looked up.
fn guard_user(builtin: &str, name: &str, uid: i64) -> Result<()> {
    if name.starts_with('_') || uid < HUMAN_ID_MIN {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: {name} has uid {uid}; accounts below {HUMAN_ID_MIN}, and names starting with '_', are system accounts and this builtin never changes them"
            ),
            "system accounts are managed with macOS's own tools; this builtin changes only accounts with uid >= 501",
        ));
    }
    if is_running_account(name, uid) {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: {name} is the account running this process, or the one sudo ran for"
            ),
            "changing it could lock this machine out; use another tool",
        ));
    }
    Ok(())
}

/// Refuses a system group, or a name starting with '_'.
fn guard_group(builtin: &str, name: &str, gid: i64) -> Result<()> {
    if name.starts_with('_') || gid < HUMAN_ID_MIN {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: group {name} has gid {gid}; groups below {HUMAN_ID_MIN}, and names starting with '_', are system groups and this builtin never changes or joins them"
            ),
            "system groups are managed with macOS's own tools",
        ));
    }
    Ok(())
}

/// The process runs as root under sudo, so its own uid cannot name the human
/// who asked; SUDO_USER and SUDO_UID do. Either one, or the process's real uid,
/// is the running account.
fn is_running_account(name: &str, uid: i64) -> bool {
    // SAFETY: getuid cannot fail and takes no arguments.
    let real = i64::from(unsafe { libc::getuid() });
    if uid == real {
        return true;
    }
    let sudo_user = std::env::var("SUDO_USER").is_ok_and(|s| s.as_str() == name);
    let sudo_uid = std::env::var("SUDO_UID")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        == Some(uid);
    sudo_user || sudo_uid
}

/// Directory Service writes need root. Refused as E_OS_DENIED before any write,
/// which is how Windows reports the same lack of rights.
fn require_root(builtin: &str) -> Result<()> {
    // SAFETY: geteuid cannot fail and takes no arguments.
    let euid = unsafe { libc::geteuid() };
    if euid == 0 {
        return Ok(());
    }
    Err(os_denied(
        builtin,
        format!("{builtin}: needs root, and this process runs as uid {euid}"),
        "run it with sudo",
    ))
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

struct Account {
    uid: i64,
    home: String,
}

/// The account's uid and home, or None when no such account exists. A record
/// with no UniqueID is not listed, so it reads as missing.
fn lookup_user(builtin: &str, name: &str) -> Result<Option<Account>> {
    let Some(uid) = records(builtin, "/Users", "UniqueID")?
        .into_iter()
        .find(|(n, _)| n.as_str() == name)
        .map(|(_, uid)| uid)
    else {
        return Ok(None);
    };
    let path = user_path(name);
    let home = read_attr(builtin, path.as_str(), "NFSHomeDirectory")?.unwrap_or_default();
    Ok(Some(Account { uid, home }))
}

/// The group's gid, or None when no such group exists.
fn lookup_group(builtin: &str, name: &str) -> Result<Option<i64>> {
    Ok(records(builtin, "/Groups", "PrimaryGroupID")?
        .into_iter()
        .find(|(n, _)| n.as_str() == name)
        .map(|(_, gid)| gid))
}

/// `dscl . -list <path> <attr>`: each record's name with the number its attr
/// holds. A record whose value is not a number is left out.
fn records(builtin: &str, path: &str, attr: &str) -> Result<Vec<(String, i64)>> {
    let out = spawn(builtin, "dscl", &[".", "-list", path, attr])?;
    if !out.status.success() {
        return Err(tool_failed(
            builtin,
            "dscl",
            out.status.code(),
            &String::from_utf8_lossy(&out.stderr),
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    Ok(text
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim_end().rsplit_once(char::is_whitespace)?;
            let id: i64 = value.parse().ok()?;
            Some((name.trim().to_string(), id))
        })
        .collect())
}

/// One attribute's single value, read with `dscl . -read`, or None when the
/// record does not carry it.
fn read_attr(builtin: &str, path: &str, attr: &str) -> Result<Option<String>> {
    let out = spawn(builtin, "dscl", &[".", "-read", path, attr])?;
    if !out.status.success() {
        return Err(tool_failed(
            builtin,
            "dscl",
            out.status.code(),
            &String::from_utf8_lossy(&out.stderr),
        ));
    }
    let prefix = format!("{attr}:");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    Ok(text
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(|value| value.trim().to_string()))
}

/// The smallest id at or above `start` that no record uses.
fn first_free(used: &[i64], start: i64) -> i64 {
    let mut id = start;
    while used.contains(&id) {
        id += 1;
    }
    id
}

fn user_path(name: &str) -> String {
    format!("/Users/{name}")
}

fn group_path(name: &str) -> String {
    format!("/Groups/{name}")
}

fn user_home(name: &str) -> String {
    format!("{HOME_ROOT}/{name}")
}

// ---------------------------------------------------------------------------
// Tools and errors
// ---------------------------------------------------------------------------

/// Runs a mutating tool; a nonzero exit becomes a coded failure. `secret` is
/// blanked out of stderr before it can reach an error.
fn run(builtin: &str, tool: &str, args: &[&str], secret: Option<&str>) -> Result<()> {
    let out = spawn(builtin, tool, args)?;
    if out.status.success() {
        return Ok(());
    }
    let mut stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if let Some(secret) = secret {
        if !secret.is_empty() {
            stderr = stderr.replace(secret, "<redacted>");
        }
    }
    Err(tool_failed(builtin, tool, out.status.code(), &stderr))
}

/// Starts a macOS tool by its absolute path, found in the usual system
/// directories, with argv only and under a deadline. The bare name is the
/// fallback when no candidate exists, so a missing tool is E_TOOL_MISSING.
fn spawn(builtin: &str, tool: &str, args: &[&str]) -> Result<Output> {
    let program = ["/usr/bin", "/usr/sbin", "/bin", "/sbin"]
        .iter()
        .map(|dir| format!("{dir}/{tool}"))
        .find(|candidate| Path::new(candidate).is_file())
        .unwrap_or_else(|| tool.to_string());
    let mut cmd = Command::new(program);
    cmd.args(args);
    crate::builtins::bounded_cmd_output(builtin, tool, cmd, TOOL_DEADLINE_SECS)
}

fn no_macos_meaning(builtin: &str, option: &str, why: &str) -> anyhow::Error {
    arg_err(format!(
        "{builtin}: option {option} has no macOS meaning; {why}; NOTHING WAS CHANGED"
    ))
}
