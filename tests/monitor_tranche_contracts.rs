//! Contracts for the monitor_* builtins.
//!
//! Nothing here starts a program: the snapshot builtins read through sysinfo.
//! The alert store is redirected with IRONSHELL_DATA_HOME to a temporary
//! directory, so the real store is never written, and the store file is read
//! back with serde_json rather than through the builtins that wrote it.
//!
//! Refusals are decided before the alert store or the machine is read, so the
//! refusal table runs anywhere.

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The alert store tests share one IRONSHELL_DATA_HOME, so they run one at a time.
static STORE: Mutex<()> = Mutex::new(());

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn call_builtin(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn call_piped(name: &str, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call_with_input(name, Vec::new(), Some(input), &mut env)
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

fn refused_with(name: &str, args: Vec<Value>) -> String {
    match call_builtin(name, args) {
        Ok(v) => panic!("{name} should refuse, but returned {v:?}"),
        Err(e) => code_of(&e),
    }
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(map) => map
            .get(key)
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn as_items(v: &Value) -> &[Value] {
    match v {
        Value::Array(items) => items,
        other => panic!("expected an Array, got {other:?}"),
    }
}

/// A fresh directory for the alert store, removed at the end of a test.
struct StoreDir(PathBuf);

impl StoreDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("ae-monitor-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temporary data directory");
        std::env::set_var("IRONSHELL_DATA_HOME", &dir);
        StoreDir(dir)
    }

    fn store_file(&self) -> PathBuf {
        self.0.join("monitor").join("alerts.json")
    }
}

impl Drop for StoreDir {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_DATA_HOME");
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Read the store file directly, without the builtins.
fn read_store(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).expect("the store file exists");
    serde_json::from_str(&text).expect("the store file is JSON")
}

#[test]
fn bad_arguments_are_refused_with_e_bad_arg() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("monitor_htop", vec![Value::Int(0)]),
        ("monitor_htop", vec![Value::Int(501)]),
        ("monitor_htop", vec![s("ten")]),
        ("monitor_htop", vec![Value::Int(1), Value::Int(2)]),
        ("monitor_iotop", vec![Value::Int(-1)]),
        ("monitor_nmon", vec![Value::Int(1)]),
        ("monitor_glances", vec![s("x")]),
        ("monitor_memory", vec![Value::Int(1)]),
        ("monitor_dstat", vec![Value::Int(1)]),
        ("monitor_health_check", vec![Value::Int(1)]),
        ("monitor_watch_cpu", vec![Value::Float(101.0)]),
        ("monitor_watch_cpu", vec![s("high")]),
        ("monitor_watch_cpu", vec![Value::Float(f64::NAN)]),
        ("monitor_watch_cpu", vec![Value::Float(80.0), Value::Int(0)]),
        (
            "monitor_watch_cpu",
            vec![Value::Float(80.0), Value::Int(11)],
        ),
        (
            "monitor_watch_cpu",
            vec![Value::Float(80.0), Value::Int(-5)],
        ),
        (
            "monitor_watch_cpu",
            vec![Value::Int(1), Value::Int(1), Value::Int(1)],
        ),
        ("monitor_watch_memory", vec![Value::Float(150.0)]),
        ("monitor_watch_disk", vec![s("90")]),
        ("monitor_alert_create", vec![s("r"), s("gpu")]),
        ("monitor_alert_create", vec![s("r"), s("cpu"), s("~")]),
        (
            "monitor_alert_create",
            vec![s("r"), s("cpu"), s(">"), Value::Float(150.0)],
        ),
        (
            "monitor_alert_create",
            vec![s("r"), s("cpu"), s(">"), Value::Float(50.0), s("loud")],
        ),
        ("monitor_alert_create", vec![s("   "), s("cpu")]),
        ("monitor_alert_remove", vec![]),
        ("monitor_alert_remove", vec![Value::Int(5)]),
        ("monitor_alert_remove", vec![s("")]),
        ("monitor_alert_history", vec![Value::Int(0)]),
        ("monitor_alert_history", vec![Value::Int(1001)]),
        ("monitor_alert_list", vec![s("all")]),
    ];
    for (name, args) in cases {
        let shown = format!("{args:?}");
        assert_eq!(
            refused_with(name, args),
            "E_BAD_ARG",
            "{name}{shown} should be E_BAD_ARG"
        );
    }
}

#[test]
fn pipeline_input_is_refused_not_dropped() {
    for name in [
        "monitor_htop",
        "monitor_iotop",
        "monitor_nmon",
        "monitor_glances",
        "monitor_memory",
        "monitor_dstat",
        "monitor_health_check",
        "monitor_alert_list",
        "monitor_alert_history",
        "monitor_watch_cpu",
        "monitor_watch_memory",
        "monitor_watch_disk",
    ] {
        match call_piped(name, s("piped")) {
            Ok(v) => panic!("{name} silently accepted pipeline input and returned {v:?}"),
            Err(e) => assert_eq!(code_of(&e), "E_BAD_ARG", "{name} with pipeline input"),
        }
    }
}

#[test]
fn snapshot_builtins_answer_their_declared_types() {
    assert!(matches!(
        call_builtin("monitor_htop", vec![Value::Int(3)]),
        Ok(Value::Array(_))
    ));
    assert!(matches!(
        call_builtin("monitor_iotop", Vec::new()),
        Ok(Value::Array(_))
    ));
    for name in [
        "monitor_nmon",
        "monitor_glances",
        "monitor_memory",
        "monitor_dstat",
        "monitor_health_check",
    ] {
        match call_builtin(name, Vec::new()) {
            Ok(Value::Record(_)) => {}
            other => panic!("{name} should return a Record, got {other:?}"),
        }
    }
}

#[test]
fn memory_snapshot_is_internally_consistent() {
    let v = call_builtin("monitor_memory", Vec::new()).expect("monitor_memory");
    let total = match field(&v, "total") {
        Value::Int(n) => *n,
        other => panic!("total is {other:?}"),
    };
    let used = match field(&v, "used") {
        Value::Int(n) => *n,
        other => panic!("used is {other:?}"),
    };
    assert!(total > 0, "a machine has memory");
    assert!(
        (0..=total).contains(&used),
        "used {used} is within total {total}"
    );
}

#[test]
fn watch_cpu_is_bounded_in_samples_and_time() {
    let started = Instant::now();
    let v = call_builtin("monitor_watch_cpu", vec![Value::Float(80.0), Value::Int(2)])
        .expect("a two-sample watch");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "a two-sample watch took {elapsed:?}"
    );
    assert!(matches!(field(&v, "samples"), Value::Int(2)));
    assert_eq!(as_items(field(&v, "readings")).len(), 2);
    match field(&v, "average_cpu") {
        Value::Float(avg) => assert!(avg.is_finite() && (0.0..=100.0).contains(avg)),
        other => panic!("average_cpu is {other:?}"),
    }
}

#[test]
fn watch_cpu_default_takes_five_readings_within_the_bound() {
    let started = Instant::now();
    let v = call_builtin("monitor_watch_cpu", Vec::new()).expect("the default watch");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the default watch took {:?}",
        started.elapsed()
    );
    assert_eq!(as_items(field(&v, "readings")).len(), 5);
}

#[test]
fn alert_round_trip_in_a_temporary_store() {
    let _guard = STORE.lock().unwrap_or_else(|p| p.into_inner());
    let dir = StoreDir::new("round-trip");
    let file = dir.store_file();

    // Nothing has been created yet: the list is empty and no file exists.
    let empty = call_builtin("monitor_alert_list", Vec::new()).expect("empty list");
    assert!(as_items(&empty).is_empty());
    assert!(!file.exists(), "a read must not create the store");

    let first = call_builtin(
        "monitor_alert_create",
        vec![
            s("cpu hot"),
            s("cpu"),
            s(">"),
            Value::Float(90.0),
            s("critical"),
        ],
    )
    .expect("create the first rule");
    let second = call_builtin("monitor_alert_create", vec![s("mem warm"), s("memory")])
        .expect("create the second rule with defaults");
    let first_id = match field(&first, "id") {
        Value::Str(id) => id.clone(),
        other => panic!("id is {other:?}"),
    };
    let second_id = match field(&second, "id") {
        Value::Str(id) => id.clone(),
        other => panic!("id is {other:?}"),
    };
    assert_ne!(first_id, second_id, "ids are unique");
    assert!(matches!(field(&second, "threshold"), Value::Float(t) if *t == 80.0));
    assert!(matches!(field(&second, "condition"), Value::Str(c) if c == ">"));
    assert!(matches!(field(&second, "severity"), Value::Str(c) if c == "warning"));

    // The file the builtins wrote, read back independently.
    assert!(file.exists(), "the store is under IRONSHELL_DATA_HOME");
    let doc = read_store(&file);
    let rules = doc["rules"].as_array().expect("rules array");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0]["id"], first_id.as_str());
    assert_eq!(rules[0]["name"], "cpu hot");
    assert_eq!(rules[0]["metric"], "cpu");
    assert_eq!(rules[0]["threshold"], 90.0);
    assert_eq!(rules[0]["severity"], "critical");
    assert_eq!(rules[1]["id"], second_id.as_str());
    assert_eq!(doc["history"].as_array().expect("history").len(), 2);

    // The list reports each rule with a live reading.
    let listed = call_builtin("monitor_alert_list", Vec::new()).expect("list");
    let items = as_items(&listed);
    assert_eq!(items.len(), 2);
    for item in items {
        match field(item, "current_value") {
            Value::Float(v) => assert!(v.is_finite()),
            other => panic!("current_value is {other:?}"),
        }
        assert!(matches!(field(item, "triggered"), Value::Bool(_)));
    }

    // History is newest first and holds the two creations.
    let history = call_builtin("monitor_alert_history", vec![Value::Int(10)]).expect("history");
    let events = as_items(&history);
    assert_eq!(events.len(), 2);
    assert!(matches!(field(&events[0], "rule_id"), Value::Str(id) if *id == second_id));
    assert!(matches!(field(&events[0], "event"), Value::Str(e) if e == "created"));

    // Remove the first rule; the file shows it gone and a removal event.
    let removed = call_builtin("monitor_alert_remove", vec![s(&first_id)]).expect("remove");
    assert!(matches!(field(&removed, "removed"), Value::Bool(true)));
    let doc = read_store(&file);
    let rules = doc["rules"].as_array().expect("rules array");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["id"], second_id.as_str());
    let history = doc["history"].as_array().expect("history array");
    assert_eq!(history.len(), 3);
    assert_eq!(history[2]["event"], "removed");

    // Removing it again is a coded not-found, and the store is unchanged.
    let before = std::fs::read(&file).expect("read store");
    assert_eq!(
        refused_with("monitor_alert_remove", vec![s(&first_id)]),
        "E_NOT_FOUND"
    );
    assert_eq!(std::fs::read(&file).expect("read store"), before);

    // Newest first: the removal is the first history event.
    let history = call_builtin("monitor_alert_history", Vec::new()).expect("history");
    let events = as_items(&history);
    assert!(matches!(field(&events[0], "event"), Value::Str(e) if e == "removed"));
}

#[test]
fn a_corrupt_store_is_refused_and_left_alone() {
    let _guard = STORE.lock().unwrap_or_else(|p| p.into_inner());
    let dir = StoreDir::new("corrupt");
    let file = dir.store_file();
    std::fs::create_dir_all(file.parent().expect("parent")).expect("create monitor dir");
    std::fs::write(&file, "{not json").expect("write a corrupt store");

    assert_eq!(refused_with("monitor_alert_list", Vec::new()), "E_IO");
    assert_eq!(
        refused_with("monitor_alert_create", vec![s("r"), s("cpu")]),
        "E_IO"
    );
    assert_eq!(
        std::fs::read_to_string(&file).expect("read store"),
        "{not json",
        "a corrupt store must not be overwritten"
    );
}
