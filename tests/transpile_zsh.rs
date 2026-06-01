use aethershell::transpile::zsh::transpile_zsh_to_ae;

/// Remove all ASCII whitespace to make tests resilient to formatting.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

#[test]
fn simple_assignment() {
    let zsh = "FOO=bar";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    let ae_n = strip_ws(&ae);
    assert!(ae_n.contains(r#"letFOO="bar";"#), "got:\n{}", ae);
}

#[test]
fn numeric_assignment() {
    let zsh = "N=42";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    let ae_n = strip_ws(&ae);
    assert!(ae_n.contains("letN=42;"), "got:\n{}", ae);
}

#[test]
fn variable_arg_as_identifier() {
    let zsh = "echo $HOME";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(
        ae.contains("echo(HOME)") || ae.contains("echo( HOME )"),
        "got:\n{}",
        ae
    );
}

#[test]
fn pipeline_basic() {
    let zsh = r#"echo "hello $USER" | grep hello"#;
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("echo") && ae.contains("grep"), "got:\n{}", ae);
}

#[test]
fn preserves_comments() {
    let zsh = "# this is a comment\necho hi";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// this is a comment"), "got:\n{}", ae);
}

#[test]
fn setopt_becomes_comment() {
    let zsh = "setopt EXTENDED_GLOB";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// [zsh] setopt EXTENDED_GLOB"), "got:\n{}", ae);
}

#[test]
fn autoload_becomes_comment() {
    let zsh = "autoload -Uz compinit";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// [zsh]"), "got:\n{}", ae);
}

#[test]
fn compdef_becomes_comment() {
    let zsh = "compdef _gnu_generic ls";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// [zsh]"), "got:\n{}", ae);
}

#[test]
fn bindkey_becomes_comment() {
    let zsh = "bindkey '^R' history-incremental-search-backward";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// [zsh]"), "got:\n{}", ae);
}

#[test]
fn typeset_assoc_array() {
    let zsh = "typeset -A mymap";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("let mymap = {};"), "got:\n{}", ae);
}

#[test]
fn local_variable() {
    let zsh = "local x=hello";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("let x ="), "got:\n{}", ae);
}

#[test]
fn export_variable() {
    let zsh = "export PATH=/usr/bin";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("let PATH ="), "got:\n{}", ae);
}

#[test]
fn array_literal() {
    let zsh = "local arr=(one two three)";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("let arr = ["), "got:\n{}", ae);
}

#[test]
fn if_block_accumulation() {
    let zsh = "if [ -f foo ]; then\n  echo yes\nfi";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("sh("), "got:\n{}", ae);
    assert!(ae.contains("if") && ae.contains("fi"), "got:\n{}", ae);
}

#[test]
fn for_loop_block() {
    let zsh = "for f in *.txt; do\n  echo $f\ndone";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("for") && ae.contains("done"), "got:\n{}", ae);
}

#[test]
fn while_loop_block() {
    let zsh = "while true; do\n  echo running\ndone";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("while") && ae.contains("done"), "got:\n{}", ae);
}

#[test]
fn function_block() {
    let zsh = "myfunc() {\n  echo hello\n}";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("myfunc"), "got:\n{}", ae);
}

#[test]
fn case_block() {
    let zsh = "case $x in\n  a) echo a;;\n  *) echo other;;\nesac";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("case") && ae.contains("esac"), "got:\n{}", ae);
}

#[test]
fn fallback_on_redirection() {
    let zsh = "echo hi > out.txt";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("sh("), "got:\n{}", ae);
}

#[test]
fn zstyle_becomes_comment() {
    let zsh = "zstyle ':completion:*' menu select";
    let ae = transpile_zsh_to_ae(zsh).expect("transpile ok");
    assert!(ae.contains("// [zsh]"), "got:\n{}", ae);
}

#[test]
fn transpile_header() {
    let ae = transpile_zsh_to_ae("echo hello").expect("transpile ok");
    assert!(ae.starts_with("// Transpiled from Zsh"), "got:\n{}", ae);
}
