// Tests for conditional compilation (#[cfg(...)] attribute)

use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

fn eval_with_env(src: &str) -> (Value, Env) {
    let stmts = parse_program(src).expect("parse failed");
    let mut env = Env::new();
    let result = eval_program(&stmts, &mut env).expect("eval failed");
    (result, env)
}

fn strip_ansi(s: &str) -> String {
    // Simple ANSI escape sequence stripper
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until 'm'
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn clean_value(v: &Value) -> String {
    strip_ansi(&v.to_string()).trim_matches('"').to_string()
}

#[test]
fn cfg_platform_conditional() {
    // Use a cfg that matches the current platform
    let src = if cfg!(target_os = "windows") {
        r#"
        #[cfg(windows)]
        let x = 42
        x
        "#
    } else if cfg!(target_os = "linux") {
        r#"
        #[cfg(linux)]
        let x = 42
        x
        "#
    } else if cfg!(target_os = "macos") {
        r#"
        #[cfg(macos)]
        let x = 42
        x
        "#
    } else {
        r#"let x = 42; x"#
    };

    let (result, _) = eval_with_env(src);
    assert_eq!(clean_value(&result), "42");
}

#[test]
fn cfg_not_condition() {
    // Use #[cfg(not(...))] to exclude a platform
    let src = if cfg!(target_os = "windows") {
        r#"
        #[cfg(not(linux))]
        let x = "not linux"
        x
        "#
    } else {
        r#"
        #[cfg(not(windows))]
        let x = "not windows"
        x
        "#
    };

    let (_, env) = eval_with_env(src);

    // The variable should exist
    assert!(env.get_var("x").is_some());
}

#[test]
fn cfg_unix_alias() {
    // unix should match linux/macos but not windows
    let src = r#"
        #[cfg(unix)]
        let is_unix = true
        
        #[cfg(windows)]
        let is_unix = false
        
        is_unix
    "#;

    let (result, _) = eval_with_env(src);
    let is_windows = cfg!(target_os = "windows");

    // On Windows, unix is false; on other platforms, it's true
    if is_windows {
        assert_eq!(clean_value(&result), "false");
    } else {
        assert_eq!(clean_value(&result), "true");
    }
}

#[test]
fn cfg_feature_enabled() {
    // Use a unique feature name to avoid race conditions with other tests
    std::env::set_var("AETHER_FEATURES", "test_feature_enabled_unique");

    let (_, env) = eval_with_env(
        r#"
        #[cfg(feature = "test_feature_enabled_unique")]
        let x = "feature enabled"
        x
    "#,
    );

    // Check the variable was set
    assert!(env.get_var("x").is_some());
    let x = env.get_var("x").unwrap();
    assert_eq!(clean_value(x), "feature enabled");

    // Clean up
    std::env::remove_var("AETHER_FEATURES");
}

#[test]
fn cfg_feature_disabled() {
    // Use AETHER_FEATURES_DISABLED to avoid race with cfg_feature_enabled
    std::env::set_var("AETHER_FEATURES", "some_other_feature");

    let (_, env) = eval_with_env(
        r#"
        #[cfg(feature = "nonexistent_feature_unique")]
        let x = "should not exist"
        
        let y = "fallback"
        y
    "#,
    );

    // x should not exist because the feature is not enabled
    assert!(env.get_var("x").is_none());
    assert!(env.get_var("y").is_some());
}

#[test]
fn cfg_all_combinator() {
    let src = if cfg!(target_os = "windows") {
        r#"
        #[cfg(all(windows, not(linux)))]
        let x = "windows and not linux"
        x
        "#
    } else {
        r#"
        #[cfg(all(unix, not(windows)))]
        let x = "unix and not windows"
        x
        "#
    };

    let (_, env) = eval_with_env(src);
    assert!(env.get_var("x").is_some());
}

#[test]
fn cfg_any_combinator() {
    // any(windows, linux, macos) should always match on desktop
    let (_, env) = eval_with_env(
        r#"
        #[cfg(any(windows, linux, macos))]
        let x = "desktop platform"
        x
    "#,
    );

    assert!(env.get_var("x").is_some());
    let x = env.get_var("x").unwrap();
    assert_eq!(clean_value(x), "desktop platform");
}

#[test]
fn cfg_parse_nested() {
    // Test parsing nested conditions
    let src = r#"
        #[cfg(any(all(windows, not(linux)), all(linux, not(windows)), macos))]
        let complex = "complex condition"
        complex
    "#;

    let (_, env) = eval_with_env(src);

    // Should work on Windows, Linux, and macOS
    assert!(env.get_var("complex").is_some());
}

#[test]
fn cfg_skipped_on_wrong_platform() {
    // Test that cfg actually skips code on non-matching platforms
    let src = if cfg!(target_os = "windows") {
        r#"
        #[cfg(linux)]
        let linux_var = "linux only"
        
        let exists = true
        exists
        "#
    } else {
        r#"
        #[cfg(windows)]
        let win_var = "windows only"
        
        let exists = true
        exists
        "#
    };

    let (_, env) = eval_with_env(src);

    // The platform-specific variable should NOT exist
    if cfg!(target_os = "windows") {
        assert!(env.get_var("linux_var").is_none());
    } else {
        assert!(env.get_var("win_var").is_none());
    }

    // But the unconditional variable should exist
    assert!(env.get_var("exists").is_some());
}
