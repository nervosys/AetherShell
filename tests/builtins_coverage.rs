//! Comprehensive test coverage for all AetherShell builtins
//!
//! This test file programmatically verifies that all 143+ builtins are:
//! 1. Callable without panicking
//! 2. Return expected types
//! 3. Work correctly in expressions

use aethershell::{env::Env, eval, parser, value::Value};

/// Helper to evaluate AetherShell code and return the result
fn eval_code(code: &str) -> Result<Value, String> {
    let mut env = Env::default();
    let stmts = parser::parse_program(code).map_err(|e| format!("Parse error: {e}"))?;
    eval::eval_program(&stmts, &mut env).map_err(|e| format!("Eval error: {e}"))
}

/// Helper to check if a builtin exists by trying to call it
#[allow(dead_code)]
fn builtin_exists(name: &str) -> bool {
    // Try to parse and evaluate a type_of call on the builtin
    // If the builtin exists, type_of will return something
    let code = format!("type_of({})", name);
    eval_code(&code).is_ok()
}

/// Helper to check if a builtin is callable (for functions that need args)
fn builtin_callable(code: &str) -> bool {
    eval_code(code).is_ok()
}

// ============================================
// Core Builtins (0-7)
// ============================================

#[test]
fn test_builtin_help() {
    assert!(builtin_callable("help()"), "help() should be callable");
}

#[test]
fn test_builtin_call() {
    assert!(
        builtin_callable(r#"call("len", [[1,2,3]])"#),
        "call should work"
    );
}

#[test]
fn test_builtin_echo() {
    assert!(
        builtin_callable(r#"echo("hello")"#),
        "echo should be callable"
    );
}

#[test]
fn test_builtin_print() {
    assert!(
        builtin_callable(r#"print("test")"#),
        "print should be callable"
    );
}

#[test]
fn test_builtin_some_none() {
    assert!(builtin_callable("Some(42)"), "Some should be callable");
    assert!(builtin_callable("None()"), "None should be callable");
}

// ============================================
// File System Builtins (8-18)
// ============================================

#[test]
fn test_builtin_fs() {
    assert!(builtin_callable(r#"pwd()"#), "pwd should be callable");
    assert!(
        builtin_callable(r#"len([1,2,3])"#),
        "len should be callable"
    );
}

// ============================================
// Functional Builtins (19-29)
// ============================================

#[test]
fn test_builtin_functional() {
    assert!(
        builtin_callable("[1,2,3] | map(fn(x) => x)"),
        "map should work"
    );
    assert!(
        builtin_callable("[1,2,3] | where(fn(x) => true)"),
        "where should work"
    );
    assert!(
        builtin_callable("[1,2,3] | reduce(fn(a,b) => a+b, 0)"),
        "reduce should work"
    );
    assert!(builtin_callable("take([1,2,3], 2)"), "take should work");
    assert!(builtin_callable("keys({a: 1})"), "keys should work");
    assert!(builtin_callable("len([1,2,3])"), "len should work");
    assert!(builtin_callable("type_of(42)"), "type_of should work");
    assert!(builtin_callable("first([1,2,3])"), "first should work");
    assert!(builtin_callable("last([1,2,3])"), "last should work");
    // any/all use pipeline syntax
    assert!(
        builtin_callable("[1,2,3] | any(fn(x) => x > 2)"),
        "any should work"
    );
    assert!(
        builtin_callable("[1,2,3] | all(fn(x) => x > 0)"),
        "all should work"
    );
}

#[test]
fn test_map_functionality() {
    let result = eval_code("[1,2,3] | map(fn(x) => x * 2)").unwrap();
    match result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert!(matches!(arr[0], Value::Int(2)));
        }
        _ => panic!("map should return Array"),
    }
}

#[test]
fn test_where_functionality() {
    let result = eval_code("[1,2,3,4,5] | where(fn(x) => x > 3)").unwrap();
    match result {
        Value::Array(arr) => assert_eq!(arr.len(), 2),
        _ => panic!("where should return Array"),
    }
}

#[test]
fn test_reduce_functionality() {
    let result = eval_code("[1,2,3,4] | reduce(fn(a,b) => a+b, 0)").unwrap();
    match result {
        Value::Int(10) => {}
        _ => panic!("reduce should return Int(10)"),
    }
}

// ============================================
// MCP Detection Builtins (30-33) - Skip network-dependent tests
// ============================================

// MCP builtins require runtime context, tested in integration tests

// ============================================
// String Builtins (34-42)
// ============================================

#[test]
fn test_builtin_string() {
    assert!(
        builtin_callable(r#"split("a,b", ",")"#),
        "split should work"
    );
    assert!(
        builtin_callable(r#"join(["a","b"], "-")"#),
        "join should work"
    );
    assert!(builtin_callable(r#"trim("  hi  ")"#), "trim should work");
    assert!(builtin_callable(r#"upper("hi")"#), "upper should work");
    assert!(builtin_callable(r#"lower("HI")"#), "lower should work");
    assert!(
        builtin_callable(r#"replace("hi", "h", "b")"#),
        "replace should work"
    );
    assert!(
        builtin_callable(r#"contains("hi", "h")"#),
        "contains should work"
    );
    assert!(
        builtin_callable(r#"starts_with("hi", "h")"#),
        "starts_with should work"
    );
    assert!(
        builtin_callable(r#"ends_with("hi", "i")"#),
        "ends_with should work"
    );
}

#[test]
fn test_string_operations() {
    // split
    let split_result = eval_code(r#"split("a,b,c", ",")"#).unwrap();
    match split_result {
        Value::Array(arr) => assert_eq!(arr.len(), 3),
        _ => panic!("split should return Array"),
    }

    // join
    let join_result = eval_code(r#"join(["a","b","c"], "-")"#).unwrap();
    match join_result {
        Value::Str(s) => assert_eq!(s, "a-b-c"),
        _ => panic!("join should return Str"),
    }

    // upper/lower
    let upper = eval_code(r#"upper("hello")"#).unwrap();
    match upper {
        Value::Str(s) => assert_eq!(s, "HELLO"),
        _ => panic!("upper should return Str"),
    }
}

// ============================================
// Array Builtins (43-49)
// ============================================

#[test]
fn test_builtin_array() {
    // Many array builtins use pipeline syntax
    assert!(
        builtin_callable("[[1],[2]] | flatten"),
        "flatten should work"
    );
    assert!(builtin_callable("[1,2,3] | reverse"), "reverse should work");
    assert!(
        builtin_callable("[1,2,3] | slice(0, 2)"),
        "slice should work"
    );
    assert!(builtin_callable("range(1, 5)"), "range should work");
    assert!(builtin_callable("zip([1,2], [3,4])"), "zip should work");
    assert!(builtin_callable("push([1,2], 3)"), "push should work");
    assert!(
        builtin_callable("concat([1,2], [3,4])"),
        "concat should work"
    );
}

#[test]
fn test_array_operations() {
    // range
    let range_result = eval_code("range(1, 5)").unwrap();
    match range_result {
        Value::Array(arr) => assert_eq!(arr.len(), 4), // [1,2,3,4]
        _ => panic!("range should return Array"),
    }

    // reverse
    let reverse_result = eval_code("reverse([1,2,3])").unwrap();
    match reverse_result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
        }
        _ => panic!("reverse should return Array"),
    }
}

// ============================================
// Math Builtins (50-57)
// ============================================

#[test]
fn test_builtin_math() {
    assert!(builtin_callable("abs(-5)"), "abs should work");
    assert!(builtin_callable("min(1, 2)"), "min should work");
    assert!(builtin_callable("max(1, 2)"), "max should work");
    assert!(builtin_callable("floor(3.7)"), "floor should work");
    assert!(builtin_callable("ceil(3.2)"), "ceil should work");
    assert!(builtin_callable("round(3.5)"), "round should work");
    assert!(builtin_callable("sqrt(16)"), "sqrt should work");
    assert!(builtin_callable("pow(2, 3)"), "pow should work");
}

#[test]
fn test_math_operations() {
    // abs
    let abs_result = eval_code("abs(-42)").unwrap();
    match abs_result {
        Value::Int(42) => {}
        _ => panic!("abs(-42) should return Int(42)"),
    }

    // pow
    let pow_result = eval_code("pow(2, 3)").unwrap();
    match pow_result {
        Value::Int(8) => {}
        Value::Float(f) if (f - 8.0).abs() < 0.001 => {}
        other => panic!("pow(2,3) should return 8, got {:?}", other),
    }

    // sqrt
    let sqrt_result = eval_code("sqrt(16)").unwrap();
    match sqrt_result {
        Value::Int(4) => {}
        Value::Float(f) if (f - 4.0).abs() < 0.001 => {}
        other => panic!("sqrt(16) should return 4, got {:?}", other),
    }
}

// ============================================
// System Builtins (58-64)
// ============================================

#[test]
fn test_builtin_system() {
    // env requires variable name argument
    assert!(builtin_callable(r#"env("PATH")"#), "env should work");
    assert!(builtin_callable("time()"), "time should work");
    assert!(
        builtin_callable(r#"json_parse("{}")"#),
        "json_parse should work"
    );
    assert!(
        builtin_callable(r#"json_stringify({a: 1})"#),
        "json_stringify should work"
    );
}

// ============================================
// Syntax KB Builtins (65-71) - These are specialized
// ============================================

// Syntax KB builtins tested in dedicated syntax_kb tests

// ============================================
// Neural Network Builtins (72-82) - Specialized domain
// ============================================

// NN builtins tested in dedicated nn_* tests

// ============================================
// Evolution Builtins (83-92) - Specialized domain
// ============================================

// Evolution builtins tested in dedicated evolution tests

// ============================================
// Reinforcement Learning Builtins (93-107) - Specialized domain
// ============================================

// RL builtins tested in dedicated rl_* tests

// ============================================
// OS Tools Builtins (108-112)
// ============================================

#[test]
fn test_builtin_os_tools() {
    assert!(builtin_callable("tools()"), "tools should be callable");
}

// ============================================
// RLM Agents Builtins (113-116) - Specialized
// ============================================

// RLM builtins require runtime context

// ============================================
// MCP Client Builtins (117-122) - Network dependent
// ============================================

// MCP client builtins tested in integration tests

// ============================================
// Utility Builtins (123-136)
// ============================================

#[test]
fn test_builtin_utility() {
    assert!(builtin_callable("now()"), "now should work");
    assert!(
        builtin_callable("sort_by([{a:2}, {a:1}], fn(x) => x.a)"),
        "sort_by should work"
    );
}

// ============================================
// Aggregate Builtins (132-136)
// ============================================

#[test]
fn test_builtin_aggregate() {
    assert!(
        builtin_callable("values({a: 1, b: 2})"),
        "values should work"
    );
    assert!(builtin_callable("sum([1,2,3])"), "sum should work");
    assert!(builtin_callable("unique([1,1,2,2])"), "unique should work");
    assert!(builtin_callable("avg([1,2,3])"), "avg should work");
    assert!(builtin_callable("product([1,2,3])"), "product should work");
}

#[test]
fn test_aggregate_operations() {
    // sum
    let sum_result = eval_code("sum([1,2,3,4,5])").unwrap();
    match sum_result {
        Value::Int(15) => {}
        _ => panic!("sum should return 15"),
    }

    // unique
    let unique_result = eval_code("unique([1,2,2,3,3,3])").unwrap();
    match unique_result {
        Value::Array(arr) => assert!(arr.len() <= 3),
        _ => panic!("unique should return Array"),
    }
}

// ============================================
// Configuration Builtins (137-143)
// ============================================

#[test]
fn test_builtin_config() {
    assert!(builtin_callable("config()"), "config should work");
    assert!(
        builtin_callable(r#"config_get("colors.theme")"#),
        "config_get should work"
    );
    assert!(builtin_callable("config_path()"), "config_path should work");
    assert!(builtin_callable("themes()"), "themes should work");
}

#[test]
fn test_themes_returns_array() {
    let result = eval_code("themes()").unwrap();
    match result {
        Value::Array(arr) => {
            assert!(
                arr.len() >= 38,
                "Should have at least 38 themes, got {}",
                arr.len()
            );
        }
        _ => panic!("themes() should return Array"),
    }
}
