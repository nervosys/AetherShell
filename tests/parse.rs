use aurora_shell::parser;

#[test]
fn lambda_no_space_is_ok() {
    let code = r#"map(fn(x)=> x*2)"#;
    let stmts = parser::parse_program(code).unwrap();
    assert_eq!(stmts.len(), 1);
}

#[test]
fn precedence_and_pipes() {
    // 1 + 2 * 3 | print  →  7 printed by builtin
    let code = r#"(1 + 2 * 3) | print"#;
    let stmts = parser::parse_program(code).unwrap();
    assert_eq!(stmts.len(), 1);
}

#[test]
fn arrays_records_parse() {
    let code = r#"[1,2,3] | map(fn(x)=> x+1) ; {"a": 1, "b": 2}"#;
    let stmts = parser::parse_program(code).unwrap();
    assert_eq!(stmts.len(), 2);
}
