//! Contracts for svc_create and svc_delete on Windows, through the Service
//! Control Manager.
//!
//! The ordinary tests refuse their calls before any SCM call, so they change
//! nothing on any host. The tests that create or delete a service are ignored
//! and also return early unless AE_DISPOSABLE_HOST=1, which only the disposable
//! CI runner sets. Run those with `cargo test -- --ignored` on that runner only.

#![cfg(windows)]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

// Transactions are process-global, so the transaction test takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

const CMD: &str = r"C:\Windows\System32\cmd.exe";

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn argv(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|item| s(item)).collect())
}

fn record(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn call(name: &str, args: Vec<Value>, env: &mut Env) -> anyhow::Result<Value> {
    aethershell::builtins::call(name, args, env)
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.expect_err("the call should have been refused");
    assert!(error.to_string().contains(expected), "{error}");
}

fn fields(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Record(map) => map,
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn flag(map: &BTreeMap<String, Value>, key: &str) -> bool {
    matches!(map.get(key), Some(Value::Bool(true)))
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

/// True only on the disposable CI runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

/// `sc.exe` for inspection in the disposable tests: (exit code, stdout).
fn sc(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new("sc.exe")
        .args(args)
        .output()
        .expect("sc.exe runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn exists(service: &str) -> bool {
    sc(&["query", service]).0 == Some(0)
}

/// The START_TYPE line of `sc.exe qc`, which names the start type.
fn start_type_line(service: &str) -> String {
    let (code, out) = sc(&["qc", service]);
    assert_eq!(code, Some(0), "{out}");
    out.lines()
        .find(|line| line.contains("START_TYPE"))
        .unwrap_or("")
        .to_string()
}

/// Removes whatever a disposable test made, even when it panics.
struct Cleanup(Vec<String>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for service in &self.0 {
            let _ = Command::new("sc.exe").args(["stop", service]).output();
            let _ = Command::new("sc.exe").args(["delete", service]).output();
        }
    }
}

#[test]
fn malformed_service_calls_are_refused_before_the_manager() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();

    for args in [
        vec![s("Bad Name"), argv(&[CMD])],
        vec![s("-leading"), argv(&[CMD])],
        vec![s(""), argv(&[CMD])],
        vec![s(&"a".repeat(65)), argv(&[CMD])],
        vec![s("svc-ok"), s(CMD)],
        vec![s("svc-ok"), argv(&[])],
        vec![s("svc-ok"), argv(&["cmd.exe"])],
        vec![s("svc-ok"), argv(&["/usr/bin/true"])],
        vec![s("svc-ok"), argv(&[CMD, "/c", "echo"])],
        vec![s("svc-ok"), argv(&[r"C:\aethershell\no-such-program.exe"])],
        vec![s("svc-ok"), argv(&[CMD]), record(&[("restart", s("sometimes"))])],
        vec![s("svc-ok"), argv(&[CMD]), record(&[("enable", s("yes"))])],
        vec![s("svc-ok"), argv(&[CMD]), record(&[("bogus", Value::Bool(true))])],
    ] {
        code(call("svc_create", args, &mut env), "E_BAD_ARG");
    }
    code(
        call(
            "svc_create",
            vec![s("svc-ok"), argv(&[CMD]), record(&[("user", s("no-such-account-ae"))])],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    for args in [vec![s("Bad Name")], vec![s("-leading")], vec![Value::Int(7)]] {
        code(call("svc_delete", args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn service_changes_are_refused_inside_a_transaction() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    let refused = [
        call("svc_create", vec![s("svc-tx"), argv(&[CMD])], &mut env),
        call("svc_delete", vec![s("svc-tx")], &mut env),
    ];
    aethershell::tx::rollback().unwrap();
    for result in refused {
        code(result, "E_POLICY_DENY");
    }
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn create_query_enable_disable_and_delete_round_trip() {
    if !disposable() {
        return;
    }
    let name = format!("ci-test-{}", std::process::id());
    let service = format!("ae-{name}");
    let _cleanup = Cleanup(vec![service.clone()]);
    let mut env = Env::default();

    let created = fields(call("svc_create", vec![s(&name), argv(&[CMD])], &mut env).unwrap());
    assert_eq!(text(&created, "name"), service);
    assert!(!flag(&created, "enabled"));
    assert!(!flag(&created, "started"));
    assert!(exists(&service));
    assert!(start_type_line(&service).contains("DEMAND_START"));

    call("svc_enable", vec![s(&service)], &mut env).unwrap();
    assert!(start_type_line(&service).contains("AUTO_START"));
    call("svc_disable", vec![s(&service)], &mut env).unwrap();
    assert!(start_type_line(&service).contains("DISABLED"));

    let deleted = fields(call("svc_delete", vec![s(&name)], &mut env).unwrap());
    assert_eq!(text(&deleted, "name"), service);
    assert!(!flag(&deleted, "stopped"));
    assert!(!flag(&deleted, "disabled"));
    assert!(flag(&deleted, "removed"));
    assert!(!exists(&service), "sc query still finds {service}");
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn enable_on_create_is_auto_start_and_delete_reports_it_was_enabled() {
    if !disposable() {
        return;
    }
    let name = format!("ci-auto-{}", std::process::id());
    let service = format!("ae-{name}");
    let _cleanup = Cleanup(vec![service.clone()]);
    let mut env = Env::default();

    let created = fields(
        call(
            "svc_create",
            vec![
                s(&name),
                argv(&[CMD]),
                record(&[("enable", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap(),
    );
    assert!(flag(&created, "enabled"));
    assert!(start_type_line(&service).contains("AUTO_START"));

    let deleted = fields(call("svc_delete", vec![s(&name)], &mut env).unwrap());
    assert!(flag(&deleted, "disabled"));
    assert!(!exists(&service));
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn start_that_fails_is_a_coded_error_and_leaves_no_service() {
    if !disposable() {
        return;
    }
    let name = format!("ci-start-{}", std::process::id());
    let service = format!("ae-{name}");
    let _cleanup = Cleanup(vec![service.clone()]);
    let mut env = Env::default();

    // cmd.exe does not speak the service protocol, so the manager refuses the
    // start. The call must report that as a coded error and undo the create.
    let error = call(
        "svc_create",
        vec![
            s(&name),
            argv(&[CMD]),
            record(&[("start", Value::Bool(true))]),
        ],
        &mut env,
    )
    .expect_err("a program that is not a service must not start");
    assert!(error.to_string().contains("E_"), "{error}");
    assert!(!exists(&service), "the failed create left {service} behind");
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn duplicate_create_is_bad_state_and_the_first_service_survives() {
    if !disposable() {
        return;
    }
    let name = format!("ci-dup-{}", std::process::id());
    let service = format!("ae-{name}");
    let _cleanup = Cleanup(vec![service.clone()]);
    let mut env = Env::default();

    call("svc_create", vec![s(&name), argv(&[CMD])], &mut env).unwrap();
    code(
        call("svc_create", vec![s(&name), argv(&[CMD])], &mut env),
        "E_BAD_STATE",
    );
    assert!(exists(&service));
    call("svc_delete", vec![s(&name)], &mut env).unwrap();
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn delete_of_a_missing_service_is_not_found() {
    if !disposable() {
        return;
    }
    let mut env = Env::default();
    let name = format!("ci-absent-{}", std::process::id());
    code(call("svc_delete", vec![s(&name)], &mut env), "E_NOT_FOUND");
}

#[test]
#[ignore = "disposable Windows host only: set AE_DISPOSABLE_HOST=1"]
fn delete_refuses_a_service_without_the_marker_and_leaves_it_alone() {
    if !disposable() {
        return;
    }
    let name = format!("hand-{}", std::process::id());
    let service = format!("ae-{name}");
    let _cleanup = Cleanup(vec![service.clone()]);
    let mut env = Env::default();

    let made = Command::new("sc.exe")
        .args(["create", &service, "binPath=", CMD])
        .output()
        .expect("sc.exe runs");
    assert!(made.status.success(), "sc.exe create failed");

    code(
        call("svc_delete", vec![s(&name)], &mut env),
        "E_POLICY_DENY",
    );
    assert!(exists(&service), "the hand-written service was removed");
}
