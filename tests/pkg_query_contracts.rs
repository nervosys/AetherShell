//! Contracts for `pkg_list` and `pkg_history`: the installed-package listing
//! (dpkg-query, pacman, rpm or brew) and the apt history log.
//!
//! Ordinary tests run everywhere: argument and pipeline-input refusals come
//! before any platform check, so they are E_BAD_ARG on every OS. On Windows
//! pkg_list is checked against reg.exe; off Linux pkg_history is
//! E_UNIMPLEMENTED with nothing read. The rpm and brew parsers are unit tested
//! in src/pkg_query.rs; no RPM or Homebrew host has run them yet.
//!
//! The tests that read the real system are `#[ignore]`d and run only in the
//! disposable distro, via scripts/disposable_linux_tests.sh. Each checks the
//! builtin against a source other than itself (dpkg-query run directly, the
//! history file read directly) and installs nothing.

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use aethershell::env::Env;
use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
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

fn expect_code(name: &str, args: Vec<Value>, code: &str) {
    match call(name, args.clone()) {
        Ok(v) => panic!("{name}{args:?} should be refused with {code}, got {v:?}"),
        Err(e) => assert_eq!(code_of(&e), code, "{name}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: argument refusal and pipeline input, on every OS.
// ---------------------------------------------------------------------------

#[test]
fn pkg_list_refuses_any_argument() {
    expect_code("pkg_list", vec![Value::Str("x".into())], "E_BAD_ARG");
    expect_code("pkg_list", vec![Value::Int(1), Value::Int(2)], "E_BAD_ARG");
}

#[test]
fn pkg_list_refuses_pipeline_input() {
    match call_piped("pkg_list", vec![], Value::Int(1)) {
        Ok(v) => panic!("pkg_list accepted pipeline input and answered {v:?}"),
        Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{e}"),
    }
}

#[test]
fn pkg_history_refuses_a_limit_outside_one_to_a_thousand() {
    for limit in [0, -1, 1001, i64::MAX] {
        expect_code("pkg_history", vec![Value::Int(limit)], "E_BAD_ARG");
    }
}

#[test]
fn pkg_history_refuses_a_non_integer_limit_and_surplus_arguments() {
    expect_code("pkg_history", vec![Value::Str("5".into())], "E_BAD_ARG");
    expect_code(
        "pkg_history",
        vec![Value::Int(1), Value::Int(2)],
        "E_BAD_ARG",
    );
}

#[test]
fn pkg_history_refuses_pipeline_input() {
    match call_piped("pkg_history", vec![], Value::Int(1)) {
        Ok(v) => panic!("pkg_history accepted pipeline input and answered {v:?}"),
        Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{e}"),
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn pkg_history_is_unimplemented_off_linux_and_reads_nothing() {
    expect_code("pkg_history", vec![], "E_UNIMPLEMENTED");
    expect_code("pkg_history", vec![Value::Int(5)], "E_UNIMPLEMENTED");
}

/// The DisplayName values `reg.exe` reports under the Uninstall keys
/// `pkg_list` reads on Windows: an oracle with its own parser and none of
/// PowerShell. A hive with no match exits 1, which is not a failure here.
#[cfg(windows)]
fn reg_display_names() -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for key in [
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let out = std::process::Command::new("reg")
            .args(["query", key, "/s", "/v", "DisplayName"])
            .output()
            .expect("reg.exe runs");
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // A value line is `    DisplayName    <TYPE>    <data>`. The type is
            // REG_SZ or REG_EXPAND_SZ (an installer may store a %VAR% path
            // there); PowerShell expands both, so both are accepted.
            let value = line
                .trim_start()
                .strip_prefix("DisplayName")
                .map(str::trim_start)
                .and_then(|rest| rest.split_once(char::is_whitespace))
                .filter(|(kind, _)| kind.starts_with("REG_"))
                .map(|(_, value)| value);
            if let Some(value) = value {
                names.insert(value.replace(['\t', '\r', '\n'], " ").trim().to_string());
            }
        }
    }
    names
}

/// Reads the registry, writes nothing: every listed program is one reg.exe
/// also finds, and the rows are well formed and sorted.
#[cfg(windows)]
#[test]
fn pkg_list_on_windows_lists_add_remove_programs_entries() {
    let Value::Array(rows) = call("pkg_list", vec![]).expect("pkg_list answers on Windows") else {
        panic!("pkg_list did not answer an Array");
    };
    assert!(!rows.is_empty(), "no program is installed on this host?");
    let oracle = reg_display_names();
    let mut seen = Vec::new();
    for row in &rows {
        let Value::Record(rec) = row else {
            panic!("row {row:?} is not a Record");
        };
        let Some(Value::Str(name)) = rec.get("name") else {
            panic!("row {row:?} has no name");
        };
        assert!(!name.is_empty(), "an empty name: {row:?}");
        let version = match rec.get("version") {
            Some(Value::Str(v)) => {
                assert!(!v.is_empty(), "an empty version is Null: {row:?}");
                v.clone()
            }
            Some(Value::Null) => String::new(),
            other => panic!("version is {other:?} in {row:?}"),
        };
        // reg.exe prints in the console code page, so only ASCII names are
        // compared byte for byte.
        if name.is_ascii() {
            assert!(
                oracle.contains(name),
                "{name} is not a DisplayName reg.exe reports"
            );
        }
        seen.push((name.clone(), version));
    }
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(seen, sorted, "rows are sorted and unique");
}

// ---------------------------------------------------------------------------
// Disposable tests: these read the real system and run only in the distro.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn row_text(row: &Value, field: &str) -> Option<String> {
    match row {
        Value::Record(rec) => match rec.get(field) {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn pkg_list_contains_dpkg_at_the_version_dpkg_query_reports() {
    disposable::require();
    let version = std::process::Command::new("dpkg-query")
        .args(["-W", "-f=${Version}", "dpkg"])
        .output()
        .expect("dpkg-query runs on the disposable distro");
    assert!(version.status.success(), "dpkg-query -W dpkg failed");
    let want = String::from_utf8_lossy(&version.stdout).trim().to_string();

    let Value::Array(rows) = call("pkg_list", vec![]).expect("pkg_list answers") else {
        panic!("pkg_list did not answer an Array");
    };
    let got = rows
        .iter()
        .find(|row| row_text(row, "name").as_deref() == Some("dpkg"))
        .and_then(|row| row_text(row, "version"))
        .expect("pkg_list lists dpkg");
    assert_eq!(got, want);

    // The count matches the installed rows dpkg-query reports directly.
    let statuses = std::process::Command::new("dpkg-query")
        .args(["-W", "-f=${db:Status-Status}\\n"])
        .output()
        .expect("dpkg-query runs");
    let installed = String::from_utf8_lossy(&statuses.stdout)
        .lines()
        .filter(|s| *s == "installed")
        .count();
    assert_eq!(rows.len(), installed, "pkg_list row count");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn pkg_history_returns_the_logged_transactions_newest_first() {
    disposable::require();
    // The history log is read directly here, so the check does not rely on
    // the builtin's own parser.
    let text = std::fs::read_to_string("/var/log/apt/history.log")
        .expect("the disposable distro has an apt history log");
    let starts: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("Start-Date:"))
        .map(str::trim)
        .collect();

    let Value::Array(rows) = call("pkg_history", vec![]).expect("pkg_history answers") else {
        panic!("pkg_history did not answer an Array");
    };
    assert_eq!(rows.len(), starts.len().min(50), "transaction count");

    // Newest first: the dates are fixed width, so text order is time order.
    let got_starts: Vec<String> = rows
        .iter()
        .filter_map(|row| row_text(row, "start"))
        .collect();
    assert_eq!(got_starts.len(), rows.len(), "every record has a start");
    assert!(
        got_starts.windows(2).all(|pair| pair[0] >= pair[1]),
        "records are not newest first: {got_starts:?}"
    );
    if let Some(newest) = starts.last() {
        assert_eq!(got_starts.first().map(String::as_str), Some(*newest));
    }

    // Every record has the documented shape.
    for row in &rows {
        let Value::Record(rec) = row else {
            panic!("a transaction is not a record: {row:?}");
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
        for field in ["end", "command"] {
            assert!(
                matches!(rec.get(field), Some(Value::Str(_) | Value::Null)),
                "{field} is neither a String nor null"
            );
        }
        for field in ["installed", "upgraded", "removed"] {
            let Some(Value::Array(items)) = rec.get(field) else {
                panic!("{field} is not an Array");
            };
            assert!(
                items.iter().all(|item| matches!(item, Value::Str(_))),
                "{field} holds a non-string"
            );
        }
    }

    // A limit of one answers the newest transaction alone.
    let Value::Array(one) = call("pkg_history", vec![Value::Int(1)]).expect("limit 1") else {
        panic!("pkg_history(1) did not answer an Array");
    };
    assert_eq!(one.len(), 1usize.min(starts.len()));
    if let (Some(first), Some(only)) = (rows.first(), one.first()) {
        assert_eq!(format!("{first:?}"), format!("{only:?}"));
    }
}
