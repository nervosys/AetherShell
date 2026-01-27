// tests/builtin_consistent_syntax.rs
//! Tests for consistent builtin call syntax (both pipeline and function call)

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
// flatten - both pipeline and function call
// ============================================================================

#[test]
fn test_flatten_pipeline() {
    let result = run("[[1,2],[3,4]] | flatten");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 4);
            assert_eq!(
                arr,
                vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]
            );
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_flatten_function_call() {
    let result = run("flatten([[1,2],[3,4]])");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 4);
            assert_eq!(
                arr,
                vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]
            );
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_flatten_nested() {
    let result = run("flatten([[[1]],[[2]]])");
    match result {
        Value::Array(arr) => {
            // flatten is shallow, so we get [[1], [2]]
            assert_eq!(arr.len(), 2);
        }
        _ => panic!("Expected Array"),
    }
}

// ============================================================================
// slice - both pipeline and function call
// ============================================================================

#[test]
fn test_slice_pipeline() {
    let result = run("[1,2,3,4,5] | slice(1, 3)");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(2), Value::Int(3)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_slice_function_call() {
    let result = run("slice([1,2,3,4,5], 1, 3)");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(2), Value::Int(3)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_slice_string_pipeline() {
    let result = run(r#""hello" | slice(1, 4)"#);
    assert_eq!(result, Value::Str("ell".to_string()));
}

#[test]
fn test_slice_string_function_call() {
    let result = run(r#"slice("hello", 1, 4)"#);
    assert_eq!(result, Value::Str("ell".to_string()));
}

#[test]
fn test_slice_to_end_pipeline() {
    let result = run("[1,2,3,4,5] | slice(2)");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(3), Value::Int(4), Value::Int(5)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_slice_to_end_function_call() {
    let result = run("slice([1,2,3,4,5], 2)");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(3), Value::Int(4), Value::Int(5)]);
        }
        _ => panic!("Expected Array"),
    }
}

// ============================================================================
// any - both pipeline and function call
// ============================================================================

#[test]
fn test_any_pipeline_with_predicate() {
    let result = run("[1,2,3] | any(fn(x) => x > 2)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_any_function_call_with_predicate() {
    let result = run("any([1,2,3], fn(x) => x > 2)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_any_pipeline_no_predicate() {
    let result = run("[false, true, false] | any");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_any_function_call_no_predicate() {
    let result = run("any([false, true, false])");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_any_all_false() {
    let result = run("any([false, false, false])");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_any_predicate_none_match() {
    let result = run("any([1,2,3], fn(x) => x > 10)");
    assert_eq!(result, Value::Bool(false));
}

// ============================================================================
// all - both pipeline and function call
// ============================================================================

#[test]
fn test_all_pipeline_with_predicate() {
    let result = run("[1,2,3] | all(fn(x) => x > 0)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_all_function_call_with_predicate() {
    let result = run("all([1,2,3], fn(x) => x > 0)");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_all_pipeline_no_predicate() {
    let result = run("[true, true, true] | all");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_all_function_call_no_predicate() {
    let result = run("all([true, true, true])");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_all_one_false() {
    let result = run("all([true, false, true])");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_all_predicate_one_fails() {
    let result = run("all([1,2,3], fn(x) => x > 1)");
    assert_eq!(result, Value::Bool(false));
}

// ============================================================================
// reverse - both pipeline and function call (already worked, verify)
// ============================================================================

#[test]
fn test_reverse_pipeline() {
    let result = run("[1,2,3] | reverse");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(3), Value::Int(2), Value::Int(1)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_reverse_function_call() {
    let result = run("reverse([1,2,3])");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(3), Value::Int(2), Value::Int(1)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_reverse_string() {
    let result = run(r#"reverse("hello")"#);
    assert_eq!(result, Value::Str("olleh".to_string()));
}

// ============================================================================
// Combined usage
// ============================================================================

#[test]
fn test_combined_flatten_slice() {
    let result = run("flatten([[1,2],[3,4],[5,6]]) | slice(1, 4)");
    match result {
        Value::Array(arr) => {
            assert_eq!(arr, vec![Value::Int(2), Value::Int(3), Value::Int(4)]);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_combined_any_in_condition() {
    let result = run(r#"
        has_big = any([1,2,3,4,5], fn(x) => x > 3);
        match has_big {
            true => "found big",
            _ => "no big"
        }
    "#);
    assert_eq!(result, Value::Str("found big".to_string()));
}

#[test]
fn test_combined_all_in_validation() {
    let result = run(r#"
        all_valid = all([10,20,30], fn(x) => x > 0 && x < 100);
        match all_valid {
            true => "valid",
            _ => "invalid"
        }
    "#);
    assert_eq!(result, Value::Str("valid".to_string()));
}
