use aethershell::{env::Env, eval::eval_program, parser::parse_program, value::Value};

fn run(code: &str, env: &mut Env) -> anyhow::Result<Value> {
    eval_program(&parse_program(code)?, env)
}

#[test]
fn nested_field_reads_leave_the_source_intact() {
    let mut env = Env::new();
    let result = run(
        "r = {meta: {n: 7}, payload: [1, 2, 3]}; x = r.meta.n; [x, r.meta.n, len(r.payload)]",
        &mut env,
    )
    .unwrap();
    assert_eq!(
        result,
        Value::Array(vec![Value::Int(7), Value::Int(7), Value::Int(3)])
    );
}

#[test]
fn computed_records_and_field_errors_keep_their_behavior() {
    let mut env = Env::new();
    assert_eq!(
        run("(fn(x) => {nested: {n: x}})(9).nested.n", &mut env).unwrap(),
        Value::Int(9)
    );
    for code in [
        "r = {nested: {n: 1}}; r.nested.missing",
        "r = {nested: 1}; r.nested.n",
        "missing.n",
    ] {
        let mut env = Env::new();
        let error = run(code, &mut env).unwrap_err();
        assert!(
            error
                .downcast_ref::<aethershell::safety::SafetyError>()
                .is_some(),
            "{error}"
        );
    }
}

#[test]
fn reductions_move_arrays_and_support_both_arities() {
    let mut env = Env::new();
    assert_eq!(
        run(
            "[[1], [2], [3]] | reduce(fn(a, x) => concat(a, x), [])",
            &mut env
        )
        .unwrap(),
        Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
    assert_eq!(
        run(
            "[10, 20, 30] | reduce(fn(a, x, i) => a + x + i, 0)",
            &mut env
        )
        .unwrap(),
        Value::Int(63)
    );
    assert_eq!(
        run("reduce([1, 2, 3], fn(a, x) => a + x, 4)", &mut env).unwrap(),
        Value::Int(10)
    );
    assert_eq!(
        run("[] | reduce(fn(a, x) => a + x, 4)", &mut env).unwrap(),
        Value::Int(4)
    );
}

#[test]
fn indexed_reduce_body_error_is_not_replaced_by_a_retry_error() {
    let mut env = Env::new();
    run("a = 42; x = 43; i = 44", &mut env).unwrap();
    let error = run("[1] | reduce(fn(a, x, i) => nope(a), 0)", &mut env).unwrap_err();
    assert!(error.to_string().contains("nope"), "{error}");
    assert_eq!(env.get_var("a"), Some(&Value::Int(42)));
    assert_eq!(env.get_var("x"), Some(&Value::Int(43)));
    assert_eq!(env.get_var("i"), Some(&Value::Int(44)));
}

#[test]
fn reused_parameter_slots_restore_nested_and_shadowed_bindings() {
    let mut env = Env::new();
    run("x = 99; i = 98", &mut env).unwrap();
    let result = run(
        "[1, 2] | map(fn(x, i) => ([10, 20] | map(fn(x) => x + 1) | sum) + x + i)",
        &mut env,
    )
    .unwrap();
    assert_eq!(result, Value::Array(vec![Value::Int(33), Value::Int(35)]));
    assert_eq!(env.get_var("x"), Some(&Value::Int(99)));
    assert_eq!(env.get_var("i"), Some(&Value::Int(98)));
    run("[1, 2] | where(fn(row) => row > 1)", &mut env).unwrap();
    assert!(env.get_var("row").is_none());
}

#[test]
fn reused_parameter_slots_restore_after_a_body_failure() {
    for code in [
        "[1, 2] | map(fn(x, row) => nope(x))",
        "[1, 2] | where(fn(x, row) => nope(x))",
        "[1, 2] | reduce(fn(x, row) => nope(x), 0)",
    ] {
        let mut env = Env::new();
        run("x = 99", &mut env).unwrap();
        assert!(run(code, &mut env).is_err());
        assert_eq!(env.get_var("x"), Some(&Value::Int(99)));
        assert!(env.get_var("row").is_none());
        assert!(env.input().is_none());
    }
}

#[test]
fn closures_capture_each_iteration_before_the_slot_is_reused() {
    let mut env = Env::new();
    assert_eq!(
        run(
            "closures = [1, 2, 3] | map(fn(x) => fn(y) => x + y); closures | map(fn(f) => f(10))",
            &mut env,
        )
        .unwrap(),
        Value::Array(vec![Value::Int(11), Value::Int(12), Value::Int(13)])
    );
}
