//! Real user-crontab mutations by the cron builtins, on macOS.
//!
//! Every test is `#[ignore]`d and returns early unless AE_DISPOSABLE_HOST=1,
//! which only the disposable CI runner sets. The tests run as the ordinary
//! runner user, not root, and change that user's own crontab through
//! /usr/bin/crontab. Each test checks the builtin's answer against an
//! independent oracle: the `/usr/bin/crontab -l` text, read with
//! std::process::Command rather than through the builtins. A Drop guard
//! restores the original crontab (or removes it when there was none), even
//! when the test panics.
//!
//! The at(1) builtins are not covered: macOS has atrun disabled by default, so
//! at jobs cannot run on a stock runner and there is nothing to test there.

#![cfg(target_os = "macos")]

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Every test takes this lock: the crontab belongs to the user, so two tests
/// changing it at once would see each other's entries.
static SERIAL: Mutex<()> = Mutex::new(());

const CRONTAB: &str = "/usr/bin/crontab";

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn expect_code(result: anyhow::Result<Value>, want: &str) {
    match result {
        Ok(v) => panic!("answered {v:?}; expected {want}"),
        Err(e) => assert_eq!(code_of(&e), want, "wrong refusal: {e}"),
    }
}

/// Grants the approval the Exec and Destructive builtins ask for, for this
/// process only. The prior value is restored on drop.
struct Approve(Option<std::ffi::OsString>);

impl Approve {
    fn new() -> Self {
        let prior = std::env::var_os("IRONSHELL_APPROVE_ALL");
        std::env::set_var("IRONSHELL_APPROVE_ALL", "1");
        Approve(prior)
    }
}

impl Drop for Approve {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("IRONSHELL_APPROVE_ALL", value),
            None => std::env::remove_var("IRONSHELL_APPROVE_ALL"),
        }
    }
}

/// The user's crontab as `/usr/bin/crontab -l` prints it. macOS exits 1 with
/// "crontab: no crontab for <user>" when there is none, which reads as `None`;
/// any other failure fails the test rather than being read as an empty table.
fn crontab_read() -> Option<String> {
    let out = Command::new(CRONTAB)
        .arg("-l")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("cannot run {CRONTAB}: {e}"));
    if out.status.success() {
        return Some(String::from_utf8(out.stdout).expect("crontab output is UTF-8"));
    }
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    if err.contains("no crontab for") {
        return None;
    }
    panic!("crontab -l failed: {err}");
}

/// The crontab text the oracle compares against; an absent crontab reads as "".
fn crontab_text() -> String {
    crontab_read().unwrap_or_default()
}

fn crontab_write(text: &str) -> Result<(), String> {
    let mut child = Command::new(CRONTAB)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {CRONTAB} -: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("crontab - has no stdin")?
        .write_all(text.as_bytes())
        .map_err(|e| format!("cannot write the crontab: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("crontab - did not finish: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "crontab - failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// Puts the original crontab back. With none, `crontab -r` removes the table;
/// its failure is ignored because there may be nothing to remove.
fn crontab_restore(original: Option<&str>) -> Result<(), String> {
    match original {
        Some(text) => crontab_write(text),
        None => {
            let _ = Command::new(CRONTAB)
                .arg("-r")
                .stdin(Stdio::null())
                .output();
            Ok(())
        }
    }
}

/// A marker unique to this run: the process ID plus the clock in nanoseconds.
fn marker() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

/// True when a cron_list result has a job with this schedule and command.
fn cron_lists(listing: &Value, schedule: &str, command: &str) -> bool {
    match listing {
        Value::Array(rows) => rows.iter().any(|row| match row {
            Value::Record(fields) => {
                fields.get("schedule") == Some(&s(schedule))
                    && fields.get("command") == Some(&s(command))
            }
            _ => false,
        }),
        _ => false,
    }
}

/// Restores the crontab as it was at construction, removing it when there
/// was none. Runs on panic too. Failures are reported, never panicked on.
struct Cleanup {
    original: Option<String>,
}

impl Cleanup {
    /// Captures the crontab now, before any change.
    fn new() -> Self {
        Cleanup {
            original: crontab_read(),
        }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Err(e) = crontab_restore(self.original.as_deref()) {
            eprintln!("macos_task_admin: could not restore the original crontab: {e}");
        }
    }
}

#[test]
#[ignore = "disposable macOS host, as the runner user: set AE_DISPOSABLE_HOST=1"]
fn cron_lifecycle_touches_only_its_own_entry() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _approve = Approve::new();
    let _cleanup = Cleanup::new();

    let marker = marker();
    let seed = format!("# ae-cron unrelated {marker}\n17 3 * * * /usr/bin/true # keep-{marker}\n");
    crontab_write(&seed).expect("seed the crontab");
    let command = format!("/usr/bin/true ae-cron-{marker}");
    let entry = format!("30 4 * * * {command}");

    let added = call("cron_add", vec![s("30 4 * * *"), s(&command)]).expect("cron_add");
    assert_eq!(added, s(&entry), "cron_add returns its lifecycle target");
    assert_eq!(
        crontab_text(),
        format!("{seed}{entry}\n"),
        "crontab -l after cron_add: unrelated lines byte-for-byte, plus the one new entry"
    );
    let listed = call("cron_list", vec![]).expect("cron_list");
    assert!(
        cron_lists(&listed, "30 4 * * *", &command),
        "cron_list does not include the added job: {listed:?}"
    );

    assert_eq!(
        call("cron_disable", vec![s(&entry)]).expect("cron_disable"),
        Value::Bool(true)
    );
    assert_eq!(
        crontab_text(),
        format!("{seed}# ironshell-disabled: {entry}\n"),
        "crontab -l after cron_disable: the entry carries the IronShell comment prefix"
    );
    let listed = call("cron_list", vec![]).expect("cron_list");
    assert!(
        !cron_lists(&listed, "30 4 * * *", &command),
        "cron_list still lists a disabled job: {listed:?}"
    );

    assert_eq!(
        call("cron_enable", vec![s(&entry)]).expect("cron_enable"),
        Value::Bool(true)
    );
    assert_eq!(
        crontab_text(),
        format!("{seed}{entry}\n"),
        "crontab -l after cron_enable: the entry is active again"
    );

    assert_eq!(
        call("cron_remove", vec![s(&entry)]).expect("cron_remove"),
        Value::Bool(true)
    );
    assert_eq!(
        crontab_text(),
        seed,
        "crontab -l after cron_remove: only the unrelated lines remain"
    );
    let listed = call("cron_list", vec![]).expect("cron_list");
    assert!(
        !cron_lists(&listed, "30 4 * * *", &command),
        "cron_list still lists a removed job: {listed:?}"
    );
}

#[test]
#[ignore = "disposable macOS host, as the runner user: set AE_DISPOSABLE_HOST=1"]
fn cron_refusals_leave_the_crontab_unchanged() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _approve = Approve::new();
    let _cleanup = Cleanup::new();

    let marker = marker();
    let seed = format!("# ae-cron unrelated {marker}\n17 3 * * * /usr/bin/true # keep-{marker}\n");
    crontab_write(&seed).expect("seed the crontab");
    let command = format!("/usr/bin/true ae-cron-{marker}");
    let entry = format!("30 4 * * * {command}");

    call("cron_add", vec![s("30 4 * * *"), s(&command)]).expect("first cron_add");
    let active = format!("{seed}{entry}\n");
    assert_eq!(crontab_text(), active);

    expect_code(
        call("cron_add", vec![s("30 4 * * *"), s(&command)]),
        "E_BAD_STATE",
    );
    assert_eq!(
        crontab_text(),
        active,
        "a refused duplicate cron_add must not touch the crontab"
    );

    call("cron_disable", vec![s(&entry)]).expect("cron_disable");
    let disabled = format!("{seed}# ironshell-disabled: {entry}\n");
    assert_eq!(crontab_text(), disabled);
    expect_code(
        call("cron_add", vec![s("30 4 * * *"), s(&command)]),
        "E_BAD_STATE",
    );
    assert_eq!(
        crontab_text(),
        disabled,
        "a duplicate of a disabled entry is also refused, and nothing changes"
    );

    let missing = format!("30 4 * * * /usr/bin/true ae-cron-missing-{marker}");
    expect_code(call("cron_remove", vec![s(&missing)]), "E_BAD_STATE");
    assert_eq!(
        crontab_text(),
        disabled,
        "removing a nonexistent entry must not touch the crontab"
    );
}
