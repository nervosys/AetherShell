//! Contracts for `test_watch`: bounded polling re-runs of one argv program.
//!
//! Timing margins are generous on purpose. Windows process start-up is slow,
//! and these tests must not depend on how quickly a child exits.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

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

fn runs(result: &Value) -> Vec<Value> {
    match field(result, "runs") {
        Value::Array(runs) => runs.clone(),
        other => panic!("runs is not an Array: {other:?}"),
    }
}

fn watch(command: Value, paths: Value, options: Option<Value>) -> Result<Value, String> {
    let mut args = vec![command, paths];
    if let Some(o) = options {
        args.push(o);
    }
    call("test_watch", args, &mut Env::new()).map_err(|e| e.to_string())
}

fn ok_watch(command: Value, paths: Value, options: Option<Value>) -> Value {
    watch(command, paths, options).unwrap_or_else(|e| panic!("test_watch refused: {e}"))
}

/// A program that exits 0 quickly on every platform the tests run on.
fn trivial_command() -> Value {
    strs(&["rustc", "--version"])
}

/// A program that sleeps well past any timeout the tests use.
fn sleeping_command() -> Value {
    if cfg!(windows) {
        strs(&["powershell", "-NoProfile", "-Command", "Start-Sleep 5"])
    } else {
        strs(&["sleep", "5"])
    }
}

/// A fresh, empty directory unique to this test process and name.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_test_watch_{}_{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch directory");
    dir
}

#[test]
fn an_initial_run_passes_and_stops_at_max_runs() {
    let result = ok_watch(
        trivial_command(),
        strs(&["Cargo.toml"]),
        Some(record(&[("max_runs", Value::Int(1))])),
    );
    assert_eq!(field(&result, "stopped"), &s("max_runs"));
    assert_eq!(field(&result, "passed"), &Value::Bool(true));
    let runs = runs(&result);
    assert_eq!(runs.len(), 1, "one run expected");
    assert_eq!(field(&runs[0], "trigger"), &s("initial"));
    assert_eq!(field(&runs[0], "exit_code"), &Value::Int(0));
    assert_eq!(field(&runs[0], "timed_out"), &Value::Bool(false));
    assert_eq!(field(&runs[0], "changed_count"), &Value::Int(0));
}

#[test]
fn a_new_file_triggers_a_change_run_that_lists_it() {
    let dir = scratch_dir("change");
    std::fs::write(dir.join("a.txt"), "one").expect("seed file");
    let writer_dir = dir.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        std::fs::write(writer_dir.join("b.txt"), "two").expect("write new file");
    });

    let result = ok_watch(
        trivial_command(),
        strs(&[dir.to_str().expect("utf-8 temp path")]),
        Some(record(&[
            ("max_runs", Value::Int(2)),
            ("idle_ms", Value::Int(30_000)),
            ("total_ms", Value::Int(60_000)),
            ("poll_ms", Value::Int(50)),
        ])),
    );
    writer.join().expect("writer thread");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(field(&result, "stopped"), &s("max_runs"));
    let runs = runs(&result);
    assert_eq!(runs.len(), 2, "the change must produce a second run");
    assert_eq!(field(&runs[1], "trigger"), &s("change"));
    assert_eq!(field(&runs[1], "changed_count"), &Value::Int(1));
    let Value::Array(changed) = field(&runs[1], "changed") else {
        panic!("changed is not an Array");
    };
    assert_eq!(changed.len(), 1);
    let Value::Str(path) = &changed[0] else {
        panic!("changed element is not a String");
    };
    assert!(
        path.ends_with("b.txt"),
        "the new file must be listed: {path}"
    );
}

#[test]
fn listed_changes_are_capped_at_32_but_counted_in_full() {
    let dir = scratch_dir("many");
    let writer_dir = dir.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        for i in 0..40 {
            std::fs::write(writer_dir.join(format!("f{i:02}.txt")), "x").expect("write file");
        }
    });

    let result = ok_watch(
        trivial_command(),
        strs(&[dir.to_str().expect("utf-8 temp path")]),
        Some(record(&[
            ("max_runs", Value::Int(2)),
            ("idle_ms", Value::Int(30_000)),
            ("total_ms", Value::Int(60_000)),
            ("poll_ms", Value::Int(50)),
        ])),
    );
    writer.join().expect("writer thread");
    let _ = std::fs::remove_dir_all(&dir);

    let runs = runs(&result);
    assert_eq!(runs.len(), 2);
    // Writes may straddle two polls, so the count is at least the files
    // written before the change run began, and never more than 40.
    let Value::Int(count) = field(&runs[1], "changed_count") else {
        panic!("changed_count is not an Int");
    };
    assert!((1..=40).contains(count), "count {count}");
    let Value::Array(changed) = field(&runs[1], "changed") else {
        panic!("changed is not an Array");
    };
    assert!(changed.len() <= 32, "at most 32 are listed");
    assert_eq!(changed.len() as i64, (*count).min(32));
}

#[test]
fn no_change_stops_on_idle_after_one_run() {
    let dir = scratch_dir("idle");
    std::fs::write(dir.join("quiet.txt"), "still").expect("seed file");

    let result = ok_watch(
        trivial_command(),
        strs(&[dir.to_str().expect("utf-8 temp path")]),
        Some(record(&[
            ("max_runs", Value::Int(5)),
            ("idle_ms", Value::Int(300)),
            ("poll_ms", Value::Int(50)),
        ])),
    );
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(field(&result, "stopped"), &s("idle"));
    assert_eq!(runs(&result).len(), 1);
    assert_eq!(field(&result, "passed"), &Value::Bool(true));
}

#[test]
fn a_run_past_its_timeout_is_killed_and_reported() {
    let started = std::time::Instant::now();
    let result = ok_watch(
        sleeping_command(),
        strs(&["Cargo.toml"]),
        Some(record(&[
            ("max_runs", Value::Int(1)),
            ("timeout_ms", Value::Int(300)),
        ])),
    );
    let elapsed = started.elapsed();

    let runs = runs(&result);
    assert_eq!(runs.len(), 1);
    assert_eq!(field(&runs[0], "timed_out"), &Value::Bool(true));
    assert_eq!(field(&runs[0], "exit_code"), &Value::Null);
    assert_eq!(field(&result, "passed"), &Value::Bool(false));
    assert!(
        elapsed < Duration::from_secs(4),
        "the child must be killed, not waited for: took {elapsed:?}"
    );
    let Value::Int(duration) = field(&runs[0], "duration_ms") else {
        panic!("duration_ms is not an Int");
    };
    assert!(*duration < 4000, "duration {duration} ms");
}

#[test]
fn a_nonzero_exit_is_reported_in_the_run_not_raised() {
    let result = ok_watch(
        strs(&["rustc", "--no-such-flag-for-test-watch"]),
        strs(&["Cargo.toml"]),
        Some(record(&[("max_runs", Value::Int(1))])),
    );
    let runs = runs(&result);
    let exit = field(&runs[0], "exit_code");
    let Value::Int(code) = exit else {
        panic!("exit_code should be an Int for a process that exited: {exit:?}");
    };
    assert_ne!(*code, 0);
    assert_eq!(field(&runs[0], "timed_out"), &Value::Bool(false));
    assert_eq!(field(&result, "passed"), &Value::Bool(false));
    let Value::Str(stderr) = field(&runs[0], "stderr") else {
        panic!("stderr is not a String");
    };
    assert!(!stderr.is_empty(), "stderr should be captured");
}

#[test]
fn an_empty_command_is_refused() {
    let e = watch(strs(&[]), strs(&["Cargo.toml"]), None).expect_err("empty command");
    assert!(e.contains("E_BAD_ARG"), "{e}");
}

#[test]
fn a_non_string_command_element_is_refused() {
    let e = watch(
        Value::Array(vec![s("rustc"), Value::Int(5)]),
        strs(&["Cargo.toml"]),
        None,
    )
    .expect_err("non-String element");
    assert!(e.contains("E_BAD_ARG"), "{e}");
}

#[test]
fn a_missing_path_is_refused_before_anything_runs() {
    let e = watch(
        trivial_command(),
        strs(&["no-such-path-for-test-watch-xyz"]),
        None,
    )
    .expect_err("missing path");
    assert!(e.contains("E_BAD_ARG"), "{e}");
}

#[test]
fn an_empty_paths_array_is_refused() {
    let e = watch(trivial_command(), strs(&[]), None).expect_err("empty paths");
    assert!(e.contains("E_BAD_ARG"), "{e}");
}

#[test]
fn an_unknown_option_key_is_refused() {
    let e = watch(
        trivial_command(),
        strs(&["Cargo.toml"]),
        Some(record(&[("bogus", Value::Int(1))])),
    )
    .expect_err("unknown option");
    assert!(e.contains("E_BAD_ARG"), "{e}");
}

#[test]
fn out_of_range_and_mistyped_options_are_refused() {
    for options in [
        record(&[("max_runs", Value::Int(21))]),
        record(&[("max_runs", Value::Int(0))]),
        record(&[("poll_ms", Value::Int(49))]),
        record(&[("idle_ms", Value::Int(99))]),
        record(&[("timeout_ms", Value::Int(1_800_001))]),
        record(&[("total_ms", Value::Int(3_600_001))]),
        record(&[("max_runs", s("2"))]),
    ] {
        let e = watch(
            trivial_command(),
            strs(&["Cargo.toml"]),
            Some(options.clone()),
        )
        .expect_err("bad option");
        assert!(e.contains("E_BAD_ARG"), "{options:?}: {e}");
    }
}

#[test]
fn pipeline_input_is_refused() {
    let args = vec![trivial_command(), strs(&["Cargo.toml"])];
    let e = call_with_input("test_watch", args, Some(Value::Int(1)), &mut Env::new())
        .expect_err("pipeline input");
    assert!(e.to_string().contains("E_BAD_ARG"), "{e}");
}

#[test]
fn a_program_that_does_not_exist_is_an_error() {
    let e = watch(
        strs(&["aethershell-no-such-program-for-test-watch"]),
        strs(&["Cargo.toml"]),
        Some(record(&[("max_runs", Value::Int(1))])),
    )
    .expect_err("missing program");
    assert!(e.contains("E_TOOL_MISSING"), "{e}");
}

#[test]
fn test_watch_is_classified_as_exec() {
    assert!(matches!(effect_of("test_watch"), Effect::Exec));
}
