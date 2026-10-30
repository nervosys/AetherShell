//! Contracts for the apt package administration builtins.
//!
//! The ordinary tests run everywhere and never reach apt: they check that bad
//! input is refused before any effect, that a transaction refuses changes, and
//! that non-apt platforms answer E_UNIMPLEMENTED. The disposable tests at the
//! bottom install, hold, remove and add sources for real; they are ignored,
//! and scripts/disposable_linux_tests.sh runs them as root in a throwaway
//! distro.

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;

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

// ---------------------------------------------------------------------------
// Ordinary tests: no apt is reached by anything below.
// ---------------------------------------------------------------------------

// Not on Windows: `a` and `Hello` are valid winget identifiers, so on Windows
// this call would reach winget. tests/windows_winget_admin.rs covers the
// winget refusals instead.
#[cfg(not(windows))]
#[test]
fn package_names_and_versions_are_validated_before_any_effect() {
    let bad_names = [
        strs(&["-y"]),
        strs(&["Hello"]),
        strs(&["a"]),
        strs(&["hello world"]),
        strs(&["hello;rm"]),
        Value::Array(vec![s(&"a".repeat(129))]),
        strs(&["hello", "hello"]),
        strs(&[]),
        Value::Array((0..65).map(|i| s(&format!("pkg{i}"))).collect()),
    ];
    for names in bad_names {
        assert_refused("pkg_install", vec![names.clone()], "E_BAD_ARG");
        assert_refused("pkg_hold", vec![names.clone()], "E_BAD_ARG");
        assert_refused("pkg_unhold", vec![names.clone()], "E_BAD_ARG");
    }
    assert_refused("pkg_install", vec![strs(&["hello=1 0"])], "E_BAD_ARG");
    assert_refused("pkg_remove", vec![strs(&["hello=1.0"])], "E_BAD_ARG");
    assert_refused("pkg_hold", vec![strs(&["hello=1.0"])], "E_BAD_ARG");
    assert_refused("pkg_remove", vec![strs(&["-y"])], "E_BAD_ARG");
    assert_refused("pkg_remove", vec![s("hello")], "E_BAD_ARG");
}

#[test]
fn options_and_argument_counts_are_validated() {
    assert_refused(
        "pkg_install",
        vec![strs(&["hello"]), record(&[("bogus", Value::Bool(true))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "pkg_install",
        vec![
            strs(&["hello"]),
            record(&[("no_recommends", Value::Int(1))]),
        ],
        "E_BAD_ARG",
    );
    assert_refused(
        "pkg_remove",
        vec![strs(&["hello"]), record(&[("purge", s("yes"))])],
        "E_BAD_ARG",
    );
    assert_refused("pkg_upgrade", vec![s("x")], "E_BAD_ARG");
    assert_refused(
        "pkg_upgrade",
        vec![record(&[("dist", Value::Int(1))])],
        "E_BAD_ARG",
    );
    assert_refused("pkg_update", vec![s("x")], "E_BAD_ARG");
    assert_refused("pkg_clean", vec![s("x")], "E_BAD_ARG");
    assert_refused("pkg_autoremove", vec![s("x")], "E_BAD_ARG");
    assert_refused("pkg_install", vec![], "E_BAD_ARG");
    assert_refused("pkg_remove", vec![], "E_BAD_ARG");
    assert_refused("pkg_hold", vec![], "E_BAD_ARG");
    assert_refused("pkg_unhold", vec![], "E_BAD_ARG");
    assert_refused("pkg_add_source", vec![s("x")], "E_BAD_ARG");
}

#[test]
fn source_names_and_lines_are_validated_before_any_effect() {
    let good_line = "deb https://deb.debian.org/debian trixie main";
    for name in ["Bad Name", "-x", "../etc", "", "UPPER"] {
        assert_refused("pkg_add_source", vec![s(name), s(good_line)], "E_BAD_ARG");
    }
    let bad_lines = [
        "deb http://deb.debian.org/debian trixie main",
        "deb file:///srv/repo trixie main",
        "deb https://deb.debian.org/debian trixie main\ndeb https://evil.example trixie main",
        "deb https://deb.debian.org/debian trixie main; rm -rf /",
        "deb https://deb.debian.org/debian trixie",
        "rpm https://deb.debian.org/debian trixie main",
        " deb https://deb.debian.org/debian trixie main",
    ];
    for line in bad_lines {
        assert_refused("pkg_add_source", vec![s("x"), s(line)], "E_BAD_ARG");
    }
    assert_refused(
        "pkg_add_source",
        vec![
            s("x"),
            s(good_line),
            record(&[("bogus", Value::Bool(true))]),
        ],
        "E_BAD_ARG",
    );
}

#[test]
fn pipeline_input_is_refused_for_every_mutating_builtin() {
    let calls: [(&str, Vec<Value>); 9] = [
        ("pkg_install", vec![strs(&["hello"])]),
        ("pkg_remove", vec![strs(&["hello"])]),
        ("pkg_update", vec![]),
        ("pkg_upgrade", vec![]),
        ("pkg_clean", vec![]),
        (
            "pkg_add_source",
            vec![s("x"), s("deb https://deb.debian.org/debian trixie main")],
        ),
        ("pkg_hold", vec![strs(&["hello"])]),
        ("pkg_unhold", vec![strs(&["hello"])]),
        ("pkg_autoremove", vec![]),
    ];
    for (name, args) in calls {
        let result = call_with_input(name, args, Some(Value::Int(1)), &mut Env::new());
        let text = match result {
            Ok(v) => panic!("{name} accepted pipeline input and answered {v:?}"),
            Err(e) => e.to_string(),
        };
        assert!(text.contains("E_BAD_ARG"), "{name} with input: {text}");
    }
}

/// The transaction is process-wide state, so the one test that opens one and
/// the test that asserts exact refusal wording must not interleave.
static TRANSACTION: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn changes_are_refused_inside_a_transaction() {
    let _serial = TRANSACTION.lock().unwrap_or_else(|e| e.into_inner());
    struct Rollback;
    impl Drop for Rollback {
        fn drop(&mut self) {
            while iron_shell::tx::rollback().is_ok() {}
        }
    }
    let _rollback = Rollback;
    iron_shell::tx::begin().expect("begin transaction");
    // A valid call: the transaction check must come before any apt run.
    let text = refusal("pkg_update", vec![]);
    assert!(text.contains("E_UNIMPLEMENTED"), "{text}");
    assert!(text.contains("cannot be rolled back"), "{text}");
    assert!(text.contains("NOTHING WAS CHANGED"), "{text}");
}

#[cfg(not(any(target_os = "linux", windows)))]
#[test]
fn non_linux_platforms_refuse_valid_calls_without_changing_anything() {
    let _serial = TRANSACTION.lock().unwrap_or_else(|e| e.into_inner());
    let hello = strs(&["hello"]);
    let source = s("deb https://deb.debian.org/debian trixie main");
    let calls: [(&str, Vec<Value>); 9] = [
        ("pkg_install", vec![hello.clone()]),
        ("pkg_remove", vec![hello.clone()]),
        ("pkg_update", vec![]),
        ("pkg_upgrade", vec![]),
        ("pkg_clean", vec![]),
        ("pkg_add_source", vec![s("aepkg-test"), source]),
        ("pkg_hold", vec![hello.clone()]),
        ("pkg_unhold", vec![hello]),
        ("pkg_autoremove", vec![]),
    ];
    for (name, args) in calls {
        let text = refusal(name, args);
        assert!(text.contains("E_UNIMPLEMENTED"), "{name}: {text}");
        assert!(text.contains("NOTHING WAS CHANGED"), "{name}: {text}");
        assert!(text.contains("Linux"), "{name} should name Linux: {text}");
    }
}

/// On Windows the nine builtins split: install, remove, update and upgrade run
/// winget (tests/windows_winget_admin.rs), and the rest have no winget
/// equivalent and are refused before anything runs.
#[cfg(windows)]
#[test]
fn windows_refuses_what_winget_cannot_do_without_running_anything() {
    let _serial = TRANSACTION.lock().unwrap_or_else(|e| e.into_inner());
    let source = s("deb https://deb.debian.org/debian trixie main");
    let calls: [(&str, Vec<Value>); 5] = [
        ("pkg_clean", vec![]),
        ("pkg_autoremove", vec![]),
        ("pkg_hold", vec![strs(&["jqlang.jq"])]),
        ("pkg_unhold", vec![strs(&["jqlang.jq"])]),
        ("pkg_add_source", vec![s("aepkg-test"), source]),
    ];
    for (name, args) in calls {
        let text = refusal(name, args);
        assert!(text.contains("E_UNIMPLEMENTED"), "{name}: {text}");
        assert!(text.contains("NOTHING WAS CHANGED"), "{name}: {text}");
        assert!(text.contains("winget"), "{name} should name winget: {text}");
    }
}

// ---------------------------------------------------------------------------
// Disposable tests: real apt, real dpkg, real /etc. Run only through
// scripts/disposable_linux_tests.sh; each one calls disposable::require first.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod disposable_linux {
    use super::*;
    use std::process::{Command, Stdio};

    const SOURCE_NAME: &str = "aepkg-test";
    const SOURCE_PATH: &str = "/etc/apt/sources.list.d/ironshell-aepkg-test.list";
    const BAD_SOURCE_PATH: &str = "/etc/apt/sources.list.d/ironshell-aepkg-bad.list";

    /// Undo whatever a test made, even when it fails part way.
    struct Cleanup {
        package: Option<&'static str>,
        paths: &'static [&'static str],
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(package) = self.package {
                let _ = Command::new("/usr/bin/apt-mark")
                    .args(["unhold", package])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("/usr/bin/apt-get")
                    .args(["-y", "-q", "remove", package])
                    .env("DEBIAN_FRONTEND", "noninteractive")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            for path in self.paths {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn installed(package: &str) -> bool {
        Command::new("/usr/bin/dpkg-query")
            .args(["-W", "-f", "${Status}", package])
            .output()
            .map(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&o.stdout).split_whitespace().last()
                        == Some("installed")
            })
            .unwrap_or(false)
    }

    fn showhold() -> Vec<String> {
        let out = Command::new("/usr/bin/apt-mark")
            .arg("showhold")
            .output()
            .expect("apt-mark showhold");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .collect()
    }

    fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
        match v {
            Value::Record(map) => map
                .get(key)
                .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
            other => panic!("expected a Record, got {other:?}"),
        }
    }

    fn array_of_names(v: &Value) -> Vec<String> {
        match v {
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::Str(s) => s.clone(),
                    Value::Record(_) => match field(item, "name") {
                        Value::Str(s) => s.clone(),
                        other => panic!("name is not a String: {other:?}"),
                    },
                    other => panic!("unexpected element {other:?}"),
                })
                .collect(),
            other => panic!("expected an Array, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_pkg_update_refreshes_the_index() {
        disposable::require();
        let r = call("pkg_update", vec![], &mut Env::new()).expect("pkg_update");
        assert_eq!(field(&r, "updated"), &Value::Bool(true));
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_hello_install_hold_unhold_remove() {
        disposable::require();
        let _cleanup = Cleanup {
            package: Some("hello"),
            paths: &[],
        };
        assert!(!installed("hello"), "hello must start absent");

        let r = call("pkg_install", vec![strs(&["hello"])], &mut Env::new())
            .expect("pkg_install hello");
        let names = array_of_names(field(&r, "installed"));
        assert_eq!(names, vec!["hello".to_string()]);
        assert!(installed("hello"), "dpkg does not report hello installed");
        let version = match field(&r, "installed") {
            Value::Array(items) => match field(&items[0], "version") {
                Value::Str(v) => v.clone(),
                other => panic!("version is not a String: {other:?}"),
            },
            other => panic!("installed is not an Array: {other:?}"),
        };
        let dpkg_version = String::from_utf8_lossy(
            &Command::new("/usr/bin/dpkg-query")
                .args(["-W", "-f", "${Version}", "hello"])
                .output()
                .expect("dpkg-query")
                .stdout,
        )
        .trim()
        .to_string();
        assert_eq!(
            version, dpkg_version,
            "reported version differs from dpkg's"
        );
        assert!(!version.is_empty());
        let run = Command::new("/usr/bin/hello")
            .output()
            .expect("run /usr/bin/hello");
        assert!(run.status.success(), "/usr/bin/hello did not run");

        let held = call("pkg_hold", vec![strs(&["hello"])], &mut Env::new()).expect("pkg_hold");
        assert_eq!(
            array_of_names(field(&held, "held")),
            vec!["hello".to_string()]
        );
        assert!(
            showhold().iter().any(|l| l == "hello"),
            "apt-mark does not list hello as held"
        );

        let unheld =
            call("pkg_unhold", vec![strs(&["hello"])], &mut Env::new()).expect("pkg_unhold");
        assert_eq!(
            array_of_names(field(&unheld, "unheld")),
            vec!["hello".to_string()]
        );
        assert!(
            !showhold().iter().any(|l| l == "hello"),
            "hello is still held"
        );

        let removed =
            call("pkg_remove", vec![strs(&["hello"])], &mut Env::new()).expect("pkg_remove");
        assert_eq!(
            array_of_names(field(&removed, "removed")),
            vec!["hello".to_string()]
        );
        assert!(
            !installed("hello"),
            "hello is still installed after pkg_remove"
        );
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_essential_package_is_refused_and_left_installed() {
        disposable::require();
        assert!(
            installed("dpkg"),
            "dpkg must be installed in the disposable distro"
        );
        assert_refused("pkg_remove", vec![strs(&["dpkg"])], "E_POLICY_DENY");
        assert!(installed("dpkg"), "dpkg was removed despite the refusal");
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_add_source_writes_the_file_and_refuses_overwrite() {
        disposable::require();
        let _cleanup = Cleanup {
            package: None,
            paths: &[SOURCE_PATH],
        };
        let line = "deb https://deb.debian.org/debian trixie main";
        let r = call(
            "pkg_add_source",
            vec![s(SOURCE_NAME), s(line)],
            &mut Env::new(),
        )
        .expect("pkg_add_source");
        assert_eq!(field(&r, "path"), &s(SOURCE_PATH));
        assert_eq!(field(&r, "updated"), &Value::Bool(false));
        let contents = std::fs::read_to_string(SOURCE_PATH).expect("source file written");
        assert_eq!(
            contents,
            format!("# Created by IronShell pkg_add_source\n{line}\n")
        );
        assert_refused("pkg_add_source", vec![s(SOURCE_NAME), s(line)], "E_BAD_ARG");
        std::fs::remove_file(SOURCE_PATH).expect("remove the test source");
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_failed_update_removes_the_new_source() {
        disposable::require();
        let _cleanup = Cleanup {
            package: None,
            paths: &[BAD_SOURCE_PATH],
        };
        let r = call(
            "pkg_add_source",
            vec![
                s("aepkg-bad"),
                s("deb https://nonexistent.invalid/debian trixie main"),
                record(&[("update", Value::Bool(true))]),
            ],
            &mut Env::new(),
        );
        assert!(
            r.is_err(),
            "an update against an unresolvable host must fail"
        );
        assert!(
            !std::path::Path::new(BAD_SOURCE_PATH).exists(),
            "the source file was left behind after the failed update"
        );
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn disposable_clean_autoremove_and_upgrade_return_records() {
        disposable::require();
        let c = call("pkg_clean", vec![], &mut Env::new()).expect("pkg_clean");
        assert_eq!(field(&c, "cleaned"), &Value::Bool(true));
        let a = call("pkg_autoremove", vec![], &mut Env::new()).expect("pkg_autoremove");
        let _ = array_of_names(field(&a, "removed"));
        let u = call("pkg_upgrade", vec![], &mut Env::new()).expect("pkg_upgrade");
        match field(&u, "upgraded") {
            Value::Int(n) => assert!(*n >= 0),
            other => panic!("upgraded is not an Int: {other:?}"),
        }
    }
}
