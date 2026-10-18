use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    safety::{effect_of, Effect},
    value::Value,
};
use std::{path::PathBuf, sync::Mutex, time::Duration};

// The registry is process-global, so the live-watch count is shared by every
// test in this binary. Serialize them.
static LOCK: Mutex<()> = Mutex::new(());

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn record(entries: Vec<(&str, Value)>) -> Value {
    Value::Record(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn watch(args: Vec<Value>) -> anyhow::Result<Value> {
    call("fs_watch", args, &mut Env::default())
}

fn unwatch(handle: &str) -> anyhow::Result<Value> {
    call("fs_unwatch", vec![s(handle)], &mut Env::default())
}

fn handle_of(value: Value) -> String {
    match value {
        Value::Str(handle) => handle,
        other => panic!("expected a handle String, got {other:?}"),
    }
}

fn error_code(result: anyhow::Result<Value>) -> String {
    let error = result.expect_err("expected a refusal");
    error.to_string()
}

fn assert_code(result: anyhow::Result<Value>, code: &str) {
    let text = error_code(result);
    assert!(text.contains(code), "expected {code} in: {text}");
}

fn fields(value: &Value) -> &std::collections::BTreeMap<String, Value> {
    match value {
        Value::Record(map) => map,
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// A fresh directory under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-fswatch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn created_file_is_reported_with_its_path() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let handle = handle_of(watch(vec![s(scratch.0.to_str().unwrap()), record(vec![])]).unwrap());
    assert!(handle.starts_with("watch-"), "{handle}");

    std::fs::write(scratch.0.join("probe.txt"), "hello").unwrap();
    // Events are asynchronous, so give the OS notification time to arrive.
    std::thread::sleep(Duration::from_secs(5));

    let result = unwatch(&handle).unwrap();
    let map = fields(&result);
    assert_eq!(map["path"], s(scratch.0.to_str().unwrap()));
    assert_eq!(map["dropped"], Value::Int(0));
    let Value::Array(events) = &map["events"] else {
        panic!("events must be an Array");
    };
    let hit = events.iter().any(|event| {
        let event = fields(event);
        let kind_ok = matches!(&event["kind"], Value::Str(k) if k == "create" || k == "modify");
        let path_ok = matches!(&event["paths"], Value::Array(paths) if paths.iter().any(|p|
            matches!(p, Value::Str(p) if PathBuf::from(p).file_name().is_some_and(|n| n == "probe.txt"))));
        kind_ok && path_ok
    });
    assert!(hit, "no create/modify event for probe.txt in {events:?}");
}

#[test]
fn bounded_buffer_counts_overflow_in_dropped() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let options = record(vec![("max_events", Value::Int(1))]);
    let handle = handle_of(watch(vec![s(scratch.0.to_str().unwrap()), options]).unwrap());

    for i in 0..5 {
        std::fs::write(scratch.0.join(format!("f{i}.txt")), "x").unwrap();
    }
    std::thread::sleep(Duration::from_secs(3));

    let map = fields(&unwatch(&handle).unwrap()).clone();
    let Value::Array(events) = &map["events"] else {
        panic!("events must be an Array");
    };
    assert_eq!(events.len(), 1, "{events:?}");
    let Value::Int(dropped) = map["dropped"] else {
        panic!("dropped must be an Int");
    };
    assert!(dropped >= 1, "dropped was {dropped}");
}

#[test]
fn handle_is_gone_after_unwatch() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let handle = handle_of(watch(vec![s(scratch.0.to_str().unwrap())]).unwrap());
    unwatch(&handle).unwrap();
    assert_code(unwatch(&handle), "E_BAD_ARG");
}

#[test]
fn sixty_fifth_live_watch_is_refused_with_budget() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let path = s(scratch.0.to_str().unwrap());
    let mut handles = Vec::new();
    let mut refusal = None;
    for _ in 0..65 {
        match watch(vec![path.clone()]) {
            Ok(value) => handles.push(handle_of(value)),
            Err(error) => {
                refusal = Some(error.to_string());
                break;
            }
        }
    }
    // Clean up every handle before asserting, so a failure leaves nothing live.
    for handle in &handles {
        unwatch(handle).unwrap();
    }
    assert_eq!(handles.len(), 64);
    let refusal = refusal.expect("the 65th watch must be refused");
    assert!(refusal.contains("E_BUDGET"), "{refusal}");
}

#[test]
fn missing_path_is_a_coded_fs_error() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let missing = scratch.0.join("does-not-exist");
    assert_code(watch(vec![s(missing.to_str().unwrap())]), "E_NOT_FOUND");
}

#[test]
fn bad_options_are_refused() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let path = s(scratch.0.to_str().unwrap());
    let bad = [
        record(vec![("unknown", Value::Bool(true))]),
        record(vec![("recursive", Value::Int(1))]),
        record(vec![("max_events", Value::Int(0))]),
        record(vec![("max_events", Value::Int(10_001))]),
        record(vec![("max_events", s("5"))]),
        Value::Int(5),
    ];
    for options in bad {
        assert_code(watch(vec![path.clone(), options.clone()]), "E_BAD_ARG");
    }
}

#[test]
fn wrong_argument_shapes_are_refused() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let scratch = Scratch::new();
    let path = s(scratch.0.to_str().unwrap());
    assert_code(watch(vec![]), "E_BAD_ARG");
    assert_code(
        watch(vec![path.clone(), record(vec![]), Value::Null]),
        "E_BAD_ARG",
    );
    assert_code(watch(vec![Value::Int(3)]), "E_BAD_ARG");
    assert_code(unwatch_args(vec![]), "E_BAD_ARG");
    assert_code(unwatch_args(vec![s("a"), s("b")]), "E_BAD_ARG");
    assert_code(unwatch_args(vec![Value::Int(1)]), "E_BAD_ARG");

    let input = call_with_input(
        "fs_watch",
        vec![path.clone()],
        Some(s("piped")),
        &mut Env::default(),
    );
    assert!(input.is_err());
    let input = call_with_input(
        "fs_unwatch",
        vec![s("watch-1")],
        Some(s("piped")),
        &mut Env::default(),
    );
    assert!(input.is_err());
}

fn unwatch_args(args: Vec<Value>) -> anyhow::Result<Value> {
    call("fs_unwatch", args, &mut Env::default())
}

#[test]
fn both_builtins_are_read_local() {
    assert_eq!(effect_of("fs_watch"), Effect::ReadLocal);
    assert_eq!(effect_of("fs_unwatch"), Effect::ReadLocal);
}
