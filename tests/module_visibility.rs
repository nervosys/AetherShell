//! Tests for module visibility modifiers (pub/private) and exports

use aethershell::{env::Env, eval, parser};

fn eval_code(code: &str, env: &mut Env) -> aethershell::value::Value {
    let stmts = parser::parse_program(code).unwrap();
    eval::eval_program(&stmts, env).unwrap()
}

/// Private by default: variables should not be exported unless marked pub
#[test]
fn private_by_default() {
    let mut env = Env::default();
    eval_code("let x = 42", &mut env);

    // Variable exists in environment
    assert!(env.get_var("x").is_some());
    // But is not exported (private by default)
    assert!(!env.is_exported("x"));
}

/// Public variables should be exported
#[test]
fn pub_let_is_exported() {
    let mut env = Env::default();
    eval_code("pub let x = 42", &mut env);

    // Variable exists
    assert!(env.get_var("x").is_some());
    // And is exported
    assert!(env.is_exported("x"));
}

/// Public shorthand syntax should work
#[test]
fn pub_shorthand_is_exported() {
    let mut env = Env::default();
    eval_code("pub x = 100", &mut env);

    assert!(env.get_var("x").is_some());
    assert!(env.is_exported("x"));
}

/// Multiple public declarations
#[test]
fn multiple_pub_declarations() {
    let mut env = Env::default();
    eval_code(
        r#"
        pub let a = 1
        let b = 2
        pub let c = 3
    "#,
        &mut env,
    );

    // All variables exist
    assert!(env.get_var("a").is_some());
    assert!(env.get_var("b").is_some());
    assert!(env.get_var("c").is_some());

    // Only pub ones are exported
    assert!(env.is_exported("a"));
    assert!(!env.is_exported("b"));
    assert!(env.is_exported("c"));
}

/// Export statement should mark items as exported
#[test]
fn export_statement() {
    let mut env = Env::default();
    eval_code(
        r#"
        let x = 1
        let y = 2
        export { x, y }
    "#,
        &mut env,
    );

    assert!(env.is_exported("x"));
    assert!(env.is_exported("y"));
}

/// Export with alias
#[test]
fn export_with_alias() {
    let mut env = Env::default();
    eval_code(
        r#"
        let add = fn(a, b) => a + b
        export { add as plus }
    "#,
        &mut env,
    );

    // Original name exists
    assert!(env.get_var("add").is_some());
    // Alias is exported
    assert!(env.is_exported("plus"));
}

/// exported_vars returns only exported items
#[test]
fn exported_vars_filter() {
    let mut env = Env::default();
    eval_code(
        r#"
        pub let public_a = 1
        let private_b = 2
        pub let public_c = 3
        let private_d = 4
    "#,
        &mut env,
    );

    let exported = env.exported_vars();

    // Only public vars are in exported
    assert!(exported.contains_key("public_a"));
    assert!(exported.contains_key("public_c"));
    assert!(!exported.contains_key("private_b"));
    assert!(!exported.contains_key("private_d"));
}

/// Public function declaration
#[test]
fn pub_function() {
    let mut env = Env::default();
    eval_code(
        r#"
        pub let add = fn(a, b) => a + b
        let result = add(2, 3)
    "#,
        &mut env,
    );

    // Function is exported
    assert!(env.is_exported("add"));
    // Result is not (private by default)
    assert!(!env.is_exported("result"));

    // Function works correctly
    let result = env.get_var("result").unwrap();
    assert_eq!(result.as_int().unwrap(), 5);
}

/// Visibility doesn't affect local access
#[test]
fn visibility_local_access() {
    let mut env = Env::default();
    // Private helper function can be used by public function
    let result = eval_code(
        r#"
        let helper = fn(x) => x * 2
        pub let double_add = fn(a, b) => helper(a) + helper(b)
        double_add(3, 4)
    "#,
        &mut env,
    );

    assert_eq!(result.as_int().unwrap(), 14); // (3*2) + (4*2) = 14

    // helper is private, double_add is public
    assert!(!env.is_exported("helper"));
    assert!(env.is_exported("double_add"));
}

/// Test exports iterator
#[test]
fn exports_iterator() {
    let mut env = Env::default();
    eval_code(
        r#"
        pub let a = 1
        pub let b = 2
        let c = 3
    "#,
        &mut env,
    );

    let exports: Vec<String> = env.exports().cloned().collect();
    assert!(exports.contains(&"a".to_string()));
    assert!(exports.contains(&"b".to_string()));
    assert!(!exports.contains(&"c".to_string()));
}
