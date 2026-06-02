//! Reliability tests: argument failures are structured (`E_BAD_ARG`) and
//! self-correcting — an agent reads `e.error.code` + the expected signature
//! instead of parsing prose. See docs/AGENTIC_FIRST_DESIGN.md §5.2/§8.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

fn canon(v: Value) -> String {
    match call("canonical", vec![v]) {
        Value::Str(s) => s,
        other => panic!("canonical should return a string, got {other:?}"),
    }
}

#[test]
fn bad_arg_is_catchable_as_structured_error() {
    // `tokens()` with no argument and no piped input → E_BAD_ARG, caught as a
    // structured record by try/catch.
    let src = r#"try { tokens() } catch e { e }"#;
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::eval::eval_program(&stmts, &mut env).expect("eval");

    match result {
        Value::Record(m) => match m.get("error") {
            Some(Value::Record(e)) => {
                assert_eq!(e.get("code"), Some(&Value::Str("E_BAD_ARG".to_string())));
                assert_eq!(e.get("retryable"), Some(&Value::Bool(true)));
                assert!(e.contains_key("hint"), "carries an actionable hint");
            }
            other => panic!("error should be a nested record, got {other:?}"),
        },
        other => panic!("catch should bind a structured record, got {other:?}"),
    }
}

#[test]
fn bad_arg_reports_the_offending_type() {
    // Wrong-typed argument to a guarded builtin: the rendered error names both
    // the expected signature and the type that was actually passed.
    let err = aethershell::builtins::bi_rm(vec![Value::Int(7)], None).unwrap_err();
    let json: serde_json::Value =
        serde_json::from_str(&format!("{}", err)).expect("structured error renders as JSON");
    assert_eq!(json["error"]["code"], "E_BAD_ARG");
    let msg = json["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("path: String"), "names expected: {msg}");
    assert!(msg.contains("Int"), "names the got type: {msg}");
}

#[test]
fn shared_extraction_helpers_emit_structured_bad_arg() {
    // A wrong-typed argument to ANY builtin that uses the shared `expect_*`
    // helpers now surfaces a structured, catchable E_BAD_ARG — not ad-hoc prose.
    // `env(123)` exercises `expect_string`; the same upgrade covers the ~90
    // call sites of expect_string/expect_int/expect_array/need_lambda.
    let src = r#"try { env(123) } catch e { e }"#;
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::eval::eval_program(&stmts, &mut env).expect("eval");

    match result {
        Value::Record(m) => match m.get("error") {
            Some(Value::Record(e)) => {
                assert_eq!(e.get("code"), Some(&Value::Str("E_BAD_ARG".to_string())));
                let msg = match e.get("message") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                assert!(msg.contains("a string"), "names the expected type: {msg}");
                assert!(msg.contains("Int"), "names the got type: {msg}");
            }
            other => panic!("error should be a nested record, got {other:?}"),
        },
        other => panic!("catch should bind a structured record, got {other:?}"),
    }
}

#[test]
fn missing_argument_is_structured_bad_arg() {
    // A missing required argument to a core agent-facing verb now surfaces a
    // structured, catchable E_BAD_ARG (not ad-hoc prose) naming what's expected.
    // `reduce([1,2,3])` is missing the lambda + init.
    let src = r#"try { reduce([1,2,3]) } catch e { e }"#;
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::eval::eval_program(&stmts, &mut env).expect("eval");

    match result {
        Value::Record(m) => match m.get("error") {
            Some(Value::Record(e)) => {
                assert_eq!(e.get("code"), Some(&Value::Str("E_BAD_ARG".to_string())));
                assert!(e.contains_key("hint"), "carries an actionable hint");
            }
            other => panic!("error should be a nested record, got {other:?}"),
        },
        other => panic!("catch should bind a structured record, got {other:?}"),
    }
}

#[test]
fn canonical_is_deterministic_and_key_sorted() {
    // Exact, stable rendering with sorted keys regardless of insertion order.
    let a = canon(rec(&[("b", Value::Str("x".into())), ("a", Value::Int(1))]));
    let b = canon(rec(&[("a", Value::Int(1)), ("b", Value::Str("x".into()))]));
    assert_eq!(a, b, "insertion order must not affect canonical form");
    assert_eq!(a, r#"{"a":1,"b":"x"}"#);

    // Same value renders byte-identically every time.
    let v = Value::Array(vec![
        Value::Int(1),
        rec(&[("k", Value::Bool(true)), ("n", Value::Null)]),
        Value::Float(1.5),
    ]);
    assert_eq!(canon(v.clone()), canon(v.clone()));
    assert_eq!(canon(v), r#"[1,{"k":true,"n":null},1.5]"#);
}

#[test]
fn canonical_handles_non_finite_floats() {
    // NaN/Inf are not representable in JSON → explicit, deterministic null.
    assert_eq!(canon(Value::Float(f64::NAN)), "null");
    assert_eq!(canon(Value::Float(f64::INFINITY)), "null");
    // Strings are properly escaped.
    assert_eq!(canon(Value::Str("a\"b".into())), r#""a\"b""#);
}
