//! Linux POSIX ACLs and mounts: `acl_get`, `acl_set`, `fs_mount` and
//! `fs_unmount`.
//!
//! Every builtin checks its arguments before it touches the system, then runs
//! one tool (`getfacl`, `setfacl`, `icacls`, `mount` or `umount`) with an argv vector.
//! No shell is involved and no caller string is interpolated into a command
//! line. Off Linux, the mount builtins are refused with E_UNIMPLEMENTED after
//! their arguments are checked. On Windows, `acl_get` and `acl_set` run `icacls`,
//! and the mount builtins are refused the same way. A refusal changes nothing.
//!
//! ACL changes are not recorded by the undo journal. A call inside a
//! filesystem transaction is refused, because a rollback cannot restore the
//! permission metadata or mount state that these builtins change.
//!
//! `fs_unmount` refuses the mount points `/`, `/proc`, `/sys`, `/dev`, `/run`,
//! `/boot`, `/usr`, `/etc`, `/var` and `/home` by exact path, after the path
//! is canonicalized. `fs_mount` refuses the same paths as a target.
use crate::safety::reject_option_like;
use crate::{safety, value::Value};
use anyhow::Result;
use std::collections::BTreeMap;

/// Mount points that `fs_unmount` refuses, and that `fs_mount` refuses as a target.
#[cfg(target_os = "linux")]
const PROTECTED: &[&str] = &[
    "/", "/proc", "/sys", "/dev", "/run", "/boot", "/usr", "/etc", "/var", "/home",
];

/// A validated `acl_set` request. `arg` is the normalized entry handed to
/// `setfacl`: `tag:qualifier:perms` for a change, `tag:qualifier` for a removal.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct AclSpec {
    arg: String,
    default: bool,
    remove: bool,
}

/// A validated `fs_mount` request, before the filesystem checks.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct MountSpec {
    fstype: Option<String>,
    readonly: bool,
    bind: bool,
    options: Vec<String>,
}

pub(crate) fn acl_get(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_args("acl_get", &args, input.is_some(), 1, 1, "one path String")?;
    let path = text_arg("acl_get", &args[0], "path")?;
    reject_option_like("acl_get", std::slice::from_ref(&path))?;
    refuse_in_transaction("acl_get")?;
    sys::acl_get(&path)
}

pub(crate) fn acl_set(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_args(
        "acl_set",
        &args,
        input.is_some(),
        2,
        3,
        "a path String, an entry String and an optional options Record",
    )?;
    let path = text_arg("acl_set", &args[0], "path")?;
    reject_option_like("acl_set", std::slice::from_ref(&path))?;
    let entry = text_arg("acl_set", &args[1], "entry")?;
    let options = options_map("acl_set", args.get(2), &["default", "remove"])?;
    let default = flag("acl_set", &options, "default")?;
    let remove = flag("acl_set", &options, "remove")?;
    let spec = AclSpec {
        arg: acl_entry(&entry, remove, default)?,
        default,
        remove,
    };
    refuse_in_transaction("acl_set")?;
    sys::acl_set(&path, &spec)
}

pub(crate) fn fs_mount(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_args(
        "fs_mount",
        &args,
        input.is_some(),
        2,
        3,
        "a source path, a target path and an optional options Record",
    )?;
    let source = absolute_arg("fs_mount", &args[0], "source")?;
    let target = absolute_arg("fs_mount", &args[1], "target")?;
    let spec = mount_spec(args.get(2))?;
    refuse_in_transaction("fs_mount")?;
    sys::fs_mount(&source, &target, &spec)
}

pub(crate) fn fs_unmount(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    check_args(
        "fs_unmount",
        &args,
        input.is_some(),
        1,
        2,
        "a target path and an optional options Record",
    )?;
    let target = absolute_arg("fs_unmount", &args[0], "target")?;
    let options = options_map("fs_unmount", args.get(1), &["lazy"])?;
    let lazy = flag("fs_unmount", &options, "lazy")?;
    refuse_in_transaction("fs_unmount")?;
    sys::fs_unmount(&target, lazy)
}

// ── Argument validation (every platform; no system access) ────────────────

fn check_args(
    builtin: &str,
    args: &[Value],
    has_input: bool,
    min: usize,
    max: usize,
    shape: &str,
) -> Result<()> {
    if has_input || args.len() < min || args.len() > max {
        return Err(safety::arg_err(format!(
            "{builtin} takes {shape} and no pipeline input"
        )));
    }
    Ok(())
}

/// The value as the caller wrote it, for an error message.
fn got(value: &Value) -> String {
    match value {
        Value::Str(text) => format!("{text:?}"),
        other => other.type_name().to_string(),
    }
}

fn text_arg(builtin: &str, value: &Value, role: &str) -> Result<String> {
    match value {
        Value::Str(text) if text.is_empty() => Err(safety::arg_err(format!(
            "{builtin}: the {role} must not be empty"
        ))),
        Value::Str(text) if text.contains('\0') => Err(safety::arg_err(format!(
            "{builtin}: the {role} must not contain a NUL byte"
        ))),
        Value::Str(text) => Ok(text.clone()),
        other => Err(safety::bad_arg(
            builtin,
            &format!("{role} String"),
            &got(other),
        )),
    }
}

/// A path that must begin with `/`. The check is textual, so the same call is
/// refused with E_BAD_ARG on every platform.
fn absolute_arg(builtin: &str, value: &Value, role: &str) -> Result<String> {
    let text = text_arg(builtin, value, role)?;
    if !text.starts_with('/') {
        return Err(safety::bad_arg(
            builtin,
            &format!("absolute {role} path beginning with /"),
            &format!("{text:?}"),
        ));
    }
    Ok(text)
}

/// The optional options Record, with every key checked against `allowed`.
fn options_map(
    builtin: &str,
    value: Option<&Value>,
    allowed: &[&str],
) -> Result<BTreeMap<String, Value>> {
    match value {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(Value::Record(map)) => {
            for key in map.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(safety::arg_err(format!(
                        "{builtin}: unknown option `{key}`; allowed keys are {}",
                        allowed.join(", ")
                    )));
                }
            }
            Ok(map.clone())
        }
        Some(other) => Err(safety::bad_arg(builtin, "options Record", &got(other))),
    }
}

fn flag(builtin: &str, options: &BTreeMap<String, Value>, key: &str) -> Result<bool> {
    match options.get(key) {
        None => Ok(false),
        Some(Value::Bool(set)) => Ok(*set),
        Some(other) => Err(safety::bad_arg(
            builtin,
            &format!("{key} Bool"),
            &got(other),
        )),
    }
}

fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Validate an `acl_set` entry and return its normalized form.
///
/// A change is `tag:qualifier:perms` with a three-character perms string over
/// `rwx-`, such as `u:nobody:r--` or `u::rw-`. A removal (`remove: true`) is
/// `tag:qualifier` with no permissions, and names a user or group entry only.
/// Tags are `u`, `g`, `m` or `o`, or the spelled-out `user`, `group`, `mask`
/// or `other`. A mask or other entry has no qualifier. Numeric IDs are refused.
#[cfg(not(windows))]
fn acl_entry(entry: &str, remove: bool, _default: bool) -> Result<String> {
    const SET: &str = "an entry like u:name:r--, u::rw- or o::r-- (tag, qualifier, perms)";
    const REMOVE: &str = "an entry like u:name or g:name (tag and qualifier, no perms)";
    let parts: Vec<&str> = entry.split(':').collect();
    let letter = |tag: &str| match tag {
        "u" | "user" => Some('u'),
        "g" | "group" => Some('g'),
        "m" | "mask" => Some('m'),
        "o" | "other" => Some('o'),
        _ => None,
    };
    match parts.as_slice() {
        [tag, name] if remove => match letter(tag) {
            Some(c @ ('u' | 'g')) if is_name(name) => Ok(format!("{c}:{name}")),
            _ => Err(safety::bad_arg("acl_set", REMOVE, &format!("{entry:?}"))),
        },
        [tag, qualifier, perms] if !remove => {
            let valid_qualifier = qualifier.is_empty() || is_name(qualifier);
            let unnamed_tag = !matches!(letter(tag), Some('m' | 'o')) || qualifier.is_empty();
            let valid_perms = perms.len() == 3 && perms.chars().all(|c| "rwx-".contains(c));
            match letter(tag) {
                Some(c) if valid_qualifier && unnamed_tag && valid_perms => {
                    Ok(format!("{c}:{qualifier}:{perms}"))
                }
                _ => Err(safety::bad_arg("acl_set", SET, &format!("{entry:?}"))),
            }
        }
        _ => Err(safety::bad_arg(
            "acl_set",
            if remove { REMOVE } else { SET },
            &format!("{entry:?}"),
        )),
    }
}

/// Validate an `acl_set` entry on Windows and return the icacls argument.
///
/// A grant is `principal:rights`, such as `Users:F` or `BUILTIN\Users:RX,D`. It
/// is normalized to `principal:(rights)`, the form icacls takes. Rights are one
/// simple right (F, M, RX, R, W or D) or a comma-separated list of specific
/// rights from the icacls list. With remove, the entry is a bare principal, and
/// every ACE for it is removed. The Linux `tag:qualifier:perms` form does not
/// parse as a grant, so it is refused. `default` is refused, because icacls
/// inheritance flags are not taken.
#[cfg(windows)]
fn acl_entry(entry: &str, remove: bool, default: bool) -> Result<String> {
    const GRANT: &str = "an entry like Users:F or BUILTIN\\Users:RX,D (principal, colon, then rights; the Linux tag:qualifier:perms form is not accepted)";
    const REMOVE: &str = "a principal such as Users or BUILTIN\\Users (no colon and no rights)";
    if default {
        return Err(safety::bad_arg(
            "acl_set",
            "no default option on Windows, because icacls inheritance flags are not taken",
            "default: true",
        ));
    }
    if remove {
        if !windows_principal(entry) {
            return Err(safety::bad_arg("acl_set", REMOVE, &format!("{entry:?}")));
        }
        return Ok(entry.to_string());
    }
    let grant = entry
        .split_once(':')
        .filter(|(principal, rights)| windows_principal(principal) && rights_arg(rights));
    match grant {
        Some((principal, rights)) => Ok(format!("{principal}:({rights})")),
        None => Err(safety::bad_arg("acl_set", GRANT, &format!("{entry:?}"))),
    }
}

fn mount_spec(options: Option<&Value>) -> Result<MountSpec> {
    let map = options_map(
        "fs_mount",
        options,
        &["fstype", "readonly", "options", "bind"],
    )?;
    let readonly = flag("fs_mount", &map, "readonly")?;
    let bind = flag("fs_mount", &map, "bind")?;
    let fstype = match map.get("fstype") {
        None => None,
        Some(Value::Str(name))
            if !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) =>
        {
            Some(name.clone())
        }
        Some(other) => {
            return Err(safety::bad_arg(
                "fs_mount",
                "fstype String of lowercase letters and digits",
                &got(other),
            ))
        }
    };
    let mut mount_options = Vec::new();
    match map.get("options") {
        None => {}
        Some(Value::Array(items)) => {
            for item in items {
                let Value::Str(option) = item else {
                    return Err(safety::bad_arg(
                        "fs_mount",
                        "options Array of String",
                        &got(item),
                    ));
                };
                let well_formed = !option.is_empty()
                    && option.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'_' | b'=' | b'.' | b',' | b'-')
                    });
                if !well_formed {
                    return Err(safety::bad_arg(
                        "fs_mount",
                        "option String matching [a-z0-9_=.,-]+",
                        &format!("{option:?}"),
                    ));
                }
                for token in option.split(',') {
                    if token.is_empty() {
                        return Err(safety::bad_arg(
                            "fs_mount",
                            "options without empty comma-separated parts",
                            &format!("{option:?}"),
                        ));
                    }
                    if token == "remount" || token.starts_with("remount=") {
                        return Err(safety::policy_deny(
                            "fs_mount",
                            "fs_mount: the remount option is refused; NOTHING WAS MOUNTED",
                            "remount is not a way to mount: unmount with fs_unmount and mount again",
                        ));
                    }
                    if matches!(token, "bind" | "rbind") && !bind {
                        return Err(safety::policy_deny(
                            "fs_mount",
                            format!(
                                "fs_mount: option `{token}` is refused unless bind: true is given"
                            ),
                            "pass bind: true to bind-mount a directory instead of this option",
                        ));
                    }
                }
                mount_options.push(option.clone());
            }
        }
        Some(other) => {
            return Err(safety::bad_arg(
                "fs_mount",
                "options Array of String",
                &got(other),
            ))
        }
    }
    if bind && readonly {
        return Err(safety::arg_err(
            "fs_mount: readonly cannot be combined with bind: making a bind mount read-only needs a remount, and remount is refused",
        ));
    }
    if bind && fstype.is_some() {
        return Err(safety::arg_err(
            "fs_mount: fstype does not apply to a bind mount",
        ));
    }
    Ok(MountSpec {
        fstype,
        readonly,
        bind,
        options: mount_options,
    })
}

/// Refuse inside a filesystem transaction: its rollback cannot undo these.
fn refuse_in_transaction(builtin: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(safety::policy_deny(
            builtin,
            format!("{builtin}: cannot run inside a filesystem transaction; NOTHING WAS CHANGED"),
            "run it outside the transaction: a rollback cannot undo or restore what it does",
        ));
    }
    Ok(())
}

/// Record the effect with the safety layer before the tool runs.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn guard(builtin: &str, effect: safety::Effect, what: &str, targets: &[String]) -> Result<()> {
    crate::safety::guard(crate::safety::GuardCtx {
        builtin,
        effect,
        what,
        targets: targets.to_vec(),
        blast_radius: serde_json::json!({ "targets": targets }),
        reversible: false,
        fs_paths: false,
    })?;
    Ok(())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

/// Refusal for a builtin with no implementation on this platform. Nothing has been touched.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn off_linux(builtin: &str) -> anyhow::Error {
    safety::unimplemented(
        builtin,
        "is implemented on Linux only in 13.0.0 on this platform; NOTHING WAS CHANGED",
        "run it on Linux, or use this platform's own tool",
    )
}

/// Simple icacls rights, which icacls prints bare, such as `F` or `RX`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
const SIMPLE_RIGHTS: &[&str] = &["F", "M", "RX", "R", "W", "D"];

/// Specific icacls rights, which icacls prints in a parenthesized list.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
const SPECIFIC_RIGHTS: &[&str] = &[
    "DE", "RC", "WDAC", "WO", "S", "AS", "MA", "GR", "GW", "GE", "GA", "RD", "WD", "AD", "REA",
    "WEA", "X", "DC", "RA", "WA",
];

/// A principal icacls can take: non-empty, not padded, with no control
/// characters, no leading `-` or `/` (icacls reads those as switches), and no
/// `:`, `(` or `)`, which the icacls grammar uses.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn windows_principal(text: &str) -> bool {
    !text.is_empty()
        && text.trim() == text
        && !text.starts_with('-')
        && !text.starts_with('/')
        && !text
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '(' | ')'))
}

/// Whether text is one or more icacls rights, comma-separated, as `acl_set` takes them.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn rights_arg(text: &str) -> bool {
    !text.is_empty()
        && text
            .split(',')
            .all(|r| SIMPLE_RIGHTS.contains(&r) || SPECIFIC_RIGHTS.contains(&r))
}

/// The perms text for one icacls permission group, as the record reports it: a
/// simple right bare (`F`), and anything else parenthesized (`(RX,D)`). `N`
/// (no access) is accepted as well, because icacls can print it.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn permission_text(token: &str) -> Option<String> {
    let known = |r: &str| r == "N" || SIMPLE_RIGHTS.contains(&r) || SPECIFIC_RIGHTS.contains(&r);
    if token.split(',').any(|r| !known(r)) {
        return None;
    }
    if token == "N" || SIMPLE_RIGHTS.contains(&token) {
        Some(token.to_string())
    } else {
        Some(format!("({token})"))
    }
}

/// Split the rights of one ACE, such as `(I)(OI)(CI)(RX,D)`, into its inherited
/// bit, flag groups, deny bit and perms text. Any group it does not know, or a
/// missing or second permission group, gives None.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_rights(text: &str) -> Option<(bool, Vec<String>, bool, String)> {
    let mut inherited = false;
    let mut flags = Vec::new();
    let mut deny = false;
    let mut perms = None;
    let mut rest = text;
    while !rest.is_empty() {
        let inner = rest.strip_prefix('(')?;
        let close = inner.find(')')?;
        let token = &inner[..close];
        rest = &inner[close + 1..];
        match token {
            "I" => inherited = true,
            "OI" | "CI" | "IO" | "NP" => flags.push(token.to_string()),
            "DENY" => deny = true,
            _ if perms.is_none() => perms = Some(permission_text(token)?),
            _ => return None,
        }
    }
    Some((inherited, flags, deny, perms?))
}

/// Parse the output of `icacls <path>` into the record `acl_get` returns on
/// Windows: `{path, entries}`, where each entry is `{principal, perms,
/// inherited, flags, deny}`. The first ACE line is `<path> <principal>:<rights>`
/// and each further ACE line is indented. A blank line, then one summary line,
/// ends the output. The summary is in the display language, so its wording is
/// not read: the caller has already checked icacls's exit status, which is
/// what reports a failure. A line that is not an ACE, a rights group the
/// parser does not know, more than one line after the blank, and output with
/// no summary are all coded tool failures.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_icacls(path: &str, text: &str) -> Result<Value> {
    let unknown = |line: &str| {
        safety::tool_failed(
            "acl_get",
            "icacls",
            Some(0),
            &format!("unrecognised output line {line:?}"),
        )
    };
    let mut entries = Vec::new();
    let mut after_blank = false;
    let mut summary = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            if !entries.is_empty() {
                after_blank = true;
            }
            continue;
        }
        if after_blank {
            if summary {
                return Err(unknown(line));
            }
            summary = true;
            continue;
        }
        let rest = if let Some(after) = line.strip_prefix(path).and_then(|r| r.strip_prefix(' ')) {
            after
        } else if line.starts_with(' ') {
            line.trim_start()
        } else {
            return Err(unknown(line));
        };
        let Some((principal, rights)) = rest.split_once(':') else {
            return Err(unknown(line));
        };
        let Some((inherited, flags, deny, perms)) = parse_rights(rights) else {
            return Err(unknown(line));
        };
        if !windows_principal(principal) {
            return Err(unknown(line));
        }
        entries.push(record(vec![
            ("principal", Value::Str(principal.to_string())),
            ("perms", Value::Str(perms)),
            ("inherited", Value::Bool(inherited)),
            (
                "flags",
                Value::Array(flags.into_iter().map(Value::Str).collect()),
            ),
            ("deny", Value::Bool(deny)),
        ]));
    }
    if !summary {
        return Err(safety::tool_failed(
            "acl_get",
            "icacls",
            Some(0),
            "its output had no summary line",
        ));
    }
    Ok(record(vec![
        ("path", Value::Str(path.to_string())),
        ("entries", Value::Array(entries)),
    ]))
}

// ── System effects ────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod sys {
    use super::{guard, record, AclSpec, MountSpec, PROTECTED};
    use crate::{safety, value::Value};
    use anyhow::Result;
    use std::os::unix::fs::FileTypeExt;
    use std::path::PathBuf;

    const GETFACL: &str = "/usr/bin/getfacl";
    const SETFACL: &str = "/usr/bin/setfacl";
    const MOUNT: &str = "/usr/bin/mount";
    const UMOUNT: &str = "/usr/bin/umount";
    const PROC_MOUNTS: &str = "/proc/self/mounts";

    /// Run `program` with `args` and return its stdout. A non-zero exit is a
    /// coded tool failure carrying stderr; a missing program is E_TOOL_MISSING.
    fn run(builtin: &str, program: &str, args: &[&str]) -> Result<String> {
        let name = program.rsplit('/').next().unwrap_or(program);
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).env("LC_ALL", "C");
        // A mount tool that does not finish in 30s is stopped with its tree and
        // refused as a budget error, not reported as a failure to start.
        let output = safety::output_with_timeout(cmd, 30).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                safety::budget_exceeded(
                    builtin,
                    format!("`{name}` did not finish within 30s and was stopped"),
                    "the mount table or the tool is wedged; retry, or inspect the tool directly",
                )
            } else {
                safety::spawn_error(builtin, name, &e)
            }
        })?;
        if !output.status.success() {
            return Err(safety::tool_failed(
                builtin,
                name,
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn path_text(builtin: &str, path: PathBuf) -> Result<String> {
        path.into_os_string()
            .into_string()
            .map_err(|_| safety::arg_err(format!("{builtin}: the path is not valid UTF-8")))
    }

    fn check_protected(builtin: &str, canonical: &str, verb: &str) -> Result<()> {
        if PROTECTED.contains(&canonical) {
            return Err(safety::policy_deny(
                builtin,
                format!("{builtin}: {canonical} is a protected system path; NOTHING WAS {verb}"),
                "choose a mount point outside the system paths; these are refused by exact path",
            ));
        }
        Ok(())
    }

    /// One entry of /proc/self/mounts, with its escapes decoded.
    struct Mount {
        source: String,
        mountpoint: String,
        fstype: String,
        options: String,
    }

    /// Decode the octal escapes (`\040` for a space and so on) that the kernel
    /// uses in /proc/self/mounts.
    fn unescape(field: &str) -> String {
        let bytes = field.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let escape = bytes[i] == b'\\'
                && i + 3 < bytes.len()
                && bytes[i + 1..i + 4]
                    .iter()
                    .all(|b| (b'0'..=b'7').contains(b));
            if escape {
                let value = (bytes[i + 1] - b'0') as u32 * 64
                    + (bytes[i + 2] - b'0') as u32 * 8
                    + (bytes[i + 3] - b'0') as u32;
                out.push(value as u8);
                i += 4;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn mount_table(builtin: &str) -> Result<Vec<Mount>> {
        let text = std::fs::read_to_string(PROC_MOUNTS)
            .map_err(|e| safety::fs_error(builtin, PROC_MOUNTS, &e))?;
        let mut mounts = Vec::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 {
                continue;
            }
            mounts.push(Mount {
                source: unescape(fields[0]),
                mountpoint: unescape(fields[1]),
                fstype: fields[2].to_string(),
                options: fields[3].to_string(),
            });
        }
        Ok(mounts)
    }

    /// Parse `getfacl --absolute-names` output into the record `acl_get` returns.
    ///
    /// Names are used, not numeric IDs. The header gives owner and group. Each
    /// entry is `[default:]tag:qualifier:perms`, optionally followed by a tab
    /// and `#effective:perms`, which is dropped because the stored perms are
    /// the ones asked for. A `# flags:` header (setuid, setgid or sticky bits)
    /// is not reported. Any other line is a coded failure.
    fn parse_getfacl(path: &str, text: &str) -> Result<Value> {
        let unknown = |line: &str| {
            safety::tool_failed(
                "acl_get",
                "getfacl",
                Some(0),
                &format!("unrecognised output line {line:?}"),
            )
        };
        let mut owner = None;
        let mut group = None;
        let mut entries = Vec::new();
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix("# owner: ") {
                owner = Some(name.to_string());
                continue;
            }
            if let Some(name) = line.strip_prefix("# group: ") {
                group = Some(name.to_string());
                continue;
            }
            if line.starts_with("# file: ") || line.starts_with("# flags: ") {
                continue;
            }
            let body = match line.find("\t#effective:") {
                Some(at) => &line[..at],
                None => line,
            };
            let (default, body) = match body.strip_prefix("default:") {
                Some(rest) => (true, rest),
                None => (false, body),
            };
            let parts: Vec<&str> = body.split(':').collect();
            let [tag, qualifier, perms] = parts.as_slice() else {
                return Err(unknown(line));
            };
            let tag = match *tag {
                t @ ("user" | "group" | "mask" | "other") => t,
                _ => return Err(unknown(line)),
            };
            if perms.len() != 3 || !perms.chars().all(|c| "rwx-".contains(c)) {
                return Err(unknown(line));
            }
            entries.push(record(vec![
                ("tag", Value::Str(tag.to_string())),
                (
                    "qualifier",
                    if qualifier.is_empty() {
                        Value::Null
                    } else {
                        Value::Str(qualifier.to_string())
                    },
                ),
                ("perms", Value::Str(perms.to_string())),
                ("default", Value::Bool(default)),
            ]));
        }
        let (Some(owner), Some(group)) = (owner, group) else {
            return Err(safety::tool_failed(
                "acl_get",
                "getfacl",
                Some(0),
                "its output had no owner or group header",
            ));
        };
        Ok(record(vec![
            ("path", Value::Str(path.to_string())),
            ("owner", Value::Str(owner)),
            ("group", Value::Str(group)),
            ("entries", Value::Array(entries)),
        ]))
    }

    pub(super) fn acl_get(path: &str) -> Result<Value> {
        std::fs::metadata(path).map_err(|e| safety::fs_error("acl_get", path, &e))?;
        guard(
            "acl_get",
            safety::Effect::ReadLocal,
            "read the ACL of",
            &[path.to_string()],
        )?;
        let text = run("acl_get", GETFACL, &["--absolute-names", "--", path])?;
        parse_getfacl(path, &text)
    }

    pub(super) fn acl_set(path: &str, spec: &AclSpec) -> Result<Value> {
        let metadata =
            std::fs::metadata(path).map_err(|e| safety::fs_error("acl_set", path, &e))?;
        if spec.default && !metadata.is_dir() {
            return Err(safety::arg_err(format!(
                "acl_set: a default ACL applies to directories, and {path} is not one"
            )));
        }
        guard(
            "acl_set",
            safety::Effect::Privileged,
            "change the ACL of",
            &[path.to_string()],
        )?;
        let mut args: Vec<&str> = Vec::new();
        if spec.default {
            args.push("-d");
        }
        args.push(if spec.remove { "-x" } else { "-m" });
        args.push(&spec.arg);
        args.push("--");
        args.push(path);
        run("acl_set", SETFACL, &args)?;
        Ok(record(vec![
            ("path", Value::Str(path.to_string())),
            ("entry", Value::Str(spec.arg.clone())),
            ("applied", Value::Bool(true)),
        ]))
    }

    pub(super) fn fs_mount(source: &str, target: &str, spec: &MountSpec) -> Result<Value> {
        let target_path =
            std::fs::canonicalize(target).map_err(|e| safety::fs_error("fs_mount", target, &e))?;
        let target = path_text("fs_mount", target_path)?;
        check_protected("fs_mount", &target, "MOUNTED")?;
        let target_meta =
            std::fs::metadata(&target).map_err(|e| safety::fs_error("fs_mount", &target, &e))?;
        if !target_meta.is_dir() {
            return Err(safety::arg_err(format!(
                "fs_mount: the target {target} is not a directory"
            )));
        }
        let occupied = std::fs::read_dir(&target)
            .map_err(|e| safety::fs_error("fs_mount", &target, &e))?
            .next()
            .is_some();
        if occupied {
            return Err(safety::arg_err(format!(
                "fs_mount: the target {target} is not empty; mount on an empty directory"
            )));
        }

        let source_path =
            std::fs::canonicalize(source).map_err(|e| safety::fs_error("fs_mount", source, &e))?;
        let source = path_text("fs_mount", source_path)?;
        let source_meta =
            std::fs::metadata(&source).map_err(|e| safety::fs_error("fs_mount", &source, &e))?;
        let mut options = spec.options.clone();
        if spec.bind {
            if !source_meta.is_dir() {
                return Err(safety::arg_err(format!(
                    "fs_mount: a bind mount needs a directory source, and {source} is not one"
                )));
            }
        } else if source_meta.file_type().is_block_device() {
            // A block device mounts directly.
        } else if source_meta.is_file() {
            options.push("loop".to_string());
        } else {
            return Err(safety::arg_err(format!(
                "fs_mount: {source} is neither a block device nor a regular file; \
                 a directory needs bind: true"
            )));
        }
        if spec.readonly {
            options.push("ro".to_string());
        }

        let mut args: Vec<String> = Vec::new();
        if let Some(fstype) = &spec.fstype {
            args.push("-t".to_string());
            args.push(fstype.clone());
        }
        if spec.bind {
            args.push("--bind".to_string());
        }
        if !options.is_empty() {
            args.push("-o".to_string());
            args.push(options.join(","));
        }
        args.push(source.clone());
        args.push(target.clone());
        guard(
            "fs_mount",
            safety::Effect::Privileged,
            "mount",
            &[source.clone(), target.clone()],
        )?;
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        run("fs_mount", MOUNT, &argv)?;

        // Read the mount back from the kernel's table, not from what was asked.
        let table = mount_table("fs_mount")?;
        let Some(mounted) = table.iter().rev().find(|m| m.mountpoint == target) else {
            return Err(safety::tool_failed(
                "fs_mount",
                "mount",
                Some(0),
                &format!("{target} is not listed in {PROC_MOUNTS} after mounting"),
            ));
        };
        let readonly = mounted.options.split(',').any(|o| o == "ro");
        Ok(record(vec![
            ("source", Value::Str(source)),
            ("target", Value::Str(target)),
            ("fstype", Value::Str(mounted.fstype.clone())),
            ("readonly", Value::Bool(readonly)),
        ]))
    }

    pub(super) fn fs_unmount(target: &str, lazy: bool) -> Result<Value> {
        let canonical_path = std::fs::canonicalize(target)
            .map_err(|e| safety::fs_error("fs_unmount", target, &e))?;
        let canonical = path_text("fs_unmount", canonical_path)?;
        check_protected("fs_unmount", &canonical, "UNMOUNTED")?;
        let table = mount_table("fs_unmount")?;
        if !table.iter().any(|m| m.mountpoint == canonical) {
            return Err(safety::not_found("fs_unmount", "mount point", target));
        }
        guard(
            "fs_unmount",
            safety::Effect::Privileged,
            "unmount",
            &[canonical.clone()],
        )?;
        let mut args: Vec<&str> = Vec::new();
        if lazy {
            args.push("-l");
        }
        args.push(&canonical);
        run("fs_unmount", UMOUNT, &args)?;
        Ok(record(vec![
            ("target", Value::Str(target.to_string())),
            ("unmounted", Value::Bool(true)),
        ]))
    }
}

#[cfg(windows)]
mod sys {
    use super::{guard, off_linux, parse_icacls, record, AclSpec, MountSpec};
    use crate::{safety, value::Value};
    use anyhow::Result;

    /// The icacls program in the system directory, by absolute path, so that a
    /// program of the same name in the working directory is never run.
    fn icacls_program() -> String {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        format!("{root}\\System32\\icacls.exe")
    }

    /// Run icacls with `args` and return its stdout. A refusal by the operating
    /// system is E_OS_DENIED. Any other non-zero exit is a coded tool failure
    /// carrying stderr. A run that does not finish in 30s is stopped.
    fn run(builtin: &str, args: &[&str]) -> Result<String> {
        let mut cmd = std::process::Command::new(icacls_program());
        cmd.args(args);
        let output = safety::output_with_timeout(cmd, 30).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                safety::budget_exceeded(
                    builtin,
                    "`icacls` did not finish within 30s and was stopped".to_string(),
                    "the file system or the tool is wedged; retry, or inspect the ACL with icacls directly",
                )
            } else {
                safety::spawn_error(builtin, "icacls", &e)
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            // icacls reports this refusal as English text, the only sign it gives.
            if stderr.contains("Access is denied") || stdout.contains("Access is denied") {
                return Err(safety::os_denied(
                    builtin,
                    format!("{builtin}: the operating system denied access to the ACL"),
                    "check the file's owner and permissions; the shell did not refuse this",
                ));
            }
            return Err(safety::tool_failed(
                builtin,
                "icacls",
                output.status.code(),
                &stderr,
            ));
        }
        Ok(stdout)
    }

    /// icacls reads `*` and `?` as wildcards and a leading `/` as a switch, so a
    /// name with either would act on something other than the path given.
    fn plain_path(builtin: &str, path: &str) -> Result<()> {
        if path.starts_with('/') || path.chars().any(|c| matches!(c, '*' | '?')) {
            return Err(safety::bad_arg(
                builtin,
                "a path without wildcards * or ?, that does not begin with /",
                &format!("{path:?}"),
            ));
        }
        Ok(())
    }

    pub(super) fn acl_get(path: &str) -> Result<Value> {
        plain_path("acl_get", path)?;
        std::fs::metadata(path).map_err(|e| safety::fs_error("acl_get", path, &e))?;
        guard(
            "acl_get",
            safety::Effect::ReadLocal,
            "read the ACL of",
            &[path.to_string()],
        )?;
        let text = run("acl_get", &[path])?;
        parse_icacls(path, &text)
    }

    pub(super) fn acl_set(path: &str, spec: &AclSpec) -> Result<Value> {
        plain_path("acl_set", path)?;
        std::fs::metadata(path).map_err(|e| safety::fs_error("acl_set", path, &e))?;
        guard(
            "acl_set",
            safety::Effect::Privileged,
            "change the ACL of",
            &[path.to_string()],
        )?;
        // /Q keeps icacls from printing its per-file line.
        if spec.remove {
            run("acl_set", &[path, "/remove", spec.arg.as_str(), "/Q"])?;
        } else {
            run("acl_set", &[path, "/grant", spec.arg.as_str(), "/Q"])?;
        }
        Ok(record(vec![
            ("path", Value::Str(path.to_string())),
            ("entry", Value::Str(spec.arg.clone())),
            ("applied", Value::Bool(true)),
        ]))
    }

    pub(super) fn fs_mount(_source: &str, _target: &str, _spec: &MountSpec) -> Result<Value> {
        Err(off_linux("fs_mount"))
    }

    pub(super) fn fs_unmount(_target: &str, _lazy: bool) -> Result<Value> {
        Err(off_linux("fs_unmount"))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod sys {
    use super::{off_linux, AclSpec, MountSpec};
    use crate::value::Value;
    use anyhow::Result;

    pub(super) fn acl_get(_path: &str) -> Result<Value> {
        Err(off_linux("acl_get"))
    }

    pub(super) fn acl_set(_path: &str, _spec: &AclSpec) -> Result<Value> {
        Err(off_linux("acl_set"))
    }

    pub(super) fn fs_mount(_source: &str, _target: &str, _spec: &MountSpec) -> Result<Value> {
        Err(off_linux("fs_mount"))
    }

    pub(super) fn fs_unmount(_target: &str, _lazy: bool) -> Result<Value> {
        Err(off_linux("fs_unmount"))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_icacls;
    use crate::value::Value;

    /// `icacls` on a file, as captured on Windows 11. The local account name is
    /// replaced by a placeholder. The first line carries the path and an ACE.
    /// Inherited ACEs have (I). The last ACE has no (I), so it is explicit.
    const FILE_OUTPUT: &str = r"target-scratch\acl-probe.txt BUILTIN\Users:(RX)
                             WORKSTATION\SandboxUsers:(I)(RX)
                             NT AUTHORITY\SYSTEM:(I)(F)
                             BUILTIN\Administrators:(I)(F)
                             WORKSTATION\analyst:(I)(F)

Successfully processed 1 files; Failed processing 0 files
";

    /// `icacls` on a directory, with the (OI)(CI) inheritance flags, captured on
    /// Windows 11. The DENY line is not from a capture: no DENY ACE was available
    /// to read, so it is written in icacls's documented form.
    const DIRECTORY_OUTPUT: &str = r"target-scratch BUILTIN\Users:(I)(OI)(CI)(RX)
               NT AUTHORITY\SYSTEM:(I)(OI)(CI)(F)
               Everyone:(DENY)(W)

Successfully processed 1 files; Failed processing 0 files
";

    fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
        match value {
            Value::Record(map) => &map[key],
            _ => panic!("expected a record"),
        }
    }

    fn text<'a>(value: &'a Value, key: &str) -> &'a str {
        match field(value, key) {
            Value::Str(s) => s,
            _ => panic!("{key} is not a string"),
        }
    }

    fn flag(value: &Value, key: &str) -> bool {
        match field(value, key) {
            Value::Bool(b) => *b,
            _ => panic!("{key} is not a bool"),
        }
    }

    fn flags(value: &Value) -> Vec<&str> {
        match field(value, "flags") {
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::Str(s) => s.as_str(),
                    _ => panic!("a flag is not a string"),
                })
                .collect(),
            _ => panic!("flags is not an array"),
        }
    }

    fn entries(value: &Value) -> &[Value] {
        match field(value, "entries") {
            Value::Array(items) => items,
            _ => panic!("entries is not an array"),
        }
    }

    #[test]
    fn file_output_gives_inherited_and_explicit_aces() {
        let value = parse_icacls("target-scratch\\acl-probe.txt", FILE_OUTPUT).unwrap();
        assert_eq!(text(&value, "path"), "target-scratch\\acl-probe.txt");
        let aces = entries(&value);
        assert_eq!(aces.len(), 5);
        assert_eq!(text(&aces[0], "principal"), "BUILTIN\\Users");
        assert_eq!(text(&aces[0], "perms"), "RX");
        assert!(!flag(&aces[0], "inherited"));
        assert!(!flag(&aces[0], "deny"));
        assert!(flags(&aces[0]).is_empty());
        assert_eq!(text(&aces[1], "principal"), "WORKSTATION\\SandboxUsers");
        assert!(flag(&aces[1], "inherited"));
        assert_eq!(text(&aces[2], "principal"), "NT AUTHORITY\\SYSTEM");
        assert_eq!(text(&aces[2], "perms"), "F");
    }

    #[test]
    fn directory_output_gives_oi_ci_flags_and_deny() {
        let value = parse_icacls("target-scratch", DIRECTORY_OUTPUT).unwrap();
        let aces = entries(&value);
        assert_eq!(aces.len(), 3);
        assert!(flag(&aces[0], "inherited"));
        assert_eq!(flags(&aces[0]), vec!["OI", "CI"]);
        assert_eq!(text(&aces[0], "perms"), "RX");
        assert_eq!(text(&aces[1], "perms"), "F");
        assert_eq!(text(&aces[2], "principal"), "Everyone");
        assert!(flag(&aces[2], "deny"));
        assert!(!flag(&aces[2], "inherited"));
        assert_eq!(text(&aces[2], "perms"), "W");
    }

    #[test]
    fn the_summary_is_not_read_so_any_display_language_parses() {
        let german = "target-scratch BUILTIN\\Users:(F)\n\n1 Dateien erfolgreich verarbeitet, bei 0 Dateien ist ein Verarbeitungsfehler aufgetreten.\n";
        let value = parse_icacls("target-scratch", german).unwrap();
        assert_eq!(entries(&value).len(), 1);
    }

    #[test]
    fn unrecognised_lines_are_errors_not_skipped() {
        let summary = "\n\nSuccessfully processed 1 files; Failed processing 0 files\n";
        let cases = [
            // No colon between the principal and the rights.
            format!("target-scratch BUILTIN\\Users (RX){summary}"),
            // A rights group that is never closed.
            format!("target-scratch BUILTIN\\Users:(I)(F{summary}"),
            // A rights group icacls does not define.
            format!("target-scratch BUILTIN\\Users:(ZZ){summary}"),
            // A line that is neither the path nor indented.
            format!("other BUILTIN\\Users:(F){summary}"),
            // Output with no summary line.
            "target-scratch BUILTIN\\Users:(F)\n".to_string(),
            // Two lines after the blank one.
            format!("target-scratch BUILTIN\\Users:(F){summary}more\n"),
        ];
        for case in cases {
            assert!(
                parse_icacls("target-scratch", &case).is_err(),
                "accepted {case:?}"
            );
        }
    }
}
