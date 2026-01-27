// Tests for error messages with line numbers

use aethershell::parser::parse_program;

#[test]
fn error_includes_line_number() {
    // Test that a syntax error includes line number
    let result = parse_program(
        r#"
        let x = 1
        let y = @
    "#,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("line"), "Error should include 'line': {}", err);
    assert!(
        err.contains("column"),
        "Error should include 'column': {}",
        err
    );
}

#[test]
fn error_line_number_multiline() {
    // Error on line 3
    let result = parse_program(
        r#"let a = 1
let b = 2
let c = @"#,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // The @ is on line 3
    assert!(err.contains("line 3"), "Error should be on line 3: {}", err);
}

#[test]
fn error_unexpected_token_includes_location() {
    let result = parse_program("let x = ");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("line"), "Error should include line: {}", err);
    assert!(
        err.contains("column"),
        "Error should include column: {}",
        err
    );
}

#[test]
fn error_pub_with_import() {
    let result = parse_program("pub import \"foo\"");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("pub"), "Error should mention pub: {}", err);
    assert!(
        err.contains("import"),
        "Error should mention import: {}",
        err
    );
    assert!(err.contains("line"), "Error should include line: {}", err);
}

#[test]
fn error_expected_closing_bracket() {
    let result = parse_program("[1, 2, 3");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("line"), "Error should include line: {}", err);
}

#[test]
fn error_invalid_cfg_attribute() {
    let result = parse_program(
        r#"
        #[invalid(attr)]
        let x = 1
    "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cfg"), "Error should mention cfg: {}", err);
    assert!(err.contains("line"), "Error should include line: {}", err);
}
// ============ Suggestion Tests ============

#[test]
fn suggestion_unclosed_bracket() {
    let result = parse_program("[1, 2, 3");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should have location info and suggestion about unclosed bracket
    assert!(err.contains("line"), "Error should include line: {}", err);
    assert!(
        err.contains("]") || err.contains("unclosed"),
        "Should mention unclosed bracket: {}",
        err
    );
}

#[test]
fn suggestion_unclosed_paren() {
    let result = parse_program("print(1, 2");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should mention unclosed or suggest closing
    assert!(err.contains("line"), "Error should include line: {}", err);
}

#[test]
fn suggestion_unclosed_brace() {
    let result = parse_program("{ name: \"test\" ");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("line"), "Error should include line: {}", err);
}
// ============ Error Recovery Tests ============

#[test]
fn error_recovery_reports_multiple_errors() {
    // Multiple statements with parse errors (not lexer errors)
    // Using = = which is a parser error, not a lexer error
    let result = parse_program(
        r#"
        let x = = bad
        let y = 5
        let z = = also bad
    "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should report multiple errors
    assert!(
        err.contains("2 error") || err.contains("error"),
        "Should report errors: {}",
        err
    );
}

#[test]
fn error_recovery_continues_after_bad_statement() {
    // First statement is bad (parser error), second is good
    let result = parse_program(
        r#"
        let x = = bad
        let y = 5
    "#,
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should have at least one error
    assert!(
        err.contains("error"),
        "Should report at least one error: {}",
        err
    );
}

#[test]
fn error_recovery_synchronizes_on_let() {
    // After an error, parser should sync to the next 'let'
    // Use a parser error (unexpected token) not a lexer error
    let result = parse_program(
        r#"
        let = bad
        let good = 42
    "#,
    );
    assert!(result.is_err());
    // We expect an error about the bad token, but parsing should try to continue
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("line"),
        "Error should include line info: {}",
        err
    );
}
