//! winget package administration: the Windows half of `pkg_admin`.
//!
//! `pkg_install`, `pkg_remove`, `pkg_update` and `pkg_upgrade` run `winget`
//! directly with an argv array, never through a shell. A package identifier
//! must pass [`id_ok`] before anything runs, and every winget exit status goes
//! through [`classify`]. The builtins winget has no command for are refused by
//! `require_manager` in pkg_admin.rs.
//!
//! winget runs once per name, in order. Before the first change, install checks
//! each name with `winget show` and remove checks each with `winget list`, so a
//! name winget cannot find refuses the whole call with NOTHING WAS CHANGED. A
//! failure after the first change is E_TOOL_FAILED and does not say which names
//! already changed; one name per call is the exact form.

use super::{
    no_args_or_input, no_pipeline, options, record, refuse_in_transaction, stderr_tail, MAX_NAMES,
    RUN_TIMEOUT_SECS,
};
use crate::safety;
use crate::value::Value;
use anyhow::Result;
use std::collections::HashSet;
use std::process::{Command, Output};

/// The longest identifier this module passes to winget.
const ID_MAX: usize = 128;
/// winget's exit status when no package matches (`APPINSTALLER_CLI_ERROR_*`).
const NO_PACKAGE_FOUND: u32 = 0x8A15_0014;
/// winget's exit status when there is no applicable upgrade: already current.
const NO_APPLICABLE_UPDATE: u32 = 0x8A15_002B;

/// What one winget run meant.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Ran {
    /// Exit 0.
    Done,
    /// Exit 0x8A15002B: there was nothing to change.
    NoChange,
    /// Exit 0x8A150014: no package matches.
    NotFound,
}

/// Map a winget exit status. A code that is none of the three is `Err` with
/// the code, which the caller reports as E_TOOL_FAILED.
pub(super) fn classify(code: u32) -> std::result::Result<Ran, u32> {
    match code {
        0 => Ok(Ran::Done),
        NO_APPLICABLE_UPDATE => Ok(Ran::NoChange),
        NO_PACKAGE_FOUND => Ok(Ran::NotFound),
        other => Err(other),
    }
}

/// Whether `id` is a winget package identifier this module passes on:
/// letters, digits, `.`, `-`, `_` or `+`, at most 128 characters, and not
/// beginning with `-`.
pub(super) fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

/// `pkg_install(ids)`: `winget install --id <id> --exact` for each identifier.
/// Returns `{installed: [{name}]}`. Each identifier must be found first. An
/// identifier winget reports as already current counts as installed.
pub(super) fn install(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_install";
    no_pipeline(B, &input)?;
    let ids = one_list(B, &args)?;
    refuse_in_transaction(B)?;

    for id in &ids {
        let id = id.as_str();
        let argv = [
            "show",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
        ];
        if winget(B, &argv)? == Ran::NotFound {
            return Err(safety::not_found(B, "package", id));
        }
    }
    for id in &ids {
        let id = id.as_str();
        let argv = [
            "install",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
            "--silent",
        ];
        if winget(B, &argv)? == Ran::NotFound {
            return Err(safety::not_found(B, "package", id));
        }
    }
    let installed = ids
        .iter()
        .map(|id| record(vec![("name", Value::Str(id.clone()))]))
        .collect();
    Ok(record(vec![("installed", Value::Array(installed))]))
}

/// `pkg_remove(ids)`: `winget uninstall --id <id> --exact` for each identifier.
/// Every identifier must be installed, as `winget list` reports, before
/// anything is removed. Returns `{removed: [name]}` for the names requested.
pub(super) fn remove(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_remove";
    no_pipeline(B, &input)?;
    let ids = one_list(B, &args)?;
    refuse_in_transaction(B)?;

    for id in &ids {
        let argv = [
            "list",
            "--id",
            id.as_str(),
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
        ];
        if winget(B, &argv)? == Ran::NotFound {
            return Err(safety::not_found(B, "installed package", id));
        }
    }
    for id in &ids {
        let argv = [
            "uninstall",
            "--id",
            id.as_str(),
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
            "--silent",
        ];
        if winget(B, &argv)? == Ran::NotFound {
            return Err(safety::not_found(B, "installed package", id));
        }
    }
    let removed = ids.iter().map(|id| Value::Str(id.clone())).collect();
    Ok(record(vec![("removed", Value::Array(removed))]))
}

/// `pkg_update()`: `winget source update`, which refreshes winget's sources.
pub(super) fn update(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_update";
    no_args_or_input(B, &args, &input)?;
    refuse_in_transaction(B)?;
    match winget(B, &["source", "update", "--disable-interactivity"])? {
        Ran::Done | Ran::NoChange => Ok(record(vec![("updated", Value::Bool(true))])),
        Ran::NotFound => Err(safety::tool_failed_code(
            B,
            "winget",
            NO_PACKAGE_FOUND,
            "winget reported that no source matched",
        )),
    }
}

/// `pkg_upgrade(options?)`. The option `id` (String) upgrades that one package
/// with `winget upgrade --id`; without it, `winget upgrade --all` upgrades
/// every package winget has an upgrade for. Returns `{changed: Bool}`, false
/// when winget reports nothing to upgrade. The `--all` form does not say how
/// many packages it changed, so it reports only that winget exited 0.
pub(super) fn upgrade(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const B: &str = "pkg_upgrade";
    no_pipeline(B, &input)?;
    if args.len() > 1 {
        return Err(safety::arg_err(format!(
            "{B} takes an optional options Record"
        )));
    }
    let opts = options(B, args.first(), &["id"])?;
    let id = match opts.get("id") {
        None => None,
        Some(Value::Str(id)) if id_ok(id) => Some(id.clone()),
        Some(Value::Str(id)) => {
            return Err(safety::bad_arg(B, WINGET_ID_EXPECTED, &format!("{id:?}")))
        }
        Some(other) => {
            return Err(safety::bad_arg(
                B,
                "String for option id",
                other.type_name(),
            ))
        }
    };
    refuse_in_transaction(B)?;

    let mut argv = vec!["upgrade"];
    match &id {
        Some(id) => argv.extend(["--id", id.as_str(), "--exact"]),
        None => argv.push("--all"),
    }
    argv.extend([
        "--source",
        "winget",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
        "--silent",
    ]);
    let changed = match winget(B, &argv)? {
        Ran::Done => true,
        Ran::NoChange => false,
        Ran::NotFound => {
            return Err(safety::not_found(
                B,
                "package",
                id.as_deref().unwrap_or("(all)"),
            ))
        }
    };
    Ok(record(vec![("changed", Value::Bool(changed))]))
}

const WINGET_ID_EXPECTED: &str = "a winget identifier: letters, digits, '.', '-', '_' or '+', \
     at most 128 characters, not beginning with '-'";

/// The one argument of `pkg_install`, `pkg_remove`: an Array of identifiers.
fn one_list(builtin: &str, args: &[Value]) -> Result<Vec<String>> {
    if args.len() != 1 {
        return Err(safety::arg_err(format!(
            "{builtin} takes one Array of package identifiers"
        )));
    }
    identifiers(builtin, &args[0])
}

/// Validate an Array of winget identifiers: nonempty, at most `MAX_NAMES`,
/// each passing [`id_ok`], and no identifier named twice. winget identifiers
/// are case-insensitive, so the duplicate check ignores case.
fn identifiers(builtin: &str, value: &Value) -> Result<Vec<String>> {
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
            "{builtin}: identifiers must be a nonempty Array of String"
        )));
    }
    if items.len() > MAX_NAMES {
        return Err(safety::arg_err(format!(
            "{builtin}: at most {MAX_NAMES} identifiers per call, got {}",
            items.len()
        )));
    }
    let mut ids = Vec::with_capacity(items.len());
    let mut seen = HashSet::new();
    for item in items {
        let text = match item {
            Value::Str(s) => s.clone(),
            other => return Err(safety::bad_arg(builtin, "String", other.type_name())),
        };
        if !id_ok(&text) {
            return Err(safety::bad_arg(
                builtin,
                WINGET_ID_EXPECTED,
                &format!("{text:?}"),
            ));
        }
        if !seen.insert(text.to_ascii_lowercase()) {
            return Err(safety::arg_err(format!(
                "{builtin}: package {text:?} is named more than once"
            )));
        }
        ids.push(text);
    }
    Ok(ids)
}

/// Run `winget` with `args` and classify its exit status.
fn winget(builtin: &str, args: &[&str]) -> Result<Ran> {
    let output = run(builtin, args)?;
    let Some(code) = output.status.code() else {
        return Err(safety::tool_failed(builtin, "winget", None, &tail(&output)));
    };
    match classify(code as u32) {
        Ok(ran) => Ok(ran),
        Err(code) => Err(safety::tool_failed_code(
            builtin,
            "winget",
            code,
            &tail(&output),
        )),
    }
}

/// Run `winget` directly, as a process tree bounded by the same deadline as
/// apt. Its output comes back whatever its exit status; only a failure to
/// start, or a run that times out, is an error here.
fn run(builtin: &str, args: &[&str]) -> Result<Output> {
    use std::process::Stdio;
    use std::time::Duration;

    let mut cmd = Command::new("winget");
    cmd.args(args).stdin(Stdio::null());
    let run = super::super::run_tree_cmd(
        cmd,
        None,
        Duration::from_secs(RUN_TIMEOUT_SECS),
        super::super::Keep::Head(usize::MAX),
        super::super::Keep::Head(usize::MAX),
    )
    .map_err(|e| match e {
        super::super::TreeError::Spawn(e) | super::super::TreeError::Wait(e) => {
            safety::spawn_error(builtin, "winget", &e)
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
                "`winget` did not finish within {RUN_TIMEOUT_SECS} s and its process tree \
                 was killed; whether it changed anything is UNKNOWN and must be checked"
            ),
            "check the state with pkg_list or pkg_info before retrying",
        )),
    }
}

/// The tail of winget's stderr, or of its stdout when stderr is blank: winget
/// writes its errors to either.
fn tail(output: &Output) -> String {
    let bytes = if output.stderr.iter().any(|b| !b.is_ascii_whitespace()) {
        &output.stderr
    } else {
        &output.stdout
    };
    stderr_tail(bytes)
}
