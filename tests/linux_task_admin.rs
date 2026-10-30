//! Real crontab and at(1) mutations by the cron and at builtins, on Linux.
//!
//! Every test is `#[ignore]`d and calls `disposable::require()` first, so it
//! runs only in the disposable distro, via scripts/disposable_linux_tests.sh
//! (IRONSHELL_DISPOSABLE=1, /etc/ae-disposable and root). Each test verifies the
//! builtin's answer against an independent oracle: the `crontab -l` text, the
//! `atq` queue and `at -c <id>`, all read with std::process::Command rather
//! than through the builtins. A Drop guard restores the user's original
//! crontab (or removes it when there was none) and removes any at job the test
//! queued, even when the test panics.
//!
//! The cron tests need the `cron` package (crontab); the at tests need the `at`
//! package (at, atq, atrm, and the atd daemon binary). A missing tool fails the
//! test with a message naming it; it never passes silently.

#![cfg(target_os = "linux")]

#[path = "support/disposable.rs"]
mod disposable;

use iron_shell::env::Env;
use iron_shell::value::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Every test takes this lock: crontab and the at queue belong to the user,
/// so two tests changing them at once would see each other's entries.
static SERIAL: Mutex<()> = Mutex::new(());

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

/// Panics naming the package when a tool cannot be started at all.
fn require_tool(tool: &str, probe: &[&str], package: &str) {
    match Command::new(tool)
        .args(probe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => panic!(
            "`{tool}` is missing from this distro: install the `{package}` package \
             (apt install {package}) before running linux_task_admin"
        ),
        Err(e) => panic!("cannot run `{tool}`: {e}"),
    }
}

/// The user's crontab as `crontab -l` prints it. A missing crontab is `None`;
/// any other failure fails the test rather than being read as an empty table.
fn crontab_read() -> Option<String> {
    let out = Command::new("crontab")
        .arg("-l")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("cannot run `crontab` (install the `cron` package): {e}"));
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
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run crontab -: {e}"))?;
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

fn crontab_restore(original: Option<&str>) -> Result<(), String> {
    match original {
        Some(text) => crontab_write(text),
        None => {
            let out = Command::new("crontab")
                .arg("-r")
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("cannot run crontab -r: {e}"))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "crontab -r failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ))
            }
        }
    }
}

/// The first field (job ID) of every line `atq` prints.
fn atq_ids() -> Vec<String> {
    let out = Command::new("atq")
        .stdin(Stdio::null())
        .output()
        .expect("cannot run atq");
    assert!(
        out.status.success(),
        "atq failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

/// The script `at -c <id>` prints for a queued job, or `None` when no such job exists.
fn at_script(id: &str) -> Option<String> {
    let out = Command::new("at")
        .arg("-c")
        .arg(id)
        .stdin(Stdio::null())
        .output()
        .expect("cannot run at -c");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A local `YYYY-MM-DDTHH:MM` one week from now, the shape at_schedule takes.
fn seven_days_ahead() -> String {
    let out = Command::new("date")
        .args(["-d", "+7 days", "+%Y-%m-%dT%H:%M"])
        .stdin(Stdio::null())
        .output()
        .expect("cannot run date");
    assert!(
        out.status.success(),
        "date failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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

/// True when an at_list result has a job with this job_id.
fn at_lists(listing: &Value, id: &str) -> bool {
    match listing {
        Value::Array(rows) => rows.iter().any(|row| match row {
            Value::Record(fields) => fields.get("job_id") == Some(&s(id)),
            _ => false,
        }),
        _ => false,
    }
}

/// Removes every at job the test queued, and, when the test touches the
/// crontab, restores it as it was at construction (removing it when there was
/// none). Runs on panic too. Failures are reported, never panicked on.
struct Cleanup {
    /// `None` for a test that never touches the crontab: it must not be
    /// restored, since `crontab -r` on an unrelated user's table would be wrong.
    crontab: Option<Option<String>>,
    at_jobs: Vec<String>,
}

impl Cleanup {
    /// For a test that changes the crontab: captures it now, before any change.
    fn with_crontab() -> Self {
        Cleanup {
            crontab: Some(crontab_read()),
            at_jobs: Vec::new(),
        }
    }

    /// For a test that only touches the at queue.
    fn at_only() -> Self {
        Cleanup {
            crontab: None,
            at_jobs: Vec::new(),
        }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for id in &self.at_jobs {
            // atrm fails for a job that was already removed; that is the goal.
            let _ = Command::new("atrm").arg(id).stdin(Stdio::null()).output();
        }
        if let Some(original) = &self.crontab {
            if let Err(e) = crontab_restore(original.as_deref()) {
                eprintln!("linux_task_admin: could not restore the original crontab: {e}");
            }
        }
    }
}

#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn cron_lifecycle_touches_only_its_own_entry() {
    disposable::require();
    require_tool("crontab", &["-l"], "cron");
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _approve = Approve::new();
    let _cleanup = Cleanup::with_crontab();

    let pid = std::process::id();
    let seed = format!("# aeci unrelated {pid}\n17 3 * * * /bin/true # keep-{pid}\n");
    crontab_write(&seed).expect("seed the crontab");
    let command = format!("/bin/true aeci-{pid}");
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
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn cron_refusals_leave_the_crontab_unchanged() {
    disposable::require();
    require_tool("crontab", &["-l"], "cron");
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _approve = Approve::new();
    let _cleanup = Cleanup::with_crontab();

    let pid = std::process::id();
    let seed = format!("# aeci unrelated {pid}\n17 3 * * * /bin/true # keep-{pid}\n");
    crontab_write(&seed).expect("seed the crontab");
    let command = format!("/bin/true aeci-{pid}");
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

    let missing = format!("30 4 * * * /bin/true aeci-missing-{pid}");
    expect_code(call("cron_remove", vec![s(&missing)]), "E_BAD_STATE");
    assert_eq!(
        crontab_text(),
        disabled,
        "removing a nonexistent entry must not touch the crontab"
    );
}

#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn at_lifecycle_queues_lists_and_removes_one_job() {
    disposable::require();
    require_tool("at", &["-V"], "at");
    require_tool("atq", &[], "at");
    assert!(
        Path::new("/usr/sbin/atd").is_file(),
        "atd is missing: install the `at` package (apt install at); at_schedule needs its binary"
    );
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _approve = Approve::new();
    let mut cleanup = Cleanup::at_only();

    let when = seven_days_ahead();
    let id = match call("at_schedule", vec![s(&when), s("/bin/true")]).expect("at_schedule") {
        Value::Str(id) => id,
        other => panic!("at_schedule answered {other:?}; expected a job ID String"),
    };
    cleanup.at_jobs.push(id.clone());

    assert!(
        atq_ids().contains(&id),
        "atq does not list job {id} after at_schedule for {when}"
    );
    let script = at_script(&id).unwrap_or_else(|| panic!("at -c {id} found no job"));
    assert!(
        script.contains("/bin/true"),
        "at -c {id} does not show the scheduled command: {script}"
    );

    let listed = call("at_list", vec![]).expect("at_list");
    assert!(
        at_lists(&listed, &id),
        "at_list does not include {id}: {listed:?}"
    );

    assert_eq!(
        call("at_remove", vec![s(&id)]).expect("at_remove"),
        Value::Bool(true)
    );
    assert!(
        !atq_ids().contains(&id),
        "atq still lists job {id} after at_remove"
    );
    assert!(
        at_script(&id).is_none(),
        "at -c {id} still finds a job after at_remove"
    );
    let listed = call("at_list", vec![]).expect("at_list");
    assert!(
        !at_lists(&listed, &id),
        "at_list still includes {id}: {listed:?}"
    );
}
