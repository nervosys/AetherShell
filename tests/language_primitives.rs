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

// ── Closures (AS-2026-13) ────────────────────────────────────────────────

/// The finding: a lambda closed over nothing, so the inner lambda of a curried
/// function lost the outer parameter the moment the outer call returned.
#[test]
fn a_returned_lambda_captures_the_enclosing_parameter() {
    let out = run("let mk = fn(factor) => fn(x) => x * factor\nlet times3 = mk(3)\ntimes3(2)")
        .expect("currying should work");
    assert_eq!(out, Value::Int(6));
}

/// The shape that made this Medium rather than informational: `Mul` rejects
/// `Null` loudly, but concatenation accepted it and produced a wrong answer
/// with no error at all.
#[test]
fn the_silent_failure_shape_is_gone() {
    assert_eq!(
        s("let mk = fn(f) => fn(x) => \"v: \" + f\nmk(\"A\")(1)"),
        "v: A"
    );
}

#[test]
fn capture_does_not_shadow_a_parameter_of_the_same_name() {
    // A parameter must win over a captured binding, or a lambda could not
    // shadow a name it also closes over.
    let out = run("let x = 100\nlet f = fn(x) => x + 1\nf(1)").expect("eval");
    assert_eq!(out, Value::Int(2));
}

#[test]
fn capture_does_not_leak_into_the_caller() {
    // Captured names are installed for the call and restored afterwards.
    let out = run("let v = 1\nlet f = fn(q) => v\nlet _ = f(0)\nv").expect("eval");
    assert_eq!(out, Value::Int(1));
}

/// Backwards compatibility: a lambda referring to a binding that does not exist
/// yet keeps resolving dynamically at call time. Only names already bound are
/// captured, so this long-standing behaviour is untouched.
#[test]
fn a_binding_introduced_after_the_lambda_still_resolves() {
    let out = run("let f = fn(x) => x * later\nlet later = 4\nf(2)").expect("eval");
    assert_eq!(out, Value::Int(8));
}

#[test]
fn async_lambdas_capture_too() {
    let out = run("let mk = fn(k) => async fn(x) => x + k\nlet add5 = mk(5)\nawait add5(1)")
        .expect("async currying should work");
    assert_eq!(out, Value::Int(6));
}

// ── The string size bound (AS-2026-10) ───────────────────────────────────

/// The cap used to sit only on `*`, which made it a speed bump: `a + a`
/// doubled straight past the number the constant appeared to promise.
#[test]
fn concatenation_cannot_exceed_the_string_limit() {
    let err = run("let a = \"x\" * 8000000\na + a")
        .expect_err("16 MB via concatenation should be refused")
        .to_string();
    assert!(err.contains("limit"), "unexpected error: {err}");
}

#[test]
fn a_single_byte_over_the_limit_is_refused() {
    let err = run("let a = \"x\" * 8388608\na + \"y\"")
        .expect_err("one byte over should be refused")
        .to_string();
    assert!(err.contains("limit"), "unexpected error: {err}");
}

#[test]
fn ordinary_string_work_is_unaffected() {
    assert_eq!(s("\"a\" + \"b\""), "ab");
    assert_eq!(s("\"=\" * 10"), "==========");
    assert_eq!(s("\"n: \" + 1"), "n: 1");
}

// ── Capture must not change existing semantics (AS-2026-14) ──────────────

/// Capture is by value, so a mutable binding must *not* be captured: doing so
/// would make a later assignment invisible to the lambda, silently changing a
/// behaviour scripts already rely on.
#[test]
fn a_mutable_binding_is_not_snapshotted() {
    let out = run("let mut k = 1\nlet f = fn(q) => k\nk = 2\nf(0)").expect("eval");
    assert_eq!(
        out,
        Value::Int(2),
        "the lambda saw a stale copy of a `let mut` binding"
    );
}

#[test]
fn an_immutable_binding_is_still_captured() {
    let out = run("let k = 1\nlet f = fn(q) => k\nf(0)").expect("eval");
    assert_eq!(out, Value::Int(1));
}

/// Lambda parameters are immutable from the user's point of view, so they must
/// be captured — that is the whole of currying. They are bound internally with
/// `set_var_unchecked`, which marks them mutable for bookkeeping, so capture
/// has to consult user-declared mutability rather than that flag.
#[test]
fn a_parameter_is_captured_even_though_it_is_internally_mutable() {
    let out = run("let mk = fn(k) => fn(x) => x * k\nmk(3)(2)").expect("eval");
    assert_eq!(out, Value::Int(6));
}

// ── The catch variable actually binds (AS-2026-15) ───────────────────────

/// `catch e` used `set_var`, which refuses to overwrite an immutable binding
/// and whose error was discarded. With any variable of the same name already
/// in scope the handler silently read the *old* value instead of the error.
#[test]
fn catch_binds_the_error_even_when_the_name_is_taken() {
    let out = run("let e = \"outer\"\ntry { throw \"boom\" } catch e { e }").expect("eval");
    assert_eq!(
        out,
        Value::Str("boom".into()),
        "the handler saw a stale value instead of the error"
    );
}

#[test]
fn catch_restores_the_previous_binding() {
    let out =
        run("let e = \"outer\"\nlet _ = try { throw \"boom\" } catch e { e }\ne").expect("eval");
    assert_eq!(out, Value::Str("outer".into()));
}

#[test]
fn catch_still_works_with_no_outer_binding() {
    let out = run("try { throw \"boom\" } catch e { e }").expect("eval");
    assert_eq!(out, Value::Str("boom".into()));
}
