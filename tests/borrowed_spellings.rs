//! Spellings borrowed from the languages models already write.
//!
//! A local E7 run (`benches/agentic/results/e7-local-llama3.2-3b.md`) scored
//! the AetherShell arm 0/10, and the failures were habits rather than
//! misunderstandings: `and`/`not` for `&&`/`!`, `fn(x) x.a` without the
//! arrow, `from_json("issues.json")`. The first two now parse as meant; the
//! third stays an error with a mechanical repair in its hint. Keyword field
//! names (`r.from`, `{match: 1}`) were a parse error before any of this and
//! are fixed with it, since `and`/`or`/`not` would otherwise have joined them.

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

fn run(code: &str) -> Value {
    let stmts = parse_program(code).unwrap_or_else(|e| panic!("parse failed for {code}: {e}"));
    let mut env = Env::new();
    eval_program(&stmts, &mut env).unwrap_or_else(|e| panic!("eval failed for {code}: {e}"))
}

#[test]
fn word_operators_mean_the_symbols() {
    for (words, symbols) in [
        ("true and false", "true && false"),
        ("true or false", "true || false"),
        ("not true", "!true"),
        (
            "not false and (1 > 0 or false)",
            "!false && (1 > 0 || false)",
        ),
    ] {
        assert_eq!(run(words), run(symbols), "{words}");
    }
}

#[test]
fn word_not_has_pythons_precedence_and_stays_a_name_elsewhere() {
    // Looser than a comparison, as in Python and SQL: not (1 > 5).
    assert_eq!(run("not 1 > 5"), Value::Bool(true));
    assert_eq!(
        run("[{b: 9}, {b: 1}] | where(not .b > 5) | len"),
        Value::Int(1)
    );
    // The prelude binds `not` as a function; binding and calling it still work.
    assert_eq!(run("let not = fn(x) => !x\nnot(false)"), Value::Bool(true));
}

#[test]
fn a_lambda_may_leave_out_the_arrow() {
    assert_eq!(
        run("[{a: 1}, {a: 2}] | map(fn(x) x.a * 10)"),
        run("[{a: 1}, {a: 2}] | map(fn(x) => x.a * 10)")
    );
}

#[test]
fn keyword_named_fields_are_ordinary_fields() {
    assert_eq!(
        run("let r = {from: 1, match: 2, and: 3, not: 4}\nr.from + r.match + r.and + r.not"),
        Value::Int(10)
    );
    assert_eq!(
        run("[{type: 1}, {type: 2}] |.type"),
        Value::Array(vec![Value::Int(1), Value::Int(2)])
    );
}

#[test]
fn from_json_of_a_path_names_the_repair() {
    let err = aethershell::builtins::call(
        "from_json",
        vec![Value::Str("Cargo.toml".into())],
        &mut Env::new(),
    )
    .unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("open("),
        "the hint should name open(): {text}"
    );
}
