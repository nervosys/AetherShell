use aether_shell::typecheck;
use aether_shell::types::Type;

#[test]
fn literals_basic() {
    let t_int = typecheck::infer_last_type("1").unwrap();
    assert_eq!(t_int.name(), "Int");

    let t_float = typecheck::infer_last_type("1.5").unwrap();
    assert_eq!(t_float.name(), "Float");

    let t_bool = typecheck::infer_last_type("true").unwrap();
    assert_eq!(t_bool.name(), "Bool");

    let t_str = typecheck::infer_last_type(r#""hello""#).unwrap();
    assert_eq!(t_str.name(), "String");

    let t_uri = typecheck::infer_last_type(r#""s3://bucket/key""#).unwrap();
    assert_eq!(t_uri.name(), "Uri");
}

#[test]
fn array_element_promotion() {
    // [1, 2.0] -> Array<Float>
    let t = typecheck::infer_last_type("[1, 2.0]").unwrap();
    assert_eq!(t.name(), "Array<Float>");
}

#[test]
fn lambda_and_map_pipeline() {
    // [1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0) -> Int
    let t = typecheck::infer_last_type(r#"[1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)"#)
        .unwrap();
    assert_eq!(t.name(), "Int");
}

#[test]
fn boolean_ops_and_comparisons() {
    let t = typecheck::infer_last_type("3 > 2 && 5 == 5").unwrap();
    assert_eq!(t.name(), "Bool");
}

#[test]
fn bound_ident_in_env() {
    let env = typecheck::typecheck_program(
        r#"
        let x = 41;
        x + 1
        "#,
    )
    .unwrap();
    let xty = env.get("x").expect("x should be bound");
    assert_eq!(xty.name(), "Int");
}

#[test]
fn lambda_value_called_explicitly() {
    // let f = fn(x)=> x+1; f(3) -> Int
    let t = typecheck::infer_last_type(
        r#"
        let f = fn(x)=> x + 1;
        f(3)
        "#,
    )
    .unwrap();
    assert_eq!(t.name(), "Int");
}

#[test]
fn where_preserves_array_type() {
    // where([1,2,3], fn(x)=> x>1) -> Array<Int>
    let t = typecheck::infer_last_type(r#" where([1,2,3], fn(x)=> x > 1) "#).unwrap();
    assert_eq!(t.name(), "Array<Int>");
}

#[test]
fn http_get_record_shape() {
    // http_get("https://example.com") -> Record{url:String, status:Int, headers:Record{* : String}, body:Any}
    let t = typecheck::infer_last_type(r#" http_get("https://example.com") "#).unwrap();

    // Coarse checks on the shape:
    match t {
        Type::Record(mut m) => {
            assert!(m.remove("url").is_some(), "should have url");
            assert!(m.remove("status").is_some(), "should have status");
            assert!(m.remove("headers").is_some(), "should have headers");
            assert!(m.remove("body").is_some(), "should have body");
        }
        other => panic!("expected Record, got {:?}", other),
    }
}

#[test]
fn unbound_identifier_is_error() {
    // Using an unknown variable should surface an error during inference
    let err = typecheck::infer_last_type("y + 1").unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("unbound") || msg.contains("identifier"),
        "err was: {msg}"
    );
}

#[test]
fn uri_detection_various_schemes() {
    let t1 = typecheck::infer_last_type(r#""mailto:someone@example.com""#).unwrap();
    assert_eq!(t1.name(), "Uri");

    let t2 = typecheck::infer_last_type(r#""data:text/plain;base64,SGVsbG8=""#).unwrap();
    assert_eq!(t2.name(), "Uri");

    let t3 = typecheck::infer_last_type(r#""custom+scheme-1.2:stuff""#).unwrap();
    assert_eq!(t3.name(), "Uri");
}
