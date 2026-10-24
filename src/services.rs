//! systemd service creation and deletion, and non-interactive sudo.
//!
//! Linux only. Argument validation runs on every platform, so a malformed call
//! is refused the same way everywhere. Valid calls on Windows and macOS then
//! refuse with E_UNIMPLEMENTED and change nothing.
//!
//! `svc_create` writes `/etc/systemd/system/ae-<name>.service`, tagged with a
//! marker line on its first line. `svc_delete` removes only units that carry
//! that marker, so the `ae-` namespace cannot reach hand-written units.
//! `sudo_exec` runs `sudo -n -- argv...` and never prompts for a password.

use crate::value::Value;
use anyhow::Result;
use std::collections::BTreeMap;

/// Written as the first line of every unit this module creates.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const MARKER: &str =
    "# Created by AetherShell svc_create; svc_delete removes only units carrying this line";
const UNIT_PREFIX: &str = "ae-";
const MAX_ARGV: usize = 256;
const MAX_ARG_BYTES: usize = 4096;
const MAX_DESCRIPTION_BYTES: usize = 256;
const SUDO_TIMEOUT_MIN_MS: i64 = 100;
const SUDO_TIMEOUT_MAX_MS: i64 = 600_000;
const SUDO_TIMEOUT_DEFAULT_MS: u64 = 60_000;
/// Each of stdout and stderr keeps only its last this many bytes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const OUTPUT_TAIL_BYTES: usize = 64 * 1024;

const NAME_RULE: &str = "a name matching ^[a-z0-9][a-z0-9_-]{0,63}$";

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

fn name_arg(builtin: &str, value: &Value) -> Result<String> {
    match value {
        Value::Str(s) if valid_name(s) => Ok(s.clone()),
        Value::Str(s) => Err(crate::safety::bad_arg(
            builtin,
            NAME_RULE,
            &format!("{s:?}"),
        )),
        other => Err(crate::safety::bad_arg(
            builtin,
            "a String name",
            other.type_name(),
        )),
    }
}

fn arity(builtin: &str, args: &[Value], min: usize, max: usize, shape: &str) -> Result<()> {
    if args.len() < min || args.len() > max {
        return Err(crate::safety::bad_arg(
            builtin,
            shape,
            &format!("{} arguments", args.len()),
        ));
    }
    Ok(())
}

fn no_pipeline_input(builtin: &str, input: &Option<Value>) -> Result<()> {
    if input.is_some() {
        return Err(crate::safety::bad_arg(
            builtin,
            "no pipeline input",
            "pipeline input",
        ));
    }
    Ok(())
}

/// Options must be a Record whose keys are all in `allowed`. Absent options
/// are an empty map.
fn options_arg(
    builtin: &str,
    value: Option<&Value>,
    allowed: &[&str],
    shape: &str,
) -> Result<BTreeMap<String, Value>> {
    match value {
        None => Ok(BTreeMap::new()),
        Some(Value::Record(map)) => {
            if let Some(key) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
                return Err(crate::safety::bad_arg(
                    builtin,
                    shape,
                    &format!("unknown option {key:?}"),
                ));
            }
            Ok(map.clone())
        }
        Some(other) => Err(crate::safety::bad_arg(builtin, shape, other.type_name())),
    }
}

fn bool_option(builtin: &str, map: &BTreeMap<String, Value>, key: &str) -> Result<bool> {
    match map.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(crate::safety::bad_arg(
            builtin,
            &format!("{key} as a Bool"),
            other.type_name(),
        )),
    }
}

/// Parse an argv Array. `argv[0]` is validated by the caller; every element
/// must be a String without NUL, and within the length limits.
fn argv_arg(builtin: &str, value: &Value, shape: &str) -> Result<Vec<String>> {
    let Value::Array(items) = value else {
        return Err(crate::safety::bad_arg(builtin, shape, value.type_name()));
    };
    if items.is_empty() || items.len() > MAX_ARGV {
        return Err(crate::safety::bad_arg(
            builtin,
            &format!("{shape} with 1 to {MAX_ARGV} elements"),
            &format!("{} elements", items.len()),
        ));
    }
    let mut argv = Vec::with_capacity(items.len());
    for item in items {
        let Value::Str(s) = item else {
            return Err(crate::safety::bad_arg(
                builtin,
                "String elements",
                item.type_name(),
            ));
        };
        if s.contains('\0') || s.len() > MAX_ARG_BYTES {
            return Err(crate::safety::bad_arg(
                builtin,
                &format!("arguments without NUL and at most {MAX_ARG_BYTES} bytes"),
                &format!("an argument of {} bytes", s.len()),
            ));
        }
        argv.push(s.clone());
    }
    Ok(argv)
}

/// Transactions cannot roll back a service or privilege change, so a call
/// inside one is refused before it does anything.
fn refuse_in_transaction(builtin: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(crate::safety::policy_deny(
            builtin,
            format!(
                "{builtin}: refusing inside a transaction: systemd and sudo effects cannot be rolled back"
            ),
            "run it outside the transaction, after commit or rollback",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn not_linux(builtin: &str) -> anyhow::Error {
    crate::safety::unimplemented(
        builtin,
        "systemd services and sudo are implemented on Linux only in 14.0.0; NOTHING WAS CHANGED",
        "use the platform's own service manager or `sudo` directly",
    )
}

// ── svc_create ───────────────────────────────────────────────────────────────

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct CreateSpec {
    name: String,
    argv: Vec<String>,
    description: String,
    user: Option<String>,
    restart: &'static str,
    enable: bool,
    start: bool,
}

fn parse_create(args: &[Value], input: &Option<Value>) -> Result<CreateSpec> {
    const BUILTIN: &str = "svc_create";
    arity(
        BUILTIN,
        args,
        2,
        3,
        "2 or 3 arguments (name, command, options?)",
    )?;
    no_pipeline_input(BUILTIN, input)?;
    let name = name_arg(BUILTIN, &args[0])?;
    let argv = argv_arg(BUILTIN, &args[1], "an Array of String argv")?;
    let first = &argv[0];
    if !first.starts_with('/') || first.len() < 2 {
        return Err(crate::safety::bad_arg(
            BUILTIN,
            "an absolute path as argv[0]",
            &format!("{first:?}"),
        ));
    }
    for arg in &argv {
        if arg.contains(['\n', '\r', '%']) {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "arguments without newline, carriage return or '%'",
                &format!("{arg:?}"),
            ));
        }
    }
    let map = options_arg(
        BUILTIN,
        args.get(2),
        &["description", "user", "restart", "enable", "start"],
        "options {description?, user?, restart?, enable?, start?}",
    )?;
    let description = match map.get("description") {
        None => format!("AetherShell service {UNIT_PREFIX}{name}"),
        Some(Value::Str(d))
            if !d.is_empty()
                && d.len() <= MAX_DESCRIPTION_BYTES
                && !d.contains(['\n', '\r', '\0']) =>
        {
            d.clone()
        }
        Some(other) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "description as a non-empty single-line String of at most 256 bytes",
                other.type_name(),
            ))
        }
    };
    let user = match map.get("user") {
        None => None,
        Some(Value::Str(u)) if valid_account_name(u) => Some(u.clone()),
        Some(Value::Str(u)) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "user as an account name (^[a-z_][a-z0-9_-]{0,31}$)",
                &format!("{u:?}"),
            ))
        }
        Some(other) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "user as an account name (^[a-z_][a-z0-9_-]{0,31}$)",
                other.type_name(),
            ))
        }
    };
    let restart = match map.get("restart") {
        None => "no",
        Some(Value::Str(r)) if r == "no" => "no",
        Some(Value::Str(r)) if r == "on-failure" => "on-failure",
        Some(Value::Str(r)) if r == "always" => "always",
        Some(Value::Str(r)) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "restart as \"no\", \"on-failure\" or \"always\"",
                &format!("{r:?}"),
            ))
        }
        Some(other) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "restart as \"no\", \"on-failure\" or \"always\"",
                other.type_name(),
            ))
        }
    };
    Ok(CreateSpec {
        name,
        argv,
        description,
        user,
        restart,
        enable: bool_option(BUILTIN, &map, "enable")?,
        start: bool_option(BUILTIN, &map, "start")?,
    })
}

fn valid_account_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 32
        && (bytes[0].is_ascii_lowercase() || bytes[0] == b'_')
        && bytes[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

#[cfg(target_os = "linux")]
pub(crate) fn svc_create(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let spec = parse_create(&args, &input)?;
    refuse_in_transaction("svc_create")?;
    linux::create(spec)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn svc_create(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    parse_create(&args, &input)?;
    refuse_in_transaction("svc_create")?;
    Err(not_linux("svc_create"))
}

// ── svc_delete ───────────────────────────────────────────────────────────────

fn parse_delete(args: &[Value], input: &Option<Value>) -> Result<String> {
    const BUILTIN: &str = "svc_delete";
    arity(BUILTIN, args, 1, 1, "1 argument (name)")?;
    no_pipeline_input(BUILTIN, input)?;
    name_arg(BUILTIN, &args[0])
}

#[cfg(target_os = "linux")]
pub(crate) fn svc_delete(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let name = parse_delete(&args, &input)?;
    refuse_in_transaction("svc_delete")?;
    linux::delete(name)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn svc_delete(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    parse_delete(&args, &input)?;
    refuse_in_transaction("svc_delete")?;
    Err(not_linux("svc_delete"))
}

// ── sudo_exec ────────────────────────────────────────────────────────────────

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct SudoSpec {
    argv: Vec<String>,
    timeout_ms: u64,
}

fn parse_sudo(args: &[Value], input: &Option<Value>) -> Result<SudoSpec> {
    const BUILTIN: &str = "sudo_exec";
    arity(BUILTIN, args, 1, 2, "1 or 2 arguments (command, options?)")?;
    no_pipeline_input(BUILTIN, input)?;
    let argv = argv_arg(BUILTIN, &args[0], "an Array of String argv")?;
    if argv[0].is_empty() || argv[0].starts_with('-') {
        return Err(crate::safety::bad_arg(
            BUILTIN,
            "an argv[0] that is non-empty and does not start with '-'",
            &format!("{:?}", argv[0]),
        ));
    }
    let map = options_arg(
        BUILTIN,
        args.get(1),
        &["timeout_ms"],
        "options {timeout_ms?}",
    )?;
    let timeout_ms = match map.get("timeout_ms") {
        None => SUDO_TIMEOUT_DEFAULT_MS,
        Some(Value::Int(n)) if (SUDO_TIMEOUT_MIN_MS..=SUDO_TIMEOUT_MAX_MS).contains(n) => *n as u64,
        Some(Value::Int(n)) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "timeout_ms as an Int from 100 to 600000",
                &n.to_string(),
            ))
        }
        Some(other) => {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                "timeout_ms as an Int from 100 to 600000",
                other.type_name(),
            ))
        }
    };
    Ok(SudoSpec { argv, timeout_ms })
}

#[cfg(target_os = "linux")]
pub(crate) fn sudo_exec(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let spec = parse_sudo(&args, &input)?;
    refuse_in_transaction("sudo_exec")?;
    linux::sudo(spec)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn sudo_exec(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    parse_sudo(&args, &input)?;
    refuse_in_transaction("sudo_exec")?;
    Err(not_linux("sudo_exec"))
}

// ── Linux backend ───────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::safety::{
        budget_exceeded, fs_error, not_found, policy_deny, spawn_error, tool_failed,
    };
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    const SYSTEMCTL: &str = "/usr/bin/systemctl";
    const GETENT: &str = "/usr/bin/getent";
    const SUDO: &str = "/usr/bin/sudo";
    const UNIT_DIR: &str = "/etc/systemd/system";
    /// Bound on each systemctl call, so a stuck manager cannot hang the shell.
    const TOOL_TIMEOUT_SECS: u64 = 60;

    fn unit_name(name: &str) -> String {
        format!("{UNIT_PREFIX}{name}.service")
    }

    fn run_tool(builtin: &str, args: &[&str]) -> Result<std::process::Output> {
        let mut cmd = Command::new(SYSTEMCTL);
        cmd.args(args);
        crate::safety::output_with_timeout(cmd, TOOL_TIMEOUT_SECS).map_err(|e| {
            if e.kind() == ErrorKind::TimedOut {
                budget_exceeded(
                    builtin,
                    format!(
                        "`systemctl {}` did not finish within {TOOL_TIMEOUT_SECS}s",
                        args.join(" ")
                    ),
                    "check the manager with `systemctl status`, then retry",
                )
            } else {
                spawn_error(builtin, SYSTEMCTL, &e)
            }
        })
    }

    fn run_checked(builtin: &str, args: &[&str]) -> Result<()> {
        let out = run_tool(builtin, args)?;
        if out.status.success() {
            return Ok(());
        }
        Err(tool_failed(
            builtin,
            &format!("systemctl {}", args.join(" ")),
            out.status.code(),
            &String::from_utf8_lossy(&out.stderr),
        ))
    }

    /// Quote one ExecStart word in systemd's syntax: double quotes, with
    /// backslash and double quote escaped and `$` doubled so it is not
    /// expanded as a variable. `%` and newlines are refused before this.
    pub(super) fn quote_word(word: &str) -> String {
        let mut quoted = String::with_capacity(word.len() + 2);
        quoted.push('"');
        for c in word.chars() {
            match c {
                '\\' => quoted.push_str("\\\\"),
                '"' => quoted.push_str("\\\""),
                '$' => quoted.push_str("$$"),
                other => quoted.push(other),
            }
        }
        quoted.push('"');
        quoted
    }

    pub(super) fn render_unit(spec: &CreateSpec) -> String {
        let exec: Vec<String> = spec.argv.iter().map(|a| quote_word(a)).collect();
        let mut text = String::new();
        text.push_str(MARKER);
        text.push('\n');
        text.push_str("[Unit]\n");
        // Description undergoes specifier expansion, so a literal % is %%.
        text.push_str(&format!(
            "Description={}\n",
            spec.description.replace('%', "%%")
        ));
        text.push_str("\n[Service]\nType=simple\n");
        text.push_str(&format!("ExecStart={}\n", exec.join(" ")));
        text.push_str(&format!("Restart={}\n", spec.restart));
        if let Some(user) = &spec.user {
            text.push_str(&format!("User={user}\n"));
        }
        text.push_str("\n[Install]\nWantedBy=multi-user.target\n");
        text
    }

    /// Create the file without ever replacing one. The text goes to a temporary
    /// file in the same directory, which is then hard-linked into place. The
    /// link fails with AlreadyExists if a unit appeared meanwhile, which a
    /// rename would silently overwrite.
    fn write_unit_file(path: &Path, text: &str) -> std::io::Result<()> {
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let tmp = path.with_file_name(format!(".{file}.tmp-{}", std::process::id()));
        let result: std::io::Result<()> = (|| {
            let mut handle = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .open(&tmp)?;
            handle.write_all(text.as_bytes())?;
            handle.sync_all()?;
            drop(handle);
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
            std::fs::hard_link(&tmp, path)
        })();
        let _ = std::fs::remove_file(&tmp);
        result
    }

    fn account_exists(user: &str) -> Result<()> {
        let mut cmd = Command::new(GETENT);
        cmd.args(["passwd", user]);
        let out = crate::safety::output_with_timeout(cmd, TOOL_TIMEOUT_SECS)
            .map_err(|e| spawn_error("svc_create", GETENT, &e))?;
        match out.status.code() {
            Some(0) => Ok(()),
            Some(2) => Err(not_found("svc_create", "account", user)),
            code => Err(tool_failed(
                "svc_create",
                &format!("getent passwd {user}"),
                code,
                &String::from_utf8_lossy(&out.stderr),
            )),
        }
    }

    fn check_executable(argv0: &str) -> Result<()> {
        let refuse = |why: &str| {
            crate::safety::bad_arg(
                "svc_create",
                "an existing executable file as argv[0]",
                &format!("{argv0:?} ({why})"),
            )
        };
        let meta = std::fs::metadata(argv0).map_err(|_| refuse("does not exist"))?;
        if !meta.is_file() {
            return Err(refuse("not a regular file"));
        }
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(refuse("not executable"));
        }
        Ok(())
    }

    pub(super) fn create(spec: CreateSpec) -> Result<Value> {
        const BUILTIN: &str = "svc_create";
        let unit = unit_name(&spec.name);
        let unit_path = PathBuf::from(UNIT_DIR).join(&unit);

        check_executable(&spec.argv[0])?;
        if let Some(user) = &spec.user {
            account_exists(user)?;
        }
        if unit_path.symlink_metadata().is_ok() {
            return Err(policy_deny(
                BUILTIN,
                format!(
                    "{BUILTIN}: {unit_path:?} already exists; a unit file is never overwritten"
                ),
                "choose another name",
            ));
        }

        let text = render_unit(&spec);
        match write_unit_file(&unit_path, &text) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(policy_deny(
                    BUILTIN,
                    format!("{BUILTIN}: {unit_path:?} appeared while writing; a unit file is never overwritten"),
                    "choose another name",
                ));
            }
            Err(e) => return Err(fs_error(BUILTIN, &unit_path, &e)),
        }

        if let Err(e) = configure(&spec, &unit) {
            // Undo everything this call did, so no half-created service stays.
            if spec.start {
                let _ = run_tool(BUILTIN, &["stop", unit.as_str()]);
            }
            if spec.enable {
                let _ = run_tool(BUILTIN, &["disable", unit.as_str()]);
            }
            let _ = std::fs::remove_file(&unit_path);
            let _ = run_tool(BUILTIN, &["daemon-reload"]);
            return Err(e);
        }

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(unit));
        record.insert(
            "unit_path".to_string(),
            Value::Str(unit_path.display().to_string()),
        );
        record.insert("enabled".to_string(), Value::Bool(spec.enable));
        record.insert("started".to_string(), Value::Bool(spec.start));
        Ok(Value::Record(record))
    }

    fn configure(spec: &CreateSpec, unit: &str) -> Result<()> {
        const BUILTIN: &str = "svc_create";
        run_checked(BUILTIN, &["daemon-reload"])?;
        if spec.enable {
            run_checked(BUILTIN, &["enable", unit])?;
        }
        if spec.start {
            run_checked(BUILTIN, &["start", unit])?;
        }
        Ok(())
    }

    fn is_running(builtin: &str, unit: &str) -> Result<bool> {
        let out = run_tool(builtin, &["is-active", unit])?;
        let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(matches!(
            state.as_str(),
            "active" | "activating" | "reloading" | "deactivating"
        ))
    }

    fn is_enabled(builtin: &str, unit: &str) -> Result<bool> {
        let out = run_tool(builtin, &["is-enabled", unit])?;
        Ok(String::from_utf8_lossy(&out.stdout).trim() == "enabled")
    }

    pub(super) fn delete(name: String) -> Result<Value> {
        const BUILTIN: &str = "svc_delete";
        let unit = unit_name(&name);
        let unit_path = PathBuf::from(UNIT_DIR).join(&unit);

        let meta = match unit_path.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(not_found(BUILTIN, "unit", &unit));
            }
            Err(e) => return Err(fs_error(BUILTIN, &unit_path, &e)),
        };
        if !meta.is_file() {
            return Err(policy_deny(
                BUILTIN,
                format!("{BUILTIN}: {unit_path:?} is not a regular file; refusing to remove it"),
                "only unit files svc_create made can be deleted here",
            ));
        }
        let text = std::fs::read(&unit_path).map_err(|e| fs_error(BUILTIN, &unit_path, &e))?;
        let text = String::from_utf8_lossy(&text);
        if !text.lines().any(|line| line == MARKER) {
            return Err(policy_deny(
                BUILTIN,
                format!("{BUILTIN}: {unit_path:?} has no AetherShell marker line; refusing to remove it"),
                "only units svc_create created can be deleted; remove hand-written units by hand",
            ));
        }

        let stopped = if is_running(BUILTIN, &unit)? {
            let out = run_tool(BUILTIN, &["stop", unit.as_str()])?;
            // A stop that fails only because the unit was already down is fine;
            // anything still running afterwards is an error.
            if !out.status.success() && is_running(BUILTIN, &unit)? {
                return Err(tool_failed(
                    BUILTIN,
                    &format!("systemctl stop {unit}"),
                    out.status.code(),
                    &String::from_utf8_lossy(&out.stderr),
                ));
            }
            true
        } else {
            false
        };
        let disabled = if is_enabled(BUILTIN, &unit)? {
            run_checked(BUILTIN, &["disable", unit.as_str()])?;
            true
        } else {
            false
        };
        std::fs::remove_file(&unit_path).map_err(|e| fs_error(BUILTIN, &unit_path, &e))?;
        run_checked(BUILTIN, &["daemon-reload"])?;

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(unit));
        record.insert("stopped".to_string(), Value::Bool(stopped));
        record.insert("disabled".to_string(), Value::Bool(disabled));
        record.insert("removed".to_string(), Value::Bool(true));
        Ok(Value::Record(record))
    }

    /// True when sudo refused because it would have to ask for a password.
    /// Matched on a whole line, so a command that merely mentions the phrase
    /// inside a longer message does not trip it.
    pub(super) fn sudo_password_required(stderr: &str) -> bool {
        stderr
            .lines()
            .any(|line| line.trim() == "sudo: a password is required")
    }

    #[derive(Default)]
    struct Capture {
        bytes: Vec<u8>,
        finished: bool,
    }

    type Shared = Arc<Mutex<Capture>>;

    fn lock(shared: &Shared) -> MutexGuard<'_, Capture> {
        shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reads a pipe until EOF, keeping only the most recent output. The
    /// buffer is shared so the caller can read it even when a grandchild
    /// holds the pipe open after the timeout kill.
    fn spawn_reader<R: Read + Send + 'static>(mut pipe: R, shared: Shared) {
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut capture = lock(&shared);
                        capture.bytes.extend_from_slice(&chunk[..n]);
                        if capture.bytes.len() > 2 * OUTPUT_TAIL_BYTES {
                            let cut = capture.bytes.len() - OUTPUT_TAIL_BYTES;
                            capture.bytes.drain(..cut);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            lock(&shared).finished = true;
        });
    }

    fn tail_text(shared: &Shared) -> String {
        let capture = lock(shared);
        let start = capture.bytes.len().saturating_sub(OUTPUT_TAIL_BYTES);
        String::from_utf8_lossy(&capture.bytes[start..]).into_owned()
    }

    fn finished(shared: &Shared) -> bool {
        lock(shared).finished
    }

    /// SIGKILL the child's whole process group. The child was started as a
    /// group leader, so this also reaches the command that sudo ran.
    fn kill_group(pid: u32) {
        // SAFETY: plain libc call with a negated pid; it signals a process group
        // this function created, and has no memory effects.
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
    }

    pub(super) fn sudo(spec: SudoSpec) -> Result<Value> {
        use std::os::unix::process::CommandExt;

        const BUILTIN: &str = "sudo_exec";
        let mut cmd = Command::new(SUDO);
        cmd.arg("-n")
            .arg("--")
            .args(&spec.argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = cmd.spawn().map_err(|e| spawn_error(BUILTIN, "sudo", &e))?;

        let stdout = Arc::new(Mutex::new(Capture::default()));
        let stderr = Arc::new(Mutex::new(Capture::default()));
        if let Some(pipe) = child.stdout.take() {
            spawn_reader(pipe, Arc::clone(&stdout));
        } else {
            lock(&stdout).finished = true;
        }
        if let Some(pipe) = child.stderr.take() {
            spawn_reader(pipe, Arc::clone(&stderr));
        } else {
            lock(&stderr).finished = true;
        }

        let deadline = Instant::now() + Duration::from_millis(spec.timeout_ms);
        let (status, timed_out) = loop {
            match child.try_wait() {
                Ok(Some(status)) => break (Some(status), false),
                Ok(None) if Instant::now() >= deadline => {
                    kill_group(child.id());
                    let _ = child.kill();
                    break (child.wait().ok(), true);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    kill_group(child.id());
                    let _ = child.kill();
                    return Err(spawn_error(BUILTIN, "sudo", &e));
                }
            }
        };

        // Give the readers a moment to reach EOF. A grandchild that keeps a pipe
        // open must not hold the call indefinitely.
        let grace = Instant::now() + Duration::from_secs(2);
        while !(finished(&stdout) && finished(&stderr)) && Instant::now() < grace {
            std::thread::sleep(Duration::from_millis(10));
        }

        let stdout_text = tail_text(&stdout);
        let stderr_text = tail_text(&stderr);
        if !timed_out && sudo_password_required(&stderr_text) {
            return Err(policy_deny(
                BUILTIN,
                format!("{BUILTIN}: sudo needs a password for this user, so the command was not run"),
                "run it as root, or grant this command NOPASSWD in sudoers; AetherShell never supplies a password",
            ));
        }

        let exit_code = match status.and_then(|s| s.code()) {
            Some(code) if !timed_out => Value::Int(i64::from(code)),
            _ => Value::Null,
        };
        let mut record = BTreeMap::new();
        record.insert("exit_code".to_string(), exit_code);
        record.insert("stdout".to_string(), Value::Str(stdout_text));
        record.insert("stderr".to_string(), Value::Str(stderr_text));
        record.insert("timed_out".to_string(), Value::Bool(timed_out));
        Ok(Value::Record(record))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn quoting_escapes_backslash_quote_and_dollar() {
            assert_eq!(quote_word(r#"a b"c\d$e"#), r#""a b\"c\\d$$e""#);
            assert_eq!(quote_word(""), "\"\"");
        }

        #[test]
        fn password_prompt_is_recognised_only_as_a_whole_line() {
            assert!(sudo_password_required("sudo: a password is required\n"));
            assert!(sudo_password_required("  sudo: a password is required  \n"));
            assert!(!sudo_password_required(
                "the tool said: sudo: a password is required, really\n"
            ));
            assert!(!sudo_password_required(
                "sudo: a terminal is required to read the password\n"
            ));
            assert!(!sudo_password_required(""));
        }
    }
}
