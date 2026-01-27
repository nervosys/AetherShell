//! Theme Coverage Tests
//!
//! Verifies all 38 themes are properly defined and accessible

use aethershell::{env::Env, eval, parser, value::Value};

/// Test that themes() returns an array with expected count
#[test]
fn test_themes_returns_correct_count() {
    let mut env = Env::default();
    let stmts = parser::parse_program("themes()").unwrap();
    let result = eval::eval_program(&stmts, &mut env).unwrap();

    match result {
        Value::Array(themes) => {
            assert_eq!(
                themes.len(),
                38,
                "themes() should return 38 themes, got {}",
                themes.len()
            );
        }
        _ => panic!("themes() should return Array"),
    }
}

/// Test that specific expected themes exist
#[test]
fn test_expected_themes_exist() {
    let mut env = Env::default();
    let stmts = parser::parse_program("themes()").unwrap();
    let result = eval::eval_program(&stmts, &mut env).unwrap();

    let expected = vec![
        "catppuccin",
        "monokai",
        "dracula",
        "nord",
        "gruvbox",
        "solarized",
        "tokyo-night",
        "rose-pine",
        "vscode",
    ];

    if let Value::Array(themes) = result {
        let theme_strings: Vec<String> = themes
            .iter()
            .filter_map(|v| {
                if let Value::Str(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect();

        for expected_theme in expected {
            assert!(
                theme_strings.contains(&expected_theme.to_string()),
                "Theme '{}' should be in the list. Available: {:?}",
                expected_theme,
                theme_strings
            );
        }
    }
}

/// Test config_get for theme
#[test]
fn test_config_get_theme() {
    let mut env = Env::default();
    let stmts = parser::parse_program(r#"config_get("colors.theme")"#).unwrap();
    let result = eval::eval_program(&stmts, &mut env).unwrap();

    // Should return a string (the current theme name)
    match result {
        Value::Str(theme_name) => {
            assert!(!theme_name.is_empty(), "Theme name should not be empty");
        }
        Value::Null => {
            // Theme might not be set yet, that's ok
        }
        other => panic!("config_get for theme should return Str, got {:?}", other),
    }
}

/// Test that all themes are strings
#[test]
fn test_all_themes_are_strings() {
    let mut env = Env::default();
    let stmts = parser::parse_program("themes()").unwrap();
    let result = eval::eval_program(&stmts, &mut env).unwrap();

    if let Value::Array(themes) = result {
        for (i, theme) in themes.iter().enumerate() {
            match theme {
                Value::Str(s) => {
                    assert!(!s.is_empty(), "Theme at index {} should not be empty", i);
                }
                other => panic!("Theme at index {} should be Str, got {:?}", i, other),
            }
        }
    }
}

/// Test that themes can be looked up
#[test]
fn test_theme_lookup_with_in() {
    let mut env = Env::default();

    // Check if catppuccin is in themes
    let code = r#"in("catppuccin", themes())"#;
    let stmts = parser::parse_program(code).unwrap();
    let result = eval::eval_program(&stmts, &mut env).unwrap();

    match result {
        Value::Bool(true) => {}
        other => panic!("catppuccin should be in themes(), got {:?}", other),
    }
}

/// Test themes are listed in sorted/consistent order
#[test]
fn test_themes_order_consistent() {
    let mut env = Env::default();

    // Call themes() twice, should return same order
    let stmts = parser::parse_program("themes()").unwrap();
    let result1 = eval::eval_program(&stmts, &mut env.clone()).unwrap();
    let result2 = eval::eval_program(&stmts, &mut env).unwrap();

    match (&result1, &result2) {
        (Value::Array(a1), Value::Array(a2)) => {
            assert_eq!(a1.len(), a2.len(), "Theme count should be consistent");
            for (i, (t1, t2)) in a1.iter().zip(a2.iter()).enumerate() {
                assert_eq!(t1, t2, "Theme at index {} should be consistent", i);
            }
        }
        _ => panic!("Both calls should return arrays"),
    }
}
