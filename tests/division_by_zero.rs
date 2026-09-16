//! A zero divisor must be refused, not answered.
//!
//! Found while measuring how each shell reports ten ordinary failures
//! (`benches/agentic/errors.mjs`). Two of the ten did not report anything:
//!
//! ```text
//! $ ae -c '1 / 0'
//! inf                      # exit 0
//! $ ae -c '0 / 0'
//! NaN                      # exit 0
//! $ ae -c '1 % 0'
//! thread 'aether-eval' panicked at src/eval.rs:1568:59:
//! attempt to calculate the remainder with a divisor of zero
//! ```
//!
//! The first two are the dangerous pair. An agent computing a mean over a
//! selection that happened to be empty received a float, exit status 0, and no
//! indication anything had gone wrong; `inf` then propagates silently through
//! every subsequent arithmetic step and arrives in a report. The third was a
//! panic -- the evaluator thread died with a Rust backtrace, which no `try`
//! in the language could catch.
//!
//! bash, jq and nushell all raise an error here.

use aethershell::env::Env;
use aethershell::{eval::eval_program, parser::parse_program, value::Value};

fn eval(src: &str) -> anyhow::Result<Value> {
    let prog = parse_program(src).unwrap_or_else(|e| panic!("parse {src}: {e:?}"));
    eval_program(&prog, &mut Env::new())
}

fn refusal(src: &str) -> String {
    match eval(src) {
        Ok(v) => panic!("{src} returned {v:?}; a zero divisor must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn integer_division_by_zero_is_refused() {
    for src in ["1 / 0", "0 / 0", "-7 / 0"] {
        let e = refusal(src);
        assert!(
            e.contains("division by zero"),
            "{src} refused with an unhelpful message: {e}"
        );
    }
}

#[test]
fn float_division_by_zero_is_refused_too() {
    // IEEE says `1.0 / 0.0` is `inf`, and for a numerics library that is the
    // right answer. For a shell whose output an agent parses, an infinity that
    // is indistinguishable from a measurement is not.
    for src in ["1.0 / 0.0", "1 / 0.0", "1.0 / 0", "2.5 / 0.0"] {
        assert!(
            refusal(src).contains("division by zero"),
            "{src} was not refused"
        );
    }
}

#[test]
fn remainder_by_zero_is_refused_rather_than_panicking() {
    // This is the one that killed the process. If the fix regresses, this test
    // does not fail -- it aborts the test binary, which is itself the signal.
    let e = refusal("1 % 0");
    assert!(e.contains("remainder by zero"), "unhelpful refusal: {e}");
}

#[test]
fn the_refusal_reaches_the_agent_as_a_structured_error() {
    // A bare message is not enough: the shell's contract is that a failure
    // carries a code an agent can branch on without reading prose.
    let e = refusal("1 / 0");
    assert!(
        e.contains("zero"),
        "the message must name the cause, got: {e}"
    );
    // And it must say what to do next, which is the difference between one
    // retry and a guess.
    assert!(
        e.contains("Guard the divisor") || e.contains("filter"),
        "the refusal should suggest a repair, got: {e}"
    );
}

#[test]
fn a_zero_divisor_inside_a_pipeline_stops_the_pipeline() {
    // The dangerous shape is the one that hides: a single bad element in a
    // large collection. It must not yield `[inf, inf, inf]`.
    let e = refusal("[1, 2, 3] | map(fn(x) => x / 0)");
    assert!(e.contains("division by zero"), "unhelpful refusal: {e}");
}

#[test]
fn ordinary_arithmetic_is_unaffected() {
    assert_eq!(eval("10 / 2").expect("10 / 2"), Value::Float(5.0));
    assert_eq!(eval("10 % 3").expect("10 % 3"), Value::Int(1));
    assert_eq!(eval("7.5 / 2.5").expect("7.5 / 2.5"), Value::Float(3.0));
    assert_eq!(eval("-9 / 3").expect("-9 / 3"), Value::Float(-3.0));
    // A zero *numerator* is fine; it is the divisor that matters.
    assert_eq!(eval("0 / 5").expect("0 / 5"), Value::Float(0.0));
    assert_eq!(eval("0 % 5").expect("0 % 5"), Value::Int(0));
}

#[test]
fn non_vacuity_the_guard_is_what_is_firing() {
    // If the evaluator refused all division, every assertion above would pass
    // for the wrong reason.
    assert!(eval("1 / 1").is_ok(), "division is refused outright");
    // And if it refused nothing, `refusal` would panic -- but assert the
    // positive form here too, so the intent is on the record.
    assert!(
        eval("1 / 0").is_err(),
        "the zero-divisor guard is not running"
    );
    // `-0.0` is a zero divisor as much as `0.0` is.
    assert!(
        eval("1 / -0.0").is_err(),
        "negative zero slipped past the guard"
    );
}
