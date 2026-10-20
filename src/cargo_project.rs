//! Cargo for one project directory: the diag, test, refactor and docs builtins.
//!
//! Every builtin here takes its project directory explicitly. The directory
//! must exist, lie inside the workspace and hold a `Cargo.toml`, and all of
//! that is checked before any process starts. Nothing defaults to the shell's
//! current directory: the earlier bodies ran `cargo test` on whatever project
//! the shell happened to be started in, the AetherShell repository included.
//!
//! Each call runs under the long tool deadline and keeps its output bounded.
//! Diagnostics are read from cargo's JSON messages, so a build that never
//! finished (a broken manifest, a missing toolchain component) is a coded
//! failure rather than an empty list.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{tool_run, tool_text, Keep, ToolRun, TOOL_KEEP_TAIL, TOOL_MAX_JSON};
use crate::value::Value;

/// The longest project path accepted.
const MAX_DIR_BYTES: usize = 4096;
/// Stdout tail quoted in a failed test run's message.
const FAILURE_TAIL_BYTES: usize = 4096;

/// The project directory named by `value`, checked before anything runs.
///
/// Refused with `E_BAD_ARG` when absent, not a string, empty or NUL-bearing;
/// `E_OUTSIDE_WORKSPACE` when it lies outside the workspace (symlinks and `..`
/// included); `E_NOT_FOUND` when it is not a directory or holds no
/// `Cargo.toml`. A relative path is taken against the workspace root.
pub(crate) fn project_dir(builtin: &str, value: Option<&Value>) -> Result<PathBuf> {
    let raw = match value {
        Some(Value::Str(s)) => s.as_str(),
        Some(other) => {
            return Err(crate::safety::bad_arg(
                builtin,
                "dir: String, the project directory inside the workspace",
                other.type_name(),
            ))
        }
        None => {
            return Err(crate::safety::arg_err(format!(
                "{builtin} requires the project directory as its first argument"
            )))
        }
    };
    if raw.trim().is_empty() || raw.len() > MAX_DIR_BYTES || raw.contains('\0') {
        return Err(crate::safety::bad_arg(
            builtin,
            "dir: a nonempty path of at most 4096 bytes without NUL",
            "an empty, oversized or NUL path",
        ));
    }
    let given = Path::new(raw);
    let path = if given.is_absolute() {
        given.to_path_buf()
    } else {
        crate::safety::workspace_root().join(given)
    };
    if !crate::safety::within_workspace(&path.to_string_lossy()) {
        return Err(crate::safety::outside_workspace(builtin, raw));
    }
    if !path.is_dir() || !path.join("Cargo.toml").is_file() {
        return Err(crate::safety::not_found(
            builtin,
            "project (a directory holding Cargo.toml)",
            raw,
        ));
    }
    Ok(path)
}

/// A path argument that must lie inside the workspace, returned absolute.
///
/// Relative paths are taken against the workspace root. Refused with
/// `E_BAD_ARG` when absent, not a string, empty or NUL-bearing, and with
/// `E_OUTSIDE_WORKSPACE` when it resolves outside the workspace.
pub(crate) fn in_workspace_path(
    builtin: &str,
    what: &str,
    value: Option<&Value>,
) -> Result<String> {
    let raw = match value {
        Some(Value::Str(s)) => s.as_str(),
        Some(other) => {
            return Err(crate::safety::bad_arg(
                builtin,
                &format!("{what}: String, a path inside the workspace"),
                other.type_name(),
            ))
        }
        None => return Err(crate::safety::arg_err(format!("{builtin} requires {what}"))),
    };
    if raw.trim().is_empty() || raw.len() > MAX_DIR_BYTES || raw.contains('\0') {
        return Err(crate::safety::bad_arg(
            builtin,
            &format!("{what}: a nonempty path of at most 4096 bytes without NUL"),
            "an empty, oversized or NUL path",
        ));
    }
    let given = Path::new(raw);
    let path = if given.is_absolute() {
        given.to_path_buf()
    } else {
        crate::safety::workspace_root().join(given)
    };
    let text = path.to_string_lossy().into_owned();
    if !crate::safety::within_workspace(&text) {
        return Err(crate::safety::outside_workspace(builtin, raw));
    }
    Ok(text)
}

/// Run `cargo` with `argv` in `dir`, under the long deadline.
pub(crate) fn cargo(builtin: &str, dir: &Path, argv: &[&str], keep: Keep) -> Result<ToolRun> {
    tool_run(
        builtin,
        "cargo",
        Some(dir),
        argv,
        keep,
        super::TOOL_LONG_DEADLINE_SECS,
    )
}

/// Run a cargo call whose only answer is that it succeeded.
pub(crate) fn cargo_done(builtin: &str, dir: &Path, argv: &[&str]) -> Result<()> {
    let run = cargo(builtin, dir, argv, Keep::Tail(TOOL_KEEP_TAIL))?;
    tool_text(builtin, run)?;
    Ok(())
}

/// Run a cargo call and return its stdout. A nonzero exit is a coded failure
/// that quotes stderr and the end of stdout, where test output is printed.
pub(crate) fn cargo_text(builtin: &str, dir: &Path, argv: &[&str], keep: Keep) -> Result<String> {
    let run = cargo(builtin, dir, argv, keep)?;
    if !run.success {
        let mut said = run.stderr.clone();
        let out = tail_bytes(run.stdout.trim_end(), FAILURE_TAIL_BYTES);
        if !out.is_empty() {
            if !said.is_empty() {
                said.push('\n');
            }
            said.push_str("stdout tail:\n");
            said.push_str(out);
        }
        return Err(crate::safety::tool_failed(
            builtin,
            &run.command,
            run.code,
            &said,
        ));
    }
    tool_text(builtin, run)
}

/// The last `max` bytes of `s`, starting on a character boundary.
fn tail_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut cut = s.len() - max;
    while !s.is_char_boundary(cut) {
        cut += 1;
    }
    &s[cut..]
}

/// The diagnostics `cargo <argv> --message-format=json` reports for `dir`, as
/// Records `{level, code, file, line, message}`, one per error or warning.
///
/// Notes and help lines are left out. The build must reach `build-finished`;
/// without it, cargo never ran the compiler and the call fails with
/// `E_TOOL_FAILED`. Errors and warnings are the answer, not a failure, so a
/// build with errors returns them.
pub(crate) fn diagnostics(builtin: &str, dir: &Path, argv: &[&str]) -> Result<Vec<Value>> {
    let mut args: Vec<&str> = argv.to_vec();
    args.push("--message-format=json");
    let run = cargo(builtin, dir, &args, Keep::Head(TOOL_MAX_JSON))?;
    if run.stdout_truncated {
        return Err(crate::safety::budget_exceeded(
            builtin,
            format!(
                "`{}` printed more than {TOOL_MAX_JSON} bytes of diagnostics",
                run.command
            ),
            "check a narrower target",
        ));
    }
    let mut finished = false;
    let mut findings = Vec::new();
    for line in run.stdout.lines() {
        let message: serde_json::Value = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(_) => continue,
        };
        match message.get("reason").and_then(|r| r.as_str()) {
            Some("compiler-message") => {
                if let Some(record) = finding(&message["message"]) {
                    findings.push(Value::Record(record));
                }
            }
            Some("build-finished") => finished = true,
            _ => {}
        }
    }
    if !finished {
        return Err(crate::safety::tool_failed(
            builtin,
            &run.command,
            run.code,
            &run.stderr,
        ));
    }
    Ok(findings)
}

/// One compiler message as a Record, or `None` for notes and help.
fn finding(message: &serde_json::Value) -> Option<BTreeMap<String, Value>> {
    let level = message.get("level")?.as_str()?;
    if !is_error_level(level) && level != "warning" {
        return None;
    }
    let text = |v: Option<&serde_json::Value>| {
        v.and_then(|x| x.as_str())
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null)
    };
    let spans = message.get("spans").and_then(|s| s.as_array());
    let primary = spans.and_then(|spans| {
        spans
            .iter()
            .find(|s| s.get("is_primary").and_then(|p| p.as_bool()) == Some(true))
    });
    let mut record = BTreeMap::new();
    record.insert("level".to_string(), Value::Str(level.to_string()));
    record.insert(
        "code".to_string(),
        text(message.get("code").and_then(|c| c.get("code"))),
    );
    record.insert(
        "file".to_string(),
        text(primary.and_then(|s| s.get("file_name"))),
    );
    record.insert(
        "line".to_string(),
        primary
            .and_then(|s| s.get("line_start"))
            .and_then(|n| n.as_i64())
            .map(Value::Int)
            .unwrap_or(Value::Null),
    );
    record.insert("message".to_string(), text(message.get("message")));
    Some(record)
}

/// Rustc reports errors at level `error`, and some internal failures at
/// `error: internal compiler error`, so the prefix is what counts.
pub(crate) fn is_error_level(level: &str) -> bool {
    level.starts_with("error")
}

/// The findings whose level satisfies `keep`.
pub(crate) fn at_level(findings: Vec<Value>, keep: fn(&str) -> bool) -> Vec<Value> {
    findings
        .into_iter()
        .filter(|v| match v {
            Value::Record(r) => matches!(r.get("level"), Some(Value::Str(l)) if keep(l)),
            _ => false,
        })
        .collect()
}
