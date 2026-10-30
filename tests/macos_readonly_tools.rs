//! The read-only macOS backends of blkid, journalctl, monitor_ethtool and
//! pkg_history, called in-process. Each result is checked against an oracle
//! that this test gets on its own, from the same macOS tool the builtin runs
//! (diskutil, ifconfig) or from the file it reads, parsed here and not by the
//! crate.
//!
//! Nothing here changes state: every tool is run read-only, so ordinary macOS
//! CI runners can run these tests without a flag.

#![cfg(target_os = "macos")]

use iron_shell::builtins::call;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::process::Command;

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn record(v: Value) -> BTreeMap<String, Value> {
    match v {
        Value::Record(m) => m,
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// Standard output of a read-only tool, which must exit 0.
fn tool_stdout(program: &str, args: &[&str]) -> String {
    let out = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{program} did not start: {e}"));
    assert!(
        out.status.success(),
        "{program} {args:?} exited {:?}",
        out.status
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The text of the `<string>` element that follows `<key>name</key>` in a flat
/// property list. It does not look past the next key, so a nested value cannot
/// answer for a missing one.
fn plist_string(text: &str, name: &str) -> Option<String> {
    let key = format!("<key>{name}</key>");
    let at = text.find(&key)? + key.len();
    let rest = &text[at..];
    let next_key = rest.find("<key>").unwrap_or(rest.len());
    let open = rest.find("<string>")?;
    if open > next_key {
        return None;
    }
    let body = &rest[open + "<string>".len()..];
    let close = body.find("</string>")?;
    Some(body[..close].to_string())
}

#[test]
fn blkid_of_the_root_device_matches_diskutil_info() {
    // diskutil's own answer for the root volume, read here and not by the crate.
    let plist = tool_stdout("diskutil", &["info", "-plist", "/"]);
    let node = plist_string(&plist, "DeviceNode").expect("diskutil names the root device");
    let want_uuid = plist_string(&plist, "VolumeUUID")
        .or_else(|| plist_string(&plist, "DiskUUID"))
        .map(Value::Str);
    let want_type = plist_string(&plist, "FilesystemType").map(|t| Value::Str(t.to_lowercase()));
    assert!(want_uuid.is_some(), "diskutil gives a UUID for /: {plist}");

    let rec =
        record(run("blkid", vec![Value::Str(node.clone())]).expect("blkid of the root device"));
    assert_eq!(rec.get("devname").cloned(), Some(Value::Str(node)));
    assert_eq!(rec.get("uuid").cloned(), want_uuid);
    assert_eq!(rec.get("type").cloned(), want_type);
}

#[test]
fn blkid_of_a_missing_device_is_a_coded_error() {
    let err = run(
        "blkid",
        vec![Value::Str("/dev/ae-no-such-disk99".to_string())],
    )
    .expect_err("a device diskutil cannot find is refused");
    assert!(err.to_string().contains("E_"), "{err}");
}

#[test]
fn journalctl_returns_at_most_the_rows_asked_each_with_the_log_fields() {
    let v = run("journalctl", vec![Value::Int(5)]).expect("log show reads the last hour");
    let Value::Array(rows) = v else {
        panic!("journalctl returns an Array");
    };
    assert!(rows.len() <= 5, "asked for 5, got {}", rows.len());
    for row in rows {
        let m = record(row);
        assert!(m.contains_key("timestamp"), "no timestamp in {m:?}");
        assert!(m.contains_key("eventMessage"), "no eventMessage in {m:?}");
    }
}

#[test]
fn journalctl_with_a_process_name_still_caps_its_rows() {
    let v = run(
        "journalctl",
        vec![Value::Int(3), Value::Str("launchd".to_string())],
    )
    .expect("a process predicate reads the log");
    let Value::Array(rows) = v else {
        panic!("journalctl returns an Array");
    };
    assert!(rows.len() <= 3, "asked for 3, got {}", rows.len());
}

#[test]
fn monitor_ethtool_on_lo0_reports_the_mtu_that_ifconfig_reports() {
    // The header line of `ifconfig lo0` ends with `mtu N`.
    let text = tool_stdout("ifconfig", &["lo0"]);
    let header = text.lines().next().expect("ifconfig prints a header");
    let want_mtu = header
        .split(" mtu ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("the header names an mtu")
        .to_string();

    let rec = record(
        run("monitor_ethtool", vec![Value::Str("lo0".to_string())]).expect("ethtool on lo0"),
    );
    assert_eq!(
        rec.get("device").cloned(),
        Some(Value::Str("lo0".to_string()))
    );
    assert_eq!(rec.get("mtu").cloned(), Some(Value::Str(want_mtu)));
    assert_eq!(rec.get("speed").cloned(), Some(Value::Null));
    assert_eq!(rec.get("duplex").cloned(), Some(Value::Null));
}

#[test]
fn monitor_ethtool_on_a_missing_interface_is_not_found() {
    let err = run(
        "monitor_ethtool",
        vec![Value::Str("ae-no-such-if99".to_string())],
    )
    .expect_err("an interface that does not exist is refused");
    assert!(err.to_string().contains("E_"), "{err}");
}

#[test]
fn pkg_history_reads_install_log_rows_in_the_linux_shape() {
    let v = run("pkg_history", vec![Value::Int(5)]).expect("/var/log/install.log is read");
    let Value::Array(rows) = v else {
        panic!("pkg_history returns an Array");
    };
    assert!(rows.len() <= 5, "asked for 5, got {}", rows.len());
    for row in rows {
        let m = record(row);
        let keys: Vec<&str> = m.keys().map(String::as_str).collect();
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
        assert_eq!(m.get("end").cloned(), Some(Value::Null));
        assert_eq!(m.get("command").cloned(), Some(Value::Null));
        assert!(matches!(m.get("start"), Some(Value::Str(_))), "{m:?}");
        assert!(matches!(m.get("installed"), Some(Value::Array(_))), "{m:?}");
        assert!(matches!(m.get("removed"), Some(Value::Array(a)) if a.is_empty()));
        assert!(matches!(m.get("upgraded"), Some(Value::Array(a)) if a.is_empty()));
    }
}

#[test]
fn pkg_history_without_an_argument_succeeds() {
    let v = run("pkg_history", vec![]).expect("the default read succeeds");
    assert!(matches!(v, Value::Array(_)));
}
