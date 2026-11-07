//! Exponent number literals, run through the real binary.
//!
//! Reproduced on 14.0.0 in a script file: `x = 1e10` then `print(x)` printed
//! `1` with exit 0, `print(1.5e3)` was E_PARSE "expected ')'", and
//! `y = 2.5E-3` was E_UNKNOWN_BUILTIN "unknown builtin: E". The lexer read the
//! `e` as the start of an identifier. The unit tests in src/parser.rs pin the
//! tokens; these tests pin what a script prints.

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_numlit_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Runs `source` as a script file and returns (stdout, stderr, exit code).
fn run_script(tag: &str, source: &str) -> (String, String, i32) {
    let dir = scratch(tag);
    let script = dir.join("literals.ish");
    std::fs::write(&script, source).expect("write script");
    let out = Command::new(env!("CARGO_BIN_EXE_ish"))
        .arg(&script)
        .env("NO_COLOR", "1")
        .env("IRONSHELL_SESSION_DIR", dir.join("session"))
        .output()
        .expect("run ish");
    let _ = std::fs::remove_dir_all(&dir);
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_bare_exponent_literal_prints_its_value() {
    let (out, err, code) = run_script("bare", "x = 1e10\nprint(x)\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, "10000000000", "1e10 must be the float 1e10, not 1");
}

#[test]
fn an_exponent_literal_inside_a_call_prints_its_value() {
    let (out, err, code) = run_script("call", "print(1.5e3)\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, "1500");
}

#[test]
fn a_negative_exponent_and_uppercase_e_are_floats() {
    let (out, err, code) = run_script("negexp", "y = 2.5E-3\nprint(y)\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, (2.5e-3_f64).to_string());
}

#[test]
fn an_explicit_plus_exponent_and_a_large_exponent() {
    let (out, err, code) = run_script("plus", "print(1e+3)\nprint(6.02e23)\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, format!("1000\n{}", 6.02e23_f64));
}

#[test]
fn a_number_running_into_an_identifier_is_a_parse_error_before_anything_runs() {
    // The print must not run: a malformed number is refused at parse time, so
    // the script never starts and nothing is printed.
    let (out, err, code) = run_script("bad", "print(\"ran\")\nx = 12abc\n");
    assert_ne!(code, 0, "a malformed literal must fail: stdout {out}");
    assert!(!out.contains("ran"), "the script must not run: {out}");
    let all = format!("{out}\n{err}");
    assert!(all.contains("E_PARSE"), "expected E_PARSE: {all}");
    assert!(
        all.contains("malformed number"),
        "expected a clear message: {all}"
    );
}

#[test]
fn a_dangling_exponent_marker_is_a_parse_error() {
    for source in ["x = 1e\n", "x = 1ex\n"] {
        let (out, err, code) = run_script("dangling", source);
        assert_ne!(code, 0, "{source:?} must fail: {out}");
        let all = format!("{out}\n{err}");
        assert!(all.contains("E_PARSE"), "{source:?}: {all}");
    }
}
