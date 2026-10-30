//! Contracts for `gui_find_image`: template matching over image files.
//!
//! Fixtures live in tests/fixtures/find_image/ and were generated once with the
//! `image` crate (the crate is not a dev-dependency, so integration tests cannot
//! encode PNGs themselves). Layout, all RGBA8:
//! - haystack.png (8x4): gray background with needle.png at (1,1) and (4,2)
//! - needle.png (3x2), needle_near.png (same, one channel off by 3)
//! - uniform_needle.png (2x2) and uniform_grid.png (4x3) of one color, for
//!   overlapping matches
//! - huge_header.png: a PNG whose IHDR claims 100000 x 100000 and whose IDAT
//!   is empty; it must be refused from its header, never decoded
//! - not_an_image.txt: a text file

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/find_image/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    )
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn opts(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn call(args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call("gui_find_image", args, &mut env)
}

fn find(haystack: &str, needle: &str, options: Option<Value>) -> anyhow::Result<Value> {
    let mut args = vec![s(&fixture(haystack)), s(&fixture(needle))];
    args.extend(options);
    call(args)
}

/// The `E_*` code named in a refusal, or the whole message when there is none.
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

fn expect_code(result: anyhow::Result<Value>, code: &str) {
    match result {
        Ok(v) => panic!("expected {code}, got {v:?}"),
        Err(e) => assert_eq!(code_of(&e), code, "message: {e}"),
    }
}

/// Unwrap a Record into (x, y, width, height).
fn xywh(v: &Value) -> (i64, i64, i64, i64) {
    let Value::Record(m) = v else {
        panic!("expected a Record, got {v:?}");
    };
    let int = |k: &str| match m.get(k) {
        Some(Value::Int(n)) => *n,
        other => panic!("field {k}: expected Int, got {other:?}"),
    };
    (int("x"), int("y"), int("width"), int("height"))
}

fn positions(v: &Value) -> Vec<(i64, i64)> {
    let Value::Array(items) = v else {
        panic!("expected an Array, got {v:?}");
    };
    items
        .iter()
        .map(|item| {
            let (x, y, w, h) = xywh(item);
            assert_eq!((w, h), (3, 2), "needle size is reported");
            (x, y)
        })
        .collect()
}

#[test]
fn exact_match_is_found_at_known_coordinates() {
    let v = find("haystack.png", "needle.png", None).unwrap();
    assert_eq!(xywh(&v), (1, 1, 3, 2));
}

#[test]
fn explicit_default_options_match_absent_options() {
    let v = find(
        "haystack.png",
        "needle.png",
        Some(opts(&[
            ("tolerance", Value::Int(0)),
            ("all", Value::Bool(false)),
            ("limit", Value::Int(100)),
        ])),
    )
    .unwrap();
    assert_eq!(xywh(&v), (1, 1, 3, 2));
}

#[test]
fn no_match_is_null() {
    let v = find("haystack.png", "needle_near.png", None).unwrap();
    assert_eq!(v, Value::Null);
}

#[test]
fn tolerance_admits_a_near_match() {
    // The near needle differs from both copies by exactly 3 on one channel.
    let at_two = find(
        "haystack.png",
        "needle_near.png",
        Some(opts(&[("tolerance", Value::Int(2))])),
    )
    .unwrap();
    assert_eq!(at_two, Value::Null);
    let at_three = find(
        "haystack.png",
        "needle_near.png",
        Some(opts(&[("tolerance", Value::Int(3))])),
    )
    .unwrap();
    assert_eq!(xywh(&at_three), (1, 1, 3, 2));
}

#[test]
fn all_returns_every_position_in_row_major_order() {
    let v = find(
        "haystack.png",
        "needle.png",
        Some(opts(&[("all", Value::Bool(true))])),
    )
    .unwrap();
    assert_eq!(positions(&v), vec![(1, 1), (4, 2)]);
}

#[test]
fn all_with_no_match_is_an_empty_array() {
    let v = find(
        "haystack.png",
        "needle_near.png",
        Some(opts(&[("all", Value::Bool(true))])),
    )
    .unwrap();
    assert_eq!(v, Value::Array(Vec::new()));
}

#[test]
fn limit_caps_all_matches() {
    let v = find(
        "haystack.png",
        "needle.png",
        Some(opts(&[
            ("all", Value::Bool(true)),
            ("limit", Value::Int(1)),
        ])),
    )
    .unwrap();
    assert_eq!(positions(&v), vec![(1, 1)]);
}

#[test]
fn overlapping_matches_are_all_reported() {
    // A uniform 2x2 needle matches at every position of a 4x3 grid.
    let v = find(
        "uniform_grid.png",
        "uniform_needle.png",
        Some(opts(&[("all", Value::Bool(true))])),
    )
    .unwrap();
    let Value::Array(items) = v else {
        panic!("expected an Array");
    };
    let got: Vec<(i64, i64)> = items
        .iter()
        .map(|item| {
            let (x, y, w, h) = xywh(item);
            assert_eq!((w, h), (2, 2));
            (x, y)
        })
        .collect();
    assert_eq!(got, vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]);
}

#[test]
fn needle_larger_than_haystack_is_refused() {
    expect_code(find("needle.png", "haystack.png", None), "E_BAD_ARG");
}

#[test]
fn oversized_header_is_refused_before_decoding() {
    expect_code(
        find("huge_header.png", "needle.png", None),
        "E_BUDGET_EXCEEDED",
    );
}

#[test]
fn missing_file_is_not_found() {
    expect_code(
        find("no-such-haystack-ae.png", "needle.png", None),
        "E_NOT_FOUND",
    );
    expect_code(
        find("haystack.png", "no-such-needle-ae.png", None),
        "E_NOT_FOUND",
    );
}

#[test]
fn a_file_that_is_not_an_image_is_refused() {
    expect_code(find("not_an_image.txt", "needle.png", None), "E_BAD_ARG");
}

#[test]
fn unknown_option_key_is_refused() {
    expect_code(
        find(
            "haystack.png",
            "needle.png",
            Some(opts(&[("threshold", Value::Int(1))])),
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn option_values_out_of_range_or_wrong_type_are_refused() {
    for bad in [
        opts(&[("tolerance", Value::Int(256))]),
        opts(&[("tolerance", Value::Int(-1))]),
        opts(&[("tolerance", s("3"))]),
        opts(&[("all", Value::Int(1))]),
        opts(&[("limit", Value::Int(0))]),
        opts(&[("limit", Value::Int(1001))]),
    ] {
        expect_code(
            find("haystack.png", "needle.png", Some(bad.clone())),
            "E_BAD_ARG",
        );
    }
}

#[test]
fn options_that_are_not_a_record_are_refused() {
    expect_code(
        find("haystack.png", "needle.png", Some(Value::Int(3))),
        "E_BAD_ARG",
    );
}

#[test]
fn wrong_argument_count_and_types_are_refused() {
    expect_code(call(vec![s(&fixture("haystack.png"))]), "E_BAD_ARG");
    expect_code(
        call(vec![
            s(&fixture("haystack.png")),
            s(&fixture("needle.png")),
            opts(&[]),
            Value::Null,
        ]),
        "E_BAD_ARG",
    );
    expect_code(
        call(vec![Value::Int(1), s(&fixture("needle.png"))]),
        "E_BAD_ARG",
    );
}

#[test]
fn pipeline_input_is_refused() {
    let mut env = Env::new();
    let args = vec![s(&fixture("haystack.png")), s(&fixture("needle.png"))];
    let result = builtins::call_with_input(
        "gui_find_image",
        args,
        Some(Value::Str("piped".to_string())),
        &mut env,
    );
    expect_code(result, "E_BAD_ARG");
}
