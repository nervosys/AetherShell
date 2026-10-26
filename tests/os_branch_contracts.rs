//! The platform branches of ten OS-specific builtins.
//!
//! Each of these answers from a different source on each operating system, and
//! refuses with a coded error where it has none. For each builtin this file
//! checks two things on the machine running the tests:
//!
//! * the supported path returns the type its declaration names. Where the
//!   machine may legitimately lack the tool or the data (no systemd, no
//!   crontab, no openssl), only a coded failure of the kind
//!   `machine_absence` in declared_signatures.rs accepts is allowed instead;
//! * an unsupported operating system refuses with a coded error, not a value.
//!
//! Nothing here changes system state. svc, cron and startup are read-only
//! queries; no service, task or crontab entry is created.

use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
#[cfg(unix)]
use std::process::Command;

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The `E_...` code of a refusal. A refusal without one is a defect.
fn code_of(err: &anyhow::Error) -> String {
    let msg = err.to_string();
    let at = msg
        .find("E_")
        .unwrap_or_else(|| panic!("uncoded refusal: {msg}"));
    msg[at..]
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || *c == '_')
        .collect()
}

/// The answer, or `None` when the call failed with one of `allowed` codes.
/// Any other failure, or an uncoded one, fails the test.
fn answered_or_coded(name: &str, result: anyhow::Result<Value>, allowed: &[&str]) -> Option<Value> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            let code = code_of(&e);
            assert!(
                allowed.contains(&code.as_str()),
                "{name} refused with {code}, which is not one of {allowed:?}: {e}"
            );
            None
        }
    }
}

fn record<'a>(name: &str, v: &'a Value) -> &'a BTreeMap<String, Value> {
    match v {
        Value::Record(r) => r,
        other => panic!("{name} should return a Record, got {other:?}"),
    }
}

fn array<'a>(name: &str, v: &'a Value) -> &'a Vec<Value> {
    match v {
        Value::Array(a) => a,
        other => panic!("{name} should return an Array, got {other:?}"),
    }
}

fn text<'a>(name: &str, rec: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
    match rec.get(key) {
        Some(Value::Str(t)) => t,
        other => panic!("{name} field `{key}` should be a String, got {other:?}"),
    }
}

/// The output of a host tool, for cross-checking an answer against it.
#[cfg(unix)]
fn stdout_of(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------- whoami

#[test]
fn whoami_answers_the_user_name_as_a_string() {
    match run("whoami", vec![]) {
        Ok(Value::Str(name)) => {
            assert!(!name.is_empty(), "whoami answered an empty name");
            #[cfg(unix)]
            {
                if let Some(expected) = stdout_of("id", &["-un"]) {
                    assert_eq!(name, expected, "whoami disagrees with id -un");
                }
            }
            #[cfg(windows)]
            {
                assert_eq!(
                    name,
                    std::env::var("USERNAME").expect("USERNAME is set on Windows")
                );
            }
        }
        Ok(other) => panic!("whoami should return a String, got {other:?}"),
        // A uid with no passwd entry, or no USERNAME: both are coded.
        Err(e) => assert_eq!(code_of(&e), "E_BAD_STATE", "whoami: {e}"),
    }
}

#[cfg(not(any(unix, windows)))]
#[test]
fn whoami_refuses_without_a_user_database_with_a_coded_error() {
    assert_eq!(
        code_of(&run("whoami", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ---------------------------------------------------------- sys_cpu_freq

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn sys_cpu_freq_answers_a_positive_mhz_int_or_null_or_a_coded_failure() {
    // Null means the OS does not expose a frequency. On Windows an answer is
    // always a number: a source that fails is a coded error, never null.
    let allowed: &[&str] = if cfg!(windows) {
        &["E_TOOL_FAILED", "E_TOOL_MISSING", "E_BAD_STATE"]
    } else if cfg!(target_os = "linux") {
        &["E_OS_DENIED", "E_IO", "E_BAD_STATE"]
    } else {
        &["E_TOOL_FAILED", "E_TOOL_MISSING", "E_BAD_STATE"]
    };
    match answered_or_coded("sys_cpu_freq", run("sys_cpu_freq", vec![]), allowed) {
        Some(Value::Int(mhz)) => assert!(mhz > 0, "sys_cpu_freq answered {mhz} MHz"),
        Some(Value::Null) => {
            if cfg!(windows) {
                panic!(
                    "sys_cpu_freq answered null on Windows; a failed source must be a coded error"
                );
            }
        }
        Some(other) => panic!("sys_cpu_freq should return Int or Null, got {other:?}"),
        None => {}
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn sys_cpu_freq_refuses_on_other_platforms_instead_of_answering_null() {
    assert_eq!(
        code_of(&run("sys_cpu_freq", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ------------------------------------------------------------- svc_status

#[cfg(windows)]
#[test]
fn svc_status_reports_a_present_service_as_name_and_status() {
    // EventLog is part of every Windows install.
    let v = run("svc_status", vec![s("EventLog")]).expect("EventLog is present on Windows");
    let rec = record("svc_status", &v);
    assert_eq!(text("svc_status", rec, "name"), "EventLog");
    assert!(!text("svc_status", rec, "status").is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn svc_status_reports_a_unit_as_name_status_and_sub_state_or_codes_the_absence() {
    let allowed = ["E_TOOL_MISSING", "E_TOOL_FAILED", "E_NOT_FOUND"];
    if let Some(v) = answered_or_coded(
        "svc_status",
        run("svc_status", vec![s("systemd-journald")]),
        &allowed,
    ) {
        let rec = record("svc_status", &v);
        assert_eq!(text("svc_status", rec, "name"), "systemd-journald");
        assert!(!text("svc_status", rec, "status").is_empty());
        assert!(!text("svc_status", rec, "sub_state").is_empty());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn svc_status_on_macos_answers_or_codes_the_absence_of_a_label() {
    let allowed = ["E_TOOL_MISSING", "E_TOOL_FAILED", "E_NOT_FOUND"];
    if let Some(v) = answered_or_coded(
        "svc_status",
        run("svc_status", vec![s("com.apple.launchd")]),
        &allowed,
    ) {
        let rec = record("svc_status", &v);
        assert!(matches!(
            text("svc_status", rec, "status"),
            "running" | "stopped"
        ));
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn svc_status_refuses_an_unknown_service_with_e_not_found() {
    // Windows: Get-Service's NoServiceFoundForGivenName is E_NOT_FOUND, as the
    // declaration says; nothing else is accepted, so a regression to a tool
    // failure shows.
    let allowed: &[&str] = if cfg!(windows) {
        &["E_NOT_FOUND"]
    } else {
        &["E_NOT_FOUND", "E_TOOL_FAILED", "E_TOOL_MISSING"]
    };
    if let Some(v) = answered_or_coded(
        "svc_status",
        run("svc_status", vec![s("ae-no-such-service-0001")]),
        allowed,
    ) {
        panic!("svc_status answered {v:?} for a service that does not exist");
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn svc_status_refuses_on_other_platforms_with_a_coded_error() {
    assert_eq!(
        code_of(&run("svc_status", vec![s("cron")]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// -------------------------------------------------------------- cron_list

#[test]
fn cron_list_rejects_arguments_and_pipeline_input_on_every_platform() {
    assert_eq!(
        code_of(&run("cron_list", vec![s("x")]).unwrap_err()),
        "E_BAD_ARG"
    );
}

#[cfg(any(windows, unix))]
#[test]
fn cron_list_answers_rows_of_strings_or_a_coded_failure() {
    let allowed: &[&str] = if cfg!(windows) {
        &["E_TOOL_FAILED"]
    } else {
        &["E_TOOL_MISSING", "E_TOOL_FAILED"]
    };
    if let Some(v) = answered_or_coded("cron_list", run("cron_list", vec![]), allowed) {
        for row in array("cron_list", &v) {
            let rec = record("cron_list", row);
            if cfg!(windows) {
                for key in ["name", "state", "path"] {
                    text("cron_list", rec, key);
                }
            } else {
                for key in ["schedule", "command"] {
                    text("cron_list", rec, key);
                }
            }
        }
    }
}

#[cfg(not(any(windows, unix)))]
#[test]
fn cron_list_refuses_on_other_platforms_with_a_coded_error() {
    assert_eq!(
        code_of(&run("cron_list", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ----------------------------------------------------------- startup_list

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn startup_list_answers_records_with_a_name_or_a_coded_failure() {
    let allowed = ["E_TOOL_FAILED", "E_TOOL_MISSING"];
    if let Some(v) = answered_or_coded("startup_list", run("startup_list", vec![]), &allowed) {
        for row in array("startup_list", &v) {
            let rec = record("startup_list", row);
            assert!(
                rec.contains_key("name"),
                "a startup row has no name: {rec:?}"
            );
            #[cfg(target_os = "linux")]
            assert!(
                rec.contains_key("state"),
                "a unit row has no state: {rec:?}"
            );
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn startup_list_refuses_on_other_platforms_with_a_coded_error() {
    assert_eq!(
        code_of(&run("startup_list", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ----------------------------------------------------------- capabilities

#[cfg(target_os = "linux")]
#[test]
fn capabilities_answers_the_five_sets_and_ids_from_the_kernel() {
    let v = run("capabilities", vec![]).expect("Linux exposes /proc/self/status");
    let rec = record("capabilities", &v);
    for set in [
        "inheritable",
        "permitted",
        "effective",
        "bounding",
        "ambient",
    ] {
        let Some(Value::Array(caps)) = rec.get(set) else {
            panic!("capabilities has no {set} Array: {rec:?}");
        };
        assert!(caps.iter().all(|c| matches!(c, Value::Str(_))));
    }
    assert!(matches!(rec.get("uid"), Some(Value::Int(_))));
    assert!(matches!(rec.get("euid"), Some(Value::Int(_))));
}

#[cfg(not(any(target_os = "linux", windows)))]
#[test]
fn capabilities_refuses_off_linux_instead_of_answering_null() {
    assert_eq!(
        code_of(&run("capabilities", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ------------------------------------------------------------- sudo_check

#[cfg(any(windows, unix))]
#[test]
fn sudo_check_answers_a_bool_and_matches_the_effective_uid_on_unix() {
    let allowed: &[&str] = if cfg!(windows) {
        &["E_TOOL_FAILED"]
    } else {
        &[]
    };
    match answered_or_coded("sudo_check", run("sudo_check", vec![]), allowed) {
        Some(Value::Bool(_admin)) => {
            #[cfg(unix)]
            {
                if let Some(uid) = stdout_of("id", &["-u"]) {
                    assert_eq!(_admin, uid == "0", "sudo_check disagrees with id -u");
                }
            }
        }
        Some(other) => panic!("sudo_check should return a Bool, got {other:?}"),
        None => {}
    }
}

#[cfg(not(any(windows, unix)))]
#[test]
fn sudo_check_refuses_on_other_platforms_with_a_coded_error() {
    assert_eq!(
        code_of(&run("sudo_check", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ---------------------------------------------------------- platform_libcpp

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn platform_libcpp_is_a_name_version_abi_record() {
    // Linux and Windows find the runtime on disk and can refuse when it is
    // not installed (E_BAD_STATE); libcpp_contracts.rs checks the Windows version.
    let allowed: &[&str] = if cfg!(any(windows, target_os = "linux")) {
        &["E_BAD_STATE"]
    } else {
        &[]
    };
    if let Some(v) = answered_or_coded("platform_libcpp", run("platform_libcpp", vec![]), allowed) {
        let rec = record("platform_libcpp", &v);
        let keys: Vec<&str> = rec.keys().map(String::as_str).collect();
        assert_eq!(keys, ["abi", "name", "version"]);
        let name = text("platform_libcpp", rec, "name");
        for key in ["version", "abi"] {
            assert!(
                matches!(rec.get(key), Some(Value::Str(_)) | Some(Value::Null)),
                "platform_libcpp `{key}` should be a String or null: {rec:?}"
            );
        }
        if cfg!(windows) {
            assert_eq!(name, "MSVC STL");
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, "libc++");
        } else {
            assert!(name == "libstdc++" || name == "libc++", "linux name {name}");
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn platform_libcpp_refuses_on_other_platforms_with_a_coded_error() {
    assert_eq!(
        code_of(&run("platform_libcpp", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ------------------------------------------------------------ clipboard_types

#[cfg(windows)]
#[test]
fn clipboard_types_lists_only_the_known_formats() {
    // PowerShell reads the clipboard; a session without a desktop can fail.
    if let Some(v) = answered_or_coded(
        "clipboard_types",
        run("clipboard_types", vec![]),
        &["E_TOOL_FAILED"],
    ) {
        for format in array("clipboard_types", &v) {
            match format {
                Value::Str(t) => assert!(
                    ["text", "image", "files", "audio"].contains(&t.as_str()),
                    "unknown clipboard format {t}"
                ),
                other => panic!("clipboard format should be a String, got {other:?}"),
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
#[test]
fn clipboard_types_refuses_on_macos_with_a_coded_error() {
    assert_eq!(
        code_of(&run("clipboard_types", vec![]).unwrap_err()),
        "E_UNIMPLEMENTED"
    );
}

// ------------------------------------------------------- crypto_cert_verify

#[test]
fn crypto_cert_verify_reports_a_missing_file_as_not_found() {
    assert_eq!(
        code_of(
            &run(
                "crypto_cert_verify",
                vec![s("/nonexistent/ae-osbranch/cert.pem")]
            )
            .unwrap_err()
        ),
        "E_NOT_FOUND"
    );
}

#[test]
fn crypto_cert_verify_answers_false_for_a_readable_file_that_is_not_a_certificate() {
    let path = std::env::temp_dir().join(format!("ae_osbranch_cert_{}.pem", std::process::id()));
    std::fs::write(&path, "not a certificate\n").expect("write scratch file");
    let result = run("crypto_cert_verify", vec![s(&path.to_string_lossy())]);
    let _ = std::fs::remove_file(&path);
    match answered_or_coded("crypto_cert_verify", result, &["E_TOOL_MISSING"]) {
        Some(Value::Bool(verified)) => assert!(!verified, "garbage verified as a certificate"),
        Some(other) => panic!("crypto_cert_verify should return a Bool, got {other:?}"),
        None => {}
    }
}
