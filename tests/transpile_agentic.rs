use aethershell::transpile::agentic::transpile_agentic_to_ae;

/// Behavior harness for the transpiler-retirement refactor: transpile agentic
/// source, then parse + evaluate the resulting `.ae`, returning the value. This
/// asserts *behavior* rather than exact text, so transpiler passes can be thinned
/// (forms the grammar now handles natively) without rewriting every test.
fn eval_aeg(src: &str) -> aethershell::value::Value {
    let ae = transpile_agentic_to_ae(src).expect("transpile");
    let stmts = aethershell::parser::parse_program(&ae).expect("transpiled .ae should parse");
    let mut env = aethershell::env::Env::new();
    aethershell::eval::eval_program(&stmts, &mut env).expect("transpiled .ae should eval")
}

// ── SI suffix expansion ──────────────────────────────────────────────
// Behavior assertions: `1k`/`5M`/`2G` evaluate to scaled integers. SI scaling
// now lives in the grammar lexer (`read_number`), so the transpiler no longer
// rewrites these — proving the pass is retired without behavior change.

#[test]
fn si_suffix_k() {
    // Bare expression (single-letter names are builtin shorthands, so avoid them).
    assert_eq!(eval_aeg("1k\n"), aethershell::value::Value::Int(1000));
}

#[test]
fn si_suffix_m() {
    assert_eq!(eval_aeg("5M\n"), aethershell::value::Value::Int(5_000_000));
}

#[test]
fn si_suffix_g() {
    assert_eq!(eval_aeg("2G\n"), aethershell::value::Value::Int(2_000_000_000));
}

#[test]
fn si_suffix_not_in_string() {
    let ae = transpile_agentic_to_ae("#e \"1k\"\n").unwrap();
    assert!(ae.contains("\"1k\""), "got:\n{ae}");
}

// ── Lambda expansion ─────────────────────────────────────────────────

#[test]
fn lambda_single_param() {
    let ae = transpile_agentic_to_ae("#m \\x:x*2\n").unwrap();
    assert!(ae.contains("fn(x) => x*2"), "got:\n{ae}");
}

#[test]
fn lambda_multi_param() {
    let ae = transpile_agentic_to_ae("#r \\a,b:a+b\n").unwrap();
    assert!(ae.contains("fn(a, b) => a+b"), "got:\n{ae}");
}

#[test]
fn lambda_implicit_param() {
    let ae = transpile_agentic_to_ae("#w \\.size>100\n").unwrap();
    assert!(ae.contains("fn(__) => __.size>100"), "got:\n{ae}");
}

// ── Module sigil expansion ───────────────────────────────────────────

#[test]
fn module_file() {
    let ae = transpile_agentic_to_ae("@f.read(\"p\")\n").unwrap();
    assert!(ae.contains("file.read(\"p\")"), "got:\n{ae}");
}

#[test]
fn module_sys() {
    let ae = transpile_agentic_to_ae("@s.hostname()\n").unwrap();
    assert!(ae.contains("sys.hostname()"), "got:\n{ae}");
}

#[test]
fn module_http() {
    let ae = transpile_agentic_to_ae("@h.get(url)\n").unwrap();
    assert!(ae.contains("http.get(url)"), "got:\n{ae}");
}

#[test]
fn module_ai_direct_call() {
    let ae = transpile_agentic_to_ae("@ai(\"prompt\")\n").unwrap();
    assert!(ae.contains("ai(\"prompt\")"), "got:\n{ae}");
}

#[test]
fn module_docker() {
    let ae = transpile_agentic_to_ae("@dk.ps()\n").unwrap();
    assert!(ae.contains("docker.ps()"), "got:\n{ae}");
}

#[test]
fn module_k8s() {
    let ae = transpile_agentic_to_ae("@k.pods()\n").unwrap();
    assert!(ae.contains("k8s.pods()"), "got:\n{ae}");
}

// ── Builtin shorthand expansion ──────────────────────────────────────

#[test]
fn builtin_echo() {
    let ae = transpile_agentic_to_ae("#e \"hello\"\n").unwrap();
    assert!(ae.contains("echo(\"hello\")"), "got:\n{ae}");
}

#[test]
fn builtin_ls() {
    let ae = transpile_agentic_to_ae("#l \".\"\n").unwrap();
    assert!(ae.contains("ls(\".\")"), "got:\n{ae}");
}

#[test]
fn builtin_take() {
    let ae = transpile_agentic_to_ae("#t 5\n").unwrap();
    assert!(ae.contains("take(5)"), "got:\n{ae}");
}

#[test]
fn builtin_grep() {
    let ae = transpile_agentic_to_ae("#g \"pattern\"\n").unwrap();
    assert!(ae.contains("grep(\"pattern\")"), "got:\n{ae}");
}

#[test]
fn builtin_no_args() {
    let ae = transpile_agentic_to_ae("#k\n").unwrap();
    assert!(ae.contains("keys()"), "got:\n{ae}");
}

// ── Pipeline expansion ──────────────────────────────────────────────

#[test]
fn pipeline_single() {
    let ae = transpile_agentic_to_ae("a > b\n").unwrap();
    assert!(ae.contains(" | "), "got:\n{ae}");
    assert!(!ae.contains(">"), "should not contain raw > : got:\n{ae}");
}

#[test]
fn pipeline_preserves_gte() {
    let ae = transpile_agentic_to_ae("x >= 5\n").unwrap();
    assert!(ae.contains(">="), "got:\n{ae}");
}

#[test]
fn pipeline_chained() {
    let ae = transpile_agentic_to_ae("#l \".\" > #w \\.size>1000 > #m \\.name\n").unwrap();
    assert!(ae.contains("ls("), "got:\n{ae}");
    assert!(ae.contains("where("), "got:\n{ae}");
    assert!(ae.contains("map("), "got:\n{ae}");
    // Should contain pipe operators
    assert!(ae.contains(" | "), "got:\n{ae}");
}

// ── Assignment expansion ─────────────────────────────────────────────

#[test]
fn assignment_immutable() {
    let ae = transpile_agentic_to_ae("x=42\n").unwrap();
    assert!(ae.contains("let x = 42"), "got:\n{ae}");
}

#[test]
fn assignment_mutable() {
    let ae = transpile_agentic_to_ae("counter:=0\n").unwrap();
    assert!(ae.contains("let mut counter = 0"), "got:\n{ae}");
}

#[test]
fn assignment_not_equality() {
    let ae = transpile_agentic_to_ae("x==42\n").unwrap();
    assert!(
        !ae.contains("let"),
        "should not produce let for ==: got:\n{ae}"
    );
}

// ── Match expansion ──────────────────────────────────────────────────

#[test]
fn match_basic() {
    // `?` match shorthand is parsed natively by the grammar now (transpiler
    // passes it through), so assert behavior: scrutinee 2 selects the "b" arm.
    assert_eq!(
        eval_aeg("?2{1=>\"a\",2=>\"b\",_=>\"z\"}\n"),
        aethershell::value::Value::Str("b".to_string())
    );
}

// ── Try/catch expansion ──────────────────────────────────────────────

#[test]
fn try_catch_basic() {
    let ae = transpile_agentic_to_ae("!{risky()}{\"fallback\"}\n").unwrap();
    assert!(ae.contains("try"), "got:\n{ae}");
    assert!(ae.contains("catch"), "got:\n{ae}");
    assert!(ae.contains("risky()"), "got:\n{ae}");
    assert!(ae.contains("fallback"), "got:\n{ae}");
}

// ── Comments ─────────────────────────────────────────────────────────

#[test]
fn comment_conversion() {
    let ae = transpile_agentic_to_ae("; This is a comment\n").unwrap();
    assert!(ae.contains("// This is a comment"), "got:\n{ae}");
}

// ── Full integration examples ────────────────────────────────────────

#[test]
fn full_agent_workflow() {
    let input = r#"; Find large .rs files and list names
#l "./src" > #w \.size>1k > #m \.name
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("// Find large .rs files"), "got:\n{ae}");
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains(" | "), "got:\n{ae}");
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) =>"), "got:\n{ae}");
}

#[test]
fn full_http_pipeline() {
    let input = "@h.get(\"https://api.com/data\") > @j.parse(resp)\n";
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("http.get("), "got:\n{ae}");
    assert!(ae.contains("json.parse("), "got:\n{ae}");
    assert!(ae.contains(" | "), "got:\n{ae}");
}

#[test]
fn full_mixed_syntax() {
    let input = r#"data=@f.read("config.json")
parsed=@j.parse(data)
name=parsed.name
#e "Hello ${name}"
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("let data = file.read"), "got:\n{ae}");
    assert!(ae.contains("let parsed = json.parse"), "got:\n{ae}");
    assert!(ae.contains("let name ="), "got:\n{ae}");
    assert!(ae.contains("echo(\"Hello ${name}\")"), "got:\n{ae}");
}

// ── v2 Ultra-compressed integration tests ────────────────────────────

#[test]
fn v2_tilde_lambda_single() {
    let ae = transpile_agentic_to_ae("#m ~x:x*2\n").unwrap();
    assert!(ae.contains("fn(x) => x*2"), "got:\n{ae}");
}

#[test]
fn v2_tilde_lambda_implicit() {
    let ae = transpile_agentic_to_ae("#w ~.size>100\n").unwrap();
    assert!(ae.contains("fn(__) => __.size>100"), "got:\n{ae}");
}

#[test]
fn v2_pipe_bar() {
    let ae = transpile_agentic_to_ae("a|b\n").unwrap();
    assert!(ae.contains(" | "), "got:\n{ae}");
}

#[test]
fn v2_bare_builtin_echo() {
    let ae = transpile_agentic_to_ae("e\"hello\"\n").unwrap();
    assert!(ae.contains("echo(\"hello\")"), "got:\n{ae}");
}

#[test]
fn v2_bare_builtin_ls() {
    let ae = transpile_agentic_to_ae("l\".\"\n").unwrap();
    assert!(ae.contains("ls(\".\")"), "got:\n{ae}");
}

#[test]
fn v2_bare_builtin_with_tilde() {
    let ae = transpile_agentic_to_ae("w~.size>100\n").unwrap();
    assert!(ae.contains("where(fn(__) => __.size>100)"), "got:\n{ae}");
}

#[test]
fn v2_bare_builtin_numeric_arg() {
    let ae = transpile_agentic_to_ae("t5\n").unwrap();
    assert!(ae.contains("take(5)"), "got:\n{ae}");
}

#[test]
fn v2_bare_module_file() {
    let ae = transpile_agentic_to_ae("F.read(\"p\")\n").unwrap();
    assert!(ae.contains("file.read(\"p\")"), "got:\n{ae}");
}

#[test]
fn v2_bare_module_http() {
    let ae = transpile_agentic_to_ae("H.get(url)\n").unwrap();
    assert!(ae.contains("http.get(url)"), "got:\n{ae}");
}

#[test]
fn v2_bare_module_docker() {
    let ae = transpile_agentic_to_ae("DK.ps()\n").unwrap();
    assert!(ae.contains("docker.ps()"), "got:\n{ae}");
}

#[test]
fn v2_func_abbreviation() {
    let ae = transpile_agentic_to_ae("F.r(\"README.md\")\n").unwrap();
    assert!(ae.contains("file.read(\"README.md\")"), "got:\n{ae}");
}

#[test]
fn v2_func_abbreviation_json() {
    let ae = transpile_agentic_to_ae("J.p(data)\n").unwrap();
    assert!(ae.contains("json.parse(data)"), "got:\n{ae}");
}

#[test]
fn v2_func_abbreviation_sys() {
    let ae = transpile_agentic_to_ae("S.h()\n").unwrap();
    assert!(ae.contains("sys.hostname()"), "got:\n{ae}");
}

#[test]
fn v2_ultra_pipeline() {
    let ae = transpile_agentic_to_ae("l\"./src\"|w~.size>1k|m~.name\n").unwrap();
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains(" | "), "got:\n{ae}");
}

#[test]
fn v2_mixed_bare_and_v1() {
    // v1 and v2 syntax mixed in one script
    let input = r#"e"start"
#l "./src" > #w \.size>1k
F.r("config.json")
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("echo(\"start\")"), "got:\n{ae}");
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("file.read(\"config.json\")"), "got:\n{ae}");
}

#[test]
fn v2_full_workflow() {
    let input = r#"; Ultra-compressed agent workflow
F.r("input.json")|J.p(data)|w~.size>1k|m~.name
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("// Ultra-compressed"), "got:\n{ae}");
    assert!(ae.contains("file.read("), "got:\n{ae}");
    assert!(ae.contains("json.parse("), "got:\n{ae}");
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) =>"), "got:\n{ae}");
}

// ── v3 symbol→value integration tests ───────────────────────────────

#[test]
fn v3_symbol_true_null() {
    let ae = transpile_agentic_to_ae("x=T\ny=N\n").unwrap();
    assert!(ae.contains("let x = true"), "got:\n{ae}");
    assert!(ae.contains("let y = null"), "got:\n{ae}");
}

#[test]
fn v3_single_quote_strings() {
    let ae = transpile_agentic_to_ae("e'hello world'\n").unwrap();
    assert!(ae.contains("echo(\"hello world\")"), "got:\n{ae}");
}

#[test]
fn v3_backtick_exec() {
    let ae = transpile_agentic_to_ae("x=`uname -a`\n").unwrap();
    assert!(ae.contains("sh(\"uname -a\")"), "got:\n{ae}");
}

#[test]
fn v3_bare_path_relative() {
    let ae = transpile_agentic_to_ae("l./src\n").unwrap();
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
}

#[test]
fn v3_bare_path_absolute() {
    let ae = transpile_agentic_to_ae("l/usr/bin\n").unwrap();
    assert!(ae.contains("ls(\"/usr/bin\")"), "got:\n{ae}");
}

#[test]
fn v3_bare_glob() {
    let ae = transpile_agentic_to_ae("g*.rs\n").unwrap();
    assert!(ae.contains("grep(\"*.rs\")"), "got:\n{ae}");
}

#[test]
fn v3_bare_path_pipeline() {
    let ae = transpile_agentic_to_ae("l./src|w~.size>1k|m~.name\n").unwrap();
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) =>"), "got:\n{ae}");
}

#[test]
fn v3_full_workflow() {
    let input = r#"; v3 maximum density
active=T
data=N
l./src|w~.size>1k|m~.name
result=`uname -a`
e'done'
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("let active = true"), "got:\n{ae}");
    assert!(ae.contains("let data = null"), "got:\n{ae}");
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("sh(\"uname -a\")"), "got:\n{ae}");
    assert!(ae.contains("echo(\"done\")"), "got:\n{ae}");
}

// ── v4 compactness + expandability integration tests ────────────────

#[test]
fn v4_env_var_standalone() {
    let ae = transpile_agentic_to_ae("x=$HOME\n").unwrap();
    assert!(ae.contains("sys.env(\"HOME\")"), "got:\n{ae}");
}

#[test]
fn v4_env_var_in_pipeline() {
    let ae = transpile_agentic_to_ae("e$USER\n").unwrap();
    assert!(ae.contains("echo(sys.env(\"USER\"))"), "got:\n{ae}");
}

#[test]
fn v4_field_projection() {
    let ae = transpile_agentic_to_ae("l\"./src\"|.name\n").unwrap();
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.name)"), "got:\n{ae}");
}

#[test]
fn v4_field_projection_chained() {
    let ae = transpile_agentic_to_ae("l\"./src\"|.data.items\n").unwrap();
    assert!(ae.contains("map(fn(__) => __.data.items)"), "got:\n{ae}");
}

#[test]
fn v4_field_projection_with_filter() {
    let ae = transpile_agentic_to_ae("l\"./src\"|w~.size>1k|.name\n").unwrap();
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.name)"), "got:\n{ae}");
}

#[test]
fn v4_conditional_simple() {
    let ae = transpile_agentic_to_ae("^x>0{x*2}\n").unwrap();
    assert!(
        ae.contains("match (x>0) { true => (x*2), _ => null }"),
        "got:\n{ae}"
    );
}

#[test]
fn v4_conditional_with_else() {
    let ae = transpile_agentic_to_ae("^x>0{x*2}{0}\n").unwrap();
    assert!(
        ae.contains("match (x>0) { true => (x*2), _ => (0) }"),
        "got:\n{ae}"
    );
}

#[test]
fn v4_preamble_def() {
    let input = "%def fetch H.g\nfetch(\"https://api.com\")\n";
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("http.get(\"https://api.com\")"), "got:\n{ae}");
}

#[test]
fn v4_preamble_multiple() {
    let input = "%def fetch H.g\n%def parse J.p\nfetch(\"url\")|parse(data)|.items\n";
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("http.get(\"url\")"), "got:\n{ae}");
    assert!(ae.contains("json.parse(data)"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.items)"), "got:\n{ae}");
}

#[test]
fn v4_full_workflow() {
    let input = r#"; v4 maximum compactness + extensibility
%def api H.g("https://api.example.com")
%def parse J.p
api|parse(_)|.data|w~.active|.name
home=$HOME
^x>0{x*2}{0}
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("// v4 maximum"), "got:\n{ae}");
    assert!(
        ae.contains("http.get(\"https://api.example.com\")"),
        "got:\n{ae}"
    );
    assert!(ae.contains("json.parse(_)"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.data)"), "got:\n{ae}");
    assert!(ae.contains("where(fn(__) =>"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.name)"), "got:\n{ae}");
    assert!(ae.contains("sys.env(\"HOME\")"), "got:\n{ae}");
    assert!(
        ae.contains("match (x>0) { true => (x*2), _ => (0) }"),
        "got:\n{ae}"
    );
}

// ── Edge-case robustness tests ──────────────────────────────────────

#[test]
fn edge_inline_comment() {
    let ae = transpile_agentic_to_ae("x=42 ; my var\n").unwrap();
    assert!(ae.contains("let x = 42"), "got:\n{ae}");
    assert!(ae.contains("// my var"), "got:\n{ae}");
}

#[test]
fn edge_inline_comment_in_string() {
    // Semicolon inside a string is NOT a comment
    let ae = transpile_agentic_to_ae("e\"a;b\"\n").unwrap();
    assert!(ae.contains("echo(\"a;b\")"), "got:\n{ae}");
}

#[test]
fn edge_single_quote_embedded_double() {
    // Embedded double quotes inside single-quoted strings should be escaped
    let ae = transpile_agentic_to_ae("e'she said \"hi\"'\n").unwrap();
    assert!(ae.contains(r#"echo("she said \"hi\"")"#), "got:\n{ae}");
}

#[test]
fn edge_backtick_preserves_dollar() {
    // $ inside backticks should NOT be expanded to sys.env()
    let ae = transpile_agentic_to_ae("x=`echo $HOME`\n").unwrap();
    assert!(ae.contains("sh(\"echo $HOME\")"), "got:\n{ae}");
}

#[test]
fn edge_multiple_env_vars() {
    let ae = transpile_agentic_to_ae("e$HOME\ne$PATH\n").unwrap();
    assert!(ae.contains("sys.env(\"HOME\")"), "got:\n{ae}");
    assert!(ae.contains("sys.env(\"PATH\")"), "got:\n{ae}");
}

#[test]
fn edge_gt_bare_comparison() {
    // Bare > (no spaces) should remain as comparison. (SI scaling now happens in
    // the grammar lexer, so `1k` is passed through unchanged here.)
    let ae = transpile_agentic_to_ae("w~.size>1k\n").unwrap();
    assert!(ae.contains("fn(__) => __.size>1k"), "got:\n{ae}");
}

#[test]
fn edge_gte_preserved() {
    // >= should never become pipe
    let ae = transpile_agentic_to_ae("x >= 5\n").unwrap();
    assert!(ae.contains(">="), "got:\n{ae}");
    assert!(!ae.contains(" | "), "should not contain pipe: got:\n{ae}");
}

#[test]
fn edge_fat_arrow_preserved() {
    // => should never become pipe
    let ae = transpile_agentic_to_ae("?x{1=>\"a\",_=>\"b\"}\n").unwrap();
    assert!(ae.contains("=>"), "got:\n{ae}");
}

#[test]
fn edge_nested_conditional() {
    // Nested braces inside conditional body
    let ae = transpile_agentic_to_ae("^x>0{{name:\"a\"}}{N}\n").unwrap();
    assert!(ae.contains("match (x>0)"), "got:\n{ae}");
    assert!(ae.contains("name:\"a\""), "got:\n{ae}");
    assert!(ae.contains("null"), "got:\n{ae}");
}

#[test]
fn edge_mixed_all_versions() {
    // Mix v1, v2, v3, v4 syntax in one script
    let input = r#"; all versions
#l "./src" > #w \.size>1k
e"hello"
l./src|w~.size>1k|m~.name
active=T
home=$HOME
^active{e'done'}
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(ae.contains("// all versions"), "got:\n{ae}");
    assert!(ae.contains("ls(\"./src\")"), "got:\n{ae}");
    assert!(ae.contains("echo(\"hello\")"), "got:\n{ae}");
    assert!(ae.contains("let active = true"), "got:\n{ae}");
    assert!(ae.contains("sys.env(\"HOME\")"), "got:\n{ae}");
    assert!(
        ae.contains("match (active) { true => (echo(\"done\")), _ => null }"),
        "got:\n{ae}"
    );
}

#[test]
fn edge_empty_script() {
    let ae = transpile_agentic_to_ae("").unwrap();
    assert!(ae.contains("Transpiled"), "got:\n{ae}");
    // Should only have the header line
    assert_eq!(ae.lines().count(), 1, "got:\n{ae}");
}

#[test]
fn edge_comment_only_script() {
    let ae = transpile_agentic_to_ae("; just a comment\n; another\n").unwrap();
    assert!(ae.contains("// just a comment"), "got:\n{ae}");
    assert!(ae.contains("// another"), "got:\n{ae}");
}

#[test]
fn edge_field_projection_chain() {
    // Multiple field projections chained
    let ae = transpile_agentic_to_ae("l\".\"|.name|.upper()\n").unwrap();
    assert!(ae.contains("map(fn(__) => __.name)"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.upper())"), "got:\n{ae}");
}

#[test]
fn edge_preamble_with_all_features() {
    let input = r#"%def api H.g("https://api.example.com")
%def parse J.p
%def big ~.size>1k
api|parse(_)|.data ; fetch and parse
"#;
    let ae = transpile_agentic_to_ae(input).unwrap();
    assert!(
        ae.contains("http.get(\"https://api.example.com\")"),
        "got:\n{ae}"
    );
    assert!(ae.contains("json.parse(_)"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.data)"), "got:\n{ae}");
    assert!(ae.contains("// fetch and parse"), "got:\n{ae}");
}

// ═══════════════════════════════════════════════════════════════════════
// Auto-parens integration tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn auto_parens_bare_module() {
    // F.r"path" → file.read("path")
    let ae = transpile_agentic_to_ae("F.r\"config.toml\"\n").unwrap();
    assert!(ae.contains("file.read(\"config.toml\")"), "got:\n{ae}");
}

#[test]
fn auto_parens_full_module_name() {
    // file.read"path" → file.read("path")
    let ae = transpile_agentic_to_ae("file.read\"data.json\"\n").unwrap();
    assert!(ae.contains("file.read(\"data.json\")"), "got:\n{ae}");
}

#[test]
fn auto_parens_in_pipeline() {
    // F.r"a" | J.p(data) → file.read("a") | json.parse(data)
    let ae = transpile_agentic_to_ae("F.r\"a.json\"|J.p(data)\n").unwrap();
    assert!(ae.contains("file.read(\"a.json\")"), "got:\n{ae}");
    assert!(ae.contains("json.parse(data)"), "got:\n{ae}");
}

#[test]
fn auto_parens_http_get() {
    // H.g"url" → http.get("url")
    let ae = transpile_agentic_to_ae("H.g\"https://api.com\"\n").unwrap();
    assert!(ae.contains("http.get(\"https://api.com\")"), "got:\n{ae}");
}

#[test]
fn auto_parens_doesnt_double_wrap() {
    // F.r("path") → file.read("path") — no double parens
    let ae = transpile_agentic_to_ae("F.r(\"path\")\n").unwrap();
    assert!(ae.contains("file.read(\"path\")"), "got:\n{ae}");
    assert!(!ae.contains("file.read(("), "got:\n{ae}");
}

// ═══════════════════════════════════════════════════════════════════════
// For-each loop integration tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn for_each_array_literal() {
    let ae = transpile_agentic_to_ae("*[1,2,3]~x:echo(x)\n").unwrap();
    assert!(
        ae.contains("([1,2,3]) | each(fn(x) => echo(x))"),
        "got:\n{ae}"
    );
}

#[test]
fn for_each_variable() {
    let ae = transpile_agentic_to_ae("*items~item:proc(item)\n").unwrap();
    assert!(
        ae.contains("(items) | each(fn(item) => proc(item))"),
        "got:\n{ae}"
    );
}

#[test]
fn for_each_with_function_call() {
    let ae = transpile_agentic_to_ae("*arr.range(5)~i:echo(i)\n").unwrap();
    assert!(
        ae.contains("(arr.range(5)) | each(fn(i) => echo(i))"),
        "got:\n{ae}"
    );
}

#[test]
fn for_each_does_not_fire_in_math() {
    // Multiplication inside assignment should NOT trigger for-each
    let ae = transpile_agentic_to_ae("x=2*3\n").unwrap();
    assert!(ae.contains("let x = 2*3"), "got:\n{ae}");
    assert!(!ae.contains("for"), "got:\n{ae}");
}

#[test]
fn for_each_backslash_lambda() {
    let ae = transpile_agentic_to_ae("*data\\x:print(x)\n").unwrap();
    assert!(
        ae.contains("(data) | each(fn(x) => print(x))"),
        "got:\n{ae}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Extended FUNC_ABBREV integration tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn func_abbrev_helm() {
    let ae = transpile_agentic_to_ae("HM.l()\n").unwrap();
    assert!(ae.contains("helm.list()"), "got:\n{ae}");
}

#[test]
fn func_abbrev_terraform() {
    let ae = transpile_agentic_to_ae("TF.p()\n").unwrap();
    assert!(ae.contains("terraform.plan()"), "got:\n{ae}");
}

#[test]
fn func_abbrev_npm() {
    let ae = transpile_agentic_to_ae("NP.i(\"express\")\n").unwrap();
    assert!(ae.contains("npm.install(\"express\")"), "got:\n{ae}");
}

#[test]
fn func_abbrev_go() {
    let ae = transpile_agentic_to_ae("GO.b()\nGO.t()\nGO.r(\"main.go\")\n").unwrap();
    assert!(ae.contains("go.build()"), "got:\n{ae}");
    assert!(ae.contains("go.test()"), "got:\n{ae}");
    assert!(ae.contains("go.run(\"main.go\")"), "got:\n{ae}");
}

#[test]
fn func_abbrev_container_tools() {
    let ae = transpile_agentic_to_ae("DK.p()\nDK.r(\"nginx\")\n").unwrap();
    assert!(ae.contains("docker.ps()"), "got:\n{ae}");
    assert!(ae.contains("docker.run(\"nginx\")"), "got:\n{ae}");
}

#[test]
fn func_abbrev_security() {
    let ae = transpile_agentic_to_ae("TV.s(\"./\")\n").unwrap();
    assert!(ae.contains("trivy.scan(\"./\")"), "got:\n{ae}");
}

#[test]
fn func_abbrev_a2a_a2ui() {
    let ae = transpile_agentic_to_ae("A2.s(\"agent1\", msg)\nUI.n(\"done\", \"info\")\n").unwrap();
    assert!(ae.contains("a2a.send(\"agent1\", msg)"), "got:\n{ae}");
    assert!(ae.contains("a2ui.notify(\"done\", \"info\")"), "got:\n{ae}");
}

// ═══════════════════════════════════════════════════════════════════════
// Chaining & composition tests — proving Turing completeness
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn chain_conditional_in_for_each() {
    // *items~x:^x>0{echo(x)} → (items) | each(fn(x) => match (x>0) { true => (echo(x)), _ => null })
    let ae = transpile_agentic_to_ae("*items~x:^x>0{echo(x)}\n").unwrap();
    assert!(ae.contains("(items) | each(fn(x) =>"), "got:\n{ae}");
    assert!(ae.contains("match (x>0)"), "got:\n{ae}");
    assert!(ae.contains("true => (echo(x))"), "got:\n{ae}");
}

#[test]
fn chain_conditional_with_else_in_for_each() {
    // *arr~v:^v>1k{\"big\"}{\"small\"} → contains both match and each
    let ae = transpile_agentic_to_ae("*arr~v:^v>1000{\"big\"}{\"small\"}\n").unwrap();
    assert!(ae.contains("(arr) | each(fn(v) =>"), "got:\n{ae}");
    assert!(ae.contains("true => (\"big\")"), "got:\n{ae}");
    assert!(ae.contains("_ => (\"small\")"), "got:\n{ae}");
}

#[test]
fn chain_for_each_in_pipeline() {
    // Pipeline with map and reduce
    let ae = transpile_agentic_to_ae("data|m~x:x*2|r~a,b:a+b\n").unwrap();
    assert!(
        ae.contains("data | map(fn(x) => x*2) | reduce(fn(a, b) => a+b)"),
        "got:\n{ae}"
    );
}

#[test]
fn chain_nested_match_in_each() {
    // *items~item:?item{1=>"one",_=>"other"} — use 'item' to avoid builtin conflict with 'x'
    let ae = transpile_agentic_to_ae("*items~item:?item{1=>\"one\",_=>\"other\"}\n").unwrap();
    assert!(ae.contains("(items) | each(fn(item) =>"), "got:\n{ae}");
    // `?` is passed through (grammar handles it); the nested match-expr lives in
    // the each body and the whole thing is valid .ae.
    assert!(ae.contains("?item{1=>\"one\",_=>\"other\"}"), "got:\n{ae}");
    assert!(
        aethershell::parser::parse_program(&ae).is_ok(),
        "transpiled output should parse: {ae}"
    );
}

#[test]
fn chain_try_catch_in_pipeline() {
    // !{H.g(url)}{"err"}|J.p(_)|.data → try/catch piped into json parse and projection
    let ae = transpile_agentic_to_ae("!{H.g(url)}{\"err\"}|J.p(_)|.data\n").unwrap();
    assert!(
        ae.contains("try { http.get(url) } catch e { \"err\" }"),
        "got:\n{ae}"
    );
    assert!(ae.contains("json.parse(_)"), "got:\n{ae}");
    assert!(ae.contains("map(fn(__) => __.data)"), "got:\n{ae}");
}

#[test]
fn chain_lambda_with_conditional() {
    // m~x:^x>0{x}{0} → map(fn(x) => match (x>0) { true => (x), _ => (0) })
    let ae = transpile_agentic_to_ae("[1,-2,3]|m~x:^x>0{x}{0}\n").unwrap();
    assert!(ae.contains("map(fn(x) =>"), "got:\n{ae}");
    assert!(
        ae.contains("match (x>0) { true => (x), _ => (0) }"),
        "got:\n{ae}"
    );
}

#[test]
fn chain_recursion_via_let_binding() {
    // Named function + self-reference = recursion (Turing completeness)
    // Use explicit fn() to avoid sigil conflicts in body
    let ae = transpile_agentic_to_ae("fact=~num:^num<2{1}{num*fact(num-1)}\n").unwrap();
    assert!(ae.contains("let fact = fn(num) =>"), "got:\n{ae}");
    assert!(ae.contains("match (num<2)"), "got:\n{ae}");
    assert!(ae.contains("true => (1)"), "got:\n{ae}");
    assert!(ae.contains("_ => (num*fact(num-1))"), "got:\n{ae}");
}

#[test]
fn chain_multiple_for_each_pipeline() {
    // Two for-each in sequence (multi-line) — bare builtins in body aren't expanded
    // because preprocess_ultra runs before for-each extraction
    let ae = transpile_agentic_to_ae("*[1,2]~idx:echo(idx)\n*[3,4]~idx:echo(idx)\n").unwrap();
    assert!(
        ae.contains("([1,2]) | each(fn(idx) => echo(idx))"),
        "got:\n{ae}"
    );
    assert!(
        ae.contains("([3,4]) | each(fn(idx) => echo(idx))"),
        "got:\n{ae}"
    );
}

#[test]
fn chain_assign_conditional_pipeline() {
    // result=^active{data|m~.name}{[]} → let result = match (active) ...
    let ae = transpile_agentic_to_ae("result=^active{data}{[]}\n").unwrap();
    assert!(
        ae.contains("let result = match (active) { true => (data), _ => ([]) }"),
        "got:\n{ae}"
    );
}

// ── New builtins: b=flatten, q=reverse ───────────────────────────────

#[test]
fn builtin_b_flatten() {
    let ae = transpile_agentic_to_ae("#b\n").unwrap();
    assert!(ae.contains("flatten()"), "got:\n{ae}");
}

#[test]
fn builtin_q_reverse() {
    let ae = transpile_agentic_to_ae("#q\n").unwrap();
    assert!(ae.contains("reverse()"), "got:\n{ae}");
}

#[test]
fn bare_builtin_b_flatten() {
    let ae = transpile_agentic_to_ae("data|b\n").unwrap();
    assert!(ae.contains("flatten()"), "got:\n{ae}");
}

#[test]
fn bare_builtin_q_reverse() {
    let ae = transpile_agentic_to_ae("data|q\n").unwrap();
    assert!(ae.contains("reverse()"), "got:\n{ae}");
}

// ── New single-char module sigils ────────────────────────────────────

#[test]
fn module_sigil_a_arr() {
    let ae = transpile_agentic_to_ae("A.r(10)\n").unwrap();
    assert!(ae.contains("arr.range(10)"), "got:\n{ae}");
}

#[test]
fn module_sigil_r_str() {
    let ae = transpile_agentic_to_ae("R.s(x, \",\")\n").unwrap();
    assert!(ae.contains("str.split(x, \",\")"), "got:\n{ae}");
}

#[test]
fn module_sigil_v_vm() {
    let ae = transpile_agentic_to_ae("V.l()\n").unwrap();
    assert!(ae.contains("vm.list()"), "got:\n{ae}");
}

#[test]
fn module_sigil_u_uv() {
    let ae = transpile_agentic_to_ae("U.i(\"pkg\")\n").unwrap();
    assert!(ae.contains("uv.install(\"pkg\")"), "got:\n{ae}");
}

#[test]
fn module_sigil_w_wsl() {
    let ae = transpile_agentic_to_ae("W.l()\n").unwrap();
    assert!(ae.contains("wsl.list()"), "got:\n{ae}");
}

#[test]
fn module_sigil_y_yarn() {
    let ae = transpile_agentic_to_ae("Y.a(\"pkg\")\n").unwrap();
    assert!(ae.contains("yarn.add(\"pkg\")"), "got:\n{ae}");
}

#[test]
fn module_sigil_z_zoxide() {
    let ae = transpile_agentic_to_ae("Z.q(\"proj\")\n").unwrap();
    assert!(ae.contains("zoxide.query(\"proj\")"), "got:\n{ae}");
}

#[test]
fn module_sigil_b_bun() {
    let ae = transpile_agentic_to_ae("B.r(\"script\")\n").unwrap();
    assert!(ae.contains("bun.run(\"script\")"), "got:\n{ae}");
}

#[test]
fn module_sigil_e_evo() {
    let ae = transpile_agentic_to_ae("E.p(100)\n").unwrap();
    assert!(ae.contains("evo.population(100)"), "got:\n{ae}");
}

#[test]
fn module_sigil_i_ai() {
    let ae = transpile_agentic_to_ae("I.q(\"prompt\")\n").unwrap();
    assert!(ae.contains("ai.q(\"prompt\")"), "got:\n{ae}");
}

// ── Old sigils still work (backward compat) ──────────────────────────

#[test]
fn module_sigil_ar_still_works() {
    let ae = transpile_agentic_to_ae("AR.r(10)\n").unwrap();
    assert!(ae.contains("arr.range(10)"), "got:\n{ae}");
}

#[test]
fn module_sigil_st_still_works() {
    let ae = transpile_agentic_to_ae("ST.u(x)\n").unwrap();
    assert!(ae.contains("str.upper(x)"), "got:\n{ae}");
}
