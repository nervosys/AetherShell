//! Contracts for the Windows scheduled-task builtins: `cron_*` and `at_*`.
//!
//! Compiled on Windows only. The ordinary tests are refused before any
//! scheduler call: each one fails argument validation, or the transaction
//! check, before a PowerShell or schtasks process starts. The system-changing
//! tests are `#[ignore]`d, and they also return at once unless
//! AE_DISPOSABLE_HOST=1, which only the disposable Windows runner sets.
//!
//! The builtins name their tasks themselves: cron_add registers
//! `AetherShell-cron-<uuid>` and at_schedule registers `AetherShell-at-<uuid>`,
//! both in the root task folder (`\`). The tests learn each name from the
//! builtin's return value, then check every step with `schtasks.exe` and never
//! with the builtin under test. Anything a test created is deleted in a Drop
//! guard, even when an assertion failed first.

#![cfg(windows)]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

/// Every test takes this lock, so scheduler changes never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

const CMD: &str = "exit 0";

/// True only on the disposable Windows runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::default();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_ok(name: &str, args: Vec<Value>) -> Value {
    call(name, args).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
}

fn expect_code(result: anyhow::Result<Value>, expected: &str) {
    match result {
        Ok(v) => panic!("answered {v:?}; expected {expected}"),
        Err(e) => assert!(e.to_string().contains(expected), "{e}"),
    }
}

fn text(row: &BTreeMap<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

/// The record in an Array whose `key` field equals `name`, if there is one.
fn row_named(list: &Value, key: &str, name: &str) -> Option<BTreeMap<String, Value>> {
    let Value::Array(rows) = list else {
        panic!("expected an Array, got {list:?}");
    };
    rows.iter().find_map(|row| match row {
        Value::Record(map) => match map.get(key) {
            Some(Value::Str(found)) if found == name => Some(map.clone()),
            _ => None,
        },
        _ => None,
    })
}

/// `schtasks.exe` for inspection: (exit code, stdout).
fn schtasks(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new("schtasks.exe")
        .args(args)
        .output()
        .expect("schtasks.exe runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn task_exists(name: &str) -> bool {
    schtasks(&["/Query", "/TN", name]).0 == Some(0)
}

/// The "Scheduled Task State" line of `schtasks /Query /FO LIST /V`, which is
/// Enabled or Disabled.
fn task_state(name: &str) -> String {
    let (code, out) = schtasks(&["/Query", "/TN", name, "/FO", "LIST", "/V"]);
    assert_eq!(code, Some(0), "{out}");
    out.lines()
        .find_map(|line| line.trim().strip_prefix("Scheduled Task State:"))
        .map(|rest| rest.trim().to_string())
        .unwrap_or_default()
}

/// Removes whatever a disposable test made, even when it panics.
#[derive(Default)]
struct Cleanup(Vec<String>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for task in &self.0 {
            let _ = Command::new("schtasks.exe")
                .args(["/Delete", "/F", "/TN", task.as_str()])
                .output();
        }
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: each call is refused before any scheduler process starts
// ---------------------------------------------------------------------------

#[test]
fn malformed_scheduler_calls_are_refused_before_the_scheduler() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let long = "x".repeat(4097);
    let cases: Vec<(&str, Vec<Value>)> = vec![
        // Windows recurring schedules are only @hourly, @daily or @weekly.
        ("cron_add", vec![s("daily"), s(CMD)]),
        ("cron_add", vec![s("@daily"), s("")]),
        ("cron_add", vec![s("@daily"), s("echo a\necho b")]),
        ("cron_add", vec![s("@daily"), s(&long)]),
        ("cron_add", vec![s("@daily")]),
        ("cron_add", vec![Value::Int(1), s(CMD)]),
        ("cron_add", vec![s("@daily"), s(CMD), s("extra")]),
        ("cron_remove", vec![s("*")]),
        ("cron_remove", vec![s("\\Folder\\*")]),
        ("cron_remove", vec![s("-x")]),
        ("cron_remove", vec![s("")]),
        ("cron_remove", vec![Value::Int(7)]),
        ("cron_enable", vec![s("AetherShell-cron-?")]),
        ("cron_disable", vec![s("*")]),
        ("cron_list", vec![s("x")]),
        // One-time times must be future, valid local YYYY-MM-DDTHH:MM values.
        ("at_schedule", vec![s("2000-01-01T00:00"), s(CMD)]),
        ("at_schedule", vec![s("2099-02-30T00:00"), s(CMD)]),
        ("at_schedule", vec![s("2099-01-01T12:00Z"), s(CMD)]),
        ("at_schedule", vec![s("2099-01-02T03:04"), s(&long)]),
        ("at_schedule", vec![s("2099-01-02T03:04")]),
        // A Windows job ID is exactly AetherShell-at plus a canonical UUID.
        ("at_remove", vec![s("12")]),
        ("at_remove", vec![s("AetherShell-at-*")]),
        (
            "at_remove",
            vec![s("AetherShell-at-12345678-1234-4234-8234-12345678901")],
        ),
        (
            "at_remove",
            vec![s("AetherShell-at-12345678-1234-4234-8234-12345678901A")],
        ),
        ("at_list", vec![s("x")]),
    ];
    for (name, args) in cases {
        expect_code(call(name, args), "E_BAD_ARG");
    }
}

#[test]
fn scheduler_changes_are_refused_inside_a_transaction() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    aethershell::tx::begin().unwrap();
    let refused = [
        call("cron_add", vec![s("@daily"), s(CMD)]),
        call("cron_remove", vec![s("AetherShell-cron-tx")]),
        call("at_schedule", vec![s("2099-01-02T03:04"), s(CMD)]),
        call(
            "at_remove",
            vec![s("AetherShell-at-00000000-0000-4000-8000-000000000001")],
        ),
    ];
    aethershell::tx::rollback().unwrap();
    for result in refused {
        expect_code(result, "E_UNIMPLEMENTED");
    }
}

// ---------------------------------------------------------------------------
// System-changing tests: disposable Windows runner only
// ---------------------------------------------------------------------------

#[test]
#[ignore = "changes Task Scheduler: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn cron_lifecycle_verifies_each_step() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut cleanup = Cleanup::default();

    // add: a daily job, named by the builtin and returned as its target
    let Value::Str(task) = call_ok("cron_add", vec![s("@daily"), s(CMD)]) else {
        panic!("cron_add did not return a String");
    };
    cleanup.0.push(task.clone());
    assert!(task.starts_with("AetherShell-cron-"), "{task}");
    assert!(task_exists(&task), "schtasks /Query does not find {task}");
    assert_ne!(task_state(&task), "Disabled", "a new task starts enabled");
    let listed = call_ok("cron_list", vec![]);
    let row = row_named(&listed, "name", &task).expect("cron_list omits the new task");
    assert_eq!(text(&row, "path"), "\\", "the task is in the root folder");
    assert_ne!(text(&row, "state"), "Disabled");

    // disable: twice, and each call returns true after a successful change
    assert!(matches!(
        call_ok("cron_disable", vec![s(&task)]),
        Value::Bool(true)
    ));
    assert_eq!(task_state(&task), "Disabled");
    let listed = call_ok("cron_list", vec![]);
    let row = row_named(&listed, "name", &task).expect("cron_list omits the task");
    assert_eq!(text(&row, "state"), "Disabled");
    assert!(matches!(
        call_ok("cron_disable", vec![s(&task)]),
        Value::Bool(true)
    ));
    assert_eq!(task_state(&task), "Disabled");

    // enable: the task is enabled again
    assert!(matches!(
        call_ok("cron_enable", vec![s(&task)]),
        Value::Bool(true)
    ));
    assert_eq!(task_state(&task), "Enabled");
    let listed = call_ok("cron_list", vec![]);
    let row = row_named(&listed, "name", &task).expect("cron_list omits the task");
    assert_ne!(text(&row, "state"), "Disabled");

    // remove: schtasks no longer finds it, and neither does cron_list
    assert!(matches!(
        call_ok("cron_remove", vec![s(&task)]),
        Value::Bool(true)
    ));
    assert!(!task_exists(&task), "schtasks /Query still finds {task}");
    let listed = call_ok("cron_list", vec![]);
    assert!(row_named(&listed, "name", &task).is_none());

    // a repeated removal fails in the native tool, which is a coded refusal
    expect_code(call("cron_remove", vec![s(&task)]), "E_TOOL_FAILED");
}

#[test]
#[ignore = "changes Task Scheduler: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn at_lifecycle_verifies_each_step() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut cleanup = Cleanup::default();

    // schedule: a one-time job far in the future, named by the builtin
    let Value::Str(job) = call_ok("at_schedule", vec![s("2099-01-02T03:04"), s(CMD)]) else {
        panic!("at_schedule did not return a String");
    };
    cleanup.0.push(job.clone());
    assert!(job.starts_with("AetherShell-at-"), "{job}");
    assert!(task_exists(&job), "schtasks /Query does not find {job}");
    assert_eq!(task_state(&job), "Enabled");

    // at_list shows the job, with the native trigger time
    let listed = call_ok("at_list", vec![]);
    let row = row_named(&listed, "job_id", &job).expect("at_list omits the new job");
    assert_eq!(text(&row, "queue"), "windows");
    assert!(!text(&row, "user").is_empty());
    assert!(!text(&row, "state").is_empty());
    assert!(
        text(&row, "schedule").starts_with("2099-01-02T03:04"),
        "schedule is {}",
        text(&row, "schedule")
    );

    // remove: gone from schtasks and from at_list
    assert!(matches!(
        call_ok("at_remove", vec![s(&job)]),
        Value::Bool(true)
    ));
    assert!(!task_exists(&job), "schtasks /Query still finds {job}");
    let listed = call_ok("at_list", vec![]);
    assert!(row_named(&listed, "job_id", &job).is_none());

    // a repeated removal fails in the native tool, which is a coded refusal
    expect_code(call("at_remove", vec![s(&job)]), "E_TOOL_FAILED");
}

#[test]
#[ignore = "changes Task Scheduler: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn missing_names_are_tool_failures_and_create_nothing() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let pid = std::process::id();
    let cron = format!("AetherShell-cron-missing-{pid}");
    // A canonical UUID whose last group carries the process ID.
    let job = format!("AetherShell-at-00000000-0000-4000-8000-{pid:012}");
    assert!(!task_exists(&cron), "{cron} exists before the test");
    assert!(!task_exists(&job), "{job} exists before the test");

    // Windows reports a missing task as a failed schtasks call, not E_NOT_FOUND.
    expect_code(call("cron_remove", vec![s(&cron)]), "E_TOOL_FAILED");
    expect_code(call("cron_enable", vec![s(&cron)]), "E_TOOL_FAILED");
    expect_code(call("cron_disable", vec![s(&cron)]), "E_TOOL_FAILED");
    expect_code(call("at_remove", vec![s(&job)]), "E_TOOL_FAILED");
    assert!(!task_exists(&cron) && !task_exists(&job));
}
