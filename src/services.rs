//! systemd service creation and deletion, and non-interactive sudo.
//!
//! Linux and Windows. Argument validation runs on every platform, so a malformed
//! call is refused the same way everywhere. `svc_create` and `svc_delete` act on
//! systemd units on Linux and on Service Control Manager services on Windows.
//! On macOS, and for `sudo_exec` everywhere but Linux, a valid call refuses with
//! E_UNIMPLEMENTED and changes nothing.
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
                "{builtin}: refusing inside a transaction: service and sudo effects cannot be rolled back"
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
        "systemd services and sudo are implemented on Linux only in 13.0.0, and service creation and deletion also on Windows; NOTHING WAS CHANGED",
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
    if !absolute_program(first) {
        return Err(crate::safety::bad_arg(
            BUILTIN,
            "an absolute path as argv[0]",
            &format!("{first:?}"),
        ));
    }
    windows_program_alone(BUILTIN, &argv)?;
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

/// Whether argv[0] is an absolute program path on this platform: a leading `/`
/// on Unix, a drive-letter or UNC path on Windows. On Windows a path containing
/// a double quote is refused, since the quote is what delimits the binary path.
fn absolute_program(path: &str) -> bool {
    if cfg!(windows) {
        std::path::Path::new(path).is_absolute() && !path.contains('"')
    } else {
        path.starts_with('/') && path.len() >= 2
    }
}

/// A Windows service runs one program. Its binary path is built from argv[0]
/// alone, so extra arguments are refused rather than joined into a command line.
fn windows_program_alone(builtin: &str, argv: &[String]) -> Result<()> {
    if cfg!(windows) && argv.len() > 1 {
        return Err(crate::safety::bad_arg(
            builtin,
            "argv holding the program alone on Windows: a service takes no arguments in this release",
            &format!("{} elements", argv.len()),
        ));
    }
    Ok(())
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

#[cfg(windows)]
pub(crate) fn svc_create(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let spec = parse_create(&args, &input)?;
    refuse_in_transaction("svc_create")?;
    scm::create(spec)
}

#[cfg(not(any(target_os = "linux", windows)))]
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

#[cfg(windows)]
pub(crate) fn svc_delete(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let name = parse_delete(&args, &input)?;
    refuse_in_transaction("svc_delete")?;
    scm::delete(name)
}

#[cfg(not(any(target_os = "linux", windows)))]
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

// ── Windows backend ─────────────────────────────────────────────────────────

/// The Service Control Manager, called through advapi32. A service has no unit
/// file, so the marker `svc_delete` checks for is the start of the service's
/// description: every description this module writes begins with it.
#[cfg(windows)]
mod scm {
    use super::*;
    use crate::safety::{bad_state, budget_exceeded, not_found, os_denied, policy_deny};
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};

    const WIN_MARKER: &str = "[AetherShell svc_create] ";
    /// How long `svc_delete` waits for a running service to report stopped.
    const STOP_WAIT: Duration = Duration::from_secs(30);
    /// Delay before each of the three restarts that `restart: "on-failure"` sets.
    const RESTART_DELAY_MS: u32 = 60_000;
    /// The failure count starts again after a day without a failure.
    const RESET_PERIOD_SECS: u32 = 86_400;

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> *mut c_void;
        fn OpenServiceW(scm: *mut c_void, name: *const u16, access: u32) -> *mut c_void;
        #[allow(clippy::too_many_arguments)]
        fn CreateServiceW(
            scm: *mut c_void,
            name: *const u16,
            display: *const u16,
            access: u32,
            service_type: u32,
            start_type: u32,
            error_control: u32,
            binary_path: *const u16,
            load_order_group: *const u16,
            tag_id: *mut u32,
            dependencies: *const u16,
            start_name: *const u16,
            password: *const u16,
        ) -> *mut c_void;
        fn ChangeServiceConfig2W(service: *mut c_void, level: u32, info: *mut c_void) -> i32;
        fn QueryServiceConfigW(
            service: *mut c_void,
            config: *mut c_void,
            size: u32,
            needed: *mut u32,
        ) -> i32;
        fn QueryServiceConfig2W(
            service: *mut c_void,
            level: u32,
            buffer: *mut c_void,
            size: u32,
            needed: *mut u32,
        ) -> i32;
        fn QueryServiceStatusEx(
            service: *mut c_void,
            level: u32,
            buffer: *mut c_void,
            size: u32,
            needed: *mut u32,
        ) -> i32;
        fn StartServiceW(service: *mut c_void, argc: u32, argv: *const *const u16) -> i32;
        fn ControlService(service: *mut c_void, control: u32, status: *mut c_void) -> i32;
        fn DeleteService(service: *mut c_void) -> i32;
        fn CloseServiceHandle(handle: *mut c_void) -> i32;
    }

    const SC_MANAGER_CONNECT: u32 = 0x0001;
    const SC_MANAGER_CREATE_SERVICE: u32 = 0x0002;
    const SERVICE_QUERY_CONFIG: u32 = 0x0001;
    const SERVICE_CHANGE_CONFIG: u32 = 0x0002;
    const SERVICE_QUERY_STATUS: u32 = 0x0004;
    const SERVICE_START: u32 = 0x0010;
    const SERVICE_STOP: u32 = 0x0020;
    const DELETE: u32 = 0x0001_0000;
    /// The access a created service's handle needs for the calls that follow.
    const SERVICE_RIGHTS: u32 =
        SERVICE_CHANGE_CONFIG | SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP | DELETE;
    const SERVICE_WIN32_OWN_PROCESS: u32 = 0x10;
    const SERVICE_AUTO_START: u32 = 2;
    const SERVICE_DEMAND_START: u32 = 3;
    const SERVICE_DISABLED: u32 = 4;
    const SERVICE_ERROR_NORMAL: u32 = 1;
    const SERVICE_CONTROL_STOP: u32 = 1;
    const SERVICE_STOPPED: u32 = 1;
    const SC_STATUS_PROCESS_INFO: u32 = 0;
    const SERVICE_CONFIG_DESCRIPTION: u32 = 1;
    const SERVICE_CONFIG_FAILURE_ACTIONS: u32 = 2;
    const SERVICE_CONFIG_FAILURE_ACTIONS_FLAG: u32 = 4;
    const SC_ACTION_RESTART: u32 = 1;

    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;
    const ERROR_SERVICE_CANNOT_ACCEPT_CTRL: u32 = 1061;
    const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;
    const ERROR_SERVICE_MARKED_FOR_DELETE: u32 = 1072;
    const ERROR_SERVICE_EXISTS: u32 = 1073;
    const ERROR_DUPLICATE_SERVICE_NAME: u32 = 1078;
    const ERROR_INVALID_SERVICE_ACCOUNT: u32 = 1057;
    const ERROR_SERVICE_LOGON_FAILED: u32 = 1069;
    const ERROR_LOGON_FAILURE: u32 = 1326;

    /// SERVICE_STATUS_PROCESS, the buffer QueryServiceStatusEx fills.
    #[allow(dead_code)]
    #[derive(Default)]
    #[repr(C)]
    struct ServiceStatusProcess {
        service_type: u32,
        current_state: u32,
        controls_accepted: u32,
        win32_exit_code: u32,
        service_specific_exit_code: u32,
        check_point: u32,
        wait_hint: u32,
        process_id: u32,
        service_flags: u32,
    }

    /// The start of QUERY_SERVICE_CONFIGW; only the start type is read.
    #[allow(dead_code)]
    #[repr(C)]
    struct QueryConfig {
        service_type: u32,
        start_type: u32,
        error_control: u32,
        binary_path: *mut u16,
        load_order_group: *mut u16,
        tag_id: u32,
        dependencies: *mut u16,
        start_name: *mut u16,
        display_name: *mut u16,
    }

    #[allow(dead_code)]
    #[repr(C)]
    struct ServiceDescriptionW {
        description: *mut u16,
    }

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct ScAction {
        kind: u32,
        delay_ms: u32,
    }

    #[allow(dead_code)]
    #[repr(C)]
    struct ServiceFailureActionsW {
        reset_period: u32,
        reboot_message: *mut u16,
        command: *mut u16,
        count: u32,
        actions: *mut ScAction,
    }

    #[allow(dead_code)]
    #[repr(C)]
    struct ServiceFailureFlag {
        on_non_crash_failures: i32,
    }

    /// An open SCM or service handle, closed when it goes out of scope.
    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful Open or Create call and
            // is closed exactly once, here.
            unsafe { CloseServiceHandle(self.0) };
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error() -> u32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
    }

    /// Maps a failed service-control call to a coded error. Nothing is changed
    /// by a call that fails, so each arm only names the cause.
    fn scm_error(builtin: &str, call: &str, code: u32, service: &str) -> anyhow::Error {
        let win = std::io::Error::from_raw_os_error(code as i32);
        match code {
            ERROR_ACCESS_DENIED => os_denied(
                builtin,
                format!("{builtin}: the service control manager refused {call} for '{service}' (access denied)"),
                "run as an administrator; the shell did not refuse this",
            ),
            ERROR_SERVICE_DOES_NOT_EXIST => not_found(builtin, "service", service),
            ERROR_SERVICE_EXISTS | ERROR_DUPLICATE_SERVICE_NAME => bad_state(
                builtin,
                &format!("{service} already exists, or its display name is taken; a service is never overwritten"),
                "choose another name",
            ),
            ERROR_SERVICE_MARKED_FOR_DELETE => bad_state(
                builtin,
                &format!("{service} is marked for deletion and goes when the machine restarts"),
                "restart the machine, then retry",
            ),
            ERROR_INVALID_SERVICE_ACCOUNT | ERROR_SERVICE_LOGON_FAILED | ERROR_LOGON_FAILURE => {
                os_denied(
                    builtin,
                    format!("{builtin}: {call} could not log on the account of '{service}' ({win})"),
                    "use a built-in or virtual account such as LocalSystem; AetherShell never supplies a password",
                )
            }
            _ => bad_state(
                builtin,
                &format!("{call} failed for '{service}': {win} (Win32 error {code})"),
                "check the service with `sc.exe query`, then retry",
            ),
        }
    }

    fn open_scm(builtin: &str, access: u32) -> Result<Handle> {
        // SAFETY: null machine and database names select the local machine's
        // active service database; nothing is retained past the call.
        let raw = unsafe { OpenSCManagerW(null(), null(), access) };
        if raw.is_null() {
            return Err(scm_error(builtin, "OpenSCManagerW", last_error(), ""));
        }
        Ok(Handle(raw))
    }

    fn open_service(builtin: &str, scm: &Handle, service: &str, access: u32) -> Result<Handle> {
        let name = wide(service);
        // SAFETY: `name` is NUL-terminated and lives until the call returns.
        let raw = unsafe { OpenServiceW(scm.0, name.as_ptr(), access) };
        if raw.is_null() {
            return Err(scm_error(builtin, "OpenServiceW", last_error(), service));
        }
        Ok(Handle(raw))
    }

    /// The program must be an existing .exe file: the path is the service's
    /// binary, and the manager runs nothing else.
    fn check_executable(argv0: &str) -> Result<()> {
        let refuse = |why: &str| {
            crate::safety::bad_arg(
                "svc_create",
                "an existing .exe file as argv[0]",
                &format!("{argv0:?} ({why})"),
            )
        };
        let meta = std::fs::metadata(argv0).map_err(|_| refuse("does not exist"))?;
        if !meta.is_file() {
            return Err(refuse("not a regular file"));
        }
        let is_exe = std::path::Path::new(argv0)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        if !is_exe {
            return Err(refuse("not an .exe file"));
        }
        Ok(())
    }

    fn change_config2(
        builtin: &str,
        svc: &Handle,
        service: &str,
        level: u32,
        info: *mut c_void,
    ) -> Result<()> {
        // SAFETY: `info` points at the structure `level` names, and the caller
        // keeps that structure and every pointer in it alive for the call.
        if unsafe { ChangeServiceConfig2W(svc.0, level, info) } == 0 {
            return Err(scm_error(
                builtin,
                "ChangeServiceConfig2W",
                last_error(),
                service,
            ));
        }
        Ok(())
    }

    /// The settings a new service gets after CreateServiceW: description,
    /// restart actions, and the start when asked. Any failure here is undone by
    /// the caller.
    fn configure_new(
        svc: &Handle,
        spec: &CreateSpec,
        service: &str,
        description: &str,
    ) -> Result<()> {
        const BUILTIN: &str = "svc_create";
        let mut text = wide(description);
        let mut info = ServiceDescriptionW {
            description: text.as_mut_ptr(),
        };
        change_config2(
            BUILTIN,
            svc,
            service,
            SERVICE_CONFIG_DESCRIPTION,
            (&mut info as *mut ServiceDescriptionW).cast(),
        )?;
        if spec.restart == "on-failure" {
            let mut actions = [ScAction {
                kind: SC_ACTION_RESTART,
                delay_ms: RESTART_DELAY_MS,
            }; 3];
            let mut failure = ServiceFailureActionsW {
                reset_period: RESET_PERIOD_SECS,
                reboot_message: null_mut(),
                command: null_mut(),
                count: actions.len() as u32,
                actions: actions.as_mut_ptr(),
            };
            change_config2(
                BUILTIN,
                svc,
                service,
                SERVICE_CONFIG_FAILURE_ACTIONS,
                (&mut failure as *mut ServiceFailureActionsW).cast(),
            )?;
            // A non-zero exit is a failure as well, as `on-failure` means on Linux.
            let mut flag = ServiceFailureFlag {
                on_non_crash_failures: 1,
            };
            change_config2(
                BUILTIN,
                svc,
                service,
                SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
                (&mut flag as *mut ServiceFailureFlag).cast(),
            )?;
        }
        if spec.start {
            // SAFETY: no arguments are passed, so the argument vector is null.
            if unsafe { StartServiceW(svc.0, 0, null()) } == 0 {
                return Err(scm_error(BUILTIN, "StartServiceW", last_error(), service));
            }
        }
        Ok(())
    }

    /// Stops and deletes a service that a failed create left behind. Both calls
    /// are best effort: the caller is already returning an error.
    fn undo_create(svc: &Handle) {
        let mut status = ServiceStatusProcess::default();
        // SAFETY: the status buffer is a SERVICE_STATUS-sized prefix of
        // ServiceStatusProcess, and the handle is open.
        unsafe {
            ControlService(
                svc.0,
                SERVICE_CONTROL_STOP,
                (&mut status as *mut ServiceStatusProcess).cast(),
            );
            DeleteService(svc.0);
        }
    }

    pub(super) fn create(spec: CreateSpec) -> Result<Value> {
        const BUILTIN: &str = "svc_create";
        if spec.restart == "always" {
            return Err(crate::safety::unimplemented(
                BUILTIN,
                "restart \"always\" would also restart after a clean exit, which the Windows service control manager cannot do; NOTHING WAS CHANGED",
                "use restart \"on-failure\" or \"no\"",
            ));
        }
        let service = format!("{UNIT_PREFIX}{}", spec.name);
        check_executable(&spec.argv[0])?;
        if let Some(user) = &spec.user {
            crate::builtins::windows_account_sid(BUILTIN, user)?;
        }

        let scm = open_scm(BUILTIN, SC_MANAGER_CREATE_SERVICE)?;
        let name = wide(&service);
        // The binary path is argv[0] alone, quoted; argument-free by check above.
        let binary = wide(&format!("\"{}\"", spec.argv[0]));
        let account = spec.user.as_deref().map(wide);
        let start_type = if spec.enable {
            SERVICE_AUTO_START
        } else {
            SERVICE_DEMAND_START
        };
        // SAFETY: every string is NUL-terminated and lives until the call returns.
        // Null is the documented value for the load-order group, tag, dependencies
        // and password; a null account means LocalSystem.
        let raw = unsafe {
            CreateServiceW(
                scm.0,
                name.as_ptr(),
                name.as_ptr(),
                SERVICE_RIGHTS,
                SERVICE_WIN32_OWN_PROCESS,
                start_type,
                SERVICE_ERROR_NORMAL,
                binary.as_ptr(),
                null(),
                null_mut(),
                null(),
                account.as_ref().map_or(null(), |w| w.as_ptr()),
                null(),
            )
        };
        if raw.is_null() {
            return Err(scm_error(
                BUILTIN,
                "CreateServiceW",
                last_error(),
                &service,
            ));
        }
        let svc = Handle(raw);
        let description = format!("{WIN_MARKER}{}", spec.description);
        if let Err(e) = configure_new(&svc, &spec, &service, &description) {
            undo_create(&svc);
            return Err(e);
        }

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(service.clone()));
        record.insert(
            "unit_path".to_string(),
            Value::Str(format!(
                r"HKLM\SYSTEM\CurrentControlSet\Services\{service}"
            )),
        );
        record.insert("enabled".to_string(), Value::Bool(spec.enable));
        record.insert("started".to_string(), Value::Bool(spec.start));
        Ok(Value::Record(record))
    }

    /// Reads a configuration-style buffer through the two-call pattern: a sizing
    /// call that fails with ERROR_INSUFFICIENT_BUFFER, then the call with a
    /// buffer of that size. The buffer is u64-backed, so it is 8-byte aligned.
    fn query_buffer(
        builtin: &str,
        service: &str,
        call: &str,
        mut run: impl FnMut(*mut c_void, u32, *mut u32) -> i32,
    ) -> Result<Vec<u64>> {
        let mut needed: u32 = 0;
        if run(null_mut(), 0, &mut needed) == 0 {
            let code = last_error();
            if code != ERROR_INSUFFICIENT_BUFFER {
                return Err(scm_error(builtin, call, code, service));
            }
        }
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        if run(buf.as_mut_ptr().cast(), needed, &mut needed) == 0 {
            return Err(scm_error(builtin, call, last_error(), service));
        }
        Ok(buf)
    }

    fn start_type_of(builtin: &str, svc: &Handle, service: &str) -> Result<u32> {
        let buf = query_buffer(builtin, service, "QueryServiceConfigW", |p, size, needed| {
            // SAFETY: the buffer and its size are the ones query_buffer passes.
            unsafe { QueryServiceConfigW(svc.0, p, size, needed) }
        })?;
        // SAFETY: a successful call wrote a QUERY_SERVICE_CONFIGW at the start of
        // the buffer, and the buffer outlives this read.
        Ok(unsafe { (*buf.as_ptr().cast::<QueryConfig>()).start_type })
    }

    /// The service description, or an empty string when it has none.
    fn description_of(builtin: &str, svc: &Handle, service: &str) -> Result<String> {
        let buf = query_buffer(
            builtin,
            service,
            "QueryServiceConfig2W",
            |p, size, needed| {
                // SAFETY: the buffer and its size are the ones query_buffer passes.
                unsafe {
                    QueryServiceConfig2W(svc.0, SERVICE_CONFIG_DESCRIPTION, p, size, needed)
                }
            },
        )?;
        // SAFETY: a successful call wrote a SERVICE_DESCRIPTIONW whose string lies
        // in the same buffer and is NUL-terminated; the buffer outlives the reads.
        unsafe {
            let ptr = (*buf.as_ptr().cast::<ServiceDescriptionW>()).description;
            if ptr.is_null() {
                return Ok(String::new());
            }
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            Ok(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)))
        }
    }

    fn current_state(builtin: &str, svc: &Handle, service: &str) -> Result<u32> {
        let mut status = ServiceStatusProcess::default();
        let mut needed: u32 = 0;
        // SAFETY: the buffer is a SERVICE_STATUS_PROCESS and its size is passed.
        let ok = unsafe {
            QueryServiceStatusEx(
                svc.0,
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut ServiceStatusProcess).cast(),
                std::mem::size_of::<ServiceStatusProcess>() as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(scm_error(
                builtin,
                "QueryServiceStatusEx",
                last_error(),
                service,
            ));
        }
        Ok(status.current_state)
    }

    /// Asks a running service to stop, then waits up to STOP_WAIT for it to
    /// report stopped. A service already stopping is waited for, not refused.
    fn stop_and_wait(builtin: &str, svc: &Handle, service: &str) -> Result<()> {
        let mut status = ServiceStatusProcess::default();
        // SAFETY: the status buffer is a SERVICE_STATUS-sized prefix of
        // ServiceStatusProcess, and the handle has SERVICE_STOP access.
        let ok = unsafe {
            ControlService(
                svc.0,
                SERVICE_CONTROL_STOP,
                (&mut status as *mut ServiceStatusProcess).cast(),
            )
        };
        if ok == 0 {
            let code = last_error();
            if code != ERROR_SERVICE_NOT_ACTIVE && code != ERROR_SERVICE_CANNOT_ACCEPT_CTRL {
                return Err(scm_error(builtin, "ControlService", code, service));
            }
        }
        let deadline = Instant::now() + STOP_WAIT;
        while current_state(builtin, svc, service)? != SERVICE_STOPPED {
            if Instant::now() >= deadline {
                return Err(budget_exceeded(
                    builtin,
                    format!(
                        "{builtin}: service '{service}' did not stop within {}s",
                        STOP_WAIT.as_secs()
                    ),
                    "check it with `sc.exe query`, then retry",
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(())
    }

    pub(super) fn delete(name: String) -> Result<Value> {
        const BUILTIN: &str = "svc_delete";
        let service = format!("{UNIT_PREFIX}{name}");
        let scm = open_scm(BUILTIN, SC_MANAGER_CONNECT)?;
        let svc = open_service(
            BUILTIN,
            &scm,
            &service,
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS | SERVICE_STOP | DELETE,
        )?;

        if !description_of(BUILTIN, &svc, &service)?.starts_with(WIN_MARKER) {
            return Err(policy_deny(
                BUILTIN,
                format!("{BUILTIN}: service '{service}' has no AetherShell marker in its description; refusing to remove it"),
                "only services svc_create created can be deleted; remove hand-written services with sc.exe",
            ));
        }
        let start_type = start_type_of(BUILTIN, &svc, &service)?;
        let running = current_state(BUILTIN, &svc, &service)? != SERVICE_STOPPED;
        if running {
            stop_and_wait(BUILTIN, &svc, &service)?;
        }
        // Linux reports whether the unit was enabled. A Windows service counts as
        // enabled unless its start type is demand or disabled.
        let disabled = !matches!(start_type, SERVICE_DEMAND_START | SERVICE_DISABLED);
        // SAFETY: the handle is open with DELETE access.
        if unsafe { DeleteService(svc.0) } == 0 {
            return Err(scm_error(BUILTIN, "DeleteService", last_error(), &service));
        }

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(service));
        record.insert("stopped".to_string(), Value::Bool(running));
        record.insert("disabled".to_string(), Value::Bool(disabled));
        record.insert("removed".to_string(), Value::Bool(true));
        Ok(Value::Record(record))
    }
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
