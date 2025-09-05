use aurora_shell::transpile::bash::transpile_bash_to_ae;

/// Remove all ASCII whitespace to make tests resilient to formatting.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

#[test]
fn pipeline_basic() {
    let bash = r#"echo "hello $USER" | grep hello | wc -l"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    // echo is mapped to builtin; others fall back to sh([...])
    let expected = r#"echo("hello ${USER}") | sh(["grep","hello"]) | sh(["wc","-l"])"#;
    let ex_n = strip_ws(expected);

    assert!(au_n.contains(&ex_n), "got:\n{au}");
}

#[test]
fn simple_assignment() {
    let bash = r#"FOO=bar"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    let expected = r#"let FOO = "bar";"#;
    let ex_n = strip_ws(expected);

    assert!(au_n.contains(&ex_n), "got:\n{au}");
}

#[test]
fn var_arg_as_identifier() {
    let bash = r#"echo $HOME"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    // Single $VAR becomes identifier (HOME), not an interpolated string
    let expected1 = r#"echo(HOME)"#;
    let expected2 = r#"echo(HOME);"#;
    let ex1_n = strip_ws(expected1);
    let ex2_n = strip_ws(expected2);

    assert!(au_n.contains(&ex1_n) || au_n.contains(&ex2_n), "got:\n{au}");
}

#[test]
fn fallback_on_redirection() {
    let bash = r#"echo hi > out.txt"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    let expected = r#"sh(["bash","-lc","echo hi > out.txt"]);"#;
    let ex_n = strip_ws(expected);

    assert!(au_n.contains(&ex_n), "got:\n{au}");
}

#[test]
fn single_vs_double_quotes() {
    // Use r##...## because the content includes `"#{f}"`, which contains `"#`
    // and would prematurely close an r#"..."# literal.
    let bash = r##"echo 'a $b' "c $d" "${e}" '#{f}'"##;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    // Expected: single quotes literal; double quotes expand;
    // "${e}" -> bare identifier e (our transpiler collapses var-only tokens).
    let expected1 = r##"echo("a $b", "c ${d}", e, "#{f}")"##;
    let expected2 = r##"echo("a $b", "c ${d}", e, "#{f}");"##;
    let ex1_n = strip_ws(expected1);
    let ex2_n = strip_ws(expected2);

    assert!(au_n.contains(&ex1_n) || au_n.contains(&ex2_n), "got:\n{au}");
}

#[test]
fn external_cmd_defaults_to_sh() {
    let bash = r#"ls -la"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    let au_n = strip_ws(&au);

    let expected = r#"sh(["ls","-la"])"#;
    let ex_n = strip_ws(expected);

    assert!(au_n.contains(&ex_n), "got:\n{au}");
}

#[test]
fn preserves_comments_as_aurora_comments() {
    let bash = r#"
# heading
echo "x"
"#;
    let au = transpile_bash_to_ae(bash).expect("transpile ok");
    assert!(au.contains("// heading"), "got:\n{au}");
    assert!(au.contains(r#"echo("x")"#), "got:\n{au}");
}
