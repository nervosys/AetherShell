//! Primitives the shipped examples assumed existed, and mostly did not.
//!
//! Running every `.ae` file in the repository turned up four gaps that were
//! not bugs in the examples but genuine holes in the language:
//!
//!   * no way to convert a value to a string — 41 example sites called a
//!     `str()` that had never existed;
//!   * `"=" * 50`, the way every example draws a rule, was an error;
//!   * `"n: " + 1` worked and `"hit: " + true` did not;
//!   * an async lambda could not see its enclosing scope, so it could not call
//!     a function defined next to it.

use aethershell::env::Env;
use aethershell::value::Value;

fn run(src: &str) -> anyhow::Result<Value> {
    let mut env = Env::new();
    let stmts = aethershell::parser::parse_program(src)?;
    aethershell::eval::eval_program(&stmts, &mut env)
}

fn s(src: &str) -> String {
    match run(src) {
        Ok(Value::Str(s)) => s,
        other => panic!("{src:?} did not produce a string: {other:?}"),
    }
}

// ── str() ────────────────────────────────────────────────────────────────

#[test]
fn str_renders_every_kind_of_value() {
    assert_eq!(s("str(42)"), "42");
    assert_eq!(s("str(2.5)"), "2.5");
    assert_eq!(s("str(true)"), "true");
    assert_eq!(s("str(null)"), "null");
    assert_eq!(s("str(\"already\")"), "already");
    assert_eq!(s("str([1, 2])"), "[1, 2]");
    assert_eq!(s("str({a: 1})"), "{a: 1}");
}

#[test]
fn to_string_is_the_same_function() {
    assert_eq!(s("to_string({a: 1})"), s("str({a: 1})"));
}

#[test]
fn str_works_in_a_pipeline() {
    assert_eq!(s("42 | str"), "42");
}

#[test]
fn str_renders_the_same_text_interpolation_does() {
    // Three renderers agreeing is the point: `print`, `${…}` and `str` should
    // not disagree about what a record looks like.
    assert_eq!(s("str({a: 1})"), s("\"${{a: 1}}\""));
}

// ── String repetition ────────────────────────────────────────────────────

#[test]
fn a_string_times_an_integer_repeats_it() {
    assert_eq!(s("\"=\" * 5"), "=====");
    assert_eq!(s("5 * \"=\""), "=====", "repetition should commute");
    assert_eq!(s("\"ab\" * 3"), "ababab");
}

#[test]
fn a_non_positive_repeat_count_gives_an_empty_string() {
    assert_eq!(s("\"x\" * 0"), "");
    assert_eq!(s("\"x\" * -1"), "");
}

/// A repeat count is trivially typo- or attacker-controlled, so an absurd one
/// must be refused rather than allocated.
#[test]
fn an_enormous_repeat_is_refused_rather_than_allocated() {
    let err = run("\"x\" * 99999999")
        .expect_err("a 100 MB repeat should be refused")
        .to_string();
    assert!(
        err.contains("limit"),
        "the refusal should say why, got: {err}"
    );
}

// ── Concatenation ────────────────────────────────────────────────────────

#[test]
fn a_string_concatenates_with_any_value() {
    assert_eq!(s("\"n: \" + 1"), "n: 1");
    assert_eq!(s("\"hit: \" + true"), "hit: true");
    assert_eq!(s("\"v: \" + null"), "v: null");
    assert_eq!(s("\"xs: \" + [1, 2]"), "xs: [1, 2]");
    assert_eq!(s("\"r: \" + {a: 1}"), "r: {a: 1}");
}

#[test]
fn concatenation_works_with_the_string_on_either_side() {
    assert_eq!(s("true + \" :hit\""), "true :hit");
    assert_eq!(s("[1, 2] + \" :xs\""), "[1, 2] :xs");
}

// ── Async closures ───────────────────────────────────────────────────────

#[test]
fn an_async_lambda_can_call_a_function_from_its_enclosing_scope() {
    // Awaiting used to evaluate the body in a brand new `Env`, so the body saw
    // neither outer bindings nor user-defined functions — only its own
    // arguments. A plain lambda never had the problem, which is why it went
    // unnoticed.
    let out = run("let inner = async fn(x) => x + 1\nlet outer = async fn(n) => await inner(n)\nawait outer(1)")
        .expect("async lambda should see `inner`");
    assert_eq!(out, Value::Int(2));
}

#[test]
fn an_async_lambda_can_read_an_outer_binding() {
    let out = run("let k = 10\nlet f = async fn(n) => n + k\nawait f(1)")
        .expect("async lambda should see `k`");
    assert_eq!(out, Value::Int(11));
}

#[test]
fn an_async_lambda_does_not_leak_its_parameters() {
    // Parameters are bound in the caller's environment and restored, so a name
    // used as a parameter must not survive the call.
    let out = run("let n = 99\nlet f = async fn(n) => n + 1\nlet _ = await f(1)\nn")
        .expect("outer `n` should be intact");
    assert_eq!(
        out,
        Value::Int(99),
        "the async call overwrote an outer binding with its parameter"
    );
}
