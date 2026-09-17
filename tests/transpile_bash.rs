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

    // echo, grep (str.grep), wc (file.wc) are all mapped to builtins. The
    // interpolated argument now reads the environment rather than emitting the
    // literal `"hello ${USER}"`, which resolved against bindings and rendered
    // `hello null` at runtime.
    let expected = r#"echo("hello " + env("USER", "")) | str.grep("hello") | file.wc("-l")"#;
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
fn var_arg_reads_the_environment() {
    let bash = r#"echo $HOME"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    // This test used to be `var_arg_as_identifier` and asserted `echo(HOME)`:
    // a lone `$VAR` became a bare identifier, on the theory that a preceding
    // `NAME=value` had bound it. Nothing binds `$HOME`, an unbound identifier
    // evaluates to null rather than raising, and so `ae -b -c 'echo $HOME'`
    // printed `null` and exited 0 — a confident wrong answer for the most
    // common expansion in shell. The test passed throughout, because it
    // asserted the emission rather than the answer.
    //
    // `env(name, "")` reads the variable and gives bash's empty-string
    // semantics when it is unset. See tests/bash_compat_variables.rs, which
    // checks the answer and not the shape.
    let expected = strip_ws(r#"echo(env("HOME",""))"#);
    assert!(ae_n.contains(&expected), "got:\n{ae}");
    assert!(
        !ae_n.contains(&strip_ws("echo(HOME)")),
        "the bare-identifier emission is back, which renders null at runtime:\n{ae}"
    );
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

    // Single quotes stay literal; double quotes expand. What changed is *how*
    // they expand: `"c $d"` used to become the literal `"c ${d}"`, which
    // resolved against bindings and rendered `c null`, and `"${e}"` used to
    // collapse to a bare identifier with the same fault. Both now read the
    // environment. The single-quoted arguments are untouched, which is the
    // property this test exists for.
    let expected = strip_ws(r##"echo("a $b", "c " + env("d", ""), env("e", ""), "#{f}")"##);
    assert!(ae_n.contains(&expected), "got:\n{ae}");

    // The point of the test: `'a $b'` must not have expanded.
    assert!(
        ae_n.contains(&strip_ws(r#""a $b""#)),
        "single quotes expanded:\n{ae}"
    );
}

#[test]
fn builtin_mapped_ls() {
    let bash = r#"ls -la"#;
    let ae = transpile_bash_to_ae(bash).expect("transpile ok");
    let ae_n = strip_ws(&ae);

    let expected = r#"ls("-la")"#;
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
