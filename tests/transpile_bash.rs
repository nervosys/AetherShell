#[test]
fn command_substitution() {
    let bash = r#"echo $(date)"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // Accept tokenized or single sh(...) form; ensure the sh call and $(date) appear
    assert!(
        ae.contains("sh([") && ae.contains("$(date)"),
        "got:\n{}",
        ae
    );
}

#[test]
fn arithmetic_expansion() {
    let bash = r#"echo $((1 + 2))"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // Transpiler may preserve spaces or collapse them; normalize and check core token
    let ae_n = strip_ws(&ae);
    assert!(
        ae_n.contains("$((1+2))") || ae.contains("$((1 + 2))"),
        "got:\n{}",
        ae
    );
}

#[test]
fn if_else() {
    let bash = r#"if [ -f foo ]; then echo yes; else echo no; fi"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // Check for tokenized if/then/else/fi presence inside sh wrapper
    assert!(
        ae.contains("if") && ae.contains("then") && ae.contains("else") && ae.contains("fi"),
        "got:\n{}",
        ae
    );
}

#[test]
fn for_loop() {
    let bash = r#"for f in *.txt; do echo $f; done"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // Ensure the loop keywords and glob appear in the transpiled output
    assert!(
        ae.contains("for") && ae.contains("do") && (ae.contains("*.txt") || ae.contains("*.txt;")),
        "got:\n{}",
        ae
    );
}

#[test]
fn while_loop() {
    let bash = r#"while true; do echo hi; done"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    assert!(
        ae.contains("while") && ae.contains("do") && ae.contains("done"),
        "got:\n{}",
        ae
    );
}

#[test]
fn bash_function() {
    let bash = r#"myfunc() { echo hi; }"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // The transpiler may tokenize the function; ensure function name appears and a sh call emitted
    assert!(
        ae.contains("myfunc()") && ae.contains("sh(["),
        "got:\n{}",
        ae
    );
}

#[test]
fn arrays() {
    let bash = r#"arr=(a b c); echo ${arr[0]}"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);
    assert!(
        ae_n.contains("arr=(a") && (ae.contains("arr[0]") || ae.contains("${arr[0]}")),
        "got:\n{}",
        ae
    );
}

#[test]
fn heredoc() {
    let bash = "cat <<EOF\nhello\nEOF";
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    // transpiler emits separate sh calls for the heredoc start/body/end; check for them
    assert!(
        ae.contains("cat <<EOF") && ae.contains("hello") && ae.contains("EOF"),
        "got:\n{}",
        ae
    );
}

#[test]
fn subshell() {
    let bash = r#"(echo hi)"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    assert!(ae.contains("(echo") && ae.contains("hi"), "got:\n{}", ae);
}

#[test]
fn test_brackets() {
    let bash = r#"if [[ $x -eq 1 ]]; then echo one; fi"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    assert!(
        ae.contains("[[") && ae.contains("]]") && ae.contains("-eq"),
        "got:\n{}",
        ae
    );
}
use aethershell::transpile::bash::transpile_bash_to_ae;

/// Remove all ASCII whitespace to make tests resilient to formatting.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

#[test]
fn pipeline_basic() {
    let bash = r#"echo "hello $USER" | grep hello | wc -l"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    // echo is mapped to builtin; others fall back to sh([...])
    let expected = r#"echo("hello ${USER}") | sh(["grep","hello"]) | sh(["wc","-l"])"#;
    let ex_n = strip_ws(expected);

    assert!(ae_n.contains(&ex_n), "got:\n{ae}");
}

#[test]
fn simple_assignment() {
    let bash = r#"FOO=bar"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    let expected = r#"let FOO = "bar";"#;
    let ex_n = strip_ws(expected);

    assert!(ae_n.contains(&ex_n), "got:\n{ae}");
}

#[test]
fn var_arg_as_identifier() {
    let bash = r#"echo $HOME"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    // Single $VAR becomes identifier (HOME), not an interpolated string
    let expected1 = r#"echo(HOME)"#;
    let expected2 = r#"echo(HOME);"#;
    let ex1_n = strip_ws(expected1);
    let ex2_n = strip_ws(expected2);

    assert!(ae_n.contains(&ex1_n) || ae_n.contains(&ex2_n), "got:\n{ae}");
}

#[test]
fn fallback_on_redirection() {
    let bash = r#"echo hi > out.txt"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    let expected = r#"sh(["bash","-lc","echo hi > out.txt"]);"#;
    let ex_n = strip_ws(expected);

    assert!(ae_n.contains(&ex_n), "got:\n{ae}");
}

#[test]
fn single_vs_double_quotes() {
    // Use r##...## because the content includes `"#{f}"`, which contains `"#`
    // and would prematurely close an r#"..."# literal.
    let bash = r##"echo 'a $b' "c $d" "${e}" '#{f}'"##;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    // Expected: single quotes literal; double quotes expand;
    // "${e}" -> bare identifier e (our transpiler collapses var-only tokens).
    let expected1 = r##"echo("a $b", "c ${d}", e, "#{f}")"##;
    let expected2 = r##"echo("a $b", "c ${d}", e, "#{f}");"##;
    let ex1_n = strip_ws(expected1);
    let ex2_n = strip_ws(expected2);

    assert!(ae_n.contains(&ex1_n) || ae_n.contains(&ex2_n), "got:\n{ae}");
}

#[test]
fn external_cmd_defaults_to_sh() {
    let bash = r#"ls -la"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    let expected = r#"sh(["ls","-la"])"#;
    let ex_n = strip_ws(expected);

    assert!(ae_n.contains(&ex_n), "got:\n{ae}");
}

#[test]
fn preserves_comments_as_aether_comments() {
    let bash = r#"
# heading
echo "x"
"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    assert!(ae.contains("// heading"), "got:\n{ae}");
    assert!(ae.contains(r#"echo("x")"#), "got:\n{ae}");
}
