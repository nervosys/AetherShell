//! Tests for `@nest` — flattening record-valued columns into dotted columns so
//! every existing pass (`@const`, `@dict`, `@prefix`, `@suffix`, `@same`) can reach
//! the leaves. Without it a nested cell serializes as whole JSON on every row, keys
//! included, which is the exact cost AECON exists to remove.
//!
//! As everywhere else in this format, the load-bearing assertion is the exact
//! round-trip; the token win is measured separately.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn encode(v: &Value) -> String {
    match call("aecon", vec![v.clone()]) {
        Value::Str(s) => s,
        other => panic!("aecon should return a string, got {other:?}"),
    }
}

fn roundtrip(v: &Value) -> Value {
    call("aecon_decode", vec![Value::Str(encode(v))])
}

fn tokens(s: &str) -> usize {
    match call("tokens", vec![Value::Str(s.to_string())]) {
        Value::Int(n) => n as usize,
        Value::Record(m) => match m.get("tokens") {
            Some(Value::Int(n)) => *n as usize,
            other => panic!("unexpected tokens shape: {other:?}"),
        },
        other => panic!("unexpected tokens result: {other:?}"),
    }
}

/// A realistic API-shaped payload: a constant nested block, a run-structured nested
/// field, and a nested path that shares a prefix and an extension.
fn nested_payload(n: i64) -> Value {
    Value::Array(
        (0..n)
            .map(|i| {
                rec(&[
                    ("id", Value::Int(1_700_000_000 + i * 3)),
                    (
                        "meta",
                        rec(&[
                            ("region", Value::Str("us-west-2".into())),
                            ("tier", Value::Str("standard".into())),
                            (
                                "stage",
                                Value::Str(
                                    ["build", "test", "deploy"][(i / 4 % 3) as usize].into(),
                                ),
                            ),
                        ]),
                    ),
                    (
                        "artifact",
                        rec(&[
                            (
                                "path",
                                Value::Str(format!("/out/aethershell/step_{i:02}_result.json")),
                            ),
                            ("bytes", Value::Int(4096 + i)),
                        ]),
                    ),
                ])
            })
            .collect(),
    )
}

#[test]
fn nested_records_become_dotted_columns() {
    let arr = nested_payload(12);
    let out = encode(&arr);

    assert!(
        out.contains("@nest "),
        "should declare what it expanded:\n{out}"
    );
    assert!(out.contains("artifact.bytes"), "leaf column:\n{out}");
    // The constant nested leaves collapse into @const, which is the whole point:
    // without flattening they would repeat on every row inside a JSON blob.
    assert!(out.contains("meta.region=us-west-2"), "got:\n{out}");
    assert!(out.contains("meta.tier=standard"), "got:\n{out}");
    // And a nested path is now reachable by the prefix/suffix pass.
    assert!(out.contains("@prefix artifact.path: "), "got:\n{out}");
}

#[test]
fn nested_records_round_trip_exactly() {
    let arr = nested_payload(12);
    assert_eq!(
        roundtrip(&arr),
        arr,
        "nesting must be reconstructed exactly"
    );
}

#[test]
fn deep_nesting_round_trips_to_the_depth_limit() {
    let arr = Value::Array(
        (0..6)
            .map(|i| {
                rec(&[
                    ("n", Value::Int(i)),
                    (
                        "a",
                        rec(&[("b", rec(&[("c", Value::Str(format!("leaf{i}")))]))]),
                    ),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(out.contains("a.b.c"), "three levels deep:\n{out}");
    assert_eq!(roundtrip(&arr), arr);
}

#[test]
fn a_literal_dotted_key_is_never_shadowed() {
    // A column literally named `user.id` alongside a nested `user` record: expanding
    // `user` would collide with it, so the flattener must decline and leave the
    // record whole rather than silently merge the two.
    let arr = Value::Array(
        (0..6)
            .map(|i| {
                rec(&[
                    ("user.id", Value::Int(i)),
                    ("user", rec(&[("id", Value::Str(format!("s{i}")))])),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(!out.contains("@nest"), "must decline the collision:\n{out}");
    assert_eq!(roundtrip(&arr), arr);
}

#[test]
fn an_empty_record_survives_rather_than_vanishing() {
    // Flattening an empty record would produce no columns at all, erasing the field.
    let arr = Value::Array(
        (0..6)
            .map(|i| {
                rec(&[
                    ("n", Value::Int(i)),
                    ("empty", Value::Record(BTreeMap::new())),
                ])
            })
            .collect(),
    );
    assert_eq!(roundtrip(&arr), arr, "an empty record must not be erased");
}

#[test]
fn a_ragged_nested_column_is_left_alone() {
    // If one row's field is not a record, the flattened table would be ragged.
    let arr = Value::Array(
        (0..6)
            .map(|i| {
                rec(&[
                    ("n", Value::Int(i)),
                    (
                        "v",
                        if i == 3 {
                            Value::Str("scalar".into())
                        } else {
                            rec(&[("x", Value::Int(i))])
                        },
                    ),
                ])
            })
            .collect(),
    );
    assert_eq!(roundtrip(&arr), arr, "ragged shapes must still round-trip");
}

#[test]
fn arrays_inside_records_stay_whole() {
    // Only records are expanded. An array cell remains a single atom, so the cell
    // grammar stays flat and decode stays unambiguous.
    let arr = Value::Array(
        (0..6)
            .map(|i| {
                rec(&[
                    ("n", Value::Int(i)),
                    (
                        "meta",
                        rec(&[
                            (
                                "tags",
                                Value::Array(vec![Value::Str("a".into()), Value::Int(i)]),
                            ),
                            ("ok", Value::Bool(true)),
                        ]),
                    ),
                ])
            })
            .collect(),
    );
    assert_eq!(roundtrip(&arr), arr, "array cells must survive intact");
}

/// Reports what flattening is worth against the same payload rendered with nested
/// cells left as JSON blobs. Run with `--features real-tokens` for the true cl100k
/// number; the default build reports the labeled heuristic.
#[test]
fn report_the_gain_on_a_nested_payload() {
    for n in [12, 30] {
        let arr = nested_payload(n);
        let flat = tokens(&encode(&arr));
        // The pre-@nest behaviour: each record cell serialized whole, per row.
        let blobbed = Value::Array(
            match &arr {
                Value::Array(rows) => rows.clone(),
                _ => unreachable!(),
            }
            .into_iter()
            .map(|r| match r {
                Value::Record(m) => {
                    let mut out = BTreeMap::new();
                    for (k, v) in m {
                        match v {
                            Value::Record(_) => {
                                out.insert(k, Value::Str(v.to_json().to_string()));
                            }
                            other => {
                                out.insert(k, other);
                            }
                        }
                    }
                    Value::Record(out)
                }
                other => other,
            })
            .collect(),
        );
        let baseline = tokens(&encode(&blobbed));
        println!("{n} rows — @nest: {flat} tokens; nested-as-JSON: {baseline} tokens");
        assert!(
            flat < baseline,
            "{n} rows: flattening must pay: {flat} vs {baseline}"
        );
    }
}
