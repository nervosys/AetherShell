//! Contracts of `platform_libcpp`: the {name, version, abi} record, the refusals
//! of surplus arguments and pipeline input, and the version it reports on
//! Windows, checked against PowerShell's own reading of msvcp140.dll.
//!
//! Nothing here changes system state.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;

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

fn libcpp() -> anyhow::Result<Value> {
    let mut env = Env::new();
    call("platform_libcpp", vec![], &mut env)
}

fn record(v: Value) -> std::collections::BTreeMap<String, Value> {
    match v {
        Value::Record(r) => r,
        other => panic!("platform_libcpp should return a Record, got {other:?}"),
    }
}

/// True for a dotted run of decimal numbers such as `3.4.33`.
#[cfg(target_os = "linux")]
fn is_dotted_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

#[test]
fn a_surplus_argument_is_refused_with_bad_arg() {
    let mut env = Env::new();
    let err = call(
        "platform_libcpp",
        vec![Value::Str("extra".to_string())],
        &mut env,
    )
    .expect_err("platform_libcpp takes no arguments");
    assert_eq!(code_of(&err), "E_BAD_ARG", "{err}");
}

#[test]
fn pipeline_input_is_refused_with_bad_arg() {
    let mut env = Env::new();
    let err = call_with_input("platform_libcpp", vec![], Some(Value::Int(1)), &mut env)
        .expect_err("platform_libcpp takes no pipeline input");
    assert_eq!(code_of(&err), "E_BAD_ARG", "{err}");
}

/// The record keys and value types hold wherever the call answers. An absent
/// runtime is a coded bad_state, never an uncoded failure.
#[test]
fn the_answer_is_a_name_version_abi_record() {
    match libcpp() {
        Ok(v) => {
            let rec = record(v);
            let keys: Vec<&str> = rec.keys().map(String::as_str).collect();
            assert_eq!(keys, ["abi", "name", "version"]);
            for key in ["version", "abi"] {
                assert!(
                    matches!(rec.get(key), Some(Value::Str(_)) | Some(Value::Null)),
                    "`{key}` should be a String or null: {rec:?}"
                );
            }
        }
        Err(e) => {
            let code = code_of(&e);
            assert_eq!(
                code, "E_BAD_STATE",
                "only an absent runtime may refuse: {e}"
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_reports_libstdcxx_with_a_numeric_glibcxx_version_or_libcpp_with_none() {
    let Ok(v) = libcpp() else {
        // Neither library is installed on this machine: the refusal is tested above.
        return;
    };
    let rec = record(v);
    let name = match rec.get("name") {
        Some(Value::Str(n)) => n.clone(),
        other => panic!("name should be a String, got {other:?}"),
    };
    match name.as_str() {
        "libstdc++" => {
            // The version is the highest GLIBCXX_ symbol version without its
            // prefix (highest_symbol_version returns "3.4.33", not "GLIBCXX_3.4.33").
            match rec.get("version") {
                Some(Value::Str(v)) => assert!(is_dotted_version(v), "version {v:?}"),
                other => panic!("libstdc++ should report a version, got {other:?}"),
            }
        }
        "libc++" => {
            assert_eq!(rec.get("version"), Some(&Value::Null));
            assert_eq!(rec.get("abi"), Some(&Value::Null));
        }
        other => panic!("unexpected linux library name {other}"),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_reports_libcpp_without_a_version() {
    let rec = record(libcpp().expect("libc++ is in the dyld shared cache"));
    assert_eq!(rec.get("name"), Some(&Value::Str("libc++".to_string())));
    assert_eq!(rec.get("version"), Some(&Value::Null));
    assert_eq!(rec.get("abi"), Some(&Value::Null));
}

/// The version the builtin reports is the one PowerShell reads from the same
/// file. The PowerShell command is a constant; no caller data reaches it.
#[cfg(windows)]
#[test]
fn windows_version_matches_powershell_reading_of_msvcp140() {
    let rec = match libcpp() {
        Ok(v) => record(v),
        Err(e) => {
            // The runtime is absent from this machine: the refusal must be the
            // coded one, and the oracle below has nothing to read.
            assert_eq!(code_of(&e), "E_BAD_STATE", "{e}");
            return;
        }
    };
    assert_eq!(rec.get("name"), Some(&Value::Str("MSVC STL".to_string())));
    assert_eq!(rec.get("abi"), Some(&Value::Str("msvcp140".to_string())));
    let Some(Value::Str(version)) = rec.get("version") else {
        panic!("version should be a String: {rec:?}");
    };

    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-Item $env:SystemRoot\\System32\\msvcp140.dll).VersionInfo.FileVersionRaw.ToString()",
        ])
        .output()
        .expect("powershell must be available on Windows");
    assert!(out.status.success(), "powershell failed: {out:?}");
    let oracle = String::from_utf8_lossy(&out.stdout).trim().to_string();

    assert_eq!(version, &oracle, "builtin vs PowerShell FileVersionRaw");
    assert!(
        version.starts_with("14."),
        "the MSVC STL of the Visual C++ runtime is major version 14: {version}"
    );
}
