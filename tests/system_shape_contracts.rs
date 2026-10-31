//! Shape contracts for the system readers whose answers used to differ by OS.
//!
//! Each test calls the builtin on the host it runs on and checks the shape the
//! builtin declares there. Nothing here depends on the hardware present: a host
//! with no `uptime`, no network tool or no procfs gets a coded absence, and a
//! coded absence is the only failure a test accepts. Every builtin called here
//! only reads the system; none changes it.

use iron_shell::builtins::call;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

/// Takes the lock and points the session directory at a fresh temp directory,
/// so nothing these readers do is recorded in a real session.
fn lock_with_session_dir() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = std::env::temp_dir().join(format!("ae-system-shape-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
    guard
}

fn call_builtin(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
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

/// The codes that honestly mean "not on this host", as opposed to a defect.
fn is_coded_absence(code: &str) -> bool {
    matches!(
        code,
        "E_TOOL_MISSING" | "E_TOOL_FAILED" | "E_NOT_FOUND" | "E_UNIMPLEMENTED"
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ`: UTC, to the second, and a date chrono reads back.
fn is_utc_iso_second(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 20
        && bytes.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
        && chrono::DateTime::parse_from_rfc3339(text).is_ok()
}

#[test]
fn sys_boot_time_is_utc_iso_to_the_second_on_every_os() {
    let _guard = lock_with_session_dir();
    match call_builtin("sys_boot_time", vec![]) {
        Ok(Value::Str(text)) => assert!(
            is_utc_iso_second(&text),
            "sys_boot_time answered {text:?}, not UTC ISO 8601 to the second"
        ),
        Ok(other) => panic!("sys_boot_time answered {other:?}, not a String"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "sys_boot_time failed without a coded absence: {e}"
        ),
    }
}

#[test]
fn sys_load_avg_is_refused_on_windows_and_three_floats_on_unix() {
    let _guard = lock_with_session_dir();
    let result = call_builtin("sys_load_avg", vec![]);
    #[cfg(windows)]
    {
        // Windows has no load average, so there is no value to answer with.
        let e = result.expect_err("sys_load_avg must be refused on Windows");
        assert_eq!(code_of(&e), "E_UNIMPLEMENTED", "sys_load_avg: {e}");
    }
    #[cfg(unix)]
    {
        match result {
            Ok(Value::Record(map)) => {
                for key in ["1min", "5min", "15min"] {
                    assert!(
                        matches!(map.get(key), Some(Value::Float(_))),
                        "sys_load_avg has no Float `{key}`: {map:?}"
                    );
                }
            }
            Ok(other) => panic!("sys_load_avg answered {other:?}, not a Record"),
            Err(e) => assert!(
                is_coded_absence(&code_of(&e)),
                "sys_load_avg failed without a coded absence: {e}"
            ),
        }
    }
}

#[test]
fn sys_info_has_every_key_and_a_missing_list_naming_its_nulls() {
    let _guard = lock_with_session_dir();
    let answer = call_builtin("sys_info", vec![]).expect("sys_info must answer on every host");
    let Value::Record(map) = answer else {
        panic!("sys_info answered {answer:?}, not a Record");
    };
    for key in ["os", "arch", "family", "os_name", "missing"] {
        assert!(
            map.contains_key(key),
            "sys_info has no `{key}` key: {map:?}"
        );
    }
    assert!(
        matches!(map.get("os"), Some(Value::Str(_))),
        "sys_info `os` is not a String: {map:?}"
    );
    let Some(Value::Array(missing)) = map.get("missing") else {
        panic!("sys_info `missing` is not an Array: {map:?}");
    };
    let names: Vec<&str> = missing
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.as_str(),
            other => panic!("sys_info `missing` holds {other:?}, not a field name"),
        })
        .collect();
    for (key, value) in &map {
        if key == "missing" {
            continue;
        }
        let is_null = matches!(value, Value::Null);
        assert_eq!(
            is_null,
            names.contains(&key.as_str()),
            "sys_info `{key}` is {value:?} but `missing` {} it",
            if names.contains(&key.as_str()) {
                "lists"
            } else {
                "does not list"
            }
        );
    }
}

#[test]
fn net_stats_answers_an_array_of_records_with_integer_counters() {
    let _guard = lock_with_session_dir();
    match call_builtin("net_stats", vec![]) {
        Ok(Value::Array(rows)) => {
            for row in &rows {
                let Value::Record(map) = row else {
                    panic!("net_stats row is not a Record: {row:?}");
                };
                if let Some(rx) = map.get("rx_bytes") {
                    assert!(
                        matches!(rx, Value::Int(_)),
                        "net_stats rx_bytes is {rx:?}, not an Int"
                    );
                }
                assert!(
                    matches!(map.get("counter"), Some(Value::Str(_))),
                    "net_stats row has no String `counter`: {map:?}"
                );
                assert!(
                    matches!(map.get("value"), Some(Value::Int(_) | Value::Null)),
                    "net_stats `value` is neither an Int nor null: {map:?}"
                );
            }
        }
        Ok(other) => panic!("net_stats answered {other:?}, not an Array"),
        Err(e) => assert!(
            is_coded_absence(&code_of(&e)),
            "net_stats failed without a coded absence: {e}"
        ),
    }
}
