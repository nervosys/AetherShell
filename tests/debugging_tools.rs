// tests/debugging_tools.rs
//! Tests for debugging tools: debug, assert, trace, type_assert, inspect

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

fn run(code: &str) -> Value {
    let stmts = parse_program(code).expect("parse failed");
    let mut env = Env::new();
    eval_program(&stmts, &mut env).expect(&format!("eval failed for: {}", code))
}

fn run_result(code: &str) -> Result<Value, String> {
    let stmts = parse_program(code).map_err(|e| e.to_string())?;
    let mut env = Env::new();
    eval_program(&stmts, &mut env).map_err(|e| e.to_string())
}

// ============================================================================
// debug() tests
// ============================================================================

#[test]
fn test_debug_returns_value() {
    // debug() should return the value unchanged for chaining
    let result = run("debug(42)");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_debug_string() {
    let result = run(r#"debug("hello")"#);
    assert_eq!(result, Value::Str("hello".to_string()));
}

#[test]
fn test_debug_array() {
    let result = run("debug([1, 2, 3])");
    match result {
        Value::Array(arr) => assert_eq!(arr.len(), 3),
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_debug_piped() {
    // debug can be used in pipelines
    let result = run("[1, 2, 3] | debug | first");
    assert_eq!(result, Value::Int(1));
}

#[test]
fn test_dbg_alias() {
    // dbg is an alias for debug
    let result = run("dbg(100)");
    assert_eq!(result, Value::Int(100));
}

// ============================================================================
// assert() tests
// ============================================================================

#[test]
fn test_assert_true_returns_true() {
    let result = run("assert(true)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_assert_truthy_int() {
    let result = run("assert(1)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_assert_false_returns_error() {
    let result = run("assert(false)");
    match result {
        Value::Error(msg) => assert_eq!(msg, "assertion failed"),
        _ => panic!("Expected Error, got {:?}", result),
    }
}

#[test]
fn test_assert_false_with_message() {
    let result = run(r#"assert(false, "custom message")"#);
    match result {
        Value::Error(msg) => assert_eq!(msg, "custom message"),
        _ => panic!("Expected Error, got {:?}", result),
    }
}

#[test]
fn test_assert_null_is_falsy() {
    let result = run("assert(null)");
    match result {
        Value::Error(_) => (), // Expected
        _ => panic!("Expected Error for null assertion"),
    }
}

#[test]
fn test_assert_empty_string_is_falsy() {
    let result = run(r#"assert("")"#);
    match result {
        Value::Error(_) => (), // Expected
        _ => panic!("Expected Error for empty string assertion"),
    }
}

#[test]
fn test_assert_nonempty_string_is_truthy() {
    let result = run(r#"assert("hello")"#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_assert_piped_condition() {
    // Piped: condition | assert("message")
    let result = run(r#"true | assert("should pass")"#);
    assert_eq!(result, Value::Bool(true));
}

// ============================================================================
// trace() tests
// ============================================================================

#[test]
fn test_trace_returns_value() {
    let result = run(r#"trace("label", 42)"#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_trace_single_arg() {
    // With one arg, uses default "trace" label
    let result = run("trace(100)");
    assert_eq!(result, Value::Int(100));
}

#[test]
fn test_trace_piped() {
    // Piped: value | trace("label")
    let result = run(r#"[1,2,3] | trace("my_array") | len"#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_trace_in_pipeline() {
    // Can be used at multiple points in pipeline
    let result = run(r#"
        [1, 2, 3, 4] 
        | trace("input") 
        | map(fn(x) => x * 2) 
        | trace("doubled")
        | first
    "#);
    assert_eq!(result, Value::Int(2));
}

// ============================================================================
// type_assert() tests
// ============================================================================

#[test]
fn test_type_assert_int_passes() {
    let result = run(r#"type_assert(42, "Int")"#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_type_assert_string_passes() {
    let result = run(r#"type_assert("hello", "String")"#);
    assert_eq!(result, Value::Str("hello".to_string()));
}

#[test]
fn test_type_assert_array_passes() {
    let result = run(r#"type_assert([1, 2], "Array")"#);
    match result {
        Value::Array(_) => (), // Expected
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_type_assert_wrong_type_returns_error() {
    let result = run(r#"type_assert(42, "String")"#);
    match result {
        Value::Error(msg) => assert!(msg.contains("expected String") && msg.contains("got Int")),
        _ => panic!("Expected Error, got {:?}", result),
    }
}

#[test]
fn test_type_assert_case_insensitive() {
    // Type names should be case-insensitive
    let result = run(r#"type_assert(42, "int")"#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_type_assert_piped() {
    let result = run(r#"[1, 2, 3] | type_assert("Array") | len"#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_assert_type_alias() {
    // assert_type is an alias for type_assert
    let result = run(r#"assert_type("hello", "String")"#);
    assert_eq!(result, Value::Str("hello".to_string()));
}

// ============================================================================
// inspect() tests
// ============================================================================

#[test]
fn test_inspect_int() {
    let result = run("inspect(42)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Int".to_string())));
            assert_eq!(rec.get("value"), Some(&Value::Int(42)));
            assert_eq!(rec.get("is_negative"), Some(&Value::Bool(false)));
            assert_eq!(rec.get("is_zero"), Some(&Value::Bool(false)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_negative_int() {
    let result = run("inspect(-5)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("is_negative"), Some(&Value::Bool(true)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_float() {
    let result = run("inspect(3.14)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Float".to_string())));
            assert_eq!(rec.get("is_nan"), Some(&Value::Bool(false)));
            assert_eq!(rec.get("is_infinite"), Some(&Value::Bool(false)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_string() {
    let result = run(r#"inspect("hello")"#);
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("String".to_string())));
            assert_eq!(rec.get("length"), Some(&Value::Int(5)));
            assert_eq!(rec.get("is_empty"), Some(&Value::Bool(false)));
            assert_eq!(rec.get("char_count"), Some(&Value::Int(5)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_empty_string() {
    let result = run(r#"inspect("")"#);
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("is_empty"), Some(&Value::Bool(true)));
            assert_eq!(rec.get("length"), Some(&Value::Int(0)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_array() {
    let result = run("inspect([1, 2, 3])");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Array".to_string())));
            assert_eq!(rec.get("length"), Some(&Value::Int(3)));
            assert_eq!(rec.get("is_empty"), Some(&Value::Bool(false)));
            assert_eq!(rec.get("first"), Some(&Value::Int(1)));
            assert_eq!(rec.get("last"), Some(&Value::Int(3)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_empty_array() {
    let result = run("inspect([])");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("is_empty"), Some(&Value::Bool(true)));
            assert_eq!(rec.get("length"), Some(&Value::Int(0)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_record() {
    let result = run(r#"inspect({a: 1, b: 2})"#);
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Record".to_string())));
            assert_eq!(rec.get("length"), Some(&Value::Int(2)));
            assert_eq!(rec.get("is_empty"), Some(&Value::Bool(false)));
            // keys should be present
            if let Some(Value::Array(keys)) = rec.get("keys") {
                assert_eq!(keys.len(), 2);
            } else {
                panic!("Expected keys array");
            }
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_lambda() {
    let result = run("inspect(fn(x) => x + 1)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Lambda".to_string())));
            assert_eq!(rec.get("param_count"), Some(&Value::Int(1)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_error() {
    let result = run(r#"inspect(throw "oops")"#);
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Error".to_string())));
            assert_eq!(rec.get("message"), Some(&Value::Str("oops".to_string())));
            assert_eq!(rec.get("is_error"), Some(&Value::Bool(true)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_null() {
    let result = run("inspect(null)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Null".to_string())));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_bool() {
    let result = run("inspect(true)");
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("type"), Some(&Value::Str("Bool".to_string())));
            assert_eq!(rec.get("value"), Some(&Value::Bool(true)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_inspect_piped() {
    // Can be used in pipeline
    let result = run(r#"42 | inspect | type_of"#);
    // inspect returns a Record
    assert_eq!(result, Value::Str("Record".to_string()));
}

// ============================================================================
// Integration tests - combining debugging tools
// ============================================================================

#[test]
fn test_debug_and_trace_in_pipeline() {
    let result = run(r#"
        [1, 2, 3]
        | debug
        | map(fn(x) => x * 2)
        | trace("doubled")
        | sum
    "#);
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_assert_after_transformation() {
    let result = run(r#"
        let arr = [1, 2, 3, 4, 5]
        let total = (arr | sum)
        assert(total > 10, "sum should be greater than 10")
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_type_assert_in_function() {
    let result = run(r#"
        let process = fn(x) => type_assert(x, "Int")
        process(42)
    "#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_inspect_mixed_array() {
    let result = run(r#"inspect([1, "two", true])"#);
    match result {
        Value::Record(rec) => {
            // Should have multiple element types
            if let Some(Value::Array(types)) = rec.get("element_types") {
                assert!(types.len() >= 2, "Should have multiple types");
            } else {
                panic!("Expected element_types array");
            }
        }
        _ => panic!("Expected Record"),
    }
}
