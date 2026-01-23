// tests/parser_improvements.rs
//! Tests for parser improvements: semicolon handling, pipeline assignments

use aether_shell::env::Env;
use aether_shell::eval::eval_program;
use aether_shell::parser::parse_program;
use aether_shell::value::Value;

fn run(code: &str) -> Value {
    let stmts = parse_program(code).expect("parse failed");
    let mut env = Env::new();
    eval_program(&stmts, &mut env).expect(&format!("eval failed for: {}", code))
}

// ============================================================================
// Semicolon statement separation tests
// ============================================================================

#[test]
fn test_semicolon_simple_assignments() {
    let result = run("x = 5; y = 10; y");
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_semicolon_multiple_prints() {
    // Just make sure it parses and runs without error
    let result = run("print(1); print(2); print(3); 42");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_semicolon_after_pipe() {
    let result = run("[1,2,3] | reverse; 42");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_semicolon_pipe_with_assignment() {
    let result = run(r#"
        x = [1,2,3] | reverse;
        first(x)
    "#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_semicolon_original_problem_case() {
    // This was the original failing case from the roadmap
    let result = run("x = [1,2,3] | reverse; y = first(x); y");
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_semicolon_at_end_of_line() {
    let result = run("x = 5;; y = 10; y");
    assert_eq!(result, Value::Int(10));
}

#[test]
fn test_semicolon_with_newlines() {
    let result = run(r#"
        x = 5;
        y = 10;
        x + y
    "#);
    assert_eq!(result, Value::Int(15));
}

#[test]
fn test_semicolon_in_one_liner() {
    let result = run("a = 1; b = 2; c = 3; a + b + c");
    assert_eq!(result, Value::Int(6));
}

// ============================================================================
// Pipeline assignment without parentheses
// ============================================================================

#[test]
fn test_pipe_assignment_no_parens_reverse() {
    let result = run(r#"
        x = [1,2,3] | reverse
        first(x)
    "#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_pipe_assignment_no_parens_map() {
    let result = run(r#"
        x = [1,2,3] | map(fn(n) => n * 2)
        first(x)
    "#);
    assert_eq!(result, Value::Int(2));
}

#[test]
fn test_pipe_assignment_no_parens_chained() {
    let result = run(r#"
        x = [1,2,3,4,5] | map(fn(n) => n * 2) | where(fn(n) => n > 4)
        len(x)
    "#);
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_pipe_assignment_followed_by_another() {
    let result = run(r#"
        x = [1,2,3] | reverse
        y = [4,5,6] | reverse
        [first(x), first(y)]
    "#);
    match result {
        Value::Array(arr) => {
            assert_eq!(arr[0], Value::Int(3));
            assert_eq!(arr[1], Value::Int(6));
        }
        _ => panic!("Expected Array"),
    }
}

// ============================================================================
// Mixed scenarios
// ============================================================================

#[test]
fn test_semicolon_pipe_and_function_calls() {
    let result = run(r#"
        nums = [5,3,1,4,2] | sort;
        doubled = nums | map(fn(x) => x * 2);
        sum(doubled)
    "#);
    assert_eq!(result, Value::Int(30));
}

#[test]
fn test_multiple_pipes_on_one_line() {
    let result = run("[1,2,3] | map(fn(x) => x * 2) | sum");
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_semicolon_between_unrelated_statements() {
    let result = run(r#"
        print("hello"); x = 42; print("world"); x
    "#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_pipe_to_builtin_with_no_parens() {
    let result = run(r#"
        data = [1,2,3] | reverse
        data | first
    "#);
    assert_eq!(result, Value::Int(3));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_between_semicolons() {
    // Multiple semicolons should be fine
    let result = run("x = 5;;; y = 10;;; x + y");
    assert_eq!(result, Value::Int(15));
}

#[test]
fn test_leading_semicolons() {
    let result = run("; ; ; x = 42; x");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_trailing_semicolons() {
    let result = run("x = 42; ; ;");
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_semicolon_with_word_call() {
    let result = run(r#"print "hello"; print "world"; 42"#);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_semicolon_with_record_assignment() {
    let result = run(r#"
        rec = { a: 1, b: 2 };
        rec.a + rec.b
    "#);
    assert_eq!(result, Value::Int(3));
}
