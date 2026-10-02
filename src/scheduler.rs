//! Native scheduler adapters. Mutation tests inject a backend and never create
//! host jobs. Windows registrations use fixed code and JSON data on stdin.
use super::{cron_success_output, parse_cron_listing, CRON_CHANGE_LOCK};
use crate::{safety, value::Value};
use anyhow::Result;
use chrono::TimeZone;
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

pub(super) fn run_tool(
    name: &str,
    tool: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<Output> {
    let mut child = Command::new(tool)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| safety::spawn_error(name, tool, &e))?;
    if let Some(bytes) = stdin {
        let write = child
            .stdin
            .take()
            .expect("piped scheduler stdin")
            .write_all(bytes);
        if let Err(e) = write {
            let _ = child.kill();
            let _ = child.wait();
            return Err(safety::spawn_error(name, tool, &e));
        }
    }
    child
        .wait_with_output()
        .map_err(|e| safety::spawn_error(name, tool, &e))
}

fn check(name: &str, tool: &str, output: &Output) -> Result<()> {
    if !output.status.success() {
        return Err(safety::tool_failed(
            name,
            tool,
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ));
    }
    Ok(())
}

fn command_text(command: &str) -> Result<()> {
    if command.trim().is_empty() || command.len() > 65536 || command.contains(['\0', '\r', '\n']) {
        return Err(safety::arg_err(
            "scheduler command must be one nonempty line of at most 64 KiB",
        ));
    }
    Ok(())
}

fn windows_command_text(command: &str) -> Result<()> {
    command_text(command)?;
    // Stay below cmd.exe's command-line limit, including its executable path
    // and fixed switches, instead of accepting a job that cannot execute.
    if command.encode_utf16().count() > 4096 {
        return Err(safety::arg_err(
            "Windows scheduler commands are limited to 4096 UTF-16 code units",
        ));
    }
    Ok(())
}

fn transaction(name: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(safety::unimplemented(
            name,
            "scheduler changes cannot be rolled back",
            "finish the transaction first",
        ));
    }
    Ok(())
}

fn cron_entry(schedule: &str, command: &str, windows: bool) -> Result<String> {
    command_text(command)?;
    if schedule.is_empty() || schedule.len() > 256 || schedule.contains(['\0', '\r', '\n']) {
        return Err(safety::arg_err("invalid recurring schedule"));
    }
    if windows && !matches!(schedule, "@hourly" | "@daily" | "@weekly") {
        return Err(safety::arg_err(
            "Windows recurring schedules are @hourly, @daily or @weekly",
        ));
    }
    let entry = format!("{schedule} {command}");
    if !windows {
        let rows = parse_cron_listing(cron_success_output(entry.as_bytes().to_vec()), false)
            .map_err(|_| {
                safety::arg_err("schedule must be five cron fields or a supported @ schedule")
            })?;
        if !matches!(rows, Value::Array(ref rows) if rows.len() == 1) {
            return Err(safety::arg_err("schedule must describe one crontab job"));
        }
        // Prevent a command token from being consumed as part of a short schedule.
        let fields = schedule.split_whitespace().count();
        if fields
            != if schedule.trim_start().starts_with('@') {
                1
            } else {
                5
            }
        {
            return Err(safety::arg_err(
                "schedule must be five cron fields or one @ schedule",
            ));
        }
    }
    Ok(entry)
}

pub(super) fn guard_and_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let [Value::Str(schedule), Value::Str(command)] = args.as_slice() else {
        return Err(safety::arg_err(
            "cron_add requires schedule and command Strings",
        ));
    };
    if input.is_some() {
        return Err(safety::arg_err("cron_add rejects pipeline input"));
    }
    cron_entry(schedule, command, cfg!(windows))?;
    if cfg!(windows) {
        windows_command_text(command)?;
    }
    transaction("cron_add")?;
    safety::guard_exec("cron_add", format!("schedule {schedule}: {command}"))?;
    add_backend(schedule, command, cfg!(windows), |tool, args, stdin| {
        run_tool("cron_add", tool, args, stdin)
    })
}

fn add_backend(
    schedule: &str,
    command: &str,
    windows: bool,
    mut run: impl FnMut(&str, &[&str], Option<&[u8]>) -> Result<Output>,
) -> Result<Value> {
    let entry = cron_entry(schedule, command, windows)?;
    if windows {
        return register_windows("cron_add", "cron", schedule, command, &mut run);
    }
    let _lock = CRON_CHANGE_LOCK.lock().map_err(|_| {
        safety::bad_state("cron_add", "scheduler lock poisoned", "restart the runtime")
    })?;
    let output = run("crontab", &["-l"], None)?;
    // Reuse the narrowly recognized no-crontab result; arbitrary failures must
    // never be treated as an empty table that we are allowed to overwrite.
    let text = if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|_| safety::bad_state("cron_add", "non-UTF-8 crontab", "UTF-8 user crontab"))?
    } else {
        parse_cron_listing(output, false)?;
        String::new()
    };
    if text.len() > 1024 * 1024 {
        return Err(safety::budget_exceeded(
            "cron_add",
            "crontab exceeds 1 MiB".into(),
            "edit the oversized table with its native tool",
        ));
    }
    if text
        .lines()
        .any(|line| line == entry || line.strip_prefix("# aethershell-disabled: ") == Some(&entry))
    {
        return Err(safety::bad_state(
            "cron_add",
            "job already exists",
            "use its existing target for lifecycle operations",
        ));
    }
    let mut replacement = text;
    if !replacement.is_empty() && !replacement.ends_with('\n') {
        replacement.push('\n');
    }
    replacement.push_str(&entry);
    replacement.push('\n');
    let output = run("crontab", &["-"], Some(replacement.as_bytes()))?;
    check("cron_add", "crontab", &output)?;
    Ok(Value::Str(entry))
}

fn local_time(text: &str) -> Result<chrono::NaiveDateTime> {
    if text.len() != 16 {
        return Err(safety::arg_err("time must be exactly YYYY-MM-DDTHH:MM"));
    }
    let time = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M")
        .map_err(|_| safety::arg_err("time must be YYYY-MM-DDTHH:MM in the local timezone"))?;
    if time.format("%Y-%m-%dT%H:%M").to_string() != text {
        return Err(safety::arg_err("time must be exactly YYYY-MM-DDTHH:MM"));
    }
    let local = chrono::Local
        .from_local_datetime(&time)
        .single()
        .ok_or_else(|| safety::arg_err("time is ambiguous or nonexistent in the local timezone"))?;
    if local <= chrono::Local::now() {
        return Err(safety::arg_err("one-time job must be in the future"));
    }
    Ok(time)
}

pub(super) fn guard_and_schedule(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let [Value::Str(time), Value::Str(command)] = args.as_slice() else {
        return Err(safety::arg_err(
            "at_schedule requires time and command Strings",
        ));
    };
    if input.is_some() {
        return Err(safety::arg_err("at_schedule rejects pipeline input"));
    }
    command_text(command)?;
    if cfg!(windows) {
        windows_command_text(command)?;
    }
    local_time(time)?;
    transaction("at_schedule")?;
    safety::guard_exec("at_schedule", format!("schedule at {time}: {command}"))?;
    schedule_backend(time, command, cfg!(windows), |tool, args, stdin| {
        run_tool("at_schedule", tool, args, stdin)
    })
}

fn schedule_backend(
    time: &str,
    command: &str,
    windows: bool,
    mut run: impl FnMut(&str, &[&str], Option<&[u8]>) -> Result<Output>,
) -> Result<Value> {
    let time_value = local_time(time)?;
    command_text(command)?;
    if windows {
        return register_windows("at_schedule", "at", time, command, &mut run);
    }
    let timestamp = time_value.format("%Y%m%d%H%M").to_string();
    let script = format!("{command}\n");
    let output = run("at", &["-t", &timestamp], Some(script.as_bytes()))?;
    check("at_schedule", "at", &output)?;
    let diagnostic = std::str::from_utf8(&output.stderr).map_err(|_| unknown_at_id())?;
    let mut ids = diagnostic.lines().filter_map(|line| {
        let tail = line.strip_prefix("job ")?;
        let (id, date) = tail.split_once(" at ")?;
        (!date.trim().is_empty() && valid_unix_id(id)).then_some(id)
    });
    let id = ids.next().ok_or_else(unknown_at_id)?;
    if ids.next().is_some() {
        return Err(unknown_at_id());
    }
    Ok(Value::Str(id.into()))
}

fn unknown_at_id() -> anyhow::Error {
    safety::bad_state(
        "at_schedule",
        "at accepted the job but its job ID could not be read; the job may exist",
        "inspect atq before retrying",
    )
}

fn valid_unix_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && id.bytes().all(|c| c.is_ascii_digit())
        && id.parse::<u64>().is_ok_and(|n| n > 0)
}

fn valid_windows_id(id: &str) -> bool {
    id.strip_prefix("AetherShell-at-").is_some_and(|suffix| {
        uuid::Uuid::parse_str(suffix).is_ok_and(|value| value.to_string() == suffix)
    })
}

pub(super) fn guard_and_remove(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let [Value::Str(id)] = args.as_slice() else {
        return Err(safety::arg_err("at_remove requires one job ID String"));
    };
    if input.is_some()
        || !(if cfg!(windows) {
            valid_windows_id(id)
        } else {
            valid_unix_id(id)
        })
    {
        return Err(safety::arg_err("invalid one-time job ID or pipeline input"));
    }
    transaction("at_remove")?;
    safety::guard(safety::GuardCtx {
        builtin: "at_remove",
        effect: safety::Effect::Destructive,
        what: "remove scheduled job",
        targets: vec![id.clone()],
        blast_radius: serde_json::json!({"scheduled_jobs":1}),
        reversible: false,
        fs_paths: false,
    })?;
    remove_backend(id, cfg!(windows), |tool, args, stdin| {
        run_tool("at_remove", tool, args, stdin)
    })
}

fn remove_backend(
    id: &str,
    windows: bool,
    mut run: impl FnMut(&str, &[&str], Option<&[u8]>) -> Result<Output>,
) -> Result<Value> {
    if !(if windows {
        valid_windows_id(id)
    } else {
        valid_unix_id(id)
    }) {
        return Err(safety::arg_err("invalid one-time job ID"));
    }
    let (tool, args) = if windows {
        ("schtasks", vec!["/Delete", "/TN", id, "/F"])
    } else {
        ("atrm", vec![id])
    };
    let output = run(tool, &args, None)?;
    check("at_remove", tool, &output)?;
    Ok(Value::Bool(true))
}

const WINDOWS_REGISTER: &str = r#"$ErrorActionPreference='Stop';
[Console]::InputEncoding=[System.Text.UTF8Encoding]::new($false);
[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false);
try {
 $p=ConvertFrom-Json ([Console]::In.ReadToEnd());
 $action=New-ScheduledTaskAction -Execute ([System.IO.Path]::Combine($env:SystemRoot,'System32','cmd.exe')) -Argument ('/d /s /c "'+$p.command+'"') -WorkingDirectory $p.cwd;
 if ($p.kind -eq 'at') {
  $when=[datetime]::ParseExact($p.schedule,'yyyy-MM-ddTHH:mm',[System.Globalization.CultureInfo]::InvariantCulture);
  $trigger=New-ScheduledTaskTrigger -Once -At $when;
 } else {
  switch ($p.schedule) {
   '@hourly' { $now=[datetime]::Now; $trigger=New-ScheduledTaskTrigger -Once -At ($now.Date.AddHours($now.Hour+1)) -RepetitionInterval (New-TimeSpan -Hours 1) }
   '@daily' { $trigger=New-ScheduledTaskTrigger -Daily -At ([datetime]::Today) }
   '@weekly' { $trigger=New-ScheduledTaskTrigger -Weekly -DaysOfWeek Sunday -At ([datetime]::Today) }
   default { throw 'Unsupported recurring schedule' }
  }
 }
 $principal=New-ScheduledTaskPrincipal -UserId ([System.Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited;
 $task=Register-ScheduledTask -TaskName $p.id -TaskPath '\' -Action $action -Trigger $trigger -Principal $principal -ErrorAction Stop;
 ConvertTo-Json -InputObject $task.TaskName -Compress;
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }"#;

fn register_windows(
    name: &str,
    kind: &str,
    schedule: &str,
    command: &str,
    run: &mut impl FnMut(&str, &[&str], Option<&[u8]>) -> Result<Output>,
) -> Result<Value> {
    windows_command_text(command)?;
    let id = format!("AetherShell-{kind}-{}", uuid::Uuid::new_v4());
    let cwd = std::env::current_dir().map_err(|_| {
        safety::bad_state(
            name,
            "working directory unavailable",
            "an existing working directory",
        )
    })?;
    let payload = serde_json::to_vec(
        &serde_json::json!({"id":id,"kind":kind,"schedule":schedule,"command":command,"cwd":cwd}),
    )?;
    let output = run(
        "powershell",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_REGISTER,
        ],
        Some(&payload),
    )?;
    check(name, "powershell Register-ScheduledTask", &output)?;
    let reported: String = serde_json::from_slice(&output.stdout).map_err(|_| {
        safety::bad_state(
            name,
            "task may have been registered but its ID could not be read",
            &format!("inspect task {id} before retrying"),
        )
    })?;
    if reported != id {
        return Err(safety::bad_state(
            name,
            "registered task ID differs from requested ID",
            &format!("inspect task {id} before retrying"),
        ));
    }
    Ok(Value::Str(id))
}

#[cfg(windows)]
pub(super) fn list_windows_at() -> Result<Value> {
    const SCRIPT: &str = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false);
try { ConvertTo-Json -InputObject @(Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.TaskName -like 'AetherShell-at-*' -and $_.TaskPath -eq '\' } | ForEach-Object { [pscustomobject]@{job_id=$_.TaskName; queue='windows'; user=$_.Principal.UserId; schedule=($_.Triggers | Select-Object -First 1).StartBoundary; state=$_.State.ToString()} }) -Depth 3 -Compress } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }"#;
    parse_windows_at(run_tool(
        "at_list",
        "powershell",
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ],
        None,
    )?)
}

#[cfg(any(windows, test))]
fn parse_windows_at(output: Output) -> Result<Value> {
    check("at_list", "powershell Get-ScheduledTask", &output)?;
    let malformed = || {
        safety::bad_state(
            "at_list",
            "unexpected one-time task JSON",
            "complete AetherShell task records",
        )
    };
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| malformed())?;
    let rows = json.as_array().ok_or_else(malformed)?;
    let mut result = Vec::new();
    for row in rows {
        let mut record = std::collections::BTreeMap::new();
        for field in ["job_id", "queue", "user", "schedule", "state"] {
            let value = row
                .get(field)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(malformed)?;
            if field == "job_id" && !valid_windows_id(value) {
                return Err(malformed());
            }
            record.insert(field.into(), Value::Str(value.into()));
        }
        result.push(Value::Record(record));
    }
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn failed() -> Output {
        let mut output = cron_success_output(vec![]);
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            output.status = std::process::ExitStatus::from_raw(1);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            output.status = std::process::ExitStatus::from_raw(256);
        }
        output.stderr = b"access denied".to_vec();
        output
    }
    #[test]
    fn recurring_unix_updates_preserve_existing_jobs_and_return_lifecycle_target() {
        let mut table = "# keep\nSHELL=/bin/sh\n@daily existing".to_string();
        let value = add_backend(
            "0 1 * * *",
            "echo new  value",
            false,
            |tool, args, stdin| {
                assert_eq!(tool, "crontab");
                if args == ["-l"] {
                    assert!(stdin.is_none());
                    Ok(cron_success_output(table.as_bytes().to_vec()))
                } else {
                    assert_eq!(args, &["-"]);
                    table = String::from_utf8(stdin.unwrap().to_vec()).unwrap();
                    Ok(cron_success_output(vec![]))
                }
            },
        )
        .unwrap();
        assert_eq!(value, Value::Str("0 1 * * * echo new  value".into()));
        assert_eq!(
            table,
            "# keep\nSHELL=/bin/sh\n@daily existing\n0 1 * * * echo new  value\n"
        );
        let duplicate = add_backend("0 1 * * *", "echo new  value", false, |_, args, _| {
            assert_eq!(args, &["-l"]);
            Ok(cron_success_output(table.as_bytes().to_vec()))
        });
        assert!(duplicate.unwrap_err().to_string().contains("E_BAD_STATE"));
    }
    #[test]
    fn recurring_failures_do_not_clobber_tables_or_report_success() {
        let mut calls = 0;
        let result = add_backend("@daily", "echo test", false, |_, _, _| {
            calls += 1;
            Ok(failed())
        });
        assert!(result.unwrap_err().to_string().contains("E_TOOL_FAILED"));
        assert_eq!(calls, 1);
        calls = 0;
        let result = add_backend("@daily", "echo test", false, |_, _, _| {
            calls += 1;
            Ok(if calls == 1 {
                cron_success_output(b"# preserved\n".to_vec())
            } else {
                failed()
            })
        });
        assert!(result.unwrap_err().to_string().contains("E_TOOL_FAILED"));
        assert_eq!(calls, 2);
    }
    #[test]
    fn one_time_unix_jobs_use_numeric_timestamp_and_stdin_and_check_confirmation() {
        let value = schedule_backend("2099-01-02T03:04", "echo 'owned fixture'", false, |tool, args, stdin| {
            assert_eq!(tool, "at"); assert_eq!(args, &["-t", "209901020304"]);
            assert_eq!(stdin, Some(b"echo 'owned fixture'\n".as_slice()));
            let mut output = cron_success_output(vec![]);
            output.stderr = b"warning: commands will be executed using /bin/sh\njob 12 at Fri Jan 2 03:04:00 2099\n".to_vec(); Ok(output)
        }).unwrap();
        assert_eq!(value, Value::Str("12".into()));
        for message in [
            "",
            "job 0 at date",
            "job x at date",
            "job 12 at date\njob 13 at date",
        ] {
            let result = schedule_backend("2099-01-02T03:04", "echo x", false, |_, _, _| {
                let mut output = cron_success_output(vec![]);
                output.stderr = message.as_bytes().to_vec();
                Ok(output)
            });
            assert!(result.unwrap_err().to_string().contains("E_BAD_STATE"));
        }
        assert!(
            schedule_backend("2099-01-02T03:04", "echo x", false, |_, _, _| Ok(failed()))
                .unwrap_err()
                .to_string()
                .contains("E_TOOL_FAILED")
        );
    }
    #[test]
    fn registration_passes_commands_as_json_data_and_requires_the_exact_id() {
        let command = "echo Unicode é; $([Console]::Error.WriteLine('not PowerShell code'))";
        for (kind, schedule) in [("cron", "@daily"), ("at", "2099-01-02T03:04")] {
            let value =
                register_windows("test", kind, schedule, command, &mut |tool, args, stdin| {
                    assert_eq!(tool, "powershell");
                    assert!(args.contains(&"-NoProfile"));
                    assert!(args.contains(&"-NonInteractive"));
                    assert_eq!(args.last(), Some(&WINDOWS_REGISTER));
                    assert!(!WINDOWS_REGISTER.contains(command));
                    let payload: serde_json::Value =
                        serde_json::from_slice(stdin.unwrap()).unwrap();
                    assert_eq!(payload["command"], command);
                    assert_eq!(payload["kind"], kind);
                    assert_eq!(payload["schedule"], schedule);
                    Ok(cron_success_output(
                        serde_json::to_vec(&payload["id"]).unwrap(),
                    ))
                })
                .unwrap();
            assert!(
                matches!(value, Value::Str(id) if id.starts_with(&format!("AetherShell-{kind}-")))
            );
        }
        for reply in [b"null".as_slice(), b"\"different task\"", b""] {
            assert!(register_windows(
                "test",
                "at",
                "2099-01-02T03:04",
                "echo x",
                &mut |_, _, _| Ok(cron_success_output(reply.to_vec()))
            )
            .unwrap_err()
            .to_string()
            .contains("E_BAD_STATE"));
        }
    }
    #[test]
    fn removal_is_exact_and_invalid_ids_cannot_reach_tools() {
        for (windows, id, tool, expected) in [
            (false, "12", "atrm", vec!["12"]),
            (
                true,
                "AetherShell-at-12345678-1234-4234-8234-123456789012",
                "schtasks",
                vec![
                    "/Delete",
                    "/TN",
                    "AetherShell-at-12345678-1234-4234-8234-123456789012",
                    "/F",
                ],
            ),
        ] {
            assert_eq!(
                remove_backend(id, windows, |actual, args, stdin| {
                    assert_eq!(actual, tool);
                    assert_eq!(args, expected);
                    assert!(stdin.is_none());
                    Ok(cron_success_output(vec![]))
                })
                .unwrap(),
                Value::Bool(true)
            );
            assert!(remove_backend(id, windows, |_, _, _| Ok(failed()))
                .unwrap_err()
                .to_string()
                .contains("E_TOOL_FAILED"));
        }
        for windows in [true, false] {
            for id in [
                "",
                "*",
                "-1",
                "0",
                "12\n13",
                "18446744073709551616",
                "AetherShell-at-*",
            ] {
                assert!(
                    remove_backend(id, windows, |_, _, _| panic!("invalid ID reached tool"))
                        .unwrap_err()
                        .to_string()
                        .contains("E_BAD_ARG")
                );
            }
        }
    }
    #[test]
    fn windows_queue_requires_complete_typed_records() {
        assert_eq!(
            parse_windows_at(cron_success_output(b"[]".to_vec())).unwrap(),
            Value::Array(vec![])
        );
        let row = serde_json::json!([{"job_id":"AetherShell-at-12345678-1234-4234-8234-123456789012","queue":"windows","user":"owned-user","schedule":"2099-01-02T03:04:00","state":"Ready"}]);
        assert!(
            matches!(parse_windows_at(cron_success_output(serde_json::to_vec(&row).unwrap())).unwrap(), Value::Array(rows) if rows.len() == 1)
        );
        for text in ["", "null", "{}", "[{}]"] {
            assert!(
                parse_windows_at(cron_success_output(text.as_bytes().to_vec()))
                    .unwrap_err()
                    .to_string()
                    .contains("E_BAD_STATE")
            );
        }
        assert!(parse_windows_at(failed())
            .unwrap_err()
            .to_string()
            .contains("E_TOOL_FAILED"));
    }
    #[test]
    fn malformed_commands_schedules_and_times_fail_before_tools() {
        for schedule in ["", "@unknown", "* *", "* * * * * *", "@daily\n@hourly"] {
            assert!(cron_entry(schedule, "echo x", false).is_err());
        }
        for command in ["", " ", "echo a\necho b", "echo\0bad"] {
            assert!(cron_entry("@daily", command, false).is_err());
        }
        for time in [
            "",
            "2000-01-01T00:00",
            "2099-02-30T00:00",
            "2099-01-01T25:00",
            "2099-01-01T12:00Z",
            "+10000-01-01T12:00",
        ] {
            assert!(local_time(time).is_err());
        }
        assert!(windows_command_text(&"x".repeat(4096)).is_ok());
        assert!(windows_command_text(&"x".repeat(4097)).is_err());
        assert!(windows_command_text(&"😀".repeat(2048)).is_ok());
        assert!(windows_command_text(&"😀".repeat(2049)).is_err());
        assert!(register_windows(
            "test",
            "cron",
            "@daily",
            &"x".repeat(4097),
            &mut |_, _, _| panic!("oversized command reached scheduler")
        )
        .is_err());
    }
    #[cfg(windows)]
    #[test]
    fn fixed_registration_script_runs_with_disposable_cmdlet_stubs() {
        // Only these process-local functions run: there are no calls into the
        // host scheduler. This witnesses real PowerShell parameter binding,
        // Unicode stdin decoding, quoting and all trigger branches.
        let stubs = r#"
function New-ScheduledTaskAction { param($Execute,$Argument,$WorkingDirectory) if ($Argument -ne ('/d /s /c "'+$p.command+'"')) { throw 'command changed' }; if (!(Test-Path -LiteralPath $WorkingDirectory)) { throw 'cwd missing' }; [pscustomobject]@{Argument=$Argument} }
function New-ScheduledTaskTrigger { param([switch]$Once,[switch]$Daily,[switch]$Weekly,[datetime]$At,$DaysOfWeek,[timespan]$RepetitionInterval) if (!$Once -and !$Daily -and !$Weekly) { throw 'trigger missing' }; [pscustomobject]@{At=$At} }
function New-ScheduledTaskPrincipal { param($UserId,$LogonType,$RunLevel) if ($LogonType -ne 'Interactive' -or $RunLevel -ne 'Limited') { throw 'principal changed' }; [pscustomobject]@{UserId=$UserId} }
function Register-ScheduledTask { [CmdletBinding()]param($TaskName,$TaskPath,$Action,$Trigger,$Principal) if ($TaskPath -ne '\') { throw 'unexpected task folder' }; if (!$Action -or !$Trigger -or !$Principal) { throw 'missing registration fields' }; [pscustomobject]@{TaskName=$TaskName} }
"#;
        for (kind, schedule) in [
            ("cron", "@hourly"),
            ("cron", "@daily"),
            ("cron", "@weekly"),
            ("at", "2099-01-02T03:04"),
        ] {
            register_windows(
                "test",
                kind,
                schedule,
                "echo é; $([Console]::Error.WriteLine('INJECTED'))",
                &mut |tool, args, stdin| {
                    let script = format!("{stubs}\n{}", args.last().unwrap());
                    let mut changed = args.to_vec();
                    *changed.last_mut().unwrap() = &script;
                    run_tool("test", tool, &changed, stdin)
                },
            )
            .unwrap();
        }
    }
}
