// tests/zero_param_lambda.rs
//! Tests for zero-parameter lambda support

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

fn run(code: &str) -> Value {
    let stmts = parse_program(code).expect("parse failed");
    let mut env = Env::new();
    eval_program(&stmts, &mut env).expect(&format!("eval failed for: {}", code))
}

// ============================================================================
// Basic zero-parameter lambda tests
// ============================================================================

#[test]
fn test_zero_param_simple() {
    let result = run("f = fn() => 42; f()");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_zero_param_string() {
    let result = run(r#"f = fn() => "hello"; f()"#);
    assert_eq!(result, Value::Str("hello".to_string()));
}

#[test]
fn test_zero_param_array() {
    let result = run("f = fn() => [1,2,3]; f()");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Int(1));
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_zero_param_record() {
    let result = run("f = fn() => { a: 1, b: 2 }; f()");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("a"), Some(&Value::Int(1)));
            assert_eq!(rec.get("b"), Some(&Value::Int(2)));
        }
        _ => panic!("Expected Record"),
    }
}

// ============================================================================
// Closure tests (capturing environment)
// ============================================================================

#[test]
fn test_zero_param_closure_capture() {
    let result = run("x = 10; f = fn() => x; f()");
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_zero_param_closure_capture_expression() {
    let result = run("x = 5; y = 3; f = fn() => x + y; f()");
    assert_eq!(result, Value::Int(8));
}

#[test]
fn test_zero_param_multiple_closures() {
    let result = run(r#"
        a = fn() => 1;
        b = fn() => 2;
        c = fn() => 3;
        a() + b() + c()
    "#);
    assert_eq!(result, Value::Int(6));
}

// ============================================================================
// Thunk pattern (deferred evaluation)
// ============================================================================

#[test]
fn test_zero_param_thunk_pattern() {
    let result = run(r#"
        expensive = fn() => 100 * 100;
        expensive()
    "#);
    assert_eq!(result, Value::Int(10000));
}

#[test]
fn test_zero_param_factory_pattern() {
    let result = run(r#"
        make_counter = fn() => 0;
        c1 = make_counter();
        c2 = make_counter();
        c1 + c2
    "#);
    assert_eq!(result, Value::Int(0));
}

// ============================================================================
// With expressions in body
// ============================================================================

#[test]
fn test_zero_param_with_pipeline() {
    let result = run(r#"
        get_data = fn() => [1,2,3] | map(fn(x) => x * 2);
        get_data() | sum
    "#);
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_zero_param_with_match() {
    let result = run(r#"
        toggle = fn() => match true { true => "on", _ => "off" };
        toggle()
    "#);
    assert_eq!(result, Value::Str("on".to_string()));
}

// ============================================================================
// Composition with other features
// ============================================================================

#[test]
fn test_zero_param_in_array() {
    let result = run(r#"
        fns = [fn() => 1, fn() => 2, fn() => 3];
        fns | map(fn(f) => f()) | sum
    "#);
    assert_eq!(result, Value::Int(6));
}

#[test]
fn test_zero_param_immediate_call() {
    // IIFE - Immediately Invoked Function Expression
    let result = run("(fn() => 42)()");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_zero_param_as_value() {
    let result = run(r#"
        get_fn = fn() => fn() => "inner";
        inner = get_fn();
        inner()
    "#);
    assert_eq!(result, Value::Str("inner".to_string()));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_zero_param_called_multiple_times() {
    let result = run(r#"
        f = fn() => 1;
        f() + f() + f()
    "#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_zero_param_with_null() {
    let result = run("f = fn() => null; f()");
    assert_eq!(result, Value::Null);
}

#[test]
fn test_zero_param_with_bool() {
    let result = run("f = fn() => true; f()");
    assert_eq!(result, Value::Bool(true));
}
