//! The implicit row argument: `xs | where(.state == "open")`.
//!
//! In an argument of a call on the right of `|`, a `.field` with no receiver
//! is a field of the row, and the argument becomes `fn(__) => …` -- jq's
//! spelling of "this row", which is the input syntax models already write.
//! E1's default arm went from 442 to 331 tokens with it and `open`
//! (`benches/agentic/results/e1-query-corpus.md`).
//!
//! The scope is the point of these tests. It must not reach statements, the
//! left of a pipe, or explicit lambdas, where a leading `.` would otherwise
//! quietly mean something.

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

fn run(code: &str) -> Value {
    let stmts = parse_program(code).unwrap_or_else(|e| panic!("parse failed for {code}: {e}"));
    let mut env = Env::new();
    eval_program(&stmts, &mut env).unwrap_or_else(|e| panic!("eval failed for {code}: {e}"))
}

const ROWS: &str = r#"[{n: 1, ok: true, t: ["bug"]}, {n: 2, ok: false, t: []}, {n: 3, ok: true, t: ["x", "bug"]}]"#;

#[test]
fn a_field_predicate_filters_rows() {
    assert_eq!(run(&format!("{ROWS} | where(.ok) | len")), Value::Int(2));
    assert_eq!(
        run(&format!("{ROWS} | where(.n > 1 && .ok) |.n")),
        Value::Array(vec![Value::Int(3)])
    );
    assert_eq!(run(&format!("{ROWS} | where(!.ok) | len")), Value::Int(1));
}

#[test]
fn it_means_the_same_as_the_explicit_lambda() {
    for (short, long) in [
        ("where(.ok)", "where(fn(r) => r.ok)"),
        ("map(.n * 10)", "map(fn(r) => r.n * 10)"),
        ("where(.n >= 2)", "where(fn(r) => r.n >= 2)"),
    ] {
        assert_eq!(
            run(&format!("{ROWS} | {short}")),
            run(&format!("{ROWS} | {long}")),
            "{short} vs {long}"
        );
    }
}

#[test]
fn a_nested_call_sees_the_same_row() {
    // `any(.t, …)` is not given its own row: `.t` is the outer row's field,
    // as in jq, and the explicit lambda inside keeps its own parameter.
    assert_eq!(
        run(&format!(
            r#"{ROWS} | where(any(.t, fn(l) => l == "bug")) | len"#
        )),
        Value::Int(2)
    );
}

#[test]
fn it_does_not_reach_outside_a_pipeline_stage() {
    for code in [".n", "let x = .n", "fn(r) => .n"] {
        assert!(parse_program(code).is_err(), "{code} should not parse");
    }
    // Inside an explicit lambda the parameter names the row; `.n` there is
    // an error rather than a second, silent meaning.
    assert!(parse_program(&format!("{ROWS} | map(fn(r) => .n)")).is_err());
}
