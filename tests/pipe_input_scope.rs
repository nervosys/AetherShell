//! A pipeline stage's input belongs to that stage's call, not to calls nested in
//! its arguments.
//!
//! The call arm evaluated its arguments while the stage's input was still set,
//! so `"hello" | slice(1, len("hello"))` ran `len` with "hello" piped in *and*
//! as its argument, and the declaration refused it. `lib/string.ae`'s
//! `capitalize` failed this way, which made `lib/test_stdlib.ae` stop halfway.

use aethershell::env::Env;
use aethershell::value::Value;

fn run(src: &str) -> anyhow::Result<Value> {
    let stmts = aethershell::parser::parse_program(src)?;
    aethershell::eval::eval_program(&stmts, &mut Env::new())
}

#[test]
fn a_call_in_a_stage_argument_does_not_receive_the_stage_input() {
    assert_eq!(
        run(r#""hello" | slice(1, len("hello"))"#).expect("nested len"),
        Value::Str("ello".into())
    );
    assert_eq!(
        run("[1, 2, 3] | take(len([9]))").expect("nested len in take"),
        Value::Array(vec![Value::Int(1)])
    );
}

#[test]
fn the_stdlib_capitalize_shape_works() {
    let src = r#"
        let cap = fn(s) => match len(s) {
            0 => "",
            _ => upper(s | slice(0, 1)) + lower(s | slice(1, len(s)))
        }
        cap("hELLO")
    "#;
    assert_eq!(run(src).expect("capitalize"), Value::Str("Hello".into()));
}

#[test]
fn the_stage_still_receives_its_input() {
    // The fix must not take the input away from the stage itself.
    assert_eq!(run(r#""ab" | upper()"#).expect("upper"), Value::Str("AB".into()));
    assert_eq!(
        run("[3, 1, 2] | take(2)").expect("take"),
        Value::Array(vec![Value::Int(3), Value::Int(1)])
    );
    // A `.field` argument is a lambda over the row and still sees it.
    assert_eq!(
        run("[{n: 1}, {n: 5}] | where(.n > 2) | len").expect("where"),
        Value::Int(1)
    );
}

#[test]
fn an_error_in_an_argument_leaves_the_outer_input_intact() {
    // A failing argument must restore the input it took, so an enclosing stage
    // that catches the error still sees its own row.
    let src = r#""outer" | (try { slice(1, missing_fn()) } catch e { "caught" })"#;
    assert_eq!(run(src).expect("caught"), Value::Str("caught".into()));
}
