// tests/plugins.rs
// Tests for the AetherShell plugin system builtins

use aethershell::{env::Env, eval, parser};

/// Strip ANSI escape codes from a string
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip until we find a letter (end of ANSI sequence)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Helper to evaluate AetherShell code and return the result as string
fn run(code: &str) -> String {
    let mut env = Env::default();
    match parser::parse_program(code) {
        Ok(stmts) => match eval::eval_program(&stmts, &mut env) {
            Ok(val) => strip_ansi(&format!("{}", val)),
            Err(e) => format!("ERROR: {}", e),
        },
        Err(e) => format!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_plugins_list() {
    let result = run("plugins()");
    // Should return an array of plugins
    assert!(result.contains("builtin.json"));
    assert!(result.contains("builtin.csv"));
    assert!(result.contains("builtin.toml"));
}

#[test]
fn test_plugin_list_alias() {
    let result = run("plugin_list()");
    // Should be same as plugins
    assert!(result.contains("builtin.json"));
}

#[test]
fn test_plugin_info_json_handler() {
    let result = run(r#"plugin_info("builtin.json")"#);
    assert!(result.contains("builtin.json"));
    assert!(result.contains("JSON File Handler"));
    assert!(result.contains("enabled"));
}

#[test]
fn test_plugin_info_csv_handler() {
    let result = run(r#"plugin_info("builtin.csv")"#);
    assert!(result.contains("builtin.csv"));
    assert!(result.contains("CSV File Handler"));
}

#[test]
fn test_plugin_info_toml_handler() {
    let result = run(r#"plugin_info("builtin.toml")"#);
    assert!(result.contains("builtin.toml"));
    assert!(result.contains("TOML File Handler"));
}

#[test]
fn test_plugin_info_nonexistent() {
    let result = run(r#"plugin_info("nonexistent-plugin")"#);
    // Should return null for nonexistent plugin
    assert!(result.contains("null") || result.contains("Null"));
}

#[test]
fn test_plugin_categories() {
    let result = run("plugin_categories()");
    // Should list all categories
    assert!(result.contains("AIBackend"));
    assert!(result.contains("Builtin"));
    assert!(result.contains("FileHandler"));
    assert!(result.contains("Transport"));
    assert!(result.contains("Syntax"));
    assert!(result.contains("TUIComponent"));
}

#[test]
fn test_plugin_enable_builtin() {
    // Should succeed for builtin plugins
    let result = run(r#"plugin_enable("builtin.json")"#);
    assert!(result.contains("true"));
}

#[test]
fn test_plugin_disable_builtin() {
    // First ensure it's enabled
    run(r#"plugin_enable("builtin.json")"#);

    // Should succeed for builtin plugins
    let result = run(r#"plugin_disable("builtin.json")"#);
    assert!(result.contains("true"));

    // Re-enable for other tests
    run(r#"plugin_enable("builtin.json")"#);
}

#[test]
fn test_plugin_enable_nonexistent() {
    let result = run(r#"plugin_enable("nonexistent-plugin")"#);
    // Should error for nonexistent plugin
    assert!(result.contains("ERROR"));
}

#[test]
fn test_plugin_unload_builtin_fails() {
    // Cannot unload builtin plugins
    let result = run(r#"plugin_unload("builtin.json")"#);
    assert!(result.contains("ERROR"));
    assert!(result.contains("Cannot unload built-in"));
}

#[test]
fn test_plugin_unload_nonexistent() {
    let result = run(r#"plugin_unload("nonexistent-plugin")"#);
    // Should error for nonexistent plugin
    assert!(result.contains("ERROR"));
}

#[test]
fn test_plugin_load_nonexistent_path() {
    let result = run(r#"plugin_load("/nonexistent/path/plugin.toml")"#);
    // Should error for nonexistent path
    assert!(result.contains("ERROR"));
    assert!(result.contains("not found"));
}

#[test]
fn test_plugins_count() {
    // Should have at least 3 built-in plugins
    let result = run("len(plugins())");
    let count: i64 = result.trim().parse().unwrap_or(0);
    assert!(count >= 3, "Expected at least 3 plugins, got {}", count);
}

#[test]
fn test_plugin_pipeline_filter() {
    // Filter plugins by category using pipeline
    let result = run(r#"plugins() | where(fn(p) => contains(p.id, "json"))"#);
    assert!(result.contains("builtin.json"));
    assert!(!result.contains("builtin.csv"));
}

#[test]
fn test_plugin_info_has_version() {
    let result = run(r#"plugin_info("builtin.json").version"#);
    assert!(result.contains("1.0.0"));
}

#[test]
fn test_plugin_info_has_author() {
    let result = run(r#"plugin_info("builtin.json").author"#);
    assert!(result.contains("AetherShell"));
}

#[test]
fn test_plugin_info_has_categories() {
    let result = run(r#"plugin_info("builtin.json").categories"#);
    assert!(result.contains("FileHandler"));
}

#[test]
fn test_plugin_info_enabled_status() {
    // Plugins are enabled by default, and plugin_enable is idempotent
    // The enabled field should be present in the record
    let result = run(r#"plugin_info("builtin.json").enabled"#);
    // The result should be either true or false (a valid boolean)
    // With parallel tests, another test might have disabled it
    assert!(
        result.contains("true") || result.contains("false") || result.contains("Bool"),
        "Expected boolean enabled status, got: {}",
        result
    );
}
