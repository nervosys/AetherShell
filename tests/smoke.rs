use aurora_shell::typecheck;

#[test]
fn infer_last_type_for_pipeline() {
    let ty = typecheck::infer_last_type(r#"[1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)"#)
        .unwrap();
    assert_eq!(ty.name(), "Int");
}

#[test]
fn env_has_binding_type() {
    let env = typecheck::typecheck_program(
        r#"
        let x = 1;
        x + 2
        "#,
    )
    .unwrap();
    let x_ty = env.get("x").expect("x in env");
    assert_eq!(x_ty.name(), "Int");
}

#[test]
fn compares_to_bool() {
    let ty = typecheck::infer_last_type(r#" 3 > 2 && 5 == 5 "#).unwrap();
    assert_eq!(ty.name(), "Bool");
}

#[test]
fn uri_literal_is_uri_type() {
    let ty = typecheck::infer_last_type(r#""https://example.com""#).unwrap();
    assert_eq!(ty.name(), "Uri");
}
