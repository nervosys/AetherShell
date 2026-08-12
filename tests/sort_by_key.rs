//! `sort_by` accepts a field name, not only a lambda.
//!
//! Added after hitting it while using the shell as an agent. `sort_by("size")`
//! is what most callers reach for first, and it previously failed twice over:
//! the string argument was silently ignored (only `"desc"`/`"descending"` were
//! recognised) and the call then errored for want of a lambda — a wasted
//! round-trip on a call that reads as obviously correct.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn rows() -> Value {
    Value::Array(vec![
        rec(&[("name", Value::Str("b".into())), ("size", Value::Int(20))]),
        rec(&[("name", Value::Str("a".into())), ("size", Value::Int(30))]),
        rec(&[("name", Value::Str("c".into())), ("size", Value::Int(10))]),
    ])
}

fn names(v: &Value) -> Vec<String> {
    match v {
        Value::Array(items) => items
            .iter()
            .map(|r| match r {
                Value::Record(m) => match m.get("name") {
                    Some(Value::Str(s)) => s.clone(),
                    other => panic!("unexpected name: {other:?}"),
                },
                other => panic!("expected a record, got {other:?}"),
            })
            .collect(),
        other => panic!("expected an array, got {other:?}"),
    }
}

#[test]
fn a_field_name_sorts_by_that_field() {
    let out = call("sort_by", vec![rows(), Value::Str("size".into())]).expect("sort_by");
    assert_eq!(names(&out), vec!["c", "b", "a"]);
}

#[test]
fn a_field_name_composes_with_descending() {
    let out = call(
        "sort_by",
        vec![rows(), Value::Str("size".into()), Value::Str("desc".into())],
    )
    .expect("sort_by");
    assert_eq!(names(&out), vec!["a", "b", "c"]);
}

#[test]
fn the_lambda_form_is_unchanged() {
    // The existing contract must not shift while adding sugar beside it.
    let mut env = aethershell::env::Env::new();
    let prog = aethershell::parser::parse_program(
        "[{name:\"b\",size:20},{name:\"a\",size:30},{name:\"c\",size:10}] | sort_by(fn(r) => r.size)",
    )
    .expect("parse");
    let out = aethershell::eval::eval_program(&prog, &mut env).expect("eval");
    assert_eq!(names(&out), vec!["c", "b", "a"]);
}

#[test]
fn a_missing_field_sorts_rather_than_erroring() {
    // A field absent from some rows yields Null keys — those group together
    // instead of failing the whole call, which is what a partial dataset needs.
    let ragged = Value::Array(vec![
        rec(&[("name", Value::Str("has".into())), ("size", Value::Int(5))]),
        rec(&[("name", Value::Str("lacks".into()))]),
    ]);
    let out = call("sort_by", vec![ragged, Value::Str("size".into())]).expect("sort_by");
    assert_eq!(names(&out).len(), 2, "no row is dropped");
}

#[test]
fn no_key_at_all_names_both_accepted_forms() {
    // The error an agent reads when it guesses wrong should teach the call.
    let err = call("sort_by", vec![rows()]).expect_err("a key is required");
    assert!(err.contains("field name"), "names the string form: {err}");
    assert!(err.contains("lambda"), "names the lambda form: {err}");
}

#[test]
fn desc_alone_is_still_a_direction_not_a_field() {
    // `"desc"` stays reserved. A field genuinely named `desc` must use the
    // lambda form — a real limitation, asserted rather than left implicit.
    let err = call("sort_by", vec![rows(), Value::Str("desc".into())])
        .expect_err("desc is a direction, so no key was given");
    assert!(err.contains("field name"), "got: {err}");
}
