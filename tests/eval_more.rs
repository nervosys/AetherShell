use aethershell::{env::Env, eval, parser, value::Value};

fn run(code: &str) -> Value {
    let mut env = Env::default();
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap()
}

#[test]
fn prefers_user_lambda_over_builtin() {
    // Shadow `map` with a lambda that internally calls the builtin "map"
    let code = r#"
      let map = fn(xs)=> call("map", xs, fn(x)=> x + 10);
      [1,2,3] | map
    "#;
    let out = run(code);
    match out {
        Value::Array(v) => assert_eq!(v.len(), 3),
        _ => panic!("expected array"),
    }
}

#[test]
fn falls_back_to_builtin_when_ident_is_non_lambda() {
    // Bind `map` to a non-function; pipeline must still use builtin map
    let code = r#"
      let map = 42;
      [1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)
    "#;
    let out = run(code);
    assert!(matches!(out, Value::Int(12)));
}

#[test]
fn pipeline_ident_sugar_still_calls_builtin() {
    let out = run(r#"[1,2] | print"#);
    assert!(matches!(out, Value::Str(_)));
}

#[test]
fn division_promotes_to_float() {
    let out = run("3 / 2");
    match out {
        Value::Float(f) => assert!((f - 1.5).abs() < 1e-9),
        _ => panic!("expected float"),
    }
}
