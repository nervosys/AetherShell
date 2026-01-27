// tests/platform_detection.rs
//! Tests for platform detection functions: platform, is_windows, is_linux, etc.

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
// platform() tests
// ============================================================================

#[test]
fn test_platform_returns_string() {
    let result = run("platform()");
    match result {
        Value::Str(s) => {
            // Should be one of the valid platform names
            assert!(
                ["windows", "linux", "macos", "bsd", "ios", "android"].contains(&s.as_str()),
                "Unexpected platform: {}",
                s
            );
        }
        _ => panic!("Expected String, got {:?}", result),
    }
}

#[test]
fn test_platform_matches_current_os() {
    let result = run("platform()");
    if let Value::Str(platform) = result {
        // Verify it matches the compile-time OS
        #[cfg(target_os = "windows")]
        assert_eq!(platform, "windows");
        #[cfg(target_os = "linux")]
        {
            // Could be linux or android
            assert!(platform == "linux" || platform == "android");
        }
        #[cfg(target_os = "macos")]
        assert_eq!(platform, "macos");
        #[cfg(target_os = "freebsd")]
        assert_eq!(platform, "bsd");
    }
}

// ============================================================================
// is_windows() tests
// ============================================================================

#[test]
fn test_is_windows_returns_bool() {
    let result = run("is_windows()");
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_is_windows_matches_compile_time() {
    let result = run("is_windows()");
    if let Value::Bool(is_win) = result {
        #[cfg(target_os = "windows")]
        assert!(is_win, "Should be true on Windows");
        #[cfg(not(target_os = "windows"))]
        assert!(!is_win, "Should be false on non-Windows");
    }
}

// ============================================================================
// is_linux() tests
// ============================================================================

#[test]
fn test_is_linux_returns_bool() {
    let result = run("is_linux()");
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_is_linux_matches_compile_time() {
    let result = run("is_linux()");
    if let Value::Bool(is_lin) = result {
        // Note: On Android (which is technically Linux), this returns false
        // because we exclude Android from "is_linux"
        #[cfg(target_os = "linux")]
        {
            // This test runs on regular Linux, not Android in CI
            assert!(is_lin, "Should be true on Linux");
        }
        #[cfg(not(target_os = "linux"))]
        assert!(!is_lin, "Should be false on non-Linux");
    }
}

// ============================================================================
// is_macos() tests
// ============================================================================

#[test]
fn test_is_macos_returns_bool() {
    let result = run("is_macos()");
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_is_macos_matches_compile_time() {
    let result = run("is_macos()");
    if let Value::Bool(is_mac) = result {
        #[cfg(target_os = "macos")]
        assert!(is_mac, "Should be true on macOS");
        #[cfg(not(target_os = "macos"))]
        assert!(!is_mac, "Should be false on non-macOS");
    }
}

// ============================================================================
// is_unix() tests
// ============================================================================

#[test]
fn test_is_unix_returns_bool() {
    let result = run("is_unix()");
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_is_unix_matches_compile_time() {
    let result = run("is_unix()");
    if let Value::Bool(is_unix) = result {
        #[cfg(unix)]
        assert!(is_unix, "Should be true on Unix-like systems");
        #[cfg(not(unix))]
        assert!(!is_unix, "Should be false on non-Unix");
    }
}

#[test]
fn test_is_unix_true_for_linux_and_macos() {
    // If platform is linux or macos, is_unix should be true
    let platform = run("platform()");
    let is_unix = run("is_unix()");

    if let (Value::Str(p), Value::Bool(u)) = (platform, is_unix) {
        if p == "linux" || p == "macos" || p == "bsd" || p == "android" || p == "ios" {
            assert!(u, "is_unix should be true for {}", p);
        }
        if p == "windows" {
            assert!(!u, "is_unix should be false for windows");
        }
    }
}

// ============================================================================
// is_bsd() tests
// ============================================================================

#[test]
fn test_is_bsd_returns_bool() {
    let result = run("is_bsd()");
    match result {
        Value::Bool(_) => (), // Expected
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

// ============================================================================
// platform_module() tests
// ============================================================================

#[test]
fn test_platform_module_returns_string() {
    let result = run(r#"platform_module("utils")"#);
    match result {
        Value::Str(s) => {
            assert!(s.ends_with(".ae"), "Should end with .ae");
            assert!(s.starts_with("utils_"), "Should start with utils_");
        }
        _ => panic!("Expected String, got {:?}", result),
    }
}

#[test]
fn test_platform_module_includes_platform_suffix() {
    let platform = run("platform()");
    let module_path = run(r#"platform_module("mymodule")"#);

    if let (Value::Str(p), Value::Str(m)) = (platform, module_path) {
        let expected = format!("mymodule_{}.ae", p);
        assert_eq!(m, expected, "Module path should include platform suffix");
    }
}

#[test]
fn test_platform_module_strips_ae_extension() {
    let result = run(r#"platform_module("utils.ae")"#);
    if let Value::Str(m) = result {
        // Should not have double .ae
        assert!(!m.contains(".ae_"), "Should strip .ae before adding suffix");
    }
}

#[test]
fn test_platform_module_with_path() {
    let result = run(r#"platform_module("lib/utils")"#);
    if let Value::Str(m) = result {
        assert!(m.starts_with("lib/utils_"), "Should preserve path prefix");
    }
}

#[test]
fn test_platform_module_piped() {
    let result = run(r#""mylib" | platform_module"#);
    match result {
        Value::Str(s) => {
            assert!(s.starts_with("mylib_"), "Should work with piped input");
        }
        _ => panic!("Expected String"),
    }
}

// ============================================================================
// Integration tests
// ============================================================================

#[test]
fn test_platform_conditional_logic() {
    let result = run(r#"
        let win = is_windows()
        match win { true => "win_path", _ => "unix_path" }
    "#);
    if let Value::Str(s) = result {
        #[cfg(target_os = "windows")]
        assert_eq!(s, "win_path");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(s, "unix_path");
    }
}

#[test]
fn test_platform_path_separator() {
    let result = run(r#"
        let win = is_windows()
        match win { true => "\\", _ => "/" }
    "#);
    if let Value::Str(s) = result {
        #[cfg(target_os = "windows")]
        assert_eq!(s, "\\");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(s, "/");
    }
}

#[test]
fn test_platform_in_record() {
    let result = run(r#"
        {
            platform: platform(),
            is_unix: is_unix(),
            is_windows: is_windows()
        }
    "#);
    match result {
        Value::Record(rec) => {
            assert!(rec.contains_key("platform"));
            assert!(rec.contains_key("is_unix"));
            assert!(rec.contains_key("is_windows"));
        }
        _ => panic!("Expected Record"),
    }
}

#[test]
fn test_all_platform_checks_are_exclusive() {
    // At least one platform check should be true on supported platforms
    let result = run(r#"
        let checks = [is_windows(), is_linux(), is_macos(), is_bsd()]
        checks | any(fn(x) => x)
    "#);
    // Note: This might be false on iOS/Android since we don't have is_ios/is_android
    // But for common platforms (Windows/Linux/macOS/BSD), one should be true
    match result {
        Value::Bool(b) => {
            #[cfg(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "macos",
                target_os = "freebsd"
            ))]
            assert!(b, "At least one platform check should be true");
        }
        _ => panic!("Expected Bool, got {:?}", result),
    }
}
