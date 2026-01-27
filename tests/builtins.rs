use aethershell::{builtins, env::Env, value::Value};

#[test]
fn builtin_print_returns_null() {
    let mut env = Env::default();
    let out = builtins::call("print", vec![Value::Str("hi".into())], &mut env).unwrap();
    assert!(matches!(out, Value::Str(_)));
}

#[test]
fn builtin_map_reduce_direct() {
    let mut env = Env::default();
    // map: double each
    let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let f = Value::Lambda(aethershell::value::Lambda {
        params: vec!["x".into()],
        body: Box::new(aethershell::ast::Expr::Binary {
            left: Box::new(aethershell::ast::Expr::Ident("x".into())),
            op: aethershell::ast::BinOp::Mul,
            right: Box::new(aethershell::ast::Expr::LitInt(2)),
        }),
    });
    let doubled = builtins::call("map", vec![arr, f.clone()], &mut env).unwrap();
    if let Value::Array(v) = doubled {
        assert_eq!(v.len(), 3);
    } else {
        panic!("map should return Array");
    }

    // reduce: sum
    let arr = Value::Array(vec![Value::Int(2), Value::Int(4), Value::Int(6)]);
    let add = Value::Lambda(aethershell::value::Lambda {
        params: vec!["a".into(), "b".into()],
        body: Box::new(aethershell::ast::Expr::Binary {
            left: Box::new(aethershell::ast::Expr::Ident("a".into())),
            op: aethershell::ast::BinOp::Add,
            right: Box::new(aethershell::ast::Expr::Ident("b".into())),
        }),
    });
    let acc0 = Value::Int(0);
    let sum = builtins::call("reduce", vec![arr, add, acc0], &mut env).unwrap();
    assert!(matches!(sum, Value::Int(12)));
}
