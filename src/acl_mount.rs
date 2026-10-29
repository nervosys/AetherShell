//! ACLs and mounts: `acl_get`, `acl_set`, `fs_mount` and `fs_unmount`.
//!
//! Every builtin checks its arguments before it touches the system, then runs
//! one tool (`getfacl`, `setfacl`, `icacls`, `ls`, `chmod`, `mount`, `umount`,
//! `hdiutil`) with an argv vector. No shell is involved and no caller string is
//! interpolated into a command line. On Linux, `acl_get` and `acl_set` run
//! `getfacl` and `setfacl`, and the mount builtins run `mount` and `umount`. On
//! Windows, `acl_get` and `acl_set` run `icacls`, and the mount builtins run
//! Windows PowerShell's storage cmdlets on a VHD or VHDX file, with every path
//! passed in an environment variable and none in the script text. On macOS,
//! `acl_get` runs `ls -led` and `acl_set` runs `chmod +a` or `chmod -a`, with
//! the entry in the `user:NAME allow|deny PERMS` form; `fs_mount` attaches a
//! disk-image file with `hdiutil attach` and reads the mount back with statfs,
//! and `fs_unmount` detaches a disk image with `hdiutil detach` or unmounts any
//! other mount with `umount`. Elsewhere the mount and ACL builtins are refused
//! with E_UNIMPLEMENTED after their arguments are checked. A refusal changes
//! nothing.
//!
//! ACL changes are not recorded by the undo journal. A call inside a
//! filesystem transaction is refused, because a rollback cannot restore the
//! permission metadata or mount state that these builtins change.
//!
//! `fs_unmount` refuses the mount points `/`, `/proc`, `/sys`, `/dev`, `/run`,
//! `/boot`, `/usr`, `/etc`, `/var` and `/home` by exact path, after the path
//! is canonicalized. `fs_mount` refuses the same paths as a target. On Windows
//! the protected targets are the drive roots, the Windows folder and everything
//! under it, Program Files (both), ProgramData and the Users folder. macOS has
//! its own list, in its `sys` module, and refuses the same calls by exact path.
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

/// What an absolute path begins with, as the error message names it.
#[cfg(windows)]
const ABSOLUTE_FORM: &str = r"beginning with a drive letter, such as C:\";
#[cfg(not(windows))]
const ABSOLUTE_FORM: &str = "beginning with /";

/// Whether the text begins as an absolute path: `/` off Windows, and a drive
/// letter then a separator (`C:\` or `C:/`) on Windows. A `/` path on Windows
/// names the current drive, so it is refused there.
#[cfg(windows)]
fn is_absolute(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/')
}

#[cfg(not(windows))]
fn is_absolute(text: &str) -> bool {
    text.starts_with('/')
}

/// A path that must be absolute (see [`is_absolute`]). The check is textual, so
/// the same call is refused with E_BAD_ARG before any system access.
fn absolute_arg(builtin: &str, value: &Value, role: &str) -> Result<String> {
    let text = text_arg(builtin, value, role)?;
    if !is_absolute(&text) {
        return Err(safety::bad_arg(
            builtin,
            &format!("absolute {role} path {ABSOLUTE_FORM}"),
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
#[cfg(not(any(windows, target_os = "macos")))]
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

/// The permission names `ls -le` prints and `chmod +a` takes, on macOS.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
const MAC_PERMS: &[&str] = &[
    "read",
    "write",
    "execute",
    "delete",
    "append",
    "readattr",
    "writeattr",
    "readextattr",
    "writeextattr",
    "readsecurity",
    "writesecurity",
    "chown",
    "list",
    "search",
    "add_file",
    "add_subdirectory",
    "delete_child",
];

/// The inheritance flags `ls -le` prints on an inherited or inheriting entry.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
const MAC_FLAGS: &[&str] = &[
    "file_inherit",
    "directory_inherit",
    "limit_inherit",
    "only_inherit",
];

/// A macOS account or group name: letters, digits, `_`, `-` and `.`, and no
/// leading `-`, so it cannot be read as an option.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_name(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('-')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Validate an `acl_set` entry on macOS and return the normalized form that
/// `chmod +a` takes.
///
/// An entry is `tag:name allow|deny perms`, such as `user:nobody allow
/// read,write` or `group:staff deny delete`. The tag is `user` or `group`. The
/// name is a macOS name. The perms are a comma-separated list, each word one of
/// the names `ls -le` prints. The words are joined with single spaces. A
/// removal takes the same entry, because `chmod -a` matches the entry given. The
/// Linux `u::rw-` form does not parse, so it is refused. `default` is refused,
/// because inheritance flags are not taken.
#[cfg(target_os = "macos")]
fn acl_entry(entry: &str, _remove: bool, default: bool) -> Result<String> {
    const FORM: &str = "an entry like user:nobody allow read,write or group:staff deny delete (user or group, then allow or deny, then comma-separated perms; the Linux u::rw- form is not accepted)";
    if default {
        return Err(safety::bad_arg(
            "acl_set",
            "no default option on macOS, because inheritance flags are not taken",
            "default: true",
        ));
    }
    let refuse = || safety::bad_arg("acl_set", FORM, &format!("{entry:?}"));
    let words: Vec<&str> = entry.split_whitespace().collect();
    let [who, verb, perms] = words.as_slice() else {
        return Err(refuse());
    };
    let Some((tag, name)) = who.split_once(':') else {
        return Err(refuse());
    };
    let well_formed = matches!(tag, "user" | "group")
        && matches!(*verb, "allow" | "deny")
        && macos_name(name)
        && perms.split(',').all(|p| MAC_PERMS.contains(&p));
    if !well_formed {
        return Err(refuse());
    }
    Ok(format!("{tag}:{name} {verb} {perms}"))
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
#[cfg_attr(
    any(target_os = "linux", target_os = "macos", windows),
    allow(dead_code)
)]
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

/// Parse the output of `ls -led <path>` into the record `acl_get` returns on
/// macOS: `{path, owner, group, entries}`. The first line is the long listing,
/// and its third and fourth fields are the owner and group. Each further line
/// is an entry, ` N: tag:name allow|deny perms`, where N is the index and the
/// perms are comma-separated. A word `inherited`, and the inheritance flags in
/// `MAC_FLAGS`, may also appear among the comma-separated words. They are
/// reported apart from the perms. A blank line is skipped. Any other line, word
/// or empty perms list is a coded tool failure, never dropped.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn parse_ls_acl(path: &str, text: &str) -> Result<Value> {
    let unknown = |line: &str| {
        safety::tool_failed(
            "acl_get",
            "ls",
            Some(0),
            &format!("unrecognised output line {line:?}"),
        )
    };
    let mut lines = text.lines();
    let listing = lines.next().unwrap_or("");
    let fields: Vec<&str> = listing.split_whitespace().collect();
    if listing.trim().is_empty() {
        return Err(safety::tool_failed(
            "acl_get",
            "ls",
            Some(0),
            "its output had no listing line",
        ));
    }
    let mode_ok = fields
        .first()
        .is_some_and(|mode| mode.len() >= 10 && mode.is_ascii());
    if fields.len() < 4 || !mode_ok {
        return Err(unknown(listing));
    }
    let mut entries = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let Some((index, entry)) = line.trim_start().split_once(": ") else {
            return Err(unknown(line));
        };
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            return Err(unknown(line));
        }
        let index: i64 = index.parse().map_err(|_| unknown(line))?;
        let words: Vec<&str> = entry.split_whitespace().collect();
        let [who, verb, perms] = words.as_slice() else {
            return Err(unknown(line));
        };
        let Some((tag, name)) = who.split_once(':') else {
            return Err(unknown(line));
        };
        if !matches!(tag, "user" | "group")
            || !matches!(*verb, "allow" | "deny")
            || !macos_name(name)
        {
            return Err(unknown(line));
        }
        let mut perm_words = Vec::new();
        let mut flags = Vec::new();
        let mut inherited = false;
        for token in perms.split(',') {
            match token {
                "inherited" => inherited = true,
                t if MAC_FLAGS.contains(&t) => flags.push(t.to_string()),
                t if MAC_PERMS.contains(&t) => perm_words.push(t.to_string()),
                _ => return Err(unknown(line)),
            }
        }
        if perm_words.is_empty() {
            return Err(unknown(line));
        }
        entries.push(record(vec![
            ("index", Value::Int(index)),
            ("tag", Value::Str(tag.to_string())),
            ("name", Value::Str(name.to_string())),
            ("kind", Value::Str(verb.to_string())),
            (
                "perms",
                Value::Array(perm_words.into_iter().map(Value::Str).collect()),
            ),
            ("inherited", Value::Bool(inherited)),
            (
                "flags",
                Value::Array(flags.into_iter().map(Value::Str).collect()),
            ),
        ]));
    }
    Ok(record(vec![
        ("path", Value::Str(path.to_string())),
        ("owner", Value::Str(fields[2].to_string())),
        ("group", Value::Str(fields[3].to_string())),
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
    use super::{guard, parse_icacls, record, AclSpec, MountSpec};
    use crate::{safety, value::Value};
    use anyhow::Result;
    use std::path::{Path, PathBuf};

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

    /// Run Windows PowerShell with `script` and the given environment variables,
    /// which carry every path: the script text holds no caller data. The module
    /// is removed from the child's PSModulePath, which an inherited PowerShell 7
    /// value would break (Windows PowerShell then cannot load the storage
    /// module). A run that does not finish in 60s is stopped.
    fn powershell(
        builtin: &str,
        script: &str,
        env: &[(&str, &str)],
    ) -> Result<std::process::Output> {
        let full = format!("$ErrorActionPreference = 'Stop'\n{script}");
        let mut cmd = std::process::Command::new("powershell.exe");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &full]);
        cmd.env_remove("PSModulePath");
        for (name, value) in env {
            cmd.env(name, value);
        }
        safety::output_with_timeout(cmd, 60).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                safety::budget_exceeded(
                    builtin,
                    "`powershell` did not finish within 60s and was stopped".to_string(),
                    "the storage service or the image is wedged; retry, or inspect the image with Get-DiskImage directly",
                )
            } else {
                safety::spawn_error(builtin, "powershell", &e)
            }
        })
    }

    /// The stdout of a PowerShell run that exited 0. A refusal for lack of
    /// elevation, which is what an unelevated shell gets from the storage
    /// cmdlets, is E_OS_DENIED. Any other failure is a coded tool failure with
    /// stderr. Callers read the exit codes their scripts set before this.
    fn checked(builtin: &str, output: &std::process::Output) -> Result<String> {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let lower = stderr.to_lowercase();
        if lower.contains("access is denied")
            || lower.contains("0x80070005")
            || lower.contains("requires elevation")
        {
            return Err(safety::os_denied(
                builtin,
                format!("{builtin}: the operating system denied the storage operation"),
                "run it from an elevated (Administrator) shell; the shell did not refuse this",
            ));
        }
        Err(safety::tool_failed(
            builtin,
            "powershell",
            output.status.code(),
            &stderr,
        ))
    }

    /// Parse the single JSON line a script prints.
    fn json(builtin: &str, text: &str) -> Result<serde_json::Value> {
        serde_json::from_str(text.trim()).map_err(|e| {
            safety::tool_failed(
                builtin,
                "powershell",
                Some(0),
                &format!("its output was not JSON ({e}): {:?}", text.trim()),
            )
        })
    }

    /// Read-only: how many partitions list `target` as an access path, and
    /// whether the disk holding the first is read-only. None when no partition
    /// lists it.
    const MOUNTED_SCRIPT: &str = r#"
$parts = @(Get-Partition | Where-Object { $_.AccessPaths -contains ($env:AE_DST + '\') })
$readonly = $false
if ($parts.Count -gt 0) { $readonly = [bool](Get-Disk -Number $parts[0].DiskNumber).IsReadOnly }
ConvertTo-Json ([ordered]@{ count = $parts.Count; readonly = $readonly }) -Compress
"#;

    /// Attach the image in AE_SRC (read-only when AE_RO is 1), give its first
    /// non-reserved partition the folder AE_DST, and print {device, fstype}.
    /// Exit 6: the folder is not on an NTFS volume. Exit 5: the image has no
    /// partition. Any failure after the attach detaches the image again.
    const MOUNT_SCRIPT: &str = r#"
$vol = Get-Volume -FilePath $env:AE_DST
if ($vol.FileSystem -ne 'NTFS') { exit 6 }
$attach = @{ ImagePath = $env:AE_SRC; NoDriveLetter = $true }
if ($env:AE_RO -eq '1') { $attach['Access'] = 'ReadOnly' }
Mount-DiskImage @attach | Out-Null
$code = 0
try {
    $part = Get-DiskImage -ImagePath $env:AE_SRC | Get-Disk | Get-Partition | Where-Object { $_.Type -ne 'Reserved' } | Select-Object -First 1
    if (-not $part) { $code = 5 } else {
        $part | Add-PartitionAccessPath -AccessPath ($env:AE_DST + '\')
        $fs = ($part | Get-Volume).FileSystem
        $result = [ordered]@{ device = (Get-DiskImage -ImagePath $env:AE_SRC).DevicePath; fstype = $fs }
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    $code = 1
}
if ($code -ne 0) {
    Dismount-DiskImage -ImagePath $env:AE_SRC -ErrorAction SilentlyContinue | Out-Null
    exit $code
}
ConvertTo-Json $result -Compress
"#;

    /// Remove the access path AE_DST from its partition, then detach the image
    /// behind that disk if one is attached. Exit 4: no partition lists the path.
    /// If the detach fails, the folder is already unmounted and the image stays
    /// attached.
    const UNMOUNT_SCRIPT: &str = r#"
$want = $env:AE_DST + '\'
$part = Get-Partition | Where-Object { $_.AccessPaths -contains $want } | Select-Object -First 1
if (-not $part) { exit 4 }
$disk = Get-Disk -Number $part.DiskNumber
# Found before anything changes: Windows PowerShell's Get-DiskImage needs
# -ImagePath or -DevicePath (run 40829823865), and a lookup that failed after
# the access path was removed would leave the folder unmounted and the image
# attached.
$device = '\\.\PhysicalDrive' + $disk.Number
$image = Get-DiskImage -DevicePath $device -ErrorAction SilentlyContinue
$part | Remove-PartitionAccessPath -AccessPath $want
if ($image -and $image.Attached) { Dismount-DiskImage -DevicePath $device | Out-Null }
"#;

    /// Detach the image in AE_SRC. Used only to undo a failed mount, so its
    /// own failure is ignored.
    const DETACH_SCRIPT: &str = "Dismount-DiskImage -ImagePath $env:AE_SRC | Out-Null\n";

    /// The access-path state of `target`: None when no partition lists it, else
    /// Some(whether the disk holding it is read-only).
    fn mounted_at(builtin: &str, target: &str) -> Result<Option<bool>> {
        let output = powershell(builtin, MOUNTED_SCRIPT, &[("AE_DST", target)])?;
        let state = json(builtin, &checked(builtin, &output)?)?;
        let count = state["count"].as_u64().unwrap_or(0);
        Ok((count > 0).then(|| state["readonly"].as_bool().unwrap_or(false)))
    }

    fn detach_image(builtin: &str, source: &str) {
        let _ = powershell(builtin, DETACH_SCRIPT, &[("AE_SRC", source)]);
    }

    /// An environment variable naming a folder, or the fallback when unset.
    fn env_dir(name: &str, fallback: &str) -> String {
        std::env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }

    /// The folders a mount may not use: a drive root, the Windows folder and
    /// everything under it, Program Files (both), ProgramData and the Users
    /// folder. The folders come from the environment, with the usual defaults.
    /// Compared case-insensitively, without a trailing separator.
    fn is_protected(canonical: &str) -> bool {
        let norm = canonical.trim_end_matches('\\').to_ascii_lowercase();
        let b = norm.as_bytes();
        if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
            return true;
        }
        let root = env_dir("SystemRoot", r"C:\Windows");
        let root = root.trim_end_matches('\\').to_ascii_lowercase();
        if norm == root || norm.starts_with(&format!("{root}\\")) {
            return true;
        }
        let drive = env_dir("SystemDrive", "C:");
        [
            env_dir("ProgramFiles", r"C:\Program Files"),
            env_dir("ProgramFiles(x86)", r"C:\Program Files (x86)"),
            env_dir("ProgramData", r"C:\ProgramData"),
            format!(r"{drive}\Users"),
        ]
        .iter()
        .any(|p| p.trim_end_matches('\\').eq_ignore_ascii_case(&norm))
    }

    fn check_protected(builtin: &str, canonical: &str, verb: &str) -> Result<()> {
        if is_protected(canonical) {
            return Err(safety::policy_deny(
                builtin,
                format!("{builtin}: {canonical} is a protected system path; NOTHING WAS {verb}"),
                "choose a folder outside the system folders: drive roots, the Windows folder and its contents, Program Files, ProgramData and Users are refused",
            ));
        }
        Ok(())
    }

    /// canonicalize returns a `\\?\` path. A drive path loses that prefix, for
    /// the messages and for PowerShell. A network share has no drive letter, so
    /// it is refused rather than passed on.
    fn drive_path(builtin: &str, path: PathBuf) -> Result<String> {
        let text = path
            .into_os_string()
            .into_string()
            .map_err(|_| safety::arg_err(format!("{builtin}: the path is not valid UTF-8")))?;
        match text.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => Ok(rest.to_string()),
            _ => Err(safety::arg_err(format!(
                "{builtin}: {text} is not on a drive letter; use a local path such as C:\\mnt\\disk"
            ))),
        }
    }

    pub(super) fn fs_mount(source: &str, target: &str, spec: &MountSpec) -> Result<Value> {
        if spec.bind {
            return Err(safety::arg_err(
                "fs_mount: bind mounts do not exist on Windows; mount a VHD or VHDX file on an empty folder instead",
            ));
        }
        if spec.fstype.is_some() {
            return Err(safety::arg_err(
                "fs_mount: fstype does not apply on Windows, because the file system is read from the image",
            ));
        }
        if !spec.options.is_empty() {
            return Err(safety::arg_err(
                "fs_mount: on Windows the only option is readonly; the options Array is refused",
            ));
        }
        let target_path =
            std::fs::canonicalize(target).map_err(|e| safety::fs_error("fs_mount", target, &e))?;
        let target = drive_path("fs_mount", target_path)?;
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
        let source = drive_path("fs_mount", source_path)?;
        let source_meta =
            std::fs::metadata(&source).map_err(|e| safety::fs_error("fs_mount", &source, &e))?;
        let image = Path::new(&source)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("vhd") || e.eq_ignore_ascii_case("vhdx"));
        if !image || !source_meta.is_file() {
            return Err(safety::arg_err(format!(
                "fs_mount: on Windows fs_mount takes a VHD or VHDX file, and {source} is not one"
            )));
        }

        guard(
            "fs_mount",
            safety::Effect::Privileged,
            "mount",
            &[source.clone(), target.clone()],
        )?;
        let readonly_flag = if spec.readonly { "1" } else { "0" };
        let output = powershell(
            "fs_mount",
            MOUNT_SCRIPT,
            &[
                ("AE_SRC", &source),
                ("AE_DST", &target),
                ("AE_RO", readonly_flag),
            ],
        )?;
        match output.status.code() {
            Some(5) => {
                return Err(safety::arg_err(
                    "fs_mount: the image has no partition to mount; NOTHING WAS MOUNTED",
                ))
            }
            Some(6) => {
                return Err(safety::arg_err(format!(
                    "fs_mount: {target} is not on an NTFS volume; NOTHING WAS MOUNTED"
                )))
            }
            _ => {}
        }
        let made = json("fs_mount", &checked("fs_mount", &output)?)?;
        let Some(fstype) = made["fstype"].as_str() else {
            detach_image("fs_mount", &source);
            return Err(safety::tool_failed(
                "fs_mount",
                "powershell",
                Some(0),
                "its output named no file system; the image was detached",
            ));
        };

        // Read the mount back from the system, not from what was asked.
        let readonly = match mounted_at("fs_mount", &target) {
            Ok(Some(readonly)) => readonly,
            other => {
                detach_image("fs_mount", &source);
                return Err(match other {
                    Err(e) => e,
                    _ => safety::tool_failed(
                        "fs_mount",
                        "Add-PartitionAccessPath",
                        Some(0),
                        &format!(
                            "{target} is not listed as an access path after mounting; the image was detached"
                        ),
                    ),
                });
            }
        };
        Ok(record(vec![
            ("source", Value::Str(source)),
            ("target", Value::Str(target)),
            ("fstype", Value::Str(fstype.to_string())),
            ("readonly", Value::Bool(readonly)),
        ]))
    }

    pub(super) fn fs_unmount(target: &str, lazy: bool) -> Result<Value> {
        if lazy {
            return Err(safety::arg_err(
                "fs_unmount: lazy is a Linux umount option and Windows has no lazy unmount; NOTHING WAS UNMOUNTED",
            ));
        }
        // A folder with a volume mounted on it is a mount-point reparse point,
        // and canonicalize follows it to \\?\Volume{GUID}\ (run 40828954402),
        // so the parent is canonicalized and the folder's own name put back.
        let path = std::path::Path::new(target);
        std::fs::symlink_metadata(path).map_err(|e| safety::fs_error("fs_unmount", target, &e))?;
        let canonical_path = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                std::fs::canonicalize(parent)
                    .map_err(|e| safety::fs_error("fs_unmount", target, &e))?
                    .join(name)
            }
            _ => std::fs::canonicalize(path)
                .map_err(|e| safety::fs_error("fs_unmount", target, &e))?,
        };
        let canonical = drive_path("fs_unmount", canonical_path)?;
        check_protected("fs_unmount", &canonical, "UNMOUNTED")?;
        if mounted_at("fs_unmount", &canonical)?.is_none() {
            return Err(safety::not_found("fs_unmount", "mount point", target));
        }
        guard(
            "fs_unmount",
            safety::Effect::Privileged,
            "unmount",
            &[canonical.clone()],
        )?;
        let output = powershell("fs_unmount", UNMOUNT_SCRIPT, &[("AE_DST", &canonical)])?;
        if output.status.code() == Some(4) {
            return Err(safety::not_found("fs_unmount", "mount point", target));
        }
        checked("fs_unmount", &output)?;
        if mounted_at("fs_unmount", &canonical)?.is_some() {
            return Err(safety::tool_failed(
                "fs_unmount",
                "Remove-PartitionAccessPath",
                Some(0),
                &format!("{target} is still an access path after unmounting"),
            ));
        }
        Ok(record(vec![
            ("target", Value::Str(target.to_string())),
            ("unmounted", Value::Bool(true)),
        ]))
    }
}

#[cfg(target_os = "macos")]
mod sys {
    use super::{guard, parse_ls_acl, record, AclSpec, MountSpec};
    use crate::{safety, value::Value};
    use anyhow::Result;

    const LS: &str = "/bin/ls";
    const CHMOD: &str = "/bin/chmod";
    const HDIUTIL: &str = "/usr/bin/hdiutil";
    const UMOUNT: &str = "/sbin/umount";

    /// Mount points that `fs_mount` refuses as a target and `fs_unmount` refuses,
    /// by exact canonical path. canonicalize turns /tmp into /private/tmp and
    /// /var into /private/var, so the list names the canonical forms.
    const PROTECTED: &[&str] = &[
        "/",
        "/System",
        "/System/Volumes/Data",
        "/System/Volumes/Preboot",
        "/System/Volumes/VM",
        "/System/Volumes/Update",
        "/private",
        "/private/var",
        "/usr",
        "/dev",
        "/Library",
        "/Users",
        "/Applications",
        "/Volumes",
    ];

    /// Run `program` with `args` and return the finished output, whatever its
    /// exit status. A run that does not finish in 30s is stopped and refused as
    /// a budget error; a program that cannot start is a spawn error.
    fn execute(builtin: &str, program: &str, args: &[&str]) -> Result<std::process::Output> {
        let name = program.rsplit('/').next().unwrap_or(program);
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).env("LC_ALL", "C");
        safety::output_with_timeout(cmd, 30).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                safety::budget_exceeded(
                    builtin,
                    format!("`{name}` did not finish within 30s and was stopped"),
                    "the file system or the tool is wedged; retry, or inspect the ACL with ls directly",
                )
            } else {
                safety::spawn_error(builtin, name, &e)
            }
        })
    }

    /// The stdout of a finished run. A refusal by the operating system is
    /// E_OS_DENIED. Any other non-zero exit is a coded tool failure carrying stderr.
    fn finished(builtin: &str, tool: &str, output: &std::process::Output) -> Result<String> {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if stderr.contains("Permission denied") || stderr.contains("Operation not permitted") {
            return Err(safety::os_denied(
                builtin,
                format!("{builtin}: the operating system denied access to the ACL"),
                "check the file's owner and permissions; the shell did not refuse this",
            ));
        }
        Err(safety::tool_failed(
            builtin,
            tool,
            output.status.code(),
            &stderr,
        ))
    }

    pub(super) fn acl_get(path: &str) -> Result<Value> {
        std::fs::metadata(path).map_err(|e| safety::fs_error("acl_get", path, &e))?;
        guard(
            "acl_get",
            safety::Effect::ReadLocal,
            "read the ACL of",
            &[path.to_string()],
        )?;
        // -L follows a symbolic link, as the metadata check and chmod do.
        let output = execute("acl_get", LS, &["-ledL", "--", path])?;
        let text = finished("acl_get", "ls", &output)?;
        parse_ls_acl(path, &text)
    }

    pub(super) fn acl_set(path: &str, spec: &AclSpec) -> Result<Value> {
        std::fs::metadata(path).map_err(|e| safety::fs_error("acl_set", path, &e))?;
        guard(
            "acl_set",
            safety::Effect::Privileged,
            "change the ACL of",
            &[path.to_string()],
        )?;
        // chmod reads a numeric word after the entry as an ACL index, so the
        // path it is given is absolute, which never is one.
        let target = std::fs::canonicalize(path)
            .map_err(|e| safety::fs_error("acl_set", path, &e))?
            .into_os_string()
            .into_string()
            .map_err(|_| safety::arg_err("acl_set: the path is not valid UTF-8"))?;
        let flag = if spec.remove { "-a" } else { "+a" };
        let output = execute(
            "acl_set",
            CHMOD,
            &[flag, spec.arg.as_str(), target.as_str()],
        )?;
        // chmod -a with no entry to remove says so on stderr: "No ACL
        // present" when the file has no ACL at all, "No matching" otherwise.
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if spec.remove
            && !output.status.success()
            && (stderr.contains("no matching") || stderr.contains("no acl present"))
        {
            return Err(safety::not_found("acl_set", "ACL entry", &spec.arg));
        }
        finished("acl_set", "chmod", &output)?;
        Ok(record(vec![
            ("path", Value::Str(path.to_string())),
            ("entry", Value::Str(spec.arg.clone())),
            ("applied", Value::Bool(true)),
        ]))
    }

    /// The canonical form of `path`, as a String. A path that does not exist is
    /// E_NOT_FOUND from the file system check, so nothing else is run.
    fn canonical_path(builtin: &str, path: &str) -> Result<String> {
        std::fs::canonicalize(path)
            .map_err(|e| safety::fs_error(builtin, path, &e))?
            .into_os_string()
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

    /// A C char array from statfs, read up to its NUL.
    fn c_text(field: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = field
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// What the kernel holds for the file system at a path, from statfs(2).
    struct Fs {
        mount_on: String,
        device: String,
        fstype: String,
        read_only: bool,
    }

    fn statfs_of(builtin: &str, path: &str) -> Result<Fs> {
        let c_path = std::ffi::CString::new(path).map_err(|_| {
            safety::arg_err(format!("{builtin}: the path must not contain a NUL byte"))
        })?;
        // SAFETY: statfs writes one struct statfs into `buf`, which is zeroed first,
        // and reads the NUL-terminated path, which c_path keeps alive.
        let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c_path.as_ptr(), &mut buf) } != 0 {
            return Err(safety::fs_error(
                builtin,
                path,
                &std::io::Error::last_os_error(),
            ));
        }
        Ok(Fs {
            mount_on: c_text(&buf.f_mntonname),
            device: c_text(&buf.f_mntfromname),
            fstype: c_text(&buf.f_fstypename),
            read_only: (buf.f_flags & (libc::MNT_RDONLY as u32)) != 0,
        })
    }

    /// The whole-disk node of a device: `/dev/disk4` for `/dev/disk4s1`.
    fn whole_disk(device: &str) -> Option<&str> {
        let digits = device
            .strip_prefix("/dev/disk")?
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        (digits > 0).then(|| &device[.."/dev/disk".len() + digits])
    }

    /// Whether a device is a disk image. `hdiutil info` lists the whole-disk node
    /// of every image it attached, so a device it does not list is not one and
    /// is unmounted with umount instead.
    fn disk_image(device: &str) -> Result<bool> {
        let Some(whole) = whole_disk(device) else {
            return Ok(false);
        };
        let output = execute("fs_unmount", HDIUTIL, &["info"])?;
        let text = finished("fs_unmount", "hdiutil", &output)?;
        Ok(text
            .lines()
            .any(|line| line.split_whitespace().next() == Some(whole)))
    }

    /// Attach a disk-image file on an empty directory and read the mount back.
    /// No bind, fstype or options: those are refused before anything runs.
    pub(super) fn fs_mount(source: &str, target: &str, spec: &MountSpec) -> Result<Value> {
        if spec.bind {
            return Err(safety::arg_err(
                "fs_mount: bind: true is not supported on macOS, which has no bind mounts; NOTHING WAS MOUNTED",
            ));
        }
        if spec.fstype.is_some() {
            return Err(safety::arg_err(
                "fs_mount: fstype is not taken on macOS, because hdiutil detects the format of a disk image; NOTHING WAS MOUNTED",
            ));
        }
        if !spec.options.is_empty() {
            return Err(safety::arg_err(
                "fs_mount: options are not taken on macOS, except readonly; NOTHING WAS MOUNTED",
            ));
        }
        let target = canonical_path("fs_mount", target)?;
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
        let source = canonical_path("fs_mount", source)?;
        let source_meta =
            std::fs::metadata(&source).map_err(|e| safety::fs_error("fs_mount", &source, &e))?;
        if !source_meta.is_file() {
            return Err(safety::arg_err(format!(
                "fs_mount: {source} is not a regular file; macOS fs_mount takes disk-image files \
                 (.dmg, .sparseimage, .iso), not a block device or a directory"
            )));
        }

        guard(
            "fs_mount",
            safety::Effect::Privileged,
            "mount",
            &[source.clone(), target.clone()],
        )?;
        let mut args = vec![
            "attach",
            "-nobrowse",
            "-noautoopen",
            "-mountpoint",
            target.as_str(),
        ];
        if spec.readonly {
            args.push("-readonly");
        }
        args.push(source.as_str());
        let output = execute("fs_mount", HDIUTIL, &args)?;
        finished("fs_mount", "hdiutil", &output)?;

        // Read the mount back from the kernel, not from what was asked.
        let fs = statfs_of("fs_mount", &target)?;
        if fs.mount_on != target {
            // Best effort: an empty directory with nothing attached on it has no
            // image to detach, so the result is ignored.
            let _ = execute("fs_mount", HDIUTIL, &["detach", target.as_str()]);
            return Err(safety::tool_failed(
                "fs_mount",
                "hdiutil",
                Some(0),
                &format!("{target} is not a mount point after attaching {source}"),
            ));
        }
        Ok(record(vec![
            ("source", Value::Str(source)),
            ("target", Value::Str(target)),
            ("fstype", Value::Str(fs.fstype)),
            ("readonly", Value::Bool(fs.read_only)),
        ]))
    }

    /// Detach a disk image, or unmount any other mount point. A target that is
    /// not a mount point is E_NOT_FOUND, as on Linux.
    pub(super) fn fs_unmount(target: &str, lazy: bool) -> Result<Value> {
        let canonical_target = canonical_path("fs_unmount", target)?;
        check_protected("fs_unmount", &canonical_target, "UNMOUNTED")?;
        let fs = statfs_of("fs_unmount", &canonical_target)?;
        if fs.mount_on != canonical_target {
            return Err(safety::not_found("fs_unmount", "mount point", target));
        }
        guard(
            "fs_unmount",
            safety::Effect::Privileged,
            "unmount",
            &[canonical_target.clone()],
        )?;
        let (output, tool) = if disk_image(&fs.device)? {
            let mut args: Vec<&str> = vec!["detach"];
            if lazy {
                args.push("-force");
            }
            args.push(canonical_target.as_str());
            (execute("fs_unmount", HDIUTIL, &args)?, "hdiutil")
        } else {
            let mut args: Vec<&str> = Vec::new();
            if lazy {
                args.push("-f");
            }
            args.push(canonical_target.as_str());
            (execute("fs_unmount", UMOUNT, &args)?, "umount")
        };
        finished("fs_unmount", tool, &output)?;
        let after = statfs_of("fs_unmount", &canonical_target)?;
        if after.mount_on == canonical_target {
            return Err(safety::tool_failed(
                "fs_unmount",
                tool,
                Some(0),
                &format!("{canonical_target} is still a mount point after {tool}"),
            ));
        }
        Ok(record(vec![
            ("target", Value::Str(target.to_string())),
            ("unmounted", Value::Bool(true)),
        ]))
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
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
    use super::{parse_icacls, parse_ls_acl};
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

    /// `ls -led` output for a file with two ACL entries. Written from the
    /// documented `ls -le` form, not captured on macOS: the macos_acl_admin test
    /// on the runner checks the real output.
    const LS_FILE: &str = "-rw-r--r--+ 1 adam  staff  0 Oct 28 10:00 target-scratch/file\n 0: user:nobody allow read,write\n 1: group:staff deny delete\n";

    /// `ls -led` output for a directory whose entry is inherited.
    const LS_DIR: &str = "drwxr-xr-x+ 3 adam  staff  96 Oct 28 10:00 target-scratch/dir\n 0: user:nobody allow list,search,file_inherit,directory_inherit,inherited\n";

    fn words(value: &Value, key: &str) -> Vec<String> {
        match field(value, key) {
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::Str(s) => s.clone(),
                    _ => panic!("{key} holds a non-string"),
                })
                .collect(),
            _ => panic!("{key} is not an array"),
        }
    }

    #[test]
    fn ls_output_gives_owner_group_and_each_entry() {
        let value = parse_ls_acl("target-scratch/file", LS_FILE).unwrap();
        assert_eq!(text(&value, "path"), "target-scratch/file");
        assert_eq!(text(&value, "owner"), "adam");
        assert_eq!(text(&value, "group"), "staff");
        let aces = entries(&value);
        assert_eq!(aces.len(), 2);
        assert!(matches!(field(&aces[0], "index"), Value::Int(0)));
        assert_eq!(text(&aces[0], "tag"), "user");
        assert_eq!(text(&aces[0], "name"), "nobody");
        assert_eq!(text(&aces[0], "kind"), "allow");
        assert_eq!(words(&aces[0], "perms"), vec!["read", "write"]);
        assert!(!flag(&aces[0], "inherited"));
        assert!(flags(&aces[0]).is_empty());
        assert!(matches!(field(&aces[1], "index"), Value::Int(1)));
        assert_eq!(text(&aces[1], "tag"), "group");
        assert_eq!(text(&aces[1], "name"), "staff");
        assert_eq!(text(&aces[1], "kind"), "deny");
        assert_eq!(words(&aces[1], "perms"), vec!["delete"]);
    }

    #[test]
    fn ls_inherited_entry_has_flags_apart_from_perms() {
        let value = parse_ls_acl("target-scratch/dir", LS_DIR).unwrap();
        let aces = entries(&value);
        assert_eq!(aces.len(), 1);
        assert!(flag(&aces[0], "inherited"));
        assert_eq!(words(&aces[0], "perms"), vec!["list", "search"]);
        assert_eq!(flags(&aces[0]), vec!["file_inherit", "directory_inherit"]);
    }

    #[test]
    fn ls_listing_without_entries_gives_an_empty_list() {
        let plain = "-rw-r--r--  1 adam  staff  0 Oct 28 10:00 target-scratch/file\n";
        let value = parse_ls_acl("target-scratch/file", plain).unwrap();
        assert!(entries(&value).is_empty());
    }

    #[test]
    fn ls_unrecognised_lines_are_errors_not_skipped() {
        let head = "-rw-r--r--+ 1 adam  staff  0 Oct 28 10:00 target-scratch/file\n";
        let cases = [
            // No listing at all.
            String::new(),
            // A permission word the grammar does not know.
            format!("{head} 0: user:nobody allow fly\n"),
            // A verb other than allow or deny.
            format!("{head} 0: user:nobody maybe read\n"),
            // A tag other than user or group.
            format!("{head} 0: everyone:x allow read\n"),
            // The Linux tag:qualifier:perms form.
            format!("{head} 0: u:nobody:rw-\n"),
            // No index, or a non-numeric index.
            format!("{head} user:nobody allow read\n"),
            format!("{head}  x: user:nobody allow read\n"),
            // No perms at all, and an extra word.
            format!("{head} 0: user:nobody allow\n"),
            format!("{head} 0: user:nobody allow read extra\n"),
            // A name with a character a macOS name does not have.
            format!("{head} 0: user:no+body allow read\n"),
            // An entry whose only word is an inheritance flag.
            format!("{head} 0: user:nobody allow inherited\n"),
        ];
        for case in cases {
            assert!(
                parse_ls_acl("target-scratch/file", &case).is_err(),
                "accepted {case:?}"
            );
        }
    }
}
