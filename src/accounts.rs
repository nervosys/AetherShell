//! Linux account and group administration: `user_*` and `group_*`.
//!
//! Effects are the standard shadow-utils tools (useradd, usermod, userdel,
//! chpasswd, groupadd, groupmod, groupdel), run with argv only and never
//! through a shell. Existence and ownership are read with getent. Passwords
//! reach chpasswd on stdin and nowhere else, and are blanked from any stderr
//! before it can reach an error.
//!
//! Safety rules, all checked before the tool runs:
//! - accounts and groups below uid/gid 1000 are never deleted, modified,
//!   re-passworded or locked here (root, daemon, sudo, wheel, docker, ...);
//! - the account running this process is refused as well;
//! - users are never added to a group below gid 1000, since those grant
//!   privileges;
//! - remove_home refuses a home with fewer than two path components ("/",
//!   "/home");
//! - inside a filesystem transaction every call is refused, because the
//!   journal cannot undo account changes.
//!
//! On platforms other than Linux every call validates its arguments, then
//! refuses with E_UNIMPLEMENTED before any tool is started.

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::process::Output;

use crate::safety::{arg_err, bad_arg, bad_state, not_found, policy_deny, tool_failed};
use crate::value::Value;

/// uids and gids below this are system accounts and groups.
const SYSTEM_LIMIT: i64 = 1000;
const GID_MIN: i64 = 1000;
const GID_MAX: i64 = 60000;
const PASSWORD_MAX: usize = 1024;
const COMMENT_MAX: usize = 256;
const PATH_MAX: usize = 4096;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub fn user_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserAdd::parse(&args, &input)?;
    refuse_in_transaction("user_add")?;
    if lookup_user("user_add", &plan.name)?.is_some() {
        return Err(bad_state(
            "user_add",
            &format!("user {} already exists", plan.name),
            "a different name; the existing account is not changed",
        ));
    }
    for group in &plan.groups {
        let Some(g) = lookup_group("user_add", group)? else {
            return Err(not_found("user_add", "group", group));
        };
        guard_group("user_add", group, g.gid)?;
    }
    if let Some(shell) = &plan.shell {
        if !Path::new(shell).is_file() {
            return Err(arg_err(format!(
                "user_add: shell {shell} is not an existing file"
            )));
        }
    }

    let mut argv: Vec<String> = vec![if plan.home { "-m" } else { "-M" }.to_string()];
    if plan.system {
        argv.push("-r".to_string());
    }
    if let Some(shell) = &plan.shell {
        argv.push("-s".to_string());
        argv.push(shell.clone());
    }
    if !plan.groups.is_empty() {
        argv.push("-G".to_string());
        argv.push(plan.groups.join(","));
    }
    if let Some(comment) = &plan.comment {
        argv.push("-c".to_string());
        argv.push(comment.clone());
    }
    argv.push(plan.name.clone());
    run("user_add", "useradd", &as_refs(&argv), None, None)?;

    let Some(pw) = lookup_user("user_add", &plan.name)? else {
        return Err(tool_failed(
            "user_add",
            "getent",
            Some(0),
            "the account was missing after useradd reported success",
        ));
    };
    let created_home = plan.home && Path::new(&pw.home).is_dir();
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("uid", Value::Int(pw.uid)),
        ("gid", Value::Int(pw.gid)),
        ("home", Value::Str(pw.home)),
        ("shell", Value::Str(pw.shell)),
        ("created_home", Value::Bool(created_home)),
    ]))
}

pub fn user_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserDel::parse(&args, &input)?;
    refuse_in_transaction("user_del")?;
    let Some(pw) = lookup_user("user_del", &plan.name)? else {
        return Err(not_found("user_del", "user", &plan.name));
    };
    guard_user("user_del", &plan.name, &pw)?;
    if plan.remove_home && !home_is_removable(&pw.home) {
        return Err(policy_deny(
            "user_del",
            format!(
                "user_del: refusing to remove home {}; a home directory must have at least two path components, such as /home/name",
                pw.home
            ),
            "remove the directory yourself after checking what it holds",
        ));
    }
    let mut argv: Vec<&str> = Vec::new();
    if plan.remove_home {
        argv.push("-r");
    }
    argv.push(&plan.name);
    run("user_del", "userdel", &argv, None, None)?;
    let removed_home = plan.remove_home && !Path::new(&pw.home).exists();
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("removed_home", Value::Bool(removed_home)),
    ]))
}

pub fn user_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserMod::parse(&args, &input)?;
    refuse_in_transaction("user_mod")?;
    let Some(pw) = lookup_user("user_mod", &plan.name)? else {
        return Err(not_found("user_mod", "user", &plan.name));
    };
    guard_user("user_mod", &plan.name, &pw)?;
    for group in plan
        .groups
        .iter()
        .flatten()
        .chain(plan.add_groups.iter().flatten())
    {
        let Some(g) = lookup_group("user_mod", group)? else {
            return Err(not_found("user_mod", "group", group));
        };
        guard_group("user_mod", group, g.gid)?;
    }
    if let Some(shell) = &plan.shell {
        if !Path::new(shell).is_file() {
            return Err(arg_err(format!(
                "user_mod: shell {shell} is not an existing file"
            )));
        }
    }

    let mut argv: Vec<String> = Vec::new();
    let mut changed: Vec<&str> = Vec::new();
    if let Some(shell) = &plan.shell {
        argv.extend(["-s".to_string(), shell.clone()]);
        changed.push("shell");
    }
    if let Some(comment) = &plan.comment {
        argv.extend(["-c".to_string(), comment.clone()]);
        changed.push("comment");
    }
    if let Some(home) = &plan.home {
        if plan.move_home {
            argv.push("-m".to_string());
        }
        argv.extend(["-d".to_string(), home.clone()]);
        changed.push("home");
    }
    if let Some(groups) = &plan.groups {
        argv.extend(["-G".to_string(), groups.join(",")]);
        changed.push("groups");
    }
    if let Some(groups) = &plan.add_groups {
        argv.extend(["-a".to_string(), "-G".to_string(), groups.join(",")]);
        changed.push("add_groups");
    }
    argv.push(plan.name.clone());
    run("user_mod", "usermod", &as_refs(&argv), None, None)?;

    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("changed", strings(&changed)),
    ]))
}

pub fn user_passwd(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = UserPasswd::parse(&args, &input)?;
    refuse_in_transaction("user_passwd")?;
    let Some(pw) = lookup_user("user_passwd", &plan.name)? else {
        return Err(not_found("user_passwd", "user", &plan.name));
    };
    guard_user("user_passwd", &plan.name, &pw)?;
    // The password travels on stdin only, as chpasswd's "name:password" line.
    let payload = format!("{}:{}\n", plan.name, plan.password);
    run(
        "user_passwd",
        "chpasswd",
        &[],
        Some(payload.as_str()),
        Some(plan.password.as_str()),
    )?;
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
    if lookup_group("group_add", &plan.name)?.is_some() {
        return Err(bad_state(
            "group_add",
            &format!("group {} already exists", plan.name),
            "a different name; the existing group is not changed",
        ));
    }
    let mut argv: Vec<String> = Vec::new();
    if plan.system {
        argv.push("-r".to_string());
    }
    if let Some(gid) = plan.gid {
        argv.extend(["-g".to_string(), gid.to_string()]);
    }
    argv.push(plan.name.clone());
    run("group_add", "groupadd", &as_refs(&argv), None, None)?;

    let Some(group) = lookup_group("group_add", &plan.name)? else {
        return Err(tool_failed(
            "group_add",
            "getent",
            Some(0),
            "the group was missing after groupadd reported success",
        ));
    };
    Ok(record(vec![
        ("name", Value::Str(plan.name)),
        ("gid", Value::Int(group.gid)),
    ]))
}

pub fn group_del(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = GroupDel::parse(&args, &input)?;
    refuse_in_transaction("group_del")?;
    let Some(group) = lookup_group("group_del", &plan.name)? else {
        return Err(not_found("group_del", "group", &plan.name));
    };
    guard_group("group_del", &plan.name, group.gid)?;
    run("group_del", "groupdel", &[plan.name.as_str()], None, None)?;
    Ok(record(vec![("name", Value::Str(plan.name))]))
}

pub fn group_mod(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let plan = GroupMod::parse(&args, &input)?;
    refuse_in_transaction("group_mod")?;
    let Some(group) = lookup_group("group_mod", &plan.name)? else {
        return Err(not_found("group_mod", "group", &plan.name));
    };
    guard_group("group_mod", &plan.name, group.gid)?;
    if let Some(new_name) = &plan.new_name {
        if lookup_group("group_mod", new_name)?.is_some() {
            return Err(bad_state(
                "group_mod",
                &format!("group {new_name} already exists"),
                "a different new_name; the existing group is not changed",
            ));
        }
    }
    let mut argv: Vec<String> = Vec::new();
    let mut changed: Vec<&str> = Vec::new();
    if let Some(new_name) = &plan.new_name {
        argv.extend(["-n".to_string(), new_name.clone()]);
        changed.push("new_name");
    }
    if let Some(gid) = plan.gid {
        argv.extend(["-g".to_string(), gid.to_string()]);
        changed.push("gid");
    }
    argv.push(plan.name.clone());
    run("group_mod", "groupmod", &as_refs(&argv), None, None)?;

    let final_name = plan.new_name.clone().unwrap_or(plan.name);
    Ok(record(vec![
        ("name", Value::Str(final_name)),
        ("changed", strings(&changed)),
    ]))
}

fn set_lock(builtin: &str, args: Vec<Value>, input: Option<Value>, lock: bool) -> Result<Value> {
    let name = UserOnly::parse(builtin, &args, &input)?.name;
    refuse_in_transaction(builtin)?;
    let Some(pw) = lookup_user(builtin, &name)? else {
        return Err(not_found(builtin, "user", &name));
    };
    guard_user(builtin, &name, &pw)?;
    run(
        builtin,
        "usermod",
        &[if lock { "-L" } else { "-U" }, name.as_str()],
        None,
        None,
    )?;
    Ok(record(vec![
        ("name", Value::Str(name)),
        ("locked", Value::Bool(lock)),
    ]))
}

// ---------------------------------------------------------------------------
// Argument validation (platform independent, no effects)
// ---------------------------------------------------------------------------

struct UserAdd {
    name: String,
    home: bool,
    system: bool,
    shell: Option<String>,
    comment: Option<String>,
    groups: Vec<String>,
}

impl UserAdd {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        check_call("user_add", args, input, 2)?;
        let name = name_arg("user_add", &args[0], "user name")?;
        let rec = options(
            "user_add",
            args.get(1),
            &["home", "shell", "groups", "system", "comment"],
        )?;
        let shell = str_opt("user_add", rec, "shell")?;
        if let Some(shell) = &shell {
            path_text("user_add", "shell", shell)?;
        }
        let comment = str_opt("user_add", rec, "comment")?;
        if let Some(comment) = &comment {
            plain_text("user_add", "comment", comment, COMMENT_MAX)?;
        }
        Ok(Self {
            name,
            home: bool_opt("user_add", rec, "home")?.unwrap_or(true),
            system: bool_opt("user_add", rec, "system")?.unwrap_or(false),
            shell,
            comment,
            groups: names_opt("user_add", rec, "groups")?.unwrap_or_default(),
        })
    }
}

struct UserDel {
    name: String,
    remove_home: bool,
}

impl UserDel {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        check_call("user_del", args, input, 2)?;
        let name = name_arg("user_del", &args[0], "user name")?;
        let rec = options("user_del", args.get(1), &["remove_home"])?;
        Ok(Self {
            name,
            remove_home: bool_opt("user_del", rec, "remove_home")?.unwrap_or(false),
        })
    }
}

struct UserMod {
    name: String,
    shell: Option<String>,
    comment: Option<String>,
    home: Option<String>,
    move_home: bool,
    groups: Option<Vec<String>>,
    add_groups: Option<Vec<String>>,
}

impl UserMod {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        const B: &str = "user_mod";
        check_call(B, args, input, 2)?;
        if args.len() != 2 {
            return Err(arg_err("user_mod takes a user name and an options Record"));
        }
        let name = name_arg(B, &args[0], "user name")?;
        let rec = options(
            B,
            args.get(1),
            &[
                "shell",
                "comment",
                "home",
                "move_home",
                "groups",
                "add_groups",
            ],
        )?;
        let shell = str_opt(B, rec, "shell")?;
        if let Some(shell) = &shell {
            path_text(B, "shell", shell)?;
        }
        let comment = str_opt(B, rec, "comment")?;
        if let Some(comment) = &comment {
            plain_text(B, "comment", comment, COMMENT_MAX)?;
        }
        let home = str_opt(B, rec, "home")?;
        if let Some(home) = &home {
            path_text(B, "home", home)?;
        }
        let move_home = bool_opt(B, rec, "move_home")?.unwrap_or(false);
        let groups = names_opt(B, rec, "groups")?;
        let add_groups = names_opt(B, rec, "add_groups")?;

        if move_home && home.is_none() {
            return Err(arg_err("user_mod: move_home needs home"));
        }
        if groups.is_some() && add_groups.is_some() {
            return Err(arg_err(
                "user_mod: groups replaces the supplementary groups and add_groups appends to them; pass one of them",
            ));
        }
        if matches!(&add_groups, Some(list) if list.is_empty()) {
            return Err(arg_err("user_mod: add_groups must name at least one group"));
        }
        if shell.is_none()
            && comment.is_none()
            && home.is_none()
            && groups.is_none()
            && add_groups.is_none()
        {
            return Err(arg_err(
                "user_mod needs at least one of shell, comment, home, groups, add_groups",
            ));
        }
        Ok(Self {
            name,
            shell,
            comment,
            home,
            move_home,
            groups,
            add_groups,
        })
    }
}

struct UserPasswd {
    name: String,
    password: String,
}

impl UserPasswd {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        const B: &str = "user_passwd";
        check_call(B, args, input, 2)?;
        if args.len() != 2 {
            return Err(arg_err("user_passwd takes a user name and a password"));
        }
        let name = name_arg(B, &args[0], "user name")?;
        let password = match &args[1] {
            Value::Str(p)
                if !p.is_empty()
                    && p.len() <= PASSWORD_MAX
                    && !p.chars().any(|c| c == ':' || c.is_control()) =>
            {
                p.clone()
            }
            Value::Str(_) => {
                return Err(arg_err(format!(
                "{B}: password must be 1 to {PASSWORD_MAX} bytes with no ':' or control characters"
            )))
            }
            other => return Err(bad_arg(B, "password: String", other.type_name())),
        };
        Ok(Self { name, password })
    }
}

struct UserOnly {
    name: String,
}

impl UserOnly {
    fn parse(builtin: &str, args: &[Value], input: &Option<Value>) -> Result<Self> {
        check_call(builtin, args, input, 1)?;
        Ok(Self {
            name: name_arg(builtin, &args[0], "user name")?,
        })
    }
}

struct GroupAdd {
    name: String,
    gid: Option<i64>,
    system: bool,
}

impl GroupAdd {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        check_call("group_add", args, input, 2)?;
        let name = name_arg("group_add", &args[0], "group name")?;
        let rec = options("group_add", args.get(1), &["gid", "system"])?;
        let gid = int_opt("group_add", rec, "gid")?;
        if let Some(gid) = gid {
            gid_range("group_add", gid)?;
        }
        let system = bool_opt("group_add", rec, "system")?.unwrap_or(false);
        if system && gid.is_some() {
            return Err(arg_err(
                "group_add: system takes the next free system gid; pass gid or system, not both",
            ));
        }
        Ok(Self { name, gid, system })
    }
}

struct GroupDel {
    name: String,
}

impl GroupDel {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        check_call("group_del", args, input, 1)?;
        Ok(Self {
            name: name_arg("group_del", &args[0], "group name")?,
        })
    }
}

struct GroupMod {
    name: String,
    new_name: Option<String>,
    gid: Option<i64>,
}

impl GroupMod {
    fn parse(args: &[Value], input: &Option<Value>) -> Result<Self> {
        const B: &str = "group_mod";
        check_call(B, args, input, 2)?;
        if args.len() != 2 {
            return Err(arg_err(
                "group_mod takes a group name and an options Record",
            ));
        }
        let name = name_arg(B, &args[0], "group name")?;
        let rec = options(B, args.get(1), &["new_name", "gid"])?;
        let new_name = match str_opt(B, rec, "new_name")? {
            Some(new) if valid_name(&new) => Some(new),
            Some(_) => {
                return Err(arg_err(format!(
                    "{B}: new_name must match ^[a-z_][a-z0-9_-]{{0,31}}$"
                )))
            }
            None => None,
        };
        let gid = int_opt(B, rec, "gid")?;
        if let Some(gid) = gid {
            gid_range(B, gid)?;
        }
        if new_name.is_none() && gid.is_none() {
            return Err(arg_err("group_mod needs new_name, gid, or both"));
        }
        if new_name.as_deref() == Some(name.as_str()) {
            return Err(arg_err(format!(
                "{B}: new_name is the group's current name"
            )));
        }
        Ok(Self {
            name,
            new_name,
            gid,
        })
    }
}

fn check_call(builtin: &str, args: &[Value], input: &Option<Value>, max: usize) -> Result<()> {
    if input.is_some() {
        return Err(arg_err(format!("{builtin} accepts no pipeline input")));
    }
    if args.is_empty() || args.len() > max {
        return Err(arg_err(format!(
            "{builtin} takes a name and at most {} further argument(s)",
            max - 1
        )));
    }
    Ok(())
}

/// `^[a-z_][a-z0-9_-]{0,31}$`, the shape useradd and groupadd accept.
fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || b.len() > 32 {
        return false;
    }
    (b[0].is_ascii_lowercase() || b[0] == b'_')
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

fn name_arg(builtin: &str, value: &Value, what: &str) -> Result<String> {
    match value {
        Value::Str(s) if valid_name(s) => Ok(s.clone()),
        Value::Str(_) => Err(arg_err(format!(
            "{builtin}: {what} must match ^[a-z_][a-z0-9_-]{{0,31}}$"
        ))),
        other => Err(bad_arg(
            builtin,
            &format!("{what}: String"),
            other.type_name(),
        )),
    }
}

fn options<'a>(
    builtin: &str,
    value: Option<&'a Value>,
    allowed: &[&str],
) -> Result<Option<&'a BTreeMap<String, Value>>> {
    match value {
        None => Ok(None),
        Some(Value::Record(rec)) => {
            if let Some(key) = rec.keys().find(|k| !allowed.contains(&k.as_str())) {
                return Err(arg_err(format!(
                    "{builtin}: unknown option {key}; expected one of {}",
                    allowed.join(", ")
                )));
            }
            Ok(Some(rec))
        }
        Some(other) => Err(bad_arg(builtin, "options: Record", other.type_name())),
    }
}

fn bool_opt(
    builtin: &str,
    rec: Option<&BTreeMap<String, Value>>,
    key: &str,
) -> Result<Option<bool>> {
    match rec.and_then(|r| r.get(key)) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(bad_arg(builtin, &format!("{key}: Bool"), other.type_name())),
    }
}

fn str_opt(
    builtin: &str,
    rec: Option<&BTreeMap<String, Value>>,
    key: &str,
) -> Result<Option<String>> {
    match rec.and_then(|r| r.get(key)) {
        None => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        Some(other) => Err(bad_arg(
            builtin,
            &format!("{key}: String"),
            other.type_name(),
        )),
    }
}

fn int_opt(builtin: &str, rec: Option<&BTreeMap<String, Value>>, key: &str) -> Result<Option<i64>> {
    match rec.and_then(|r| r.get(key)) {
        None => Ok(None),
        Some(Value::Int(n)) => Ok(Some(*n)),
        Some(other) => Err(bad_arg(builtin, &format!("{key}: Int"), other.type_name())),
    }
}

fn names_opt(
    builtin: &str,
    rec: Option<&BTreeMap<String, Value>>,
    key: &str,
) -> Result<Option<Vec<String>>> {
    let Some(value) = rec.and_then(|r| r.get(key)) else {
        return Ok(None);
    };
    let Value::Array(items) = value else {
        return Err(bad_arg(
            builtin,
            &format!("{key}: Array<String>"),
            value.type_name(),
        ));
    };
    let mut names: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let name = name_arg(builtin, item, &format!("{key} entry"))?;
        if names.contains(&name) {
            return Err(arg_err(format!("{builtin}: {key} lists {name} twice")));
        }
        names.push(name);
    }
    Ok(Some(names))
}

fn gid_range(builtin: &str, gid: i64) -> Result<()> {
    if (GID_MIN..=GID_MAX).contains(&gid) {
        Ok(())
    } else {
        Err(arg_err(format!(
            "{builtin}: gid must be in {GID_MIN}..={GID_MAX}, got {gid}"
        )))
    }
}

/// A text field stored in a colon-separated database: no ':' and no control
/// characters, which would split or corrupt the entry.
fn plain_text(builtin: &str, what: &str, value: &str, max: usize) -> Result<()> {
    if value.len() > max || value.chars().any(|c| c == ':' || c.is_control()) {
        return Err(arg_err(format!(
            "{builtin}: {what} must be at most {max} bytes with no ':' or control characters"
        )));
    }
    Ok(())
}

fn path_text(builtin: &str, what: &str, value: &str) -> Result<()> {
    if !value.starts_with('/') {
        return Err(arg_err(format!(
            "{builtin}: {what} must be an absolute path"
        )));
    }
    plain_text(builtin, what, value, PATH_MAX)
}

fn as_refs(argv: &[String]) -> Vec<&str> {
    argv.iter().map(String::as_str).collect()
}

fn strings(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|s| Value::Str((*s).to_string())).collect())
}

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Privilege and transaction guards
// ---------------------------------------------------------------------------

fn refuse_in_transaction(builtin: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: account changes inside a filesystem transaction cannot be rolled back; NOTHING WAS CHANGED"
            ),
            "commit or roll back the open transaction, then run this outside it",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn current_uid() -> i64 {
    // SAFETY: getuid cannot fail and takes no arguments.
    i64::from(unsafe { libc::getuid() })
}

#[cfg(not(unix))]
fn current_uid() -> i64 {
    -1
}

fn guard_user(builtin: &str, name: &str, pw: &Passwd) -> Result<()> {
    if pw.uid < SYSTEM_LIMIT {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: {name} has uid {}; accounts below {SYSTEM_LIMIT} are system accounts and this builtin never changes them",
                pw.uid
            ),
            "system accounts are managed with the system's own tools; this builtin changes only accounts with uid >= 1000",
        ));
    }
    if pw.uid == current_uid() {
        return Err(policy_deny(
            builtin,
            format!("{builtin}: {name} is the account running this process"),
            "changing it could lock this machine out; use another tool",
        ));
    }
    Ok(())
}

fn guard_group(builtin: &str, name: &str, gid: i64) -> Result<()> {
    if gid < SYSTEM_LIMIT {
        return Err(policy_deny(
            builtin,
            format!(
                "{builtin}: group {name} has gid {gid}; groups below {SYSTEM_LIMIT} grant privileges and this builtin never changes or joins them"
            ),
            "system groups are managed with the system's own tools",
        ));
    }
    Ok(())
}

/// A home directory is removable only when it is at least two components
/// deep. `/`, `/home` and `/root` are never removed recursively.
fn home_is_removable(home: &str) -> bool {
    let path = Path::new(home);
    path.is_absolute()
        && path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
            >= 2
}

// ---------------------------------------------------------------------------
// Tools and lookups
// ---------------------------------------------------------------------------

struct Passwd {
    uid: i64,
    gid: i64,
    home: String,
    shell: String,
}

struct Group {
    gid: i64,
}

fn lookup_user(builtin: &str, name: &str) -> Result<Option<Passwd>> {
    let out = spawn(builtin, "getent", &["passwd", name], None)?;
    let text = match out.status.code() {
        Some(0) => String::from_utf8_lossy(&out.stdout).into_owned(),
        // getent exits 2 for a name it does not know.
        Some(2) => return Ok(None),
        code => {
            return Err(tool_failed(
                builtin,
                "getent",
                code,
                &String::from_utf8_lossy(&out.stderr),
            ))
        }
    };
    let bad = || tool_failed(builtin, "getent", Some(0), "unexpected passwd entry");
    let fields: Vec<&str> = text.lines().next().unwrap_or_default().split(':').collect();
    if fields.len() < 7 {
        return Err(bad());
    }
    Ok(Some(Passwd {
        uid: fields[2].parse().map_err(|_| bad())?,
        gid: fields[3].parse().map_err(|_| bad())?,
        home: fields[5].to_string(),
        shell: fields[6].to_string(),
    }))
}

fn lookup_group(builtin: &str, name: &str) -> Result<Option<Group>> {
    let out = spawn(builtin, "getent", &["group", name], None)?;
    let text = match out.status.code() {
        Some(0) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Some(2) => return Ok(None),
        code => {
            return Err(tool_failed(
                builtin,
                "getent",
                code,
                &String::from_utf8_lossy(&out.stderr),
            ))
        }
    };
    let bad = || tool_failed(builtin, "getent", Some(0), "unexpected group entry");
    let fields: Vec<&str> = text.lines().next().unwrap_or_default().split(':').collect();
    if fields.len() < 3 {
        return Err(bad());
    }
    Ok(Some(Group {
        gid: fields[2].parse().map_err(|_| bad())?,
    }))
}

/// Runs a mutating tool; a nonzero exit becomes a coded failure. `secret` is
/// blanked out of stderr before it can reach an error.
fn run(
    builtin: &str,
    tool: &str,
    args: &[&str],
    stdin: Option<&str>,
    secret: Option<&str>,
) -> Result<()> {
    let out = spawn(builtin, tool, args, stdin)?;
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

/// Starts `tool` with argv only, never through a shell. The optional stdin
/// payload is the only channel for secrets.
#[cfg(target_os = "linux")]
pub(super) fn spawn(
    builtin: &str,
    tool: &str,
    args: &[&str],
    stdin: Option<&str>,
) -> Result<Output> {
    // Debian and most distributions install the shadow tools under /usr/sbin,
    // which a non-root PATH often omits. Prefer the absolute path, else PATH.
    const TOOL_DIRS: [&str; 4] = ["/usr/sbin", "/sbin", "/usr/bin", "/bin"];
    let path = TOOL_DIRS
        .iter()
        .map(|dir| format!("{dir}/{tool}"))
        .find(|candidate| Path::new(candidate).is_file())
        .unwrap_or_else(|| tool.to_string());
    // A tool that exits without reading its input is reported by its status.
    crate::builtins::run_output(
        builtin,
        tool,
        &path,
        args,
        |_| {},
        stdin.map(|payload| payload.as_bytes().to_vec()),
        ACCOUNT_TOOL_DEADLINE_SECS,
        ACCOUNT_TOOL_MAX_OUTPUT,
    )
}

/// Deadline for one account or group tool, or getent lookup.
#[cfg(target_os = "linux")]
const ACCOUNT_TOOL_DEADLINE_SECS: u64 = 60;
/// The most output one account tool call may print in either stream.
#[cfg(target_os = "linux")]
const ACCOUNT_TOOL_MAX_OUTPUT: usize = crate::builtins::TOOL_MAX_JSON;

#[cfg(not(target_os = "linux"))]
pub(super) fn spawn(
    builtin: &str,
    _tool: &str,
    _args: &[&str],
    _stdin: Option<&str>,
) -> Result<Output> {
    Err(crate::safety::unimplemented(
        builtin,
        "account and group administration is implemented on Linux only in 13.0.0; NOTHING WAS CHANGED",
        "run it on Linux, or use this platform's own account tool",
    ))
}
