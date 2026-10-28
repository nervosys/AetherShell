//! Package administration: apt on Debian-family Linux, winget on Windows.
//!
//! The nine mutating builtins (`pkg_install`, `pkg_remove`, `pkg_update`,
//! `pkg_upgrade`, `pkg_clean`, `pkg_add_source`, `pkg_hold`, `pkg_unhold`,
//! `pkg_autoremove`) delegate here. Read-only queries stay in builtins.rs.
//!
//! Every tool runs directly with an argv array; no shell is ever involved.
//! Caller strings only reach argv after they match a strict pattern, and none
//! of them can begin with `-`. Each run has a time limit and is killed when it
//! expires. A nonzero exit is a coded `E_TOOL_FAILED` carrying the last 64 KiB
//! of stderr. A dpkg or apt lock held by another process is `E_BAD_STATE`,
//! because waiting and retrying is the repair.
//!
//! On Windows, `pkg_install`, `pkg_remove`, `pkg_update` and `pkg_upgrade` run
//! winget (see `pkg_admin_winget.rs`). The other builtins have no winget
//! equivalent and are refused.
//!
//! Refused before any effect: pipeline input, an active transaction (package
//! changes cannot be rolled back), and any platform or package manager other
//! than apt or winget. Those refusals are `E_UNIMPLEMENTED`.

// The apt half is compiled on Windows but never called there.
#![cfg_attr(windows, allow(dead_code))]

use crate::safety;
use crate::value::Value;
use anyhow::Result;
use regex::Regex;
use std::collections::{BTreeMap, HashSet};
use std::process::{Command, Output};
use std::sync::OnceLock;

#[cfg(windows)]
#[path = "pkg_admin_winget.rs"]
mod winget;

/// Upper bound on one apt, dpkg-query or apt-mark run.
const RUN_TIMEOUT_SECS: u64 = 1800;
/// Upper bound on package names in one call.
const MAX_NAMES: usize = 64;
/// How much of a failed tool's stderr reaches the error.
const MAX_STDERR_TAIL: usize = 64 * 1024;
/// Where `pkg_add_source` writes; apt reads every `*.list` file here.
const SOURCES_DIR: &str = "/etc/apt/sources.list.d";
/// The mark every file written by `pkg_add_source` carries.
const SOURCE_HEADER: &str = "# Created by AetherShell pkg_add_source";

fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9+.-]{1,127}$").expect("valid pattern"))
}

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9.+~:-]{1,64}$").expect("valid pattern"))
}

fn source_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,63}$").expect("valid pattern"))
}

fn source_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^deb(-src)? (\[[a-z0-9=_ /.:-]+\] )?https://[^\s]+ [a-z0-9._/-]+( [a-z0-9._-]+)+$",
        )
        .expect("valid pattern")
    })
}

/// One package named by the caller, with an optional exact version.
struct Spec {
    name: String,
    version: Option<String>,
}

/// The dpkg fields `pkg_remove` and `pkg_hold` decide on.
struct Status {
    state: String,
    essential: String,
    priority: String,
}

// ---------------------------------------------------------------------------
// Entry points: called from the bi_pkg_* bodies in builtins.rs.
// ---------------------------------------------------------------------------

/// `pkg_install`: winget on Windows, apt elsewhere. See the apt half for its
/// contract and the winget half for its own.
pub(super) fn install(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    #[cfg(windows)]
    {
        winget::install(args, input)
    }
    #[cfg(not(windows))]
    {
        apt_install(args, input)
    }
}

/// `pkg_remove`: winget on Windows, apt elsewhere.
pub(super) fn remove(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    #[cfg(windows)]
    {
        winget::remove(args, input)
    }
    #[cfg(not(windows))]
    {
        apt_remove(args, input)
    }
}

/// `pkg_update`: winget on Windows, apt elsewhere.
pub(super) fn update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    #[cfg(windows)]
    {
        winget::update(args, input)
    }
    #[cfg(not(windows))]
    {
        apt_update(args, input)
    }
}

/// `pkg_upgrade`: winget on Windows, apt elsewhere.
pub(super) fn upgrade(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    #[cfg(windows)]
    {
        winget::upgrade(args, input)
    }
    #[cfg(not(windows))]
    {
        apt_upgrade(args, input)
    }
}


/// `pkg_install(names, options?)`. Options: `no_recommends` Bool, default true.
/// Names may be `name=version`. Returns `{installed: [{name, version}]}`, with
/// versions read back from dpkg after apt succeeds.
fn apt_install(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_install";
    no_pipeline(B, &input)?;
    if args.is_empty() || args.len() > 2 {
        return Err(safety::arg_err(format!(
            "{B} takes an Array of names and an optional options Record"
        )));
    }
    let specs = package_specs(B, &args[0], true)?;
    let opts = options(B, args.get(1), &["no_recommends"])?;
    let no_recommends = flag(B, &opts, "no_recommends", true)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    let mut argv = apt_argv("install");
    if no_recommends {
        argv.push("--no-install-recommends".to_string());
    }
    argv.extend(specs.iter().map(|s| match &s.version {
        Some(v) => format!("{}={v}", s.name),
        None => s.name.clone(),
    }));
    checked(B, "apt-get", &argv)?;

    let mut installed = Vec::with_capacity(specs.len());
    for spec in &specs {
        let version = dpkg_version(B, &spec.name)?;
        installed.push(record(vec![
            ("name", Value::Str(spec.name.clone())),
            ("version", Value::Str(version)),
        ]));
    }
    Ok(record(vec![("installed", Value::Array(installed))]))
}

/// `pkg_remove(names, options?)`. Options: `purge` Bool, default false.
/// Every name must be installed, and none may be Essential or of Priority
/// `required`; the whole call is refused before anything is removed if any
/// name fails that check. Returns `{removed: [name]}` for the names requested.
/// Packages apt removes as side effects (reverse dependencies) are not listed.
fn apt_remove(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_remove";
    no_pipeline(B, &input)?;
    if args.is_empty() || args.len() > 2 {
        return Err(safety::arg_err(format!(
            "{B} takes an Array of names and an optional options Record"
        )));
    }
    let specs = package_specs(B, &args[0], false)?;
    let opts = options(B, args.get(1), &["purge"])?;
    let purge = flag(B, &opts, "purge", false)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    for spec in &specs {
        let status = match dpkg_status(B, &spec.name)? {
            Some(s) if s.state.split_whitespace().last() == Some("installed") => s,
            _ => return Err(safety::not_found(B, "installed package", &spec.name)),
        };
        if status.essential == "yes" || status.priority == "required" {
            return Err(safety::policy_deny(
                B,
                format!(
                    "`{}` is Essential or of Priority required; removing it would break \
                     the system; NOTHING WAS REMOVED",
                    spec.name
                ),
                "keep this package; remove packages of lower priority instead",
            ));
        }
    }

    let mut argv = apt_argv(if purge { "purge" } else { "remove" });
    argv.extend(specs.iter().map(|s| s.name.clone()));
    checked(B, "apt-get", &argv)?;

    let removed = specs.iter().map(|s| Value::Str(s.name.clone())).collect();
    Ok(record(vec![("removed", Value::Array(removed))]))
}

/// `pkg_update()`: refresh the package index (`apt-get update`).
fn apt_update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_update";
    no_args_or_input(B, &args, &input)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;
    checked(B, "apt-get", &apt_argv("update"))?;
    Ok(record(vec![("updated", Value::Bool(true))]))
}

/// `pkg_upgrade(options?)`. Options: `dist` Bool, default false, which runs
/// `dist-upgrade` instead of `upgrade`. Returns `{upgraded: Int}`, read from
/// the `N upgraded, ...` summary line apt prints. A missing summary line is an
/// `E_TOOL_FAILED`, not a count of zero.
fn apt_upgrade(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_upgrade";
    no_pipeline(B, &input)?;
    if args.len() > 1 {
        return Err(safety::arg_err(format!(
            "{B} takes an optional options Record"
        )));
    }
    let opts = options(B, args.first(), &["dist"])?;
    let dist = flag(B, &opts, "dist", false)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    let subcommand = if dist { "dist-upgrade" } else { "upgrade" };
    let output = checked(B, "apt-get", &apt_argv(subcommand))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let upgraded = parse_upgraded(&text).ok_or_else(|| {
        safety::tool_failed(
            B,
            "apt-get",
            Some(0),
            "its output had no `N upgraded, ...` summary line; the count is unknown",
        )
    })?;
    Ok(record(vec![("upgraded", Value::Int(upgraded))]))
}

/// `pkg_clean()`: remove downloaded package files (`apt-get clean`).
pub(super) fn clean(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_clean";
    no_args_or_input(B, &args, &input)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;
    checked(B, "apt-get", &apt_argv("clean"))?;
    Ok(record(vec![("cleaned", Value::Bool(true))]))
}

/// `pkg_autoremove()`: remove packages nothing depends on any more. Returns
/// `{removed: [name]}`, parsed from the `Removing <name> ...` lines apt prints.
/// The removal set is whatever apt computes; nothing is filtered here.
pub(super) fn autoremove(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_autoremove";
    no_args_or_input(B, &args, &input)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;
    let output = checked(B, "apt-get", &apt_argv("autoremove"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut removed: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Removing ") {
            let token = rest.split_whitespace().next().unwrap_or("");
            // `libfoo:amd64` names the package `libfoo`.
            let name = token.split(':').next().unwrap_or("");
            if !name.is_empty() && !removed.iter().any(|r| r == name) {
                removed.push(name.to_string());
            }
        }
    }
    let removed = removed.into_iter().map(Value::Str).collect();
    Ok(record(vec![("removed", Value::Array(removed))]))
}

/// `pkg_hold(names)`: `apt-mark hold`. Each name must already be installed,
/// and the hold is verified with `apt-mark showhold`. Returns `{held: [name]}`.
pub(super) fn hold(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_hold";
    no_pipeline(B, &input)?;
    if args.len() != 1 {
        return Err(safety::arg_err(format!("{B} takes one Array of names")));
    }
    let specs = package_specs(B, &args[0], false)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    for spec in &specs {
        let installed = matches!(
            dpkg_status(B, &spec.name)?,
            Some(s) if s.state.split_whitespace().last() == Some("installed")
        );
        if !installed {
            return Err(safety::bad_state(
                B,
                &format!("`{}` is not installed, so it cannot be held", spec.name),
                "pkg_install",
            ));
        }
    }

    let mut argv = vec!["hold".to_string()];
    argv.extend(specs.iter().map(|s| s.name.clone()));
    checked(B, "apt-mark", &argv)?;

    let held = showhold(B)?;
    for spec in &specs {
        if !held.contains(&spec.name) {
            return Err(safety::tool_failed(
                B,
                "apt-mark",
                Some(0),
                &format!(
                    "`{}` does not appear in `apt-mark showhold` after the hold",
                    spec.name
                ),
            ));
        }
    }
    let names = specs.iter().map(|s| Value::Str(s.name.clone())).collect();
    Ok(record(vec![("held", Value::Array(names))]))
}

/// `pkg_unhold(names)`: `apt-mark unhold`, verified with `apt-mark showhold`
/// (none of the names may remain held). Returns `{unheld: [name]}`.
pub(super) fn unhold(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_unhold";
    no_pipeline(B, &input)?;
    if args.len() != 1 {
        return Err(safety::arg_err(format!("{B} takes one Array of names")));
    }
    let specs = package_specs(B, &args[0], false)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    let mut argv = vec!["unhold".to_string()];
    argv.extend(specs.iter().map(|s| s.name.clone()));
    checked(B, "apt-mark", &argv)?;

    let held = showhold(B)?;
    if let Some(spec) = specs.iter().find(|s| held.contains(&s.name)) {
        return Err(safety::tool_failed(
            B,
            "apt-mark",
            Some(0),
            &format!("`{}` is still held after the unhold", spec.name),
        ));
    }
    let names = specs.iter().map(|s| Value::Str(s.name.clone())).collect();
    Ok(record(vec![("unheld", Value::Array(names))]))
}

/// `pkg_add_source(name, line, options?)`. Writes `line` to
/// `/etc/apt/sources.list.d/ae-<name>.list`, after a `# Created by
/// AetherShell pkg_add_source` header line. Options: `update` Bool, default
/// false, which runs `apt-get update` afterwards; if that fails the new file
/// is removed and the error returned. An existing file is never overwritten.
pub(super) fn add_source(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_add_source";
    no_pipeline(B, &input)?;
    if args.len() < 2 || args.len() > 3 {
        return Err(safety::arg_err(format!(
            "{B} takes a name, a deb line and an optional options Record"
        )));
    }
    let name = match &args[0] {
        Value::Str(s) => s.as_str(),
        other => return Err(safety::bad_arg(B, "String", other.type_name())),
    };
    if !source_name_re().is_match(name) {
        return Err(safety::bad_arg(
            B,
            "a source name matching ^[a-z0-9][a-z0-9_-]{0,63}$",
            &format!("{name:?}"),
        ));
    }
    let line = match &args[1] {
        Value::Str(s) => s.as_str(),
        other => return Err(safety::bad_arg(B, "String", other.type_name())),
    };
    if line.chars().any(|c| c.is_control()) || !source_line_re().is_match(line) {
        return Err(safety::bad_arg(
            B,
            "one deb or deb-src line with an https:// URI, e.g. `deb https://deb.debian.org/debian trixie main`",
            &format!("{line:?}"),
        ));
    }
    let opts = options(B, args.get(2), &["update"])?;
    let update = flag(B, &opts, "update", false)?;
    refuse_in_transaction(B)?;
    require_manager(B)?;

    let path = format!("{SOURCES_DIR}/ae-{name}.list");
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(safety::bad_arg(
            B,
            "a source name whose ae-<name>.list file does not exist yet",
            &format!("{name:?}, and {path} already exists"),
        ));
    }
    let content = format!("{SOURCE_HEADER}\n{line}\n");
    install_file(B, &path, &content)?;

    if update {
        if let Err(e) = checked(B, "apt-get", &apt_argv("update")) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
    }
    Ok(record(vec![
        ("path", Value::Str(path)),
        ("updated", Value::Bool(update)),
    ]))
}

// ---------------------------------------------------------------------------
// Argument validation.
// ---------------------------------------------------------------------------

fn no_pipeline(builtin: &str, input: &Option<Value>) -> Result<()> {
    if input.is_some() {
        return Err(safety::arg_err(format!(
            "{builtin} takes no pipeline input"
        )));
    }
    Ok(())
}

fn no_args_or_input(builtin: &str, args: &[Value], input: &Option<Value>) -> Result<()> {
    if !args.is_empty() || input.is_some() {
        return Err(safety::arg_err(format!(
            "{builtin} accepts no arguments or pipeline input"
        )));
    }
    Ok(())
}

/// Validate a caller's Array of package names. With `allow_version`, each
/// entry may be `name=version`; without it, only bare names are accepted.
fn package_specs(builtin: &str, value: &Value, allow_version: bool) -> Result<Vec<Spec>> {
    let items = match value {
        Value::Array(items) => items,
        other => {
            return Err(safety::bad_arg(
                builtin,
                "Array of String",
                other.type_name(),
            ))
        }
    };
    if items.is_empty() {
        return Err(safety::arg_err(format!(
            "{builtin}: names must be a nonempty Array of String"
        )));
    }
    if items.len() > MAX_NAMES {
        return Err(safety::arg_err(format!(
            "{builtin}: at most {MAX_NAMES} names per call, got {}",
            items.len()
        )));
    }
    let mut specs = Vec::with_capacity(items.len());
    let mut seen = HashSet::new();
    for item in items {
        let text = match item {
            Value::Str(s) => s.as_str(),
            other => return Err(safety::bad_arg(builtin, "String", other.type_name())),
        };
        if text.starts_with('-') {
            return Err(safety::bad_arg(
                builtin,
                "a package name, which cannot begin with '-'",
                &format!("{text:?}"),
            ));
        }
        let (name, version) = match text.split_once('=') {
            Some((n, v)) if allow_version => (n, Some(v)),
            _ => (text, None),
        };
        if !name_re().is_match(name) {
            return Err(safety::bad_arg(
                builtin,
                "a package name matching ^[a-z0-9][a-z0-9+.-]{1,127}$",
                &format!("{text:?}"),
            ));
        }
        if let Some(v) = version {
            if !version_re().is_match(v) {
                return Err(safety::bad_arg(
                    builtin,
                    "a version matching ^[A-Za-z0-9.+~:-]{1,64}$",
                    &format!("{v:?}"),
                ));
            }
        }
        if !seen.insert(name.to_string()) {
            return Err(safety::arg_err(format!(
                "{builtin}: package {name:?} is named more than once"
            )));
        }
        specs.push(Spec {
            name: name.to_string(),
            version: version.map(str::to_string),
        });
    }
    Ok(specs)
}

/// Read an optional options Record, refusing any key not in `allowed`.
fn options(
    builtin: &str,
    value: Option<&Value>,
    allowed: &[&str],
) -> Result<BTreeMap<String, Value>> {
    match value {
        None => Ok(BTreeMap::new()),
        Some(Value::Record(map)) => {
            for key in map.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(safety::arg_err(format!(
                        "{builtin}: unknown option {key:?}; the options are: {}",
                        allowed.join(", ")
                    )));
                }
            }
            Ok(map.clone())
        }
        Some(other) => Err(safety::bad_arg(builtin, "Record", other.type_name())),
    }
}

fn flag(builtin: &str, opts: &BTreeMap<String, Value>, key: &str, default: bool) -> Result<bool> {
    match opts.get(key) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(safety::bad_arg(
            builtin,
            &format!("Bool for option {key}"),
            other.type_name(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Gates.
// ---------------------------------------------------------------------------

fn refuse_in_transaction(builtin: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(safety::unimplemented(
            builtin,
            "package changes cannot be rolled back; NOTHING WAS CHANGED",
            "finish the transaction first",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn require_manager(builtin: &str) -> Result<()> {
    let pm = super::detect_package_manager();
    if pm == "apt" {
        return Ok(());
    }
    Err(safety::unimplemented(
        builtin,
        &format!(
            "the {pm} package manager is not supported; only apt is implemented in 13.0.0; NOTHING WAS CHANGED"
        ),
        "use the package manager directly",
    ))
}

#[cfg(windows)]
fn require_manager(builtin: &str) -> Result<()> {
    Err(safety::unimplemented(
        builtin,
        &format!(
            "{builtin} has no winget equivalent in 13.0.0; winget covers pkg_install, pkg_remove, pkg_update and pkg_upgrade only; NOTHING WAS CHANGED"
        ),
        "use winget directly",
    ))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn require_manager(builtin: &str) -> Result<()> {
    Err(safety::unimplemented(
        builtin,
        "package administration is implemented on Debian-family Linux (apt) and Windows (winget) only in 13.0.0; NOTHING WAS CHANGED",
        "use the platform's package manager directly",
    ))
}

// ---------------------------------------------------------------------------
// Running tools.
// ---------------------------------------------------------------------------

/// The common argv for an apt-get subcommand: non-interactive, quiet, and
/// keep the existing configuration file on conflict.
fn apt_argv(subcommand: &str) -> Vec<String> {
    let mut argv = vec![
        "-y".to_string(),
        "-q".to_string(),
        "-o".to_string(),
        "Dpkg::Options::=--force-confold".to_string(),
        subcommand.to_string(),
    ];
    // `apt-get update` exits 0 when a source cannot be fetched, printing only
    // a W: line, so an unreachable source looked updated and pkg_add_source
    // kept it. --error-on=any (apt >= 1.9.11) makes any failed fetch an error.
    if subcommand == "update" {
        argv.push("--error-on=any".to_string());
    }
    argv
}

/// Run `/usr/bin/<tool>` with the given argv, returning its output whatever
/// its exit status. Only a failure to start or a timeout is an error here.
///
/// The child leads its own process group, so a timeout kills apt-get together
/// with the dpkg it spawned; killing only apt-get would leave dpkg running and
/// holding the lock.
fn exec(builtin: &str, tool: &str, args: &[String]) -> Result<Output> {
    exec_for(builtin, tool, args, RUN_TIMEOUT_SECS)
}

/// [`exec`] with the deadline given in seconds (a test runs it with a short one).
pub(crate) fn exec_for(
    builtin: &str,
    tool: &str,
    args: &[String],
    deadline_secs: u64,
) -> Result<Output> {
    use std::process::Stdio;
    use std::time::Duration;

    let mut cmd = Command::new(format!("/usr/bin/{tool}"));
    cmd.args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    // The shared bounded runner: the child leads its own process group, the
    // deadline kills that whole group, and a pipe that a leftover descendant
    // keeps open is killed after a grace rather than joined without bound.
    let run = super::run_tree_cmd(
        cmd,
        None,
        Duration::from_secs(deadline_secs),
        super::Keep::Head(usize::MAX),
        super::Keep::Head(usize::MAX),
    )
    .map_err(|e| match e {
        super::TreeError::Spawn(e) | super::TreeError::Wait(e) => {
            safety::spawn_error(builtin, tool, &e)
        }
    })?;
    match run.status {
        Some(status) if !run.timed_out => Ok(Output {
            status,
            stdout: run.stdout,
            stderr: run.stderr,
        }),
        _ => Err(safety::budget_exceeded(
            builtin,
            format!(
                "`{tool}` did not finish within {deadline_secs} s and its process group \
                 was killed; whether it changed anything is UNKNOWN and must be checked"
            ),
            "check the state with pkg_list or pkg_info before retrying",
        )),
    }
}

/// Run a tool that must succeed. A held lock is `E_BAD_STATE`; any other
/// nonzero exit is `E_TOOL_FAILED` with the tail of stderr.
fn checked(builtin: &str, tool: &str, args: &[String]) -> Result<Output> {
    let output = exec(builtin, tool, args)?;
    if output.status.success() {
        return Ok(output);
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if is_lock_busy(&combined) {
        return Err(safety::bad_state(
            builtin,
            "the apt/dpkg lock is held by another process; NOTHING WAS CHANGED",
            "the other apt or dpkg run to finish",
        ));
    }
    Err(safety::tool_failed(
        builtin,
        tool,
        output.status.code(),
        &stderr_tail(&output.stderr),
    ))
}

fn is_lock_busy(text: &str) -> bool {
    text.contains("Could not get lock")
        || text.contains("frontend lock")
        || text.contains("is locked by another process")
}

/// The last `MAX_STDERR_TAIL` bytes of stderr, decoded leniently.
fn stderr_tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_STDERR_TAIL);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

fn unexpected_output(builtin: &str, tool: &str, what: &str) -> anyhow::Error {
    safety::tool_failed(
        builtin,
        tool,
        Some(0),
        &format!("its output was not a recognisable {what}"),
    )
}

/// `dpkg-query` fields for one package, or None when dpkg has no record of
/// the name at all.
fn dpkg_status(builtin: &str, name: &str) -> Result<Option<Status>> {
    let args = [
        "-W".to_string(),
        "-f".to_string(),
        "${Status}|${Essential}|${Priority}".to_string(),
        name.to_string(),
    ];
    let output = exec(builtin, "dpkg-query", &args)?;
    if !output.status.success() {
        if String::from_utf8_lossy(&output.stderr).contains("no packages found") {
            return Ok(None);
        }
        return Err(safety::tool_failed(
            builtin,
            "dpkg-query",
            output.status.code(),
            &stderr_tail(&output.stderr),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or("");
    let parts: Vec<&str> = line.splitn(3, '|').collect();
    if parts.len() != 3 {
        return Err(unexpected_output(builtin, "dpkg-query", "status record"));
    }
    Ok(Some(Status {
        state: parts[0].trim().to_string(),
        essential: parts[1].trim().to_string(),
        priority: parts[2].trim().to_string(),
    }))
}

/// The installed version of `name`, read back from dpkg.
fn dpkg_version(builtin: &str, name: &str) -> Result<String> {
    let args = [
        "-W".to_string(),
        "-f".to_string(),
        "${Version}".to_string(),
        name.to_string(),
    ];
    let output = checked(builtin, "dpkg-query", &args)?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err(unexpected_output(builtin, "dpkg-query", "version"));
    }
    Ok(version)
}

/// Packages apt-mark reports as held.
fn showhold(builtin: &str) -> Result<Vec<String>> {
    let output = checked(builtin, "apt-mark", &["showhold".to_string()])?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// The `N` from apt's `N upgraded, M newly installed, ...` summary line.
fn parse_upgraded(text: &str) -> Option<i64> {
    text.lines().find_map(|line| {
        let idx = line.find(" upgraded,")?;
        line[..idx].split_whitespace().last()?.parse().ok()
    })
}

/// Create `path` with `content` and mode 0644, atomically with respect to
/// readers, and refuse to replace an existing file. The file is written to a
/// hidden temporary name in the same directory and hard-linked into place;
/// `link` fails when the destination exists, and apt ignores hidden files.
fn install_file(builtin: &str, path: &str, content: &str) -> Result<()> {
    let tmp = format!("{SOURCES_DIR}/.ae-pending-{}.tmp", std::process::id());
    let result = write_then_link(&tmp, path, content);
    let _ = std::fs::remove_file(&tmp);
    match result {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(safety::bad_arg(
            builtin,
            "a source file that does not exist yet",
            &format!("{path} already exists"),
        )),
        Err(e) => Err(safety::fs_error(builtin, path, &e)),
    }
}

fn write_then_link(tmp: &str, path: &str, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = options.open(tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(0o644))?;
    }
    std::fs::hard_link(tmp, path)
}

#[cfg(all(test, windows))]
mod winget_tests {
    use super::winget::{classify, id_ok, Ran};

    #[test]
    fn identifiers_are_an_allowlist() {
        for id in ["jqlang.jq", "Microsoft.VisualStudioCode", "a", "a+b_c-d.e"] {
            assert!(id_ok(id), "{id:?} should be accepted");
        }
        let longest = "x".repeat(128);
        let too_long = "x".repeat(129);
        assert!(id_ok(&longest), "128 characters should be accepted");
        assert!(!id_ok(&too_long), "129 characters should be refused");
        for id in ["", "-x", "a b", "a;b", "a=1.0", "a/b", "a$(b)", "\u{e9}", "a\nb"] {
            assert!(!id_ok(id), "{id:?} should be refused");
        }
    }

    #[test]
    fn exit_codes_are_classified() {
        assert_eq!(classify(0), Ok(Ran::Done));
        assert_eq!(classify(0x8A15_002B), Ok(Ran::NoChange));
        assert_eq!(classify(0x8A15_0014), Ok(Ran::NotFound));
        assert_eq!(classify(1), Err(1));
        assert_eq!(classify(0x8A15_0001), Err(0x8A15_0001));
    }
}

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}
