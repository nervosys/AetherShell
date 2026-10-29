#![cfg(windows)]
//! Contracts for the winget package administration builtins.
//!
//! The ordinary tests here refuse bad input and a transaction before winget is
//! started, so they change nothing. The mutating tests install, upgrade and
//! remove a real package through winget. They are ignored, and they return
//! early unless AE_DISPOSABLE_HOST is 1: run them only on a disposable Windows
//! host, with `cargo test --test windows_winget_admin -- --ignored` and that
//! variable set.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// A small, stable package whose winget identifier is `jqlang.jq`.
const PACKAGE: &str = "jqlang.jq";

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn strs(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|t| s(t)).collect())
}

fn record(fields: &[(&str, Value)]) -> Value {
    Value::Record(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(map) => map
            .get(key)
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// The error text for a refused call; panics if the call succeeded.
fn refusal(name: &str, args: Vec<Value>) -> String {
    match call(name, args, &mut Env::new()) {
        Ok(v) => panic!("{name} answered {v:?} instead of refusing"),
        Err(e) => e.to_string(),
    }
}

fn assert_refused(name: &str, args: Vec<Value>, code: &str) {
    let shown = format!("{args:?}");
    let text = refusal(name, args);
    assert!(
        text.contains(code),
        "{name}({shown}) should refuse with {code}; got: {text}"
    );
}

/// The transaction is process-wide state; tests that open one serialise here.
static TRANSACTION: Mutex<()> = Mutex::new(());

fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

/// Whether `winget list --id <id> --exact --source winget` shows the package:
/// it must exit 0 and name the identifier. winget exits 0 with no rows on some
/// hosts, so the output is checked as well. Without `--source`, `list` also
/// asks the msstore source, whose agreement a fresh runner has not accepted.
fn winget_lists(id: &str) -> bool {
    Command::new("winget")
        .args([
            "list",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .stdin(Stdio::null())
        .output()
        .map(|o| {
            o.status.success()
                && String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains(&id.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

/// Uninstall the test package when the test ends, even when it fails part way.
struct Uninstall;

impl Drop for Uninstall {
    fn drop(&mut self) {
        let _ = Command::new("winget")
            .args([
                "uninstall",
                "--id",
                PACKAGE,
                "--exact",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--disable-interactivity",
                "--silent",
            ])
            .stdin(Stdio::null())
            .status();
    }
}

// ---------------------------------------------------------------------------
// Ordinary tests: every call below is refused before winget is started.
// ---------------------------------------------------------------------------

#[test]
fn bad_identifiers_are_refused_before_winget_runs() {
    let long = "x".repeat(129);
    let many: Vec<Value> = (0..65).map(|i| s(&format!("pkg.{i}"))).collect();
    let bad: Vec<Value> = vec![
        strs(&["-x"]),
        strs(&["a b"]),
        strs(&["a;b"]),
        strs(&["a=1.0"]),
        strs(&["a$(b)"]),
        strs(&[""]),
        Value::Array(vec![s(&long)]),
        strs(&[PACKAGE, "JQLANG.JQ"]),
        strs(&[]),
        Value::Array(many),
    ];
    for names in bad {
        assert_refused("pkg_install", vec![names.clone()], "E_BAD_ARG");
        assert_refused("pkg_remove", vec![names], "E_BAD_ARG");
    }
}

#[test]
fn wrong_types_and_arguments_are_refused_before_winget_runs() {
    assert_refused("pkg_install", vec![s(PACKAGE)], "E_BAD_ARG");
    assert_refused(
        "pkg_install",
        vec![Value::Array(vec![Value::Int(1)])],
        "E_BAD_ARG",
    );
    assert_refused("pkg_install", vec![], "E_BAD_ARG");
    assert_refused(
        "pkg_install",
        vec![
            strs(&[PACKAGE]),
            record(&[("no_recommends", Value::Bool(true))]),
        ],
        "E_BAD_ARG",
    );
    assert_refused("pkg_remove", vec![s(PACKAGE)], "E_BAD_ARG");
    assert_refused("pkg_remove", vec![], "E_BAD_ARG");
    assert_refused(
        "pkg_remove",
        vec![strs(&[PACKAGE]), record(&[("purge", Value::Bool(true))])],
        "E_BAD_ARG",
    );
    assert_refused("pkg_update", vec![s("x")], "E_BAD_ARG");
}

#[test]
fn upgrade_options_are_checked_before_winget_runs() {
    assert_refused("pkg_upgrade", vec![s("jqlang.jq")], "E_BAD_ARG");
    assert_refused("pkg_upgrade", vec![record(&[("id", s("-x"))])], "E_BAD_ARG");
    assert_refused(
        "pkg_upgrade",
        vec![record(&[("id", s("a b"))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "pkg_upgrade",
        vec![record(&[("id", Value::Int(1))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "pkg_upgrade",
        vec![record(&[("dist", Value::Bool(true))])],
        "E_BAD_ARG",
    );
}

#[test]
fn pipeline_input_is_refused_by_the_winget_builtins() {
    let _serial = TRANSACTION.lock().unwrap_or_else(|e| e.into_inner());
    let calls: [(&str, Vec<Value>); 4] = [
        ("pkg_install", vec![strs(&[PACKAGE])]),
        ("pkg_remove", vec![strs(&[PACKAGE])]),
        ("pkg_update", vec![]),
        ("pkg_upgrade", vec![]),
    ];
    for (name, args) in calls {
        let text = match call_with_input(name, args, Some(Value::Int(1)), &mut Env::new()) {
            Ok(v) => panic!("{name} accepted pipeline input and answered {v:?}"),
            Err(e) => e.to_string(),
        };
        assert!(text.contains("E_BAD_ARG"), "{name} with input: {text}");
    }
}

#[test]
fn package_changes_are_refused_inside_a_transaction() {
    let _serial = TRANSACTION.lock().unwrap_or_else(|e| e.into_inner());
    struct Rollback;
    impl Drop for Rollback {
        fn drop(&mut self) {
            while aethershell::tx::rollback().is_ok() {}
        }
    }
    let _rollback = Rollback;
    aethershell::tx::begin().expect("begin transaction");
    let text = refusal("pkg_install", vec![strs(&[PACKAGE])]);
    assert!(text.contains("E_UNIMPLEMENTED"), "{text}");
    assert!(text.contains("cannot be rolled back"), "{text}");
    assert!(text.contains("NOTHING WAS CHANGED"), "{text}");
}

// ---------------------------------------------------------------------------
// Disposable tests: real winget, real packages. Ignored by default, and they
// return early unless AE_DISPOSABLE_HOST is 1.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "disposable Windows host only: AE_DISPOSABLE_HOST=1 and --ignored"]
fn disposable_install_upgrade_and_uninstall_jq() {
    if !disposable() {
        return;
    }
    assert!(!winget_lists(PACKAGE), "jqlang.jq must start absent");
    let _cleanup = Uninstall;

    let installed = call("pkg_install", vec![strs(&[PACKAGE])], &mut Env::new())
        .expect("pkg_install jqlang.jq");
    match field(&installed, "installed") {
        Value::Array(items) => assert_eq!(items.len(), 1, "one package installed"),
        other => panic!("installed is not an Array: {other:?}"),
    }
    assert!(
        winget_lists(PACKAGE),
        "winget list does not show jqlang.jq after pkg_install"
    );

    // Installed a moment ago, so nothing is newer: the upgrade is a no-op.
    let upgraded = call(
        "pkg_upgrade",
        vec![record(&[("id", s(PACKAGE))])],
        &mut Env::new(),
    )
    .expect("pkg_upgrade jqlang.jq");
    assert_eq!(
        field(&upgraded, "changed"),
        &Value::Bool(false),
        "an up-to-date package must report changed: false"
    );

    let removed =
        call("pkg_remove", vec![strs(&[PACKAGE])], &mut Env::new()).expect("pkg_remove jqlang.jq");
    match field(&removed, "removed") {
        Value::Array(items) => assert_eq!(items, &vec![s(PACKAGE)]),
        other => panic!("removed is not an Array: {other:?}"),
    }
    assert!(
        !winget_lists(PACKAGE),
        "winget list still shows jqlang.jq after pkg_remove"
    );
}

#[test]
#[ignore = "disposable Windows host only: AE_DISPOSABLE_HOST=1 and --ignored"]
fn disposable_unknown_identifier_is_not_found() {
    if !disposable() {
        return;
    }
    assert_refused(
        "pkg_install",
        vec![strs(&["ae.no-such-package-xyz"])],
        "E_NOT_FOUND",
    );
}
