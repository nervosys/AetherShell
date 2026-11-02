//! Opt-in, local-only recording of evaluation attempts.
//!
//! Nothing here measures anything unless the user asks. With `IRONSHELL_RECORD`
//! unset, or set to anything other than `1` or `full`, no function in this
//! module touches the filesystem. With `1` or `full`, every top-level evaluation
//! appends one JSON line to `record.jsonl` in the session directory (see
//! [`crate::handles`]). Nothing is sent anywhere.
//!
//! Stored per evaluation: a version, a timestamp, a per-process sequence number,
//! the surface it came from, whether agent mode was on, the names of the builtins
//! it calls at its top level, a 64-bit FNV-1a hash of its source, the duration,
//! and on failure the error code, whether it is retryable, and whether it was a
//! refusal. The source text itself is stored only under `full`. Outputs and
//! argument values are never stored.
//!
//! Recording must never change a command's outcome, so every write error is
//! ignored.

use crate::ast::{Expr, Stmt};
use crate::safety::{self, ErrorCode};
use crate::value::Value;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The file, inside the session directory, that holds the records.
pub const FILE_NAME: &str = "record.jsonl";

/// A failure and the attempt that follows it count as a retry only within this
/// many milliseconds.
const RETRY_WINDOW_MS: u64 = 120_000;

/// Caps the number of head names stored for one evaluation, so a long script
/// cannot make one line unbounded.
const MAX_HEADS: usize = 64;

/// Codes that mean the shell or the OS refused the call, rather than failed it.
const REFUSAL_CODES: [&str; 5] = [
    "E_POLICY_DENY",
    "E_NEEDS_APPROVAL",
    "E_OUTSIDE_WORKSPACE",
    "E_OS_DENIED",
    "E_BUDGET_EXCEEDED",
];

/// Where an evaluation came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// `ish -c` (and the other one-liner forms).
    C,
    /// `ish script.ish`.
    File,
    /// The interactive REPL.
    Repl,
    /// The Agent API (`eval` and `call`).
    Api,
}

impl Surface {
    fn as_str(self) -> &'static str {
        match self {
            Surface::C => "c",
            Surface::File => "file",
            Surface::Repl => "repl",
            Surface::Api => "api",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Off,
    /// `IRONSHELL_RECORD=1`: no source text.
    Hashes,
    /// `IRONSHELL_RECORD=full`: source text included.
    Full,
}

/// The recording level. Read on each call rather than cached: the variable is
/// one `getenv`, far below the cost of the evaluation it sits beside, and a
/// cached answer would not change when a process sets the variable for itself
/// (as the tests do).
fn level() -> Level {
    match std::env::var("IRONSHELL_RECORD").as_deref() {
        Ok("1") => Level::Hashes,
        Ok("full") => Level::Full,
        _ => Level::Off,
    }
}

/// Whether recording is on for this process.
pub fn enabled() -> bool {
    level() != Level::Off
}

/// Sequence number of the records this process has written.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// One line of `record.jsonl`.
#[derive(Serialize)]
struct Entry<'a> {
    v: u8,
    ts: u64,
    seq: u64,
    surface: &'static str,
    agent: bool,
    heads: Vec<String>,
    cmd_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cmd: Option<&'a str>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retryable: Option<bool>,
    refused: bool,
    ms: u64,
}

/// 64-bit FNV-1a. Not cryptographic: it only lets two records be matched to the
/// same source without storing the source.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The code and retryability of a failed evaluation.
///
/// A parse failure has its own code. Everything else goes through
/// [`safety::code_of`], the same classification the boundary applies.
fn failure_of(e: &anyhow::Error) -> (&'static str, bool) {
    if e.downcast_ref::<crate::parser::ParseError>().is_some() {
        return (crate::parser::ParseError::CODE, false);
    }
    let code: ErrorCode = safety::code_of(e);
    (code.as_str(), code.retryable())
}

/// Record one top-level evaluation. Does nothing unless recording is on, and
/// never fails: a record that cannot be written is dropped.
pub fn note<T>(surface: Surface, code: &str, outcome: &anyhow::Result<T>, elapsed: Duration) {
    let level = level();
    if level == Level::Off {
        return;
    }
    let Some(dir) = crate::handles::session_dir() else {
        return;
    };
    let source = code.trim();
    let (ok, code_str, retryable) = match outcome {
        Ok(_) => (true, None, None),
        Err(e) => {
            let (c, retry) = failure_of(e);
            (false, Some(c), Some(retry))
        }
    };
    let refused = code_str.is_some_and(|c| REFUSAL_CODES.contains(&c));
    let entry = Entry {
        v: 1,
        ts: now_ms(),
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        surface: surface.as_str(),
        agent: safety::current_mode() == safety::Mode::Agent,
        heads: heads_of(source),
        cmd_hash: format!("{:016x}", fnv1a64(source.as_bytes())),
        cmd: (level == Level::Full).then_some(source),
        ok,
        code: code_str,
        retryable,
        refused,
        ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    };
    let Ok(mut line) = serde_json::to_string(&entry) else {
        return;
    };
    line.push('\n');
    let _ = append(&dir, &line);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Append one complete line. The directory is created if needed; the file is
/// only ever appended to, never truncated.
fn append(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE_NAME))?;
    file.write_all(line.as_bytes())
}

/// The builtin names an evaluation calls at its top level, in source order.
///
/// Best effort, from the parsed AST: a call contributes its callee (`f`, or
/// `module.f` for a field call), and a pipeline stage that is a bare name
/// contributes that name. Lambda bodies are not entered, so the names a
/// `map(fn(x) => …)` calls inside its lambda are not listed. Source that does
/// not parse has no heads.
fn heads_of(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(stmts) = crate::parser::parse_program(source) {
        for stmt in &stmts {
            walk_stmt(stmt, &mut out);
        }
    }
    out.truncate(MAX_HEADS);
    out
}

fn walk_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Expr(e) => walk_expr(e, out),
        Stmt::Let { value, .. } => walk_expr(value, out),
        Stmt::Cfg { body, .. } => walk_stmt(body, out),
        _ => {}
    }
}

fn walk_expr(expr: &Expr, out: &mut Vec<String>) {
    if out.len() >= MAX_HEADS {
        return;
    }
    match expr {
        Expr::Call {
            callee,
            args,
            named,
        } => {
            match callee.as_ref() {
                Expr::Ident(name) => out.push(name.clone()),
                Expr::MemberAccess { object, field } => match object.as_ref() {
                    Expr::Ident(module) => out.push(format!("{module}.{field}")),
                    other => walk_expr(other, out),
                },
                other => walk_expr(other, out),
            }
            for arg in args {
                walk_expr(arg, out);
            }
            for (_, arg) in named {
                walk_expr(arg, out);
            }
        }
        Expr::Pipe { left, right } => {
            walk_expr(left, out);
            match right.as_ref() {
                Expr::Ident(name) => out.push(name.clone()),
                other => walk_expr(other, out),
            }
        }
        Expr::Array(items) => {
            for item in items {
                walk_expr(item, out);
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                walk_expr(value, out);
            }
        }
        Expr::Await(inner) | Expr::Throw(inner) => walk_expr(inner, out),
        Expr::TryCatch {
            try_expr,
            catch_expr,
            ..
        } => {
            walk_expr(try_expr, out);
            walk_expr(catch_expr, out);
        }
        Expr::Binary { left, right, .. } => {
            walk_expr(left, out);
            walk_expr(right, out);
        }
        Expr::Unary { expr: inner, .. } => walk_expr(inner, out),
        Expr::MemberAccess { object, .. } => walk_expr(object, out),
        Expr::Match { scrutinee, arms } => {
            walk_expr(scrutinee, out);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    walk_expr(guard, out);
                }
                walk_expr(&arm.body, out);
            }
        }
        // Literals, identifiers, and lambdas (whose bodies run later, not here).
        _ => {}
    }
}

/// One record as the summary reads it back.
struct Row {
    ts: u64,
    ok: bool,
    refused: bool,
    code: Option<String>,
    head: Option<String>,
}

impl Row {
    fn from_json(v: &serde_json::Value) -> Option<Row> {
        Some(Row {
            ts: v.get("ts")?.as_u64()?,
            ok: v.get("ok")?.as_bool()?,
            refused: v.get("refused").and_then(|r| r.as_bool()).unwrap_or(false),
            code: v.get("code").and_then(|c| c.as_str()).map(str::to_string),
            head: v
                .get("heads")
                .and_then(|h| h.as_array())
                .and_then(|h| h.first())
                .and_then(|h| h.as_str())
                .map(str::to_string),
        })
    }
}

/// Whether `next` is a retry of `prev`: `prev` failed, and `next` is the attempt
/// right after it, calls the same first head, and comes within the window.
fn is_retry(prev: &Row, next: &Row) -> bool {
    !prev.ok
        && prev.head.is_some()
        && prev.head == next.head
        && next.ts >= prev.ts
        && next.ts - prev.ts <= RETRY_WINDOW_MS
}

/// Read the current session's records and summarise them.
///
/// With recording off this answers zeros and `enabled: false` without reading
/// anything. Unreadable or malformed lines are skipped.
pub fn summary() -> Value {
    let on = enabled();
    let rows: Vec<Row> = if on {
        crate::handles::session_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
            .map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                    .filter_map(|v| Row::from_json(&v))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut failures = 0i64;
    let mut refusals = 0i64;
    let mut by_code: BTreeMap<String, Value> = BTreeMap::new();
    let mut retries = 0i64;
    let mut recoveries = 0i64;
    let mut first_try_ok = 0i64;
    let mut retry_flags = vec![false; rows.len()];

    for (i, row) in rows.iter().enumerate() {
        if i > 0 && is_retry(&rows[i - 1], row) {
            retry_flags[i] = true;
            retries += 1;
            if row.ok {
                recoveries += 1;
            }
        }
    }
    for (i, row) in rows.iter().enumerate() {
        if !row.ok {
            failures += 1;
            if row.refused {
                refusals += 1;
            }
            if let Some(code) = &row.code {
                let slot = by_code.entry(code.clone()).or_insert(Value::Int(0));
                if let Value::Int(n) = slot {
                    *n += 1;
                }
            }
        } else if !retry_flags[i] {
            first_try_ok += 1;
        }
    }

    let mut out = BTreeMap::new();
    out.insert("enabled".to_string(), Value::Bool(on));
    out.insert("attempts".to_string(), Value::Int(rows.len() as i64));
    out.insert("failures".to_string(), Value::Int(failures));
    out.insert("refusals".to_string(), Value::Int(refusals));
    out.insert("by_code".to_string(), Value::Record(by_code));
    out.insert("retries".to_string(), Value::Int(retries));
    out.insert("recoveries".to_string(), Value::Int(recoveries));
    out.insert("first_try_ok".to_string(), Value::Int(first_try_ok));
    Value::Record(out)
}
