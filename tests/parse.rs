use aethershell::parser;

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

#[test]
fn popular_shell_patterns_parse() {
    // Test ls-like file listing with filtering
    let ls_pattern = r#"files | where(fn(f)=> contains(name(f), "rs")) | map(fn(f)=> name(f))"#;
    let stmts = parser::parse_program(ls_pattern).unwrap();
    assert_eq!(stmts.len(), 1);

    // Test grep-like text search with pipes
    let grep_pattern = r#"lines | where(fn(line)=> contains(line, "pattern")) | take(10)"#;
    let stmts = parser::parse_program(grep_pattern).unwrap();
    assert_eq!(stmts.len(), 1);
}

#[test]
fn loop_patterns_parse() {
    // Test array iteration and transformation
    let for_loop = r#"[1,2,3,4,5] | map(fn(x)=> {"value": x, "doubled": x*2})"#;
    let stmts = parser::parse_program(for_loop).unwrap();
    assert_eq!(stmts.len(), 1);

    // Test conditional filtering and transformation
    let filter_transform =
        r#"data | where(fn(item)=> score(item) > 80) | map(fn(item)=> name(item))"#;
    let stmts = parser::parse_program(filter_transform).unwrap();
    assert_eq!(stmts.len(), 1);
}

#[test]
fn complex_shell_workflows_parse() {
    // Test multi-step data processing pipeline
    let workflow = r#"
        let files = ls("*");
        let errors = files | map(fn(f)=> cat(f)) | where(fn(line)=> contains(line, "ERROR"));
        let summary = errors | map(fn(line)=> split(line, " ")) | take(100);
        summary | sort()
    "#;
    let stmts = parser::parse_program(workflow).unwrap();
    assert_eq!(stmts.len(), 4);
}

#[test]
fn system_command_integration_parse() {
    // Test external command integration with pipes
    let git_workflow =
        r#"sh(["git", "log", "--oneline"]) | take(5) | map(fn(line)=> split(line, " "))"#;
    let stmts = parser::parse_program(git_workflow).unwrap();
    assert_eq!(stmts.len(), 1);

    // Test data processing patterns
    let process_sub =
        r#"lines | where(fn(line)=> starts_with(line, "+")) | map(fn(line)=> trim(line))"#;
    let stmts = parser::parse_program(process_sub).unwrap();
    assert_eq!(stmts.len(), 1);
}

#[test]
fn simple_assignment_syntax() {
    // Test simple assignment without 'let' keyword (type inference sugar)
    let code = r#"x = 3"#;
    let stmts = parser::parse_program(code).unwrap();
    assert_eq!(stmts.len(), 1);

    // Test multiple simple assignments
    let multi = r#"x = 3; y = 5; z = x + y"#;
    let stmts = parser::parse_program(multi).unwrap();
    assert_eq!(stmts.len(), 3);

    // Test mut shorthand
    let mut_code = r#"mut counter = 0"#;
    let stmts = parser::parse_program(mut_code).unwrap();
    assert_eq!(stmts.len(), 1);
}
