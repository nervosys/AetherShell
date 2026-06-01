//! Phase 5 (grammar-unification): terse agent-surface forms parsed by the real
//! grammar rather than a text pre-pass. First slice: `|.field` projection.

use aethershell::value::Value;

fn eval_str(src: &str) -> Value {
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let mut env = aethershell::env::Env::new();
    aethershell::eval::eval_program(&stmts, &mut env).expect("eval")
}

#[test]
fn pipe_field_projection() {
    // `|.name` desugars in-grammar to `| map(fn(__) => __.name)`.
    let r = eval_str(r#"[{name: "a", size: 1}, {name: "b", size: 2}] |.name"#);
    assert_eq!(
        r,
        Value::Array(vec![Value::Str("a".into()), Value::Str("b".into())])
    );
}

#[test]
fn pipe_field_projection_chains() {
    // Nested field chains: `|.a.b`.
    let r = eval_str(r#"[{a: {b: 5}}, {a: {b: 6}}] |.a.b"#);
    assert_eq!(r, Value::Array(vec![Value::Int(5), Value::Int(6)]));
}

#[test]
fn si_numeric_suffixes() {
    assert_eq!(eval_str("1k"), Value::Int(1000));
    assert_eq!(eval_str("5M"), Value::Int(5_000_000));
    assert_eq!(eval_str("2G"), Value::Int(2_000_000_000));
    // Composes in expressions.
    assert_eq!(eval_str("1k + 5"), Value::Int(1005));
    // A trailing alphanumeric means it's NOT an SI suffix (stays separate).
    // `10` followed by identifier `kb` would not be a single scaled literal.
}

#[test]
fn legible_forms_are_native() {
    // Forms the `.aeg` transpiler rewrites are already handled by the real
    // grammar, so the .ae surface needs no pre-pass for them — locking this in
    // makes eventual transpiler-thinning safe.
    assert_eq!(
        eval_str("'single quoted'"),
        Value::Str("single quoted".into())
    );
    assert_eq!(eval_str("x := 5; x"), Value::Int(5)); // := mut binding
    assert_eq!(eval_str("true"), Value::Bool(true));
    assert_eq!(eval_str("null"), Value::Null);
}

#[test]
fn if_expression() {
    // `if cond { then } else { else }` desugars to a boolean match.
    assert_eq!(
        eval_str(r#"if 1 < 2 { "yes" } else { "no" }"#),
        Value::Str("yes".into())
    );
    assert_eq!(
        eval_str(r#"if 2 < 1 { "yes" } else { "no" }"#),
        Value::Str("no".into())
    );
    // Missing else yields null.
    assert_eq!(eval_str(r#"if 2 < 1 { "yes" }"#), Value::Null);
    // Usable as a value in a binding.
    assert_eq!(
        eval_str(r#"let x = if true { 10 } else { 20 }; x"#),
        Value::Int(10)
    );
}

#[test]
fn question_match_prefix() {
    // `?x { arms }` is sugar for `match x { arms }`.
    assert_eq!(
        eval_str(r#"?2 { 1 => "a", 2 => "b", _ => "z" }"#),
        Value::Str("b".into())
    );
    assert_eq!(
        eval_str(r#"?5 { 1 => "a", _ => "z" }"#),
        Value::Str("z".into())
    );
}

#[test]
fn tilde_lambda_in_pipeline() {
    // `~x: body` explicit-param lambda, used as a builtin argument.
    assert_eq!(
        eval_str("[1, 2, 3] | map(~x: x * 2)"),
        Value::Array(vec![Value::Int(2), Value::Int(4), Value::Int(6)])
    );
    assert_eq!(
        eval_str("[1, 2, 3, 4] | where(~x: x > 2)"),
        Value::Array(vec![Value::Int(3), Value::Int(4)])
    );
    // Multi-parameter lambda (reduce); the `~a, b` comma is the param separator.
    assert_eq!(
        eval_str("[1, 2, 3, 4] | reduce(~a, b: a + b, 0)"),
        Value::Int(10)
    );
}

#[test]
fn tilde_lambda_body_is_bounded_by_pipe() {
    // The lambda body stops at the next `|`; the projection applies afterward.
    assert_eq!(
        eval_str(r#"[{n: 3}, {n: 1}] | where(~x: x.n > 2) |.n"#),
        Value::Array(vec![Value::Int(3)])
    );
}

#[test]
fn pipe_field_projection_composes_with_other_stages() {
    // Projection participates in a longer pipeline.
    let r = eval_str(
        r#"[{name: "a", size: 5}, {name: "b", size: 1}] | where(fn(x) => x.size > 2) |.name"#,
    );
    assert_eq!(r, Value::Array(vec![Value::Str("a".into())]));
}
