//! Test immutability enforcement

use aethershell::{env::Env, eval, parser};

#[test]
fn test_immutable_by_default() {
    let mut env = Env::new();

    // Parse and eval: x = 42 (immutable by default)
    let code = "x = 42";
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap();

    // Check that x is immutable
    assert!(
        !env.is_mutable("x"),
        "Variables should be immutable by default"
    );
}

#[test]
fn test_let_mut_creates_mutable() {
    let mut env = Env::new();

    // Parse and eval: let mut y = 10
    let code = "let mut y = 10";
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap();

    // Check that y is mutable
    assert!(
        env.is_mutable("y"),
        "let mut should create mutable variable"
    );
}

#[test]
fn test_explicit_let_is_immutable() {
    let mut env = Env::new();

    // Parse and eval: let z = 99
    let code = "let z = 99";
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap();

    // Check that z is immutable
    assert!(
        !env.is_mutable("z"),
        "let (without mut) should be immutable"
    );
}

#[test]
fn test_immutable_cannot_be_reassigned() {
    let mut env = Env::new();

    // First declaration
    let code1 = "x = 42";
    let stmts1 = parser::parse_program(code1).unwrap();
    eval::eval_program(&stmts1, &mut env).unwrap();
    assert_eq!(env.get_var("x").unwrap().as_int().unwrap(), 42);

    // Try to reassign immutable variable - should fail
    let code2 = "x = 100";
    let stmts2 = parser::parse_program(code2).unwrap();
    let result = eval::eval_program(&stmts2, &mut env);

    assert!(
        result.is_err(),
        "Should not be able to reassign immutable variable"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Cannot reassign immutable variable"));

    // x should still be 42
    assert_eq!(env.get_var("x").unwrap().as_int().unwrap(), 42);
}

#[test]
fn test_mutable_var_can_be_updated() {
    let mut env = Env::new();

    // Create mutable variable
    let code1 = "let mut count = 0";
    let stmts1 = parser::parse_program(code1).unwrap();
    eval::eval_program(&stmts1, &mut env).unwrap();
    assert_eq!(env.get_var("count").unwrap().as_int().unwrap(), 0);
    assert!(env.is_mutable("count"));

    // Update mutable variable - should succeed
    let code2 = "count = 1";
    let stmts2 = parser::parse_program(code2).unwrap();
    eval::eval_program(&stmts2, &mut env).unwrap();
    assert_eq!(env.get_var("count").unwrap().as_int().unwrap(), 1);

    // Should still be mutable after update
    assert!(env.is_mutable("count"));
}

#[test]
fn test_simple_assignment_is_immutable_by_default() {
    let mut env = Env::new();

    // Simple assignment without let
    let code = r#"
        name = "Alice"
        age = 30
        active = true
    "#;
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, &mut env).unwrap();

    // All should be immutable by default
    assert!(!env.is_mutable("name"), "name should be immutable");
    assert!(!env.is_mutable("age"), "age should be immutable");
    assert!(!env.is_mutable("active"), "active should be immutable");
}
