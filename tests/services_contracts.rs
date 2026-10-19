//! Contracts for svc_create, svc_delete and sudo_exec.
//!
//! The ordinary tests run everywhere and only ever pass arguments that are
//! refused before any effect, so they change nothing on any host. The Linux
//! tests that really create units or run sudo are ignored and run only inside
//! the disposable distro (scripts/disposable_linux_tests.sh).

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

// Transactions are process-global, so every test here takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

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

#[test]
fn malformed_arguments_are_refused_with_bad_arg_on_every_platform() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();

    let bad_creates = vec![
        vec![s("Bad Name"), argv(&["/usr/bin/true"])],
        vec![s(""), argv(&["/usr/bin/true"])],
        vec![s("-leading"), argv(&["/usr/bin/true"])],
        vec![s(&"a".repeat(65)), argv(&["/usr/bin/true"])],
        vec![Value::Int(1), argv(&["/usr/bin/true"])],
        vec![s("aesvc-ok"), s("/usr/bin/true")],
        vec![s("aesvc-ok"), argv(&[])],
        vec![s("aesvc-ok"), argv(&["true"])],
        vec![s("aesvc-ok"), argv(&["/usr/bin/true", "line\nbreak"])],
        vec![s("aesvc-ok"), argv(&["/usr/bin/true", "carriage\rreturn"])],
        vec![s("aesvc-ok"), argv(&["/usr/bin/true", "percent%s"])],
        vec![s("aesvc-ok"), argv(&["/usr/bin/true", "nul\0byte"])],
        vec![s("aesvc-ok"), Value::Array(vec![Value::Int(3)])],
        vec![s("aesvc-ok"), argv(&["/usr/bin/true"]), s("not a record")],
        vec![
            s("aesvc-ok"),
            argv(&["/usr/bin/true"]),
            record(&[("bogus", Value::Bool(true))]),
        ],
        vec![
            s("aesvc-ok"),
            argv(&["/usr/bin/true"]),
            record(&[("restart", s("sometimes"))]),
        ],
        vec![
            s("aesvc-ok"),
            argv(&["/usr/bin/true"]),
            record(&[("user", s("Not A User"))]),
        ],
        vec![
            s("aesvc-ok"),
            argv(&["/usr/bin/true"]),
            record(&[("start", s("yes"))]),
        ],
        vec![
            s("aesvc-ok"),
            argv(&["/usr/bin/true"]),
            record(&[("description", s("two\nlines"))]),
        ],
        vec![s("aesvc-ok")],
    ];
    for args in bad_creates {
        code(call("svc_create", args, &mut env), "E_BAD_ARG");
    }
    code(
        aethershell::builtins::call_with_input(
            "svc_create",
            vec![s("aesvc-ok"), argv(&["/usr/bin/true"])],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );

    for args in [
        vec![s("Bad Name")],
        vec![s("")],
        vec![s("-leading")],
        vec![Value::Int(7)],
        vec![s("aesvc-ok"), s("extra")],
    ] {
        code(call("svc_delete", args, &mut env), "E_BAD_ARG");
    }
    code(
        aethershell::builtins::call_with_input(
            "svc_delete",
            vec![s("aesvc-ok")],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );

    let bad_sudo = vec![
        vec![],
        vec![s("id")],
        vec![argv(&[])],
        vec![argv(&["-u"])],
        vec![argv(&[""])],
        vec![argv(&["/usr/bin/id", "nul\0byte"])],
        vec![Value::Array(vec![Value::Int(1)])],
        vec![
            argv(&["/usr/bin/id"]),
            record(&[("timeout_ms", Value::Int(99))]),
        ],
        vec![
            argv(&["/usr/bin/id"]),
            record(&[("timeout_ms", Value::Int(600_001))]),
        ],
        vec![argv(&["/usr/bin/id"]), record(&[("timeout_ms", s("5000"))])],
        vec![argv(&["/usr/bin/id"]), record(&[("bogus", Value::Int(1))])],
        vec![argv(&["/usr/bin/id"]), s("not a record")],
    ];
    for args in bad_sudo {
        code(call("sudo_exec", args, &mut env), "E_BAD_ARG");
    }
    code(
        aethershell::builtins::call_with_input(
            "sudo_exec",
            vec![argv(&["/usr/bin/id"])],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn every_effect_is_refused_inside_a_transaction() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    let refused = [
        call(
            "svc_create",
            vec![s("aesvc-tx"), argv(&["/usr/bin/true"])],
            &mut env,
        ),
        call("svc_delete", vec![s("aesvc-tx")], &mut env),
        call("sudo_exec", vec![argv(&["/usr/bin/true"])], &mut env),
    ];
    aethershell::tx::rollback().unwrap();
    for result in refused {
        code(result, "E_POLICY_DENY");
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn valid_calls_refuse_with_unimplemented_and_change_nothing_off_linux() {
    let _lock = LOCK.lock().unwrap();
    let mut env = Env::default();
    for result in [
        call(
            "svc_create",
            vec![
                s("aesvc-ok"),
                argv(&["/usr/bin/true"]),
                record(&[("start", Value::Bool(true))]),
            ],
            &mut env,
        ),
        call("svc_delete", vec![s("aesvc-ok")], &mut env),
        call("sudo_exec", vec![argv(&["/usr/bin/id", "-u"])], &mut env),
    ] {
        let error = result.expect_err("refused off Linux");
        let text = error.to_string();
        assert!(text.contains("E_UNIMPLEMENTED"), "{text}");
        assert!(text.contains("NOTHING WAS CHANGED"), "{text}");
    }
}

#[cfg(target_os = "linux")]
mod linux_disposable {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const UNIT_DIR: &str = "/etc/systemd/system";
    const MARKER: &str =
        "# Created by AetherShell svc_create; svc_delete removes only units carrying this line";

    fn unit_path(name: &str) -> PathBuf {
        PathBuf::from(format!("{UNIT_DIR}/ae-{name}.service"))
    }

    fn systemctl(args: &[&str]) -> String {
        let out = Command::new("/usr/bin/systemctl")
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Removes whatever a test made, even when it panics: stops and disables
    /// the units, deletes their files and any extra paths, reloads systemd.
    struct Cleanup {
        units: Vec<String>,
        paths: Vec<PathBuf>,
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            for unit in &self.units {
                let name = format!("ae-{unit}.service");
                let _ = Command::new("/usr/bin/systemctl")
                    .args(["stop", &name])
                    .output();
                let _ = Command::new("/usr/bin/systemctl")
                    .args(["disable", &name])
                    .output();
                let _ = std::fs::remove_file(unit_path(unit));
            }
            for path in &self.paths {
                let _ = std::fs::remove_file(path);
            }
            let _ = Command::new("/usr/bin/systemctl")
                .arg("daemon-reload")
                .output();
        }
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

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn svc_create_starts_a_unit_and_svc_delete_removes_it() {
        disposable::require();
        let _cleanup = Cleanup {
            units: vec!["aesvc-run".into()],
            paths: vec![],
        };
        let mut env = Env::default();

        let created = fields(
            call(
                "svc_create",
                vec![
                    s("aesvc-run"),
                    argv(&["/usr/bin/sleep", "300"]),
                    record(&[("start", Value::Bool(true))]),
                ],
                &mut env,
            )
            .unwrap(),
        );
        assert_eq!(text(&created, "name"), "ae-aesvc-run.service");
        assert_eq!(
            text(&created, "unit_path"),
            "/etc/systemd/system/ae-aesvc-run.service"
        );
        assert!(flag(&created, "started"));
        assert!(!flag(&created, "enabled"));
        assert_eq!(systemctl(&["is-active", "ae-aesvc-run.service"]), "active");

        let body = std::fs::read_to_string(unit_path("aesvc-run")).unwrap();
        assert_eq!(body.lines().next(), Some(MARKER));
        assert!(
            body.contains("ExecStart=\"/usr/bin/sleep\" \"300\"\n"),
            "{body}"
        );

        let deleted = fields(call("svc_delete", vec![s("aesvc-run")], &mut env).unwrap());
        assert!(flag(&deleted, "stopped"));
        assert!(!flag(&deleted, "disabled"));
        assert!(flag(&deleted, "removed"));
        assert_ne!(systemctl(&["is-active", "ae-aesvc-run.service"]), "active");
        assert!(!Path::new(&unit_path("aesvc-run")).exists());
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn duplicates_and_hand_written_units_are_refused_and_left_alone() {
        disposable::require();
        let _cleanup = Cleanup {
            units: vec!["aesvc-dup".into(), "aesvc-hand".into()],
            paths: vec![],
        };
        let mut env = Env::default();

        call(
            "svc_create",
            vec![s("aesvc-dup"), argv(&["/usr/bin/true"])],
            &mut env,
        )
        .unwrap();
        code(
            call(
                "svc_create",
                vec![s("aesvc-dup"), argv(&["/usr/bin/true"])],
                &mut env,
            ),
            "E_POLICY_DENY",
        );

        let hand = "[Service]\nExecStart=/usr/bin/true\n";
        std::fs::write(unit_path("aesvc-hand"), hand).unwrap();
        code(
            call("svc_delete", vec![s("aesvc-hand")], &mut env),
            "E_POLICY_DENY",
        );
        assert_eq!(
            std::fs::read_to_string(unit_path("aesvc-hand")).unwrap(),
            hand
        );

        code(
            call("svc_delete", vec![s("aesvc-absent")], &mut env),
            "E_NOT_FOUND",
        );

        call("svc_delete", vec![s("aesvc-dup")], &mut env).unwrap();
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn argument_with_spaces_and_quotes_reaches_the_program_intact() {
        disposable::require();
        let target = "/tmp/aesvc quoted \"x\" $HOME.touched";
        let _cleanup = Cleanup {
            units: vec!["aesvc-quote".into()],
            paths: vec![PathBuf::from(target)],
        };
        let mut env = Env::default();

        call(
            "svc_create",
            vec![
                s("aesvc-quote"),
                argv(&["/usr/bin/touch", target]),
                record(&[("start", Value::Bool(true))]),
            ],
            &mut env,
        )
        .unwrap();

        // Type=simple returns once touch is forked, so poll for its effect.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !Path::new(target).exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            Path::new(target).exists(),
            "touch did not create {target:?}"
        );
        assert!(!Path::new("/root.touched").exists());

        let body = std::fs::read_to_string(unit_path("aesvc-quote")).unwrap();
        assert!(
            body.contains(
                "ExecStart=\"/usr/bin/touch\" \"/tmp/aesvc quoted \\\"x\\\" $$HOME.touched\"\n"
            ),
            "{body}"
        );

        call("svc_delete", vec![s("aesvc-quote")], &mut env).unwrap();
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn sudo_exec_reports_identity_exit_codes_stderr_and_timeouts() {
        disposable::require();
        let mut env = Env::default();

        let id = fields(call("sudo_exec", vec![argv(&["/usr/bin/id", "-u"])], &mut env).unwrap());
        assert_eq!(text(&id, "stdout").trim(), "0");
        assert!(matches!(id.get("exit_code"), Some(Value::Int(0))));
        assert!(!flag(&id, "timed_out"));

        let failed = fields(call("sudo_exec", vec![argv(&["/usr/bin/false"])], &mut env).unwrap());
        assert!(matches!(failed.get("exit_code"), Some(Value::Int(1))));

        let missing = fields(
            call(
                "sudo_exec",
                vec![argv(&["/usr/bin/ls", "/aesvc-no-such-path"])],
                &mut env,
            )
            .unwrap(),
        );
        assert!(matches!(missing.get("exit_code"), Some(Value::Int(code)) if *code != 0));
        assert!(!text(&missing, "stderr").is_empty());

        let started = std::time::Instant::now();
        let slow = fields(
            call(
                "sudo_exec",
                vec![
                    argv(&["/usr/bin/sleep", "5"]),
                    record(&[("timeout_ms", Value::Int(300))]),
                ],
                &mut env,
            )
            .unwrap(),
        );
        assert!(flag(&slow, "timed_out"));
        assert!(matches!(slow.get("exit_code"), Some(Value::Null)));
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }
}
