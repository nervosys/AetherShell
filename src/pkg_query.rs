//! Bounded readers and pure parsers behind `pkg_list` and `pkg_history`.
//!
//! `pkg_list` runs `dpkg-query` (apt systems) or `pacman -Q` with an argv and
//! no shell, or on Windows reads the Add/Remove Programs registry keys through
//! PowerShell, under a 60 s deadline and a 64 MiB output cap. `pkg_history`
//! reads the uncompressed `/var/log/apt/history.log`, at most its last 64 MiB.
//! The parsers are pure functions over text, so their edge cases are unit
//! tested on every platform; the readers only run on Linux.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::linux_diagnostics::{self, Run};
use crate::safety;
use crate::value::Value;
use anyhow::Result;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

/// Most output kept from one package-tool run, and bytes read from the log.
pub(crate) const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// How long a package-tool run may take before it is killed.
pub(crate) const TOOL_LIMIT: Duration = Duration::from_secs(60);
/// Longest single line any parser accepts.
pub(crate) const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Most installed rows one listing may return.
pub(crate) const MAX_ROWS: usize = 200_000;
/// Most packages on one history line, and in one transaction.
pub(crate) const MAX_PACKAGES_PER_LINE: usize = 10_000;
pub(crate) const MAX_PACKAGES_PER_TRANSACTION: usize = 100_000;
/// The apt log `pkg_history` reads. Only this uncompressed file.
pub(crate) const HISTORY_LOG: &str = "/var/log/apt/history.log";
pub(crate) const HISTORY_DEFAULT: usize = 50;
pub(crate) const HISTORY_MAX: usize = 1000;

/// A package as a (name, version) pair.
pub(crate) type Package = (String, String);

/// Why a parser stopped. Each is a bound, or a line that is not in the format.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    /// Input exceeded a bound. The text says which.
    TooLarge(&'static str),
    /// A line did not have the fields the format requires.
    BadLine { line: usize },
}

/// One apt transaction from `history.log`, as `pkg_history` returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transaction {
    pub(crate) start: String,
    pub(crate) end: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) installed: Vec<String>,
    pub(crate) upgraded: Vec<String>,
    pub(crate) removed: Vec<String>,
}

/// Parse `dpkg-query -W -f='${binary:Package}\t${Version}\t${db:Status-Status}\n'`.
/// Keeps rows whose status is `installed`; sorted by name, then version.
pub(crate) fn parse_dpkg_rows(text: &str) -> Result<Vec<Package>, ParseError> {
    let mut rows: Vec<Package> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(ParseError::TooLarge("a line longer than 1 MiB"));
        }
        if line.is_empty() {
            continue;
        }
        let bad = ParseError::BadLine { line: index + 1 };
        let mut fields = line.split('\t');
        let (Some(name), Some(version), Some(status), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(bad);
        };
        if name.is_empty() || version.is_empty() {
            return Err(bad);
        }
        if status != "installed" {
            continue;
        }
        if rows.len() >= MAX_ROWS {
            return Err(ParseError::TooLarge("more than 200000 installed packages"));
        }
        rows.push((name.to_string(), version.to_string()));
    }
    rows.sort();
    Ok(rows)
}

/// Parse `pacman -Q`: one `name version` pair per line, split at the first space.
pub(crate) fn parse_pacman_rows(text: &str) -> Result<Vec<Package>, ParseError> {
    let mut rows: Vec<Package> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(ParseError::TooLarge("a line longer than 1 MiB"));
        }
        if line.trim().is_empty() {
            continue;
        }
        let bad = ParseError::BadLine { line: index + 1 };
        let Some((name, version)) = line.split_once(' ') else {
            return Err(bad);
        };
        let version = version.trim();
        if name.is_empty() || version.is_empty() {
            return Err(bad);
        }
        if rows.len() >= MAX_ROWS {
            return Err(ParseError::TooLarge("more than 200000 installed packages"));
        }
        rows.push((name.to_string(), version.to_string()));
    }
    rows.sort();
    Ok(rows)
}

/// The last bytes of a history log that began mid-file. The partial block at
/// the front is dropped: the text starts at the first `Start-Date:` line.
/// Bytes that begin exactly at a block boundary are kept as they are.
pub(crate) fn skip_partial_head(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(b"Start-Date:") {
        return bytes;
    }
    match find(bytes, b"\nStart-Date:") {
        Some(at) => &bytes[at + 1..],
        None => &[],
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse apt's `history.log` into its newest `limit` transactions, newest
/// first. A block runs from `Start-Date:` to `End-Date:`; a block cut off
/// before its `End-Date:` has `end` None, and a block with no `Commandline:`
/// has `command` None. Unknown keys are ignored. Each package entry is its
/// text before " (", so `curl:amd64 (7.88, automatic)` is `curl:amd64`.
pub(crate) fn parse_history(text: &str, limit: usize) -> Result<Vec<Transaction>, ParseError> {
    let mut window: VecDeque<Transaction> = VecDeque::new();
    let mut open: Option<Transaction> = None;
    let mut packages = 0usize;
    for line in text.lines() {
        if line.len() > MAX_LINE_BYTES {
            return Err(ParseError::TooLarge("a history line longer than 1 MiB"));
        }
        if let Some(rest) = line.strip_prefix("Start-Date:") {
            if let Some(done) = open.take() {
                keep_newest(&mut window, done, limit);
            }
            packages = 0;
            open = Some(Transaction {
                start: rest.trim().to_string(),
                end: None,
                command: None,
                installed: Vec::new(),
                upgraded: Vec::new(),
                removed: Vec::new(),
            });
            continue;
        }
        let Some(tx) = open.as_mut() else { continue };
        if let Some(rest) = line.strip_prefix("End-Date:") {
            tx.end = Some(rest.trim().to_string());
            if let Some(done) = open.take() {
                keep_newest(&mut window, done, limit);
            }
        } else if let Some(rest) = line.strip_prefix("Commandline:") {
            tx.command = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Install:") {
            add_packages(rest, &mut tx.installed, &mut packages)?;
        } else if let Some(rest) = line.strip_prefix("Upgrade:") {
            add_packages(rest, &mut tx.upgraded, &mut packages)?;
        } else if let Some(rest) = line
            .strip_prefix("Remove:")
            .or_else(|| line.strip_prefix("Purge:"))
        {
            add_packages(rest, &mut tx.removed, &mut packages)?;
        }
    }
    if let Some(done) = open.take() {
        keep_newest(&mut window, done, limit);
    }
    Ok(window.into_iter().rev().collect())
}

/// Keep only the newest `limit` transactions. The window holds at most
/// `limit` entries, oldest first, so memory does not grow with the log.
fn keep_newest(window: &mut VecDeque<Transaction>, tx: Transaction, limit: usize) {
    if limit == 0 {
        return;
    }
    if window.len() == limit {
        window.pop_front();
    }
    window.push_back(tx);
}

/// Add the package names in one `Install:`-style value. Entries are separated
/// by `), ` because a version may hold a comma inside its parentheses.
fn add_packages(
    value: &str,
    out: &mut Vec<String>,
    packages: &mut usize,
) -> Result<(), ParseError> {
    let mut on_line = 0usize;
    for segment in value.split("), ") {
        let name = segment.split(" (").next().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        on_line += 1;
        *packages += 1;
        if on_line > MAX_PACKAGES_PER_LINE || *packages > MAX_PACKAGES_PER_TRANSACTION {
            return Err(ParseError::TooLarge(
                "more packages in one transaction than the call allows",
            ));
        }
        out.push(name.to_string());
    }
    Ok(())
}

/// A transaction as the record `pkg_history` returns.
pub(crate) fn transaction_value(tx: Transaction) -> Value {
    let strings = |items: Vec<String>| Value::Array(items.into_iter().map(Value::Str).collect());
    let mut rec = BTreeMap::new();
    rec.insert("start".to_string(), Value::Str(tx.start));
    rec.insert("end".to_string(), tx.end.map_or(Value::Null, Value::Str));
    rec.insert(
        "command".to_string(),
        tx.command.map_or(Value::Null, Value::Str),
    );
    rec.insert("installed".to_string(), strings(tx.installed));
    rec.insert("upgraded".to_string(), strings(tx.upgraded));
    rec.insert("removed".to_string(), strings(tx.removed));
    Value::Record(rec)
}

/// Turn a parser's bound or format failure into the coded error a caller sees.
pub(crate) fn parse_failure(builtin: &str, source: &str, e: ParseError) -> anyhow::Error {
    match e {
        ParseError::TooLarge(what) => safety::budget_exceeded(
            builtin,
            format!("{source} has {what}; nothing was returned"),
            "narrow the request; the bounds are fixed",
        ),
        ParseError::BadLine { line } => safety::tool_failed(
            builtin,
            source,
            Some(0),
            &format!("output line {line} is not in the expected format; nothing was returned"),
        ),
    }
}

/// Fail unless a package-tool run finished in time, under its output cap, and
/// with exit status 0. Its stderr tail is carried into a nonzero exit.
fn check_run(builtin: &str, tool: &str, run: &Run, max_output: usize) -> Result<()> {
    if run.timed_out {
        return Err(safety::budget_exceeded(
            builtin,
            format!(
                "`{tool}` was still running after {}s and was killed; nothing was returned",
                TOOL_LIMIT.as_secs()
            ),
            "the time limit is fixed; retry when the host is less busy",
        ));
    }
    if run.truncated {
        return Err(safety::budget_exceeded(
            builtin,
            format!("`{tool}` printed more than {max_output} bytes; nothing was returned"),
            "the output cap is fixed; query the package manager directly",
        ));
    }
    if run.code != Some(0) {
        return Err(safety::tool_failed(
            builtin,
            tool,
            run.code,
            stderr_tail(&run.stderr),
        ));
    }
    Ok(())
}

/// The last 2 KiB of a stderr text, cut at a character boundary.
fn stderr_tail(text: &str) -> &str {
    const KEEP: usize = 2048;
    if text.len() <= KEEP {
        return text;
    }
    let mut at = text.len() - KEEP;
    while !text.is_char_boundary(at) {
        at += 1;
    }
    &text[at..]
}

/// Installed packages from `dpkg-query`, run as an argv with no shell.
pub(crate) fn list_dpkg(builtin: &str) -> Result<Vec<Package>> {
    let argv = [
        "-W".to_string(),
        "-f=${binary:Package}\\t${Version}\\t${db:Status-Status}\\n".to_string(),
    ];
    let run = linux_diagnostics::run_bounded_capped(
        builtin,
        "dpkg-query",
        "dpkg-query",
        &argv,
        TOOL_LIMIT,
        MAX_OUTPUT_BYTES,
    )?;
    check_run(builtin, "dpkg-query", &run, MAX_OUTPUT_BYTES)?;
    parse_dpkg_rows(&run.stdout).map_err(|e| parse_failure(builtin, "dpkg-query", e))
}

/// Installed packages from `pacman -Q`, run as an argv with no shell.
pub(crate) fn list_pacman(builtin: &str) -> Result<Vec<Package>> {
    let argv = ["-Q".to_string()];
    let run = linux_diagnostics::run_bounded_capped(
        builtin,
        "pacman",
        "pacman",
        &argv,
        TOOL_LIMIT,
        MAX_OUTPUT_BYTES,
    )?;
    check_run(builtin, "pacman", &run, MAX_OUTPUT_BYTES)?;
    parse_pacman_rows(&run.stdout).map_err(|e| parse_failure(builtin, "pacman", e))
}

/// Parse the `name<TAB>version` lines [`ARP_SCRIPT`] prints. The version may
/// be empty: a program need not record one. Sorted by name, then version,
/// with exact duplicates (the same program in two hives) kept once.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_arp_rows(text: &str) -> Result<Vec<Package>, ParseError> {
    let mut rows: Vec<Package> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(ParseError::TooLarge("a line longer than 1 MiB"));
        }
        if line.is_empty() {
            continue;
        }
        let bad = ParseError::BadLine { line: index + 1 };
        let Some((name, version)) = line.split_once('\t') else {
            return Err(bad);
        };
        if name.is_empty() || version.contains('\t') {
            return Err(bad);
        }
        if rows.len() >= MAX_ROWS {
            return Err(ParseError::TooLarge("more than 200000 installed packages"));
        }
        rows.push((name.to_string(), version.to_string()));
    }
    rows.sort();
    rows.dedup();
    Ok(rows)
}

/// Prints one `name<TAB>version` line per program in Add/Remove Programs:
/// the 64-bit and 32-bit machine Uninstall keys and the current user's. As
/// in the Control Panel list, entries without a DisplayName, system
/// components and updates (those with a ParentKeyName) are left out. Tabs
/// and line breaks inside a value become spaces, so each entry is one line.
#[cfg(windows)]
const ARP_SCRIPT: &str = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false);
$keys = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*';
foreach ($k in $keys) {
  if (-not (Test-Path ($k.TrimEnd('*')))) { continue }
  Get-ItemProperty -Path $k -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -and $_.SystemComponent -ne 1 -and -not $_.ParentKeyName } |
    ForEach-Object {
      $n = ([string]$_.DisplayName -replace '[\t\r\n]', ' ').Trim();
      $v = ([string]$_.DisplayVersion -replace '[\t\r\n]', ' ').Trim();
      if ($n) { "$n`t$v" }
    }
}"#;

/// Installed programs from the Windows Add/Remove Programs registry keys,
/// read by PowerShell with no profile under the same deadline and cap.
#[cfg(windows)]
pub(crate) fn list_windows_arp(builtin: &str) -> Result<Vec<Package>> {
    let argv = [
        "-NoLogo".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        ARP_SCRIPT.to_string(),
    ];
    let run = linux_diagnostics::run_bounded_capped(
        builtin,
        "powershell",
        "powershell",
        &argv,
        TOOL_LIMIT,
        MAX_OUTPUT_BYTES,
    )?;
    check_run(builtin, "powershell", &run, MAX_OUTPUT_BYTES)?;
    parse_arp_rows(&run.stdout)
        .map_err(|e| parse_failure(builtin, "the Uninstall registry keys", e))
}

/// The last `MAX_OUTPUT_BYTES` of the apt history log, as text. A read that
/// began mid-file drops its partial first block. The file is read directly;
/// nothing is run.
pub(crate) fn read_history_tail(builtin: &str, path: &str) -> Result<String> {
    let fail = |e: std::io::Error| safety::fs_error(builtin, path, &e);
    let mut file = std::fs::File::open(path).map_err(fail)?;
    let len = file.metadata().map_err(fail)?.len();
    let start = len.saturating_sub(MAX_OUTPUT_BYTES as u64);
    if start > 0 {
        file.seek(SeekFrom::Start(start)).map_err(fail)?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_OUTPUT_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(fail)?;
    let kept = if start > 0 {
        skip_partial_head(&bytes)
    } else {
        &bytes[..]
    };
    Ok(String::from_utf8_lossy(kept).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> Package {
        (name.to_string(), version.to_string())
    }

    fn sample_block(start: &str, command: &str) -> String {
        format!(
            "Start-Date: {start}\nCommandline: {command}\nInstall: curl:amd64 (7.88.1-10, automatic), libcurl4:amd64 (7.88.1-10, automatic)\nEnd-Date: {start}\n\n"
        )
    }

    #[test]
    fn arp_rows_are_sorted_deduplicated_and_may_lack_a_version() {
        let text = "Zoom\t5.17.1\r\nGit\t2.44.0\r\nGit\t2.44.0\r\n\r\nSome Tool\t\r\n";
        assert_eq!(
            parse_arp_rows(text),
            Ok(vec![
                pkg("Git", "2.44.0"),
                pkg("Some Tool", ""),
                pkg("Zoom", "5.17.1")
            ])
        );
    }

    #[test]
    fn arp_refuses_a_line_without_a_tab_an_empty_name_or_a_third_field() {
        assert_eq!(
            parse_arp_rows("Git\t2.44\nno tab here\n"),
            Err(ParseError::BadLine { line: 2 })
        );
        assert_eq!(
            parse_arp_rows("\t1.0\n"),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_arp_rows("a\tb\tc\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn dpkg_keeps_installed_rows_sorted_and_drops_the_rest() {
        let text = "zsh\t5.9\tinstalled\nbash\t5.2\tinstalled\nold\t1.0\tconfig-files\nbash\t5.1\tinstalled\n";
        assert_eq!(
            parse_dpkg_rows(text),
            Ok(vec![
                pkg("bash", "5.1"),
                pkg("bash", "5.2"),
                pkg("zsh", "5.9")
            ])
        );
    }

    #[test]
    fn dpkg_keeps_colons_in_versions_and_blank_lines_are_skipped() {
        let text = "\ndpkg\t1.21.22\tinstalled\n\nlibc6\t1:2.36-9\tinstalled\n";
        assert_eq!(
            parse_dpkg_rows(text),
            Ok(vec![pkg("dpkg", "1.21.22"), pkg("libc6", "1:2.36-9")])
        );
    }

    #[test]
    fn dpkg_refuses_a_line_without_three_fields() {
        assert_eq!(
            parse_dpkg_rows("bash\t5.2\tinstalled\nbroken line\n"),
            Err(ParseError::BadLine { line: 2 })
        );
        assert_eq!(
            parse_dpkg_rows("a\tb\tinstalled\textra\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn dpkg_refuses_an_empty_name_or_version() {
        assert_eq!(
            parse_dpkg_rows("\t1.0\tinstalled\n"),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_dpkg_rows("curl\t\tinstalled\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn dpkg_refuses_an_overlong_line() {
        let long = format!("{}\t1.0\tinstalled\n", "a".repeat(MAX_LINE_BYTES));
        assert!(matches!(
            parse_dpkg_rows(&long),
            Err(ParseError::TooLarge(_))
        ));
    }

    #[test]
    fn dpkg_refuses_more_installed_rows_than_the_bound() {
        let mut text = String::new();
        for _ in 0..=MAX_ROWS {
            text.push_str("p\t1\tinstalled\n");
        }
        assert!(matches!(
            parse_dpkg_rows(&text),
            Err(ParseError::TooLarge(_))
        ));
    }

    #[test]
    fn pacman_splits_name_and_version_at_the_first_space() {
        assert_eq!(
            parse_pacman_rows("linux 6.10.1.arch1-1\nbash 5.2.26-2\n\n"),
            Ok(vec![
                pkg("bash", "5.2.26-2"),
                pkg("linux", "6.10.1.arch1-1")
            ])
        );
    }

    #[test]
    fn pacman_refuses_a_line_without_a_version() {
        assert_eq!(
            parse_pacman_rows("bash\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn history_returns_blocks_newest_first_with_every_field() {
        let text = "Start-Date: 2026-09-01  10:02:11\nCommandline: apt install curl\nInstall: curl:amd64 (7.88.1-10, automatic)\nEnd-Date: 2026-09-01  10:02:14\n\nStart-Date: 2026-09-02  09:00:00\nCommandline: apt remove nano\nRemove: nano:amd64 (7.2-2)\nPurge: nano-tiny:amd64 (7.2-2)\nEnd-Date: 2026-09-02  09:00:01\n";
        let got = parse_history(text, 10).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].start, "2026-09-02  09:00:00");
        assert_eq!(got[0].end.as_deref(), Some("2026-09-02  09:00:01"));
        assert_eq!(got[0].command.as_deref(), Some("apt remove nano"));
        assert_eq!(got[0].removed, vec!["nano:amd64", "nano-tiny:amd64"]);
        assert!(got[0].installed.is_empty() && got[0].upgraded.is_empty());
        assert_eq!(got[1].installed, vec!["curl:amd64"]);
        assert_eq!(got[1].command.as_deref(), Some("apt install curl"));
    }

    #[test]
    fn history_entries_with_commas_inside_versions_split_cleanly() {
        let text = "Start-Date: x\nUpgrade: libc6:amd64 (2.36-1, 2.36-2), openssl:amd64 (3.0.11-1, 3.0.13-1)\nEnd-Date: y\n";
        let got = parse_history(text, 1).unwrap();
        assert_eq!(got[0].upgraded, vec!["libc6:amd64", "openssl:amd64"]);
    }

    #[test]
    fn history_block_cut_short_has_no_end_and_missing_command_is_none() {
        let text = "Start-Date: a\nInstall: x:amd64 (1)\n\nStart-Date: b\nCommandline: apt update\nEnd-Date: c\n";
        let got = parse_history(text, 10).unwrap();
        assert_eq!(got[0].start, "b");
        assert_eq!(got[1].start, "a");
        assert_eq!(got[1].end, None);
        assert_eq!(got[1].command, None);
        assert_eq!(got[0].command.as_deref(), Some("apt update"));
    }

    #[test]
    fn history_ignores_lines_before_the_first_block_and_unknown_keys() {
        let text = format!(
            "Log started: 2026\nReinstall: x:amd64 (1)\n{}",
            sample_block("2026-01-01  00:00:00", "apt install x")
        );
        let got = parse_history(&text, 5).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].installed, vec!["curl:amd64", "libcurl4:amd64"]);
        assert!(got[0].upgraded.is_empty() && got[0].removed.is_empty());
    }

    #[test]
    fn history_keeps_only_the_newest_limit_transactions() {
        let mut text = String::new();
        for day in 1..=5 {
            text.push_str(&sample_block(&format!("day{day}"), "apt upgrade"));
        }
        let got = parse_history(&text, 2).unwrap();
        let starts: Vec<&str> = got.iter().map(|t| t.start.as_str()).collect();
        assert_eq!(starts, vec!["day5", "day4"]);
    }

    #[test]
    fn history_limit_zero_returns_nothing() {
        let text = sample_block("d", "apt install x");
        assert_eq!(parse_history(&text, 0), Ok(Vec::new()));
    }

    #[test]
    fn history_empty_log_returns_nothing() {
        assert_eq!(parse_history("", 50), Ok(Vec::new()));
    }

    #[test]
    fn history_refuses_a_line_longer_than_the_bound() {
        let text = format!("Install: {}\n", "a (1), ".repeat(MAX_LINE_BYTES / 4));
        assert!(matches!(
            parse_history(&format!("Start-Date: s\n{text}"), 1),
            Err(ParseError::TooLarge(_))
        ));
    }

    #[test]
    fn history_refuses_more_packages_on_one_line_than_the_bound() {
        let names: Vec<String> = (0..=MAX_PACKAGES_PER_LINE)
            .map(|i| format!("p{i} (1)"))
            .collect();
        let text = format!(
            "Start-Date: s\nInstall: {}\nEnd-Date: e\n",
            names.join(", ")
        );
        assert!(matches!(
            parse_history(&text, 1),
            Err(ParseError::TooLarge(_))
        ));
    }

    #[test]
    fn partial_head_is_dropped_up_to_the_next_block() {
        let bytes = b"mmandline: apt x\nEnd-Date: y\n\nStart-Date: z\nEnd-Date: w\n";
        assert_eq!(skip_partial_head(bytes), b"Start-Date: z\nEnd-Date: w\n");
    }

    #[test]
    fn a_read_that_starts_on_a_block_boundary_is_kept_whole() {
        let bytes = b"Start-Date: z\nEnd-Date: w\n";
        assert_eq!(skip_partial_head(bytes), &bytes[..]);
    }

    #[test]
    fn a_read_with_no_block_start_yields_nothing() {
        assert_eq!(skip_partial_head(b"no block here\nat all"), b"");
    }

    #[test]
    fn stderr_tail_keeps_the_end_and_respects_characters() {
        let long = "é".repeat(3000);
        let tail = stderr_tail(&long);
        assert!(tail.len() <= 2048 && long.ends_with(tail));
        assert_eq!(stderr_tail("short"), "short");
    }

    #[test]
    fn transaction_record_has_the_documented_shape() {
        let value = transaction_value(Transaction {
            start: "s".into(),
            end: None,
            command: None,
            installed: vec!["curl:amd64".into()],
            upgraded: Vec::new(),
            removed: Vec::new(),
        });
        let Value::Record(rec) = value else {
            panic!("not a record")
        };
        let keys: Vec<&str> = rec.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "command",
                "end",
                "installed",
                "removed",
                "start",
                "upgraded"
            ]
        );
        assert!(matches!(rec.get("end"), Some(Value::Null)));
        assert!(matches!(rec.get("command"), Some(Value::Null)));
    }
}
