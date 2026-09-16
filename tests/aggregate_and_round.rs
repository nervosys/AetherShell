//! Three defects that a passing test suite did not see, because nothing asked.
//!
//! All three were found by writing ten ordinary data queries against the shell
//! and checking the answers against an oracle (`benches/agentic/`), which is
//! the one thing 1,400 builtin unit tests were not doing:
//!
//!   * `round(4.966, 2)` answered `5`. The second argument parsed, was carried
//!     into the builtin, and was discarded. A *wrong number with no error* is
//!     the worst failure shape available, because nothing in the result tells
//!     an agent to look again -- it propagates into whatever it was computing.
//!   * `max([1, 5, 3])` and `[1, 5, 3] | max` both answered "max requires two
//!     arguments", while `ontology_describe("max")` advertised category
//!     `Aggregation`, description "Get maximum value", signature `max() ->
//!     Number` and an empty parameter list. The discovery surface described a
//!     function that did not exist, so an agent that read the ontology before
//!     calling -- exactly the behaviour the ontology is for -- wrote code that
//!     could not run.
//!   * `min` had the identical shape.
//!
//! The binary forms are unchanged and are asserted here too, because widening
//! an overload is the kind of fix that quietly breaks the narrow case.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;

fn f(name: &str, args: Vec<Value>) -> Value {
    let mut env = Env::new();
    call(name, args, &mut env).unwrap_or_else(|e| panic!("{name} failed: {e}"))
}

fn piped(name: &str, input: Value) -> Value {
    let mut env = Env::new();
    call_with_input(name, vec![], Some(input), &mut env)
        .unwrap_or_else(|e| panic!("{name} (piped) failed: {e}"))
}

fn err(name: &str, args: Vec<Value>) -> String {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} unexpectedly succeeded with {v:?}"),
        Err(e) => e.to_string(),
    }
}

fn ints(xs: &[i64]) -> Value {
    Value::Array(xs.iter().map(|n| Value::Int(*n)).collect())
}

// ── round ───────────────────────────────────────────────────────────────

#[test]
fn round_honours_the_digits_argument_it_used_to_discard() {
    // The exact call the benchmark made: the mean comment count on open
    // issues, which every other engine rendered as 4.97.
    assert_eq!(
        f(
            "round",
            vec![Value::Float(4.966_101_694_915_254), Value::Int(2)]
        ),
        Value::Float(4.97)
    );
    assert_eq!(
        f("round", vec![Value::Float(4.44), Value::Int(1)]),
        Value::Float(4.4)
    );
    assert_eq!(
        f("round", vec![Value::Float(4.46), Value::Int(1)]),
        Value::Float(4.5)
    );
    assert_eq!(
        f("round", vec![Value::Float(2.345), Value::Int(2)]),
        Value::Float(2.35)
    );
}

#[test]
fn round_without_digits_still_returns_an_int() {
    // The one-argument form is what every existing caller uses. Widening the
    // overload must not turn its Int into a Float.
    assert_eq!(f("round", vec![Value::Float(4.5)]), Value::Int(5));
    assert_eq!(f("round", vec![Value::Float(4.4)]), Value::Int(4));
    assert_eq!(f("round", vec![Value::Int(7)]), Value::Int(7));
    // Explicit zero digits means the same thing as asking for an integer.
    assert_eq!(
        f("round", vec![Value::Float(4.5), Value::Int(0)]),
        Value::Int(5)
    );
}

#[test]
fn round_rejects_a_digits_argument_it_cannot_honour() {
    // The defect was accepting an argument and ignoring it. Refusing is fine;
    // silently continuing is not.
    let e = err("round", vec![Value::Float(1.0), Value::Str("2".into())]);
    assert!(e.contains("digits"), "unhelpful refusal: {e}");
    let e = err("round", vec![Value::Float(1.0), Value::Int(99)]);
    assert!(e.contains("digits"), "unhelpful refusal: {e}");
}

// ── min / max ───────────────────────────────────────────────────────────

#[test]
fn max_and_min_aggregate_an_array_as_their_ontology_says() {
    assert_eq!(f("max", vec![ints(&[1, 5, 3])]), Value::Int(5));
    assert_eq!(f("min", vec![ints(&[1, 5, 3])]), Value::Int(1));
    assert_eq!(piped("max", ints(&[1, 5, 3])), Value::Int(5));
    assert_eq!(piped("min", ints(&[1, 5, 3])), Value::Int(1));
}

#[test]
fn an_all_int_aggregate_stays_an_int() {
    // `[1, 5, 3] | max` answering `5.0` would make an agent's comparison
    // against the literal `5` fail for a reason it cannot see.
    match piped("max", ints(&[1, 5, 3])) {
        Value::Int(5) => {}
        other => panic!("expected Int(5), got {other:?}"),
    }
    match piped("max", Value::Array(vec![Value::Int(1), Value::Float(5.5)])) {
        Value::Float(x) if (x - 5.5).abs() < f64::EPSILON => {}
        other => panic!("expected Float(5.5), got {other:?}"),
    }
}

#[test]
fn the_two_number_form_is_untouched() {
    assert_eq!(f("max", vec![Value::Int(2), Value::Int(9)]), Value::Int(9));
    assert_eq!(f("min", vec![Value::Int(2), Value::Int(9)]), Value::Int(2));
    assert_eq!(
        f("max", vec![Value::Int(2), Value::Float(9.5)]),
        Value::Float(9.5)
    );
    assert_eq!(
        f("min", vec![Value::Float(2.5), Value::Int(9)]),
        Value::Float(2.5)
    );
}

#[test]
fn an_empty_or_non_numeric_aggregate_is_refused_by_name() {
    let e = err("max", vec![Value::Array(vec![])]);
    assert!(e.contains("empty"), "refusal should name the cause: {e}");
    let e = err("max", vec![Value::Array(vec![Value::Str("a".into())])]);
    assert!(e.contains("numeric"), "refusal should name the cause: {e}");
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_these_names_reach_the_implementations_under_test() {
    // If `call` silently routed elsewhere, or if a helper returned the input
    // unchanged, every assertion above could hold for the wrong reason.
    assert!(
        aethershell::builtins::is_dispatched("round")
            && aethershell::builtins::is_dispatched("max")
            && aethershell::builtins::is_dispatched("min"),
        "one of round/max/min is not dispatched; the tests above prove nothing"
    );

    // The pre-fix behaviour must now be impossible: this is the assertion that
    // would have failed before the change and must never pass again.
    assert_ne!(
        f("round", vec![Value::Float(4.966), Value::Int(2)]),
        Value::Int(5),
        "round is discarding its digits argument again"
    );
    assert!(
        call("max", vec![ints(&[1, 5, 3])], &mut Env::new()).is_ok(),
        "max no longer accepts an array; the aggregate form has regressed"
    );
}

#[test]
fn round_in_a_pipeline_rounds_the_piped_value_not_the_digit_count() {
    // `mean | round(2)` answered `2`: the unary convention "first argument if
    // present, else the input" took the digit count as the subject. In a
    // pipeline the piped value is the subject and arguments are parameters,
    // as they are for `map`, `where` and `nth`.
    let mut env = Env::new();
    let got = call_with_input(
        "round",
        vec![Value::Int(2)],
        Some(Value::Float(4.966_101_694_915_254)),
        &mut env,
    )
    .expect("piped round");
    assert_eq!(
        got,
        Value::Float(4.97),
        "round(2) in a pipeline took the wrong subject"
    );

    // Without a pipe, the single argument is still the number to round.
    assert_eq!(f("round", vec![Value::Float(2.4)]), Value::Int(2));
}
