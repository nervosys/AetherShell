// tests/feature_flags.rs
//! Tests for runtime feature flags: features, feature_enabled, feature_enable, etc.

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
// features() tests
// ============================================================================

#[test]
fn test_features_returns_array() {
    let result = run("features()");
    match result {
        Value::Array(_) => (), // Expected
        _ => panic!("Expected Array, got {:?}", result),
    }
}

#[test]
fn test_feature_list_alias() {
    let result = run("feature_list()");
    match result {
        Value::Array(_) => (), // Expected
        _ => panic!("Expected Array, got {:?}", result),
    }
}

// ============================================================================
// feature_enabled() tests
// ============================================================================

#[test]
fn test_feature_enabled_returns_bool() {
    let result = run(r#"feature_enabled("nonexistent")"#);
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_feature_enabled_false_for_nonexistent() {
    let result = run(r#"feature_enabled("some_nonexistent_feature")"#);
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_has_feature_alias() {
    let result = run(r#"has_feature("test")"#);
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

// ============================================================================
// feature_enable() tests
// ============================================================================

#[test]
fn test_feature_enable_returns_true() {
    let result = run(r#"feature_enable("test_feature")"#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_feature_enable_makes_feature_enabled() {
    let result = run(r#"
        feature_enable("my_test_feature1")
        feature_enabled("my_test_feature1")
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_feature_enable_multiple_features() {
    let result = run(r#"
        feature_enable("feat_a")
        feature_enable("feat_b")
        let a = feature_enabled("feat_a")
        let b = feature_enabled("feat_b")
        a == true && b == true
    "#);
    // The comparison returns true if both are enabled
    match result {
        Value::Bool(b) => assert!(b, "Both features should be enabled"),
        _ => panic!("Expected Bool"),
    }
}

// ============================================================================
// feature_disable() tests
// ============================================================================

#[test]
fn test_feature_disable_returns_bool() {
    let result = run(r#"feature_disable("nonexistent")"#);
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_feature_disable_returns_false_for_nonexistent() {
    let result = run(r#"feature_disable("never_existed_feature")"#);
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_feature_disable_returns_true_for_enabled() {
    let result = run(r#"
        feature_enable("to_disable")
        feature_disable("to_disable")
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_feature_disable_removes_feature() {
    let result = run(r#"
        feature_enable("temp_feature")
        feature_disable("temp_feature")
        feature_enabled("temp_feature")
    "#);
    assert_eq!(result, Value::Bool(false));
}

// ============================================================================
// feature_set() tests
// ============================================================================

#[test]
fn test_feature_set_enable() {
    let result = run(r#"
        feature_set("set_test1", true)
        feature_enabled("set_test1")
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_feature_set_disable() {
    let result = run(r#"
        feature_enable("set_test2")
        feature_set("set_test2", false)
        feature_enabled("set_test2")
    "#);
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_feature_set_returns_true() {
    let result = run(r#"feature_set("any_feature", true)"#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_feature_set_piped() {
    let result = run(r#"
        "piped_feature" | feature_set(true)
        "piped_feature" | feature_enabled
    "#);
    assert_eq!(result, Value::Bool(true));
}

// ============================================================================
// Integration tests
// ============================================================================

#[test]
fn test_enable_check_disable_cycle() {
    let result = run(r#"
        let before = feature_enabled("cycle_test")
        feature_enable("cycle_test")
        let during = feature_enabled("cycle_test")
        feature_disable("cycle_test")
        let after = feature_enabled("cycle_test")
        [before, during, after]
    "#);
    match result {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Bool(false)); // Before: not enabled
            assert_eq!(arr[1], Value::Bool(true)); // During: enabled
            assert_eq!(arr[2], Value::Bool(false)); // After: disabled
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_features_contains_enabled_feature() {
    let result = run(r#"
        feature_enable("find_me")
        let all = features()
        all | any(fn(f) => f == "find_me")
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_conditional_on_feature() {
    let result = run(r#"
        feature_enable("special_mode")
        let enabled = feature_enabled("special_mode")
        match enabled {
            true => "special",
            _ => "normal"
        }
    "#);
    assert_eq!(result, Value::Str("special".to_string()));
}

#[test]
fn test_feature_in_record() {
    let result = run(r#"
        feature_enable("rec_test")
        {
            name: "rec_test",
            enabled: feature_enabled("rec_test")
        }
    "#);
    match result {
        Value::Record(rec) => {
            assert_eq!(rec.get("enabled"), Some(&Value::Bool(true)));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_multiple_features_in_array() {
    let result = run(r#"
        feature_enable("feat1")
        feature_enable("feat2")
        feature_enable("feat3")
        ["feat1", "feat2", "feat3"] | map(fn(f) => feature_enabled(f)) | all(fn(x) => x)
    "#);
    assert_eq!(result, Value::Bool(true));
}
