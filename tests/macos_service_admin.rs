//! Contracts for svc_create and svc_delete on macOS, through launchd.
//!
//! The ordinary tests refuse their calls before any effect (bad arguments, a
//! transaction), so they change nothing on any host. The tests that load or
//! remove a LaunchDaemon are ignored and also return early unless
//! AE_DISPOSABLE_HOST=1, which only the disposable CI runner sets. They need
//! root, so run them there only:
//!
//!     sudo -E env AE_DISPOSABLE_HOST=1 <test binary> --include-ignored --test-threads=1

#![cfg(target_os = "macos")]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

// Transactions are process-global, so the transaction test takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

const SLEEP: &str = "/bin/sleep";
const DAEMON_DIR: &str = "/Library/LaunchDaemons";

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

fn label_of(name: &str) -> String {
    format!("com.aethershell.{name}")
}

fn plist_of(label: &str) -> String {
    format!("{DAEMON_DIR}/{label}.plist")
}

/// `launchctl` for inspection in the disposable tests: (exit code, stdout).
fn launchctl(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new("/bin/launchctl")
        .args(args)
        .output()
        .expect("launchctl runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// `launchctl print system/<label>`: Some(text) while the label is loaded.
fn print_of(label: &str) -> Option<String> {
    let target = format!("system/{label}");
    let (code, out) = launchctl(&["print", target.as_str()]);
    (code == Some(0)).then_some(out)
}

fn loaded(label: &str) -> bool {
    print_of(label).is_some()
}

fn running(label: &str) -> bool {
    print_of(label).is_some_and(|out| out.contains("state = running"))
}

/// Polls for up to 10 s, since a RunAtLoad job is started during bootstrap
/// but may not show as running at the first `print`.
fn wait_until_running(label: &str) -> bool {
    for _ in 0..100 {
        if running(label) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// `plutil -extract <key> raw` on a plist: the value as text.
fn plutil_raw(plist: &str, key: &str) -> String {
    let out = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", plist])
        .output()
        .expect("plutil runs");
    assert!(
        out.status.success(),
        "plutil could not read {key} of {plist}"
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Removes whatever a disposable test made, even when it panics: each label is
/// booted out of the system domain and its plist removed.
struct Cleanup(Vec<String>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for label in &self.0 {
            let target = format!("system/{label}");
            let _ = Command::new("/bin/launchctl")
                .args(["bootout", target.as_str()])
                .output();
            let _ = std::fs::remove_file(plist_of(label));
        }
    }
}

#[test]
fn malformed_service_calls_are_refused_before_the_system_is_touched() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();

    for args in [
        vec![s("Bad Name"), argv(&[SLEEP, "600"])],
        vec![s("-leading"), argv(&[SLEEP, "600"])],
        vec![s(""), argv(&[SLEEP, "600"])],
        vec![s(&"a".repeat(65)), argv(&[SLEEP, "600"])],
        vec![s("svc-ok"), s(SLEEP)],
        vec![s("svc-ok"), argv(&[])],
        vec![s("svc-ok"), argv(&["sleep", "600"])],
        vec![s("svc-ok"), argv(&[SLEEP, "percent%s"])],
        vec![s("svc-ok"), argv(&[SLEEP, "line\nbreak"])],
        vec![
            s("svc-ok"),
            argv(&[SLEEP]),
            record(&[("restart", s("sometimes"))]),
        ],
        vec![s("svc-ok"), argv(&[SLEEP]), record(&[("enable", s("yes"))])],
        vec![s("svc-ok"), argv(&[SLEEP]), record(&[("start", s("yes"))])],
        vec![
            s("svc-ok"),
            argv(&[SLEEP]),
            record(&[("bogus", Value::Bool(true))]),
        ],
        vec![
            s("svc-ok"),
            argv(&[SLEEP]),
            record(&[("description", s("two\nlines"))]),
        ],
        // start without enable has no LaunchDaemon form: refused before any effect.
        vec![
            s("svc-ok"),
            argv(&[SLEEP]),
            record(&[("enable", Value::Bool(false)), ("start", Value::Bool(true))]),
        ],
    ] {
        code(call("svc_create", args, &mut env), "E_BAD_ARG");
    }
    for args in [
        vec![s("Bad Name")],
        vec![s("")],
        vec![s("-leading")],
        vec![Value::Int(7)],
        vec![s("svc-ok"), s("extra")],
    ] {
        code(call("svc_delete", args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn service_changes_are_refused_inside_a_transaction() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    let refused = [
        call(
            "svc_create",
            vec![s("svc-tx"), argv(&[SLEEP, "600"])],
            &mut env,
        ),
        call("svc_delete", vec![s("svc-tx")], &mut env),
    ];
    aethershell::tx::rollback().unwrap();
    for result in refused {
        code(result, "E_POLICY_DENY");
    }
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn create_writes_a_root_owned_plist_and_bootstraps_it_without_starting_it() {
    if !disposable() {
        return;
    }
    let name = format!("ci-svc-{}", std::process::id());
    let label = label_of(&name);
    let plist = plist_of(&label);
    let _cleanup = Cleanup(vec![label.clone()]);
    let mut env = Env::default();

    let created = fields(
        call(
            "svc_create",
            vec![
                s(&name),
                argv(&[SLEEP, "600"]),
                record(&[("enable", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap(),
    );
    assert_eq!(text(&created, "name"), label);
    assert_eq!(text(&created, "unit_path"), plist);
    assert!(flag(&created, "enabled"));
    assert!(!flag(&created, "started"));

    // The file: marker comment, label, argv, RunAtLoad false, no Disabled key.
    let body = std::fs::read_to_string(&plist).expect("the plist was written");
    assert!(body.contains("Created by AetherShell svc_create"), "{body}");
    assert!(!body.contains("<key>Disabled</key>"), "{body}");
    assert_eq!(plutil_raw(&plist, "Label"), label);
    assert_eq!(plutil_raw(&plist, "ProgramArguments.0"), SLEEP);
    assert_eq!(plutil_raw(&plist, "ProgramArguments.1"), "600");
    assert_eq!(plutil_raw(&plist, "RunAtLoad"), "false");
    let meta = std::fs::metadata(&plist).expect("stat the plist");
    assert_eq!(meta.permissions().mode() & 0o777, 0o644);
    assert_eq!(meta.uid(), 0, "the plist must be root-owned");
    assert_eq!(meta.gid(), 0, "the plist must be wheel-group (gid 0)");

    // launchd: loaded, and not running because start was not asked for.
    assert!(loaded(&label), "launchctl print does not find {label}");
    assert!(!running(&label), "{label} is running but start was false");

    let deleted = fields(call("svc_delete", vec![s(&name)], &mut env).unwrap());
    assert_eq!(text(&deleted, "name"), label);
    assert!(!flag(&deleted, "stopped"));
    assert!(flag(&deleted, "disabled"));
    assert!(flag(&deleted, "removed"));
    assert!(!loaded(&label));
    assert!(!std::path::Path::new(&plist).exists());
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn start_runs_the_job_and_delete_reports_it_was_stopped() {
    if !disposable() {
        return;
    }
    let name = format!("ci-run-{}", std::process::id());
    let label = label_of(&name);
    let _cleanup = Cleanup(vec![label.clone()]);
    let mut env = Env::default();

    let created = fields(
        call(
            "svc_create",
            vec![
                s(&name),
                argv(&[SLEEP, "600"]),
                record(&[("enable", Value::Bool(true)), ("start", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap(),
    );
    assert!(flag(&created, "started"));
    assert_eq!(plutil_raw(&plist_of(&label), "RunAtLoad"), "true");
    assert!(wait_until_running(&label), "{label} did not start");

    let deleted = fields(call("svc_delete", vec![s(&name)], &mut env).unwrap());
    assert!(flag(&deleted, "stopped"));
    assert!(!loaded(&label));
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn enable_false_writes_a_disabled_plist_that_is_not_loaded() {
    if !disposable() {
        return;
    }
    let name = format!("ci-off-{}", std::process::id());
    let label = label_of(&name);
    let plist = plist_of(&label);
    let _cleanup = Cleanup(vec![label.clone()]);
    let mut env = Env::default();

    let created = fields(
        call(
            "svc_create",
            vec![s(&name), argv(&[SLEEP, "600"])],
            &mut env,
        )
        .unwrap(),
    );
    assert!(!flag(&created, "enabled"));
    assert!(!flag(&created, "started"));
    assert_eq!(plutil_raw(&plist, "Disabled"), "true");
    assert!(!loaded(&label), "a disabled service must not be loaded");

    let deleted = fields(call("svc_delete", vec![s(&name)], &mut env).unwrap());
    assert!(!flag(&deleted, "disabled"));
    assert!(!std::path::Path::new(&plist).exists());
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn duplicate_create_is_bad_state_and_the_first_service_survives() {
    if !disposable() {
        return;
    }
    let name = format!("ci-dup-{}", std::process::id());
    let label = label_of(&name);
    let _cleanup = Cleanup(vec![label.clone()]);
    let mut env = Env::default();
    let enable = record(&[("enable", Value::Bool(true))]);

    call(
        "svc_create",
        vec![s(&name), argv(&[SLEEP, "600"]), enable.clone()],
        &mut env,
    )
    .unwrap();
    code(
        call(
            "svc_create",
            vec![s(&name), argv(&[SLEEP, "600"]), enable],
            &mut env,
        ),
        "E_BAD_STATE",
    );
    assert!(loaded(&label));
    call("svc_delete", vec![s(&name)], &mut env).unwrap();
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn delete_of_a_missing_service_is_not_found_and_an_unknown_account_is_not_found() {
    if !disposable() {
        return;
    }
    let mut env = Env::default();
    let name = format!("ci-absent-{}", std::process::id());
    code(call("svc_delete", vec![s(&name)], &mut env), "E_NOT_FOUND");
    code(
        call(
            "svc_create",
            vec![
                s(&name),
                argv(&[SLEEP, "600"]),
                record(&[("user", s("no-such-account-ae"))]),
            ],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    assert!(!std::path::Path::new(&plist_of(&label_of(&name))).exists());
}

#[test]
#[ignore = "disposable macOS host, as root: set AE_DISPOSABLE_HOST=1"]
fn a_daemon_not_created_by_aethershell_is_never_replaced_or_removed() {
    if !disposable() {
        return;
    }
    let name = format!("hand-{}", std::process::id());
    let label = label_of(&name);
    let plist = plist_of(&label);
    let _cleanup = Cleanup(vec![label.clone()]);
    let mut env = Env::default();

    // A plain LaunchDaemon with no AetherShell marker, loaded by hand.
    let plain = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>{label}</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{SLEEP}</string>\n\t\t<string>600</string>\n\t</array>\n</dict>\n</plist>\n"
    );
    std::fs::write(&plist, plain).expect("write the hand-made plist");
    let made = Command::new("/bin/launchctl")
        .args(["bootstrap", "system", plist.as_str()])
        .output()
        .expect("launchctl runs");
    assert!(
        made.status.success(),
        "bootstrap of the hand-made plist failed"
    );

    code(
        call(
            "svc_create",
            vec![s(&name), argv(&[SLEEP, "600"])],
            &mut env,
        ),
        "E_POLICY_DENY",
    );
    code(
        call("svc_delete", vec![s(&name)], &mut env),
        "E_POLICY_DENY",
    );
    assert!(
        std::path::Path::new(&plist).exists(),
        "the hand-made plist was removed"
    );
    assert!(loaded(&label), "the hand-made daemon was unloaded");
}
