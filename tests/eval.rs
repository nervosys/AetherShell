use aether_shell::{env::Env, eval, parser, value::Value};

fn eval_code(code: &str) -> Value {
    let mut env = Env::default();
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap()
}

#[test]
fn map_reduce_pipeline_double() {
    let code = r#"[1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)"#;
    let out = eval_code(code);
    assert!(
        matches!(out, Value::Int(12)) || matches!(out, Value::Float(f) if (f - 12.0).abs() < 1e-9)
    );
}

#[test]
fn map_reduce_pipeline() {
    let code = r#"[1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)"#;
    let out = eval_code(code);
    match out {
        Value::Int(12) => {}
        Value::Float(f) if (f - 12.0).abs() < 1e-9 => {}
        other => panic!("expected 12, got {other}"),
    }
}

#[test]
fn pipe_sugar_ident() {
    // `| print` sugar should work and return a string (print returns string)
    let code = r#"[1,2] | print"#;
    let out = eval_code(code);
    assert!(matches!(out, Value::Str(_)));
}

#[test]
fn string_concat_and_compare() {
    let code = r#""au" + "rora" == "aurora""#;
    let out = eval_code(code);
    assert!(matches!(out, Value::Bool(true)));
}

#[test]
fn logical_ops() {
    let code = r#"true && false || true"#;
    let out = eval_code(code);
    assert!(matches!(out, Value::Bool(true)));
}

#[test]
fn user_lambda_shadowing_builtin_is_ok() {
    // Env contains a non-function named `map` — evaluator should fall back to builtin
    let code = r#"let map = 42; [1,2] | map(fn(x)=>x+1) | reduce(fn(a,b)=>a+b, 0)"#;
    let out = eval_code(code);
    assert!(matches!(out, Value::Int(5)));
}
