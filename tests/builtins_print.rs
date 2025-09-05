use aurora_shell::{env::Env, eval, parser, value::Value};

fn eval_code(code: &str) -> Value {
    let mut env = Env::default();
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap()
}

// helper removed: tests assert on returned Str directly now

#[test]
fn print_function() {
    let code = r#"print("hi")"#;
    let out = eval_code(code);
    // The returned Value should be a Str whose contents are the pretty
    // inline display of the original target (including surrounding quotes).
    match out {
        Value::Str(s) => assert_eq!(s, "\"hi\""),
        other => panic!("expected Str, got {:?}", other),
    }
}

#[test]
fn print_spaced() {
    let code = r#"print "hi""#;
    let out = eval_code(code);
    match out {
        Value::Str(s) => assert_eq!(s, "\"hi\""),
        other => panic!("expected Str, got {:?}", other),
    }
}

#[test]
fn print_piped() {
    let code = r#""hi" | print"#;
    let out = eval_code(code);
    match out {
        Value::Str(s) => assert_eq!(s, "\"hi\""),
        other => panic!("expected Str, got {:?}", other),
    }
}
