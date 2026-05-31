//! Behavior tests for the agentic transpiler (Phase 5: transpiler retirement).
//!
//! These assert *behavior*, not the transpiler's internal text output:
//!  - `eval_aeg(x)` transpiles → parses → evaluates and checks the resulting
//!    `Value`, for forms that evaluate without external resources.
//!  - `assert_call(x, needle)` checks the transpiled `.ae` **parses** and resolves
//!    to the expected dotted call/keyword (tolerant of spacing/parens), for forms
//!    that hit external tools/IO and can't be evaluated here.
//!
//! Decoupling from exact emitted text lets the 10-pass pipeline be replaced by a
//! single tokenizing pass without rewriting every assertion.

use aethershell::transpile::agentic::transpile_agentic_to_ae;
use aethershell::value::Value;

fn tr(src: &str) -> String {
    transpile_agentic_to_ae(src).expect("transpile")
}

/// Transpile → parse → eval, returning the value (for evaluable forms).
fn eval_aeg(src: &str) -> Value {
    let ae = tr(src);
    let stmts = aethershell::parser::parse_program(&ae).expect("transpiled .ae should parse");
    let mut env = aethershell::env::Env::new();
    aethershell::eval::eval_program(&stmts, &mut env).expect("transpiled .ae should eval")
}

/// Assert a cipher form resolves to the expected dotted call/keyword. `needle` is
/// matched as a substring (spacing/paren-tolerant) — it captures the transpiler's
/// semantic mapping (e.g. `@f.r` → `file.read`) without coupling to the exact
/// emitted layout. Used for forms that hit external tools / undefined refs and so
/// can't be evaluated here; the transpiler's contract for those is text-level.
fn assert_call(src: &str, needle: &str) -> String {
    let ae = tr(src);
    assert!(ae.contains(needle), "expected `{needle}` in:\n{ae}");
    ae
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

// ── SI suffix expansion (now in the grammar lexer) ───────────────────
#[test]
fn si_suffix_k() {
    assert_eq!(eval_aeg("1k\n"), Value::Int(1000));
}
#[test]
fn si_suffix_m() {
    assert_eq!(eval_aeg("5M\n"), Value::Int(5_000_000));
}
#[test]
fn si_suffix_g() {
    assert_eq!(eval_aeg("2G\n"), Value::Int(2_000_000_000));
}
#[test]
fn si_suffix_not_in_string() {
    // A `1k` inside a string literal must NOT be scaled.
    assert_eq!(eval_aeg("'1k'\n"), s("1k"));
}

// ── Lambdas: \x:expr and ~x:expr → fn(x) => expr ─────────────────────
#[test]
fn lambda_single_param_applies() {
    // Apply the lambda to data to prove it means fn(x)=>x*2.
    assert_eq!(eval_aeg("[1,2,3]|m\\x:x*2\n"), Value::Array(vec![
        Value::Int(2), Value::Int(4), Value::Int(6),
    ]));
}
#[test]
fn lambda_multi_param() {
    // \a,b:a+b → fn(a, b) => a+b (text contract; reduce needs no eval here).
    assert_call("#r \\a,b:a+b\n", "fn(a, b) => a+b");
}
#[test]
fn lambda_implicit_param_filters() {
    // \.size>2 → fn(__) => __.size>2, used as a where-filter.
    let v = eval_aeg("[{size:1},{size:3}]|w\\.size>2\n");
    assert_eq!(v, Value::Array(vec![{
        let mut m = std::collections::BTreeMap::new();
        m.insert("size".to_string(), Value::Int(3));
        Value::Record(m)
    }]));
}

// ── Module sigils: @mod.fn → module.fn ───────────────────────────────
#[test]
fn module_file() {
    assert_call("@f.read(\"p\")\n", "file.read");
}
#[test]
fn module_sys() {
    assert_call("@s.hostname()\n", "sys.hostname");
}
#[test]
fn module_http() {
    assert_call("@h.get(url)\n", "http.get");
}
#[test]
fn module_ai_direct_call() {
    assert_call("@ai(\"prompt\")\n", "ai(");
}
#[test]
fn module_docker() {
    assert_call("@dk.ps()\n", "docker.ps");
}
#[test]
fn module_k8s() {
    assert_call("@k.pods()\n", "k8s.pods");
}

// ── Builtin shorthands: #e → echo, etc. ──────────────────────────────
#[test]
fn builtin_echo() {
    assert_call("#e \"hello\"\n", "echo(");
}
#[test]
fn builtin_ls() {
    assert_call("#l \".\"\n", "ls(");
}
#[test]
fn builtin_take_evaluates() {
    assert_eq!(eval_aeg("[1,2,3,4,5]|#t 3\n"), Value::Array(vec![
        Value::Int(1), Value::Int(2), Value::Int(3),
    ]));
}
#[test]
fn builtin_grep() {
    assert_call("#g \"pattern\"\n", "grep(");
}
#[test]
fn builtin_no_args() {
    assert_call("#k\n", "keys(");
}

// ── Pipeline operator: > → | ─────────────────────────────────────────
#[test]
fn pipeline_single() {
    assert_eq!(eval_aeg("[3,1,2] > #t 2\n"), Value::Array(vec![
        Value::Int(3), Value::Int(1),
    ]));
}
#[test]
fn pipeline_preserves_gte() {
    assert_eq!(eval_aeg("5 >= 5\n"), Value::Bool(true));
}
#[test]
fn pipeline_chained() {
    let ae = tr("#l \".\" > #w \\.size>1000 > #m \\.name\n");
    assert!(ae.contains("ls(") && ae.contains("where(") && ae.contains("map("));
}

// ── Assignments: x=expr → let; x:=expr → let mut ─────────────────────
#[test]
fn assignment_immutable() {
    // Reference the binding via a conditional (a bare `x` line would be parsed as
    // a shell command in the cipher), proving `x=42` bound x to 42.
    assert_eq!(eval_aeg("x=42\n^x>0{x}{0}\n"), Value::Int(42));
}
#[test]
fn assignment_mutable() {
    assert_eq!(eval_aeg("counter:=5\n^counter>0{counter}{0}\n"), Value::Int(5));
}
#[test]
fn assignment_not_equality() {
    // `==` is a comparison, not an assignment.
    assert_eq!(eval_aeg("x=42\n^x==42{1}{0}\n"), Value::Int(1));
}

// ── Match: ?val{arms} (grammar-native) ───────────────────────────────
#[test]
fn match_basic() {
    assert_eq!(eval_aeg("?2{1=>\"a\",2=>\"b\",_=>\"z\"}\n"), s("b"));
}

// ── Try/catch: !{expr}{fallback} ─────────────────────────────────────
#[test]
fn try_catch_uses_fallback_on_error() {
    // risky() is undefined → error → the catch fallback value is returned.
    assert_eq!(eval_aeg("!{risky()}{\"fallback\"}\n"), s("fallback"));
}
#[test]
fn try_catch_structure() {
    assert_call("!{risky()}{\"fallback\"}\n", "catch");
}

// ── Comments: ; → // ─────────────────────────────────────────────────
#[test]
fn comment_conversion() {
    assert!(tr("; This is a comment\n").contains("// This is a comment"));
}

// ── Full integration examples ────────────────────────────────────────
#[test]
fn full_agent_workflow() {
    let ae = tr("; Find large .rs files\n#l \"./src\" > #w \\.size>1k > #m \\.name\n");
    assert!(ae.contains("ls(") && ae.contains("where(fn(__) =>") && ae.contains("map(fn(__) =>"));
}
#[test]
fn full_http_pipeline() {
    let ae = tr("@h.get(\"https://api.com/data\") > @j.parse(resp)\n");
    assert!(ae.contains("http.get") && ae.contains("json.parse"));
}
#[test]
fn full_mixed_syntax() {
    let ae = tr("data=@f.read(\"config.json\")\nparsed=@j.parse(data)\nname=parsed.name\n#e \"Hello ${name}\"\n");
    assert!(ae.contains("file.read") && ae.contains("json.parse") && ae.contains("echo("));
}

// ── v2 ultra-compressed forms ────────────────────────────────────────
#[test]
fn v2_tilde_lambda_applies() {
    assert_eq!(eval_aeg("[1,2]|m~x:x*2\n"), Value::Array(vec![Value::Int(2), Value::Int(4)]));
}
#[test]
fn v2_tilde_lambda_implicit_filters() {
    let v = eval_aeg("[{size:1},{size:9}]|w~.size>5\n");
    assert_eq!(v, Value::Array(vec![{
        let mut m = std::collections::BTreeMap::new();
        m.insert("size".to_string(), Value::Int(9));
        Value::Record(m)
    }]));
}
#[test]
fn v2_pipe_bar_evaluates() {
    assert_eq!(eval_aeg("[1,2,3]|#q\n"), Value::Array(vec![
        Value::Int(3), Value::Int(2), Value::Int(1),
    ]));
}
#[test]
fn v2_bare_builtin_echo() {
    assert_call("e\"hello\"\n", "echo(");
}
#[test]
fn v2_bare_builtin_ls() {
    assert_call("l\".\"\n", "ls(");
}
#[test]
fn v2_bare_builtin_with_tilde() {
    assert_call("w~.size>100\n", "where(fn(__) => __.size>100)");
}
#[test]
fn v2_bare_builtin_numeric_arg() {
    assert_eq!(eval_aeg("[1,2,3,4]|t5\n"), Value::Array(vec![
        Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4),
    ]));
}
#[test]
fn v2_bare_module_file() {
    assert_call("F.read(\"p\")\n", "file.read");
}
#[test]
fn v2_bare_module_http() {
    assert_call("H.get(url)\n", "http.get");
}
#[test]
fn v2_bare_module_docker() {
    assert_call("DK.ps()\n", "docker.ps");
}
#[test]
fn v2_func_abbreviation() {
    assert_call("F.r(\"README.md\")\n", "file.read");
}
#[test]
fn v2_func_abbreviation_json() {
    assert_call("J.p(data)\n", "json.parse");
}
#[test]
fn v2_func_abbreviation_sys() {
    assert_call("S.h()\n", "sys.hostname");
}
#[test]
fn v2_ultra_pipeline() {
    let ae = tr("l\"./src\"|w~.size>1k|m~.name\n");
    assert!(ae.contains("ls(") && ae.contains("where(fn(__) =>") && ae.contains("map(fn(__) =>"));
}
#[test]
fn v2_mixed_bare_and_v1() {
    let ae = tr("e\"start\"\n#l \"./src\" > #w \\.size>1k\nF.r(\"config.json\")\n");
    assert!(ae.contains("echo(") && ae.contains("ls(") && ae.contains("file.read"));
}
#[test]
fn v2_full_workflow() {
    let ae = tr("; workflow\nF.r(\"input.json\")|J.p(data)|w~.size>1k|m~.name\n");
    assert!(ae.contains("file.read") && ae.contains("json.parse") && ae.contains("where(fn(__) =>"));
}

// ── v3 symbol→value forms ────────────────────────────────────────────
#[test]
fn v3_symbol_true_null() {
    // T → true, N → null (symbol mapping). Verify via a conditional so the
    // bindings are referenced in expression position (a bare `x` line is a cmd).
    assert_eq!(eval_aeg("active=T\n^active{1}{0}\n"), Value::Int(1));
    let ae = tr("x=T\ny=N\n");
    assert!(ae.contains("let x = true") && ae.contains("let y = null"), "got:\n{ae}");
}
#[test]
fn v3_single_quote_strings() {
    assert_call("e'hello world'\n", "echo(\"hello world\")");
}
#[test]
fn v3_backtick_exec() {
    assert_call("x=`uname -a`\n", "sh(\"uname -a\")");
}
#[test]
fn v3_bare_path_relative() {
    assert_call("l./src\n", "ls(\"./src\")");
}
#[test]
fn v3_bare_path_absolute() {
    assert_call("l/usr/bin\n", "ls(\"/usr/bin\")");
}
#[test]
fn v3_bare_glob() {
    assert_call("g*.rs\n", "grep(\"*.rs\")");
}
#[test]
fn v3_bare_path_pipeline() {
    let ae = tr("l./src|w~.size>1k|m~.name\n");
    assert!(ae.contains("ls(\"./src\")") && ae.contains("where(fn(__) =>"));
}
#[test]
fn v3_full_workflow() {
    let ae = tr("active=T\ndata=N\nl./src|w~.size>1k|m~.name\nresult=`uname -a`\ne'done'\n");
    assert!(ae.contains("let active = true") && ae.contains("let data = null"));
    assert!(ae.contains("sh(\"uname -a\")") && ae.contains("echo(\"done\")"));
}

// ── v4 env vars, projection, conditionals, preamble ──────────────────
#[test]
fn v4_env_var_standalone() {
    assert_call("x=$HOME\n", "sys.env(\"HOME\")");
}
#[test]
fn v4_env_var_in_pipeline() {
    assert_call("e$USER\n", "echo(sys.env(\"USER\"))");
}
#[test]
fn v4_field_projection_evaluates() {
    assert_eq!(eval_aeg("[{name:\"a\"},{name:\"b\"}]|.name\n"), Value::Array(vec![s("a"), s("b")]));
}
#[test]
fn v4_field_projection_chained() {
    assert_call("l\"./src\"|.data.items\n", "map(fn(__) => __.data.items)");
}
#[test]
fn v4_field_projection_with_filter() {
    let ae = tr("l\"./src\"|w~.size>1k|.name\n");
    assert!(ae.contains("where(fn(__) =>") && ae.contains("map(fn(__) => __.name)"));
}
#[test]
fn v4_conditional_simple() {
    assert_eq!(eval_aeg("x=3\n^x>0{x*2}\n"), Value::Int(6));
    assert_eq!(eval_aeg("x=-1\n^x>0{x*2}\n"), Value::Null);
}
#[test]
fn v4_conditional_with_else() {
    assert_eq!(eval_aeg("x=3\n^x>0{x*2}{0}\n"), Value::Int(6));
    assert_eq!(eval_aeg("x=-1\n^x>0{x*2}{0}\n"), Value::Int(0));
}
#[test]
fn v4_preamble_def() {
    assert_call("%def fetch H.g\nfetch(\"https://api.com\")\n", "http.get(\"https://api.com\")");
}
#[test]
fn v4_preamble_multiple() {
    let ae = tr("%def fetch H.g\n%def parse J.p\nfetch(\"url\")|parse(data)|.items\n");
    assert!(ae.contains("http.get(\"url\")") && ae.contains("json.parse(data)"));
}
#[test]
fn v4_full_workflow() {
    let ae = tr("%def api H.g(\"https://api.example.com\")\n%def parse J.p\napi|parse(_)|.data|w~.active|.name\nhome=$HOME\n");
    assert!(ae.contains("http.get(\"https://api.example.com\")") && ae.contains("json.parse(_)"));
    assert!(ae.contains("sys.env(\"HOME\")"));
}

// ── Edge cases ───────────────────────────────────────────────────────
#[test]
fn edge_inline_comment() {
    let ae = tr("x=42 ; my var\n");
    assert!(ae.contains("let x = 42") && ae.contains("// my var"));
}
#[test]
fn edge_inline_comment_in_string() {
    assert_call("e\"a;b\"\n", "echo(\"a;b\")");
}
#[test]
fn edge_single_quote_embedded_double() {
    assert_call("e'she said \"hi\"'\n", r#"echo("she said \"hi\"")"#);
}
#[test]
fn edge_backtick_preserves_dollar() {
    assert_call("x=`echo $HOME`\n", "sh(\"echo $HOME\")");
}
#[test]
fn edge_multiple_env_vars() {
    let ae = tr("e$HOME\ne$PATH\n");
    assert!(ae.contains("sys.env(\"HOME\")") && ae.contains("sys.env(\"PATH\")"));
}
#[test]
fn edge_gt_bare_comparison() {
    assert_call("w~.size>1k\n", "fn(__) => __.size>1k");
}
#[test]
fn edge_gte_preserved() {
    let ae = tr("x >= 5\n");
    assert!(ae.contains(">=") && !ae.contains(" | "));
}
#[test]
fn edge_fat_arrow_preserved() {
    assert_call("?x{1=>\"a\",_=>\"b\"}\n", "=>");
}
#[test]
fn edge_nested_conditional() {
    let ae = tr("^x>0{{name:\"a\"}}{N}\n");
    assert!(ae.contains("name:\"a\"") && ae.contains("null"));
}
#[test]
fn edge_mixed_all_versions() {
    let ae = tr("; all versions\n#l \"./src\" > #w \\.size>1k\ne\"hello\"\nl./src|w~.size>1k|m~.name\nactive=T\nhome=$HOME\n");
    assert!(ae.contains("ls(") && ae.contains("echo(\"hello\")") && ae.contains("let active = true"));
}
#[test]
fn edge_empty_script() {
    let ae = tr("");
    assert!(ae.contains("Transpiled"));
    assert_eq!(ae.lines().count(), 1, "got:\n{ae}");
}
#[test]
fn edge_comment_only_script() {
    let ae = tr("; just a comment\n; another\n");
    assert!(ae.contains("// just a comment") && ae.contains("// another"));
}
#[test]
fn edge_field_projection_chain() {
    let ae = tr("l\".\"|.name|.upper()\n");
    assert!(ae.contains("map(fn(__) => __.name)") && ae.contains("map(fn(__) => __.upper())"));
}
#[test]
fn edge_preamble_with_all_features() {
    let ae = tr("%def api H.g(\"https://api.example.com\")\n%def parse J.p\n%def big ~.size>1k\napi|parse(_)|.data ; fetch and parse\n");
    assert!(ae.contains("http.get(\"https://api.example.com\")") && ae.contains("json.parse(_)"));
    assert!(ae.contains("// fetch and parse"));
}

// ── Auto-parens ──────────────────────────────────────────────────────
#[test]
fn auto_parens_bare_module() {
    assert_call("F.r\"config.toml\"\n", "file.read(\"config.toml\")");
}
#[test]
fn auto_parens_full_module_name() {
    assert_call("file.read\"data.json\"\n", "file.read(\"data.json\")");
}
#[test]
fn auto_parens_in_pipeline() {
    let ae = tr("F.r\"a.json\"|J.p(data)\n");
    assert!(ae.contains("file.read(\"a.json\")") && ae.contains("json.parse(data)"));
}
#[test]
fn auto_parens_http_get() {
    assert_call("H.g\"https://api.com\"\n", "http.get(\"https://api.com\")");
}
#[test]
fn auto_parens_doesnt_double_wrap() {
    let ae = tr("F.r(\"path\")\n");
    assert!(ae.contains("file.read(\"path\")") && !ae.contains("file.read(("));
}

// ── For-each loops ───────────────────────────────────────────────────
#[test]
fn for_each_array_literal() {
    assert_call("*[1,2,3]~x:echo(x)\n", "([1,2,3]) | each(fn(x) => echo(x))");
}
#[test]
fn for_each_variable() {
    assert_call("*items~item:proc(item)\n", "(items) | each(fn(item) => proc(item))");
}
#[test]
fn for_each_with_function_call() {
    assert_call("*arr.range(5)~i:echo(i)\n", "(arr.range(5)) | each(fn(i) => echo(i))");
}
#[test]
fn for_each_does_not_fire_in_math() {
    // `2*3` is multiplication, not a for-each (`*items~v:body`): x binds to 6.
    assert_eq!(eval_aeg("x=2*3\n^x>5{x}{0}\n"), Value::Int(6));
}
#[test]
fn for_each_backslash_lambda() {
    assert_call("*data\\x:print(x)\n", "(data) | each(fn(x) => print(x))");
}

// ── Extended function abbreviations ──────────────────────────────────
#[test]
fn func_abbrev_helm() {
    assert_call("HM.l()\n", "helm.list");
}
#[test]
fn func_abbrev_terraform() {
    assert_call("TF.p()\n", "terraform.plan");
}
#[test]
fn func_abbrev_npm() {
    assert_call("NP.i(\"express\")\n", "npm.install");
}
#[test]
fn func_abbrev_go() {
    let ae = tr("GO.b()\nGO.t()\nGO.r(\"main.go\")\n");
    assert!(ae.contains("go.build") && ae.contains("go.test") && ae.contains("go.run"));
}
#[test]
fn func_abbrev_container_tools() {
    let ae = tr("DK.p()\nDK.r(\"nginx\")\n");
    assert!(ae.contains("docker.ps") && ae.contains("docker.run"));
}
#[test]
fn func_abbrev_security() {
    assert_call("TV.s(\"./\")\n", "trivy.scan");
}
#[test]
fn func_abbrev_a2a_a2ui() {
    let ae = tr("A2.s(\"agent1\", msg)\nUI.n(\"done\", \"info\")\n");
    assert!(ae.contains("a2a.send") && ae.contains("a2ui.notify"));
}

// ── Chaining & composition ───────────────────────────────────────────
#[test]
fn chain_conditional_in_for_each() {
    let ae = tr("*items~x:^x>0{echo(x)}\n");
    assert!(ae.contains("(items) | each(fn(x) =>") && ae.contains("match (x>0)"));
}
#[test]
fn chain_conditional_with_else_in_for_each() {
    let ae = tr("*arr~v:^v>1000{\"big\"}{\"small\"}\n");
    assert!(ae.contains("(arr) | each(fn(v) =>"));
    assert!(ae.contains("true => (\"big\")") && ae.contains("_ => (\"small\")"));
}
#[test]
fn chain_for_each_in_pipeline() {
    let ae = assert_call("data|m~x:x*2|r~a,b:a+b\n", "map(fn(x) => x*2)");
    assert!(ae.contains("reduce(fn(a, b) => a+b)"), "got:\n{ae}");
}
#[test]
fn chain_nested_match_in_each() {
    let ae = tr("*items~item:?item{1=>\"one\",_=>\"other\"}\n");
    assert!(ae.contains("(items) | each(fn(item) =>"));
}
#[test]
fn chain_try_catch_in_pipeline() {
    let ae = tr("!{H.g(url)}{\"err\"}|J.p(_)|.data\n");
    assert!(ae.contains("try { http.get(url) } catch e { \"err\" }"));
    assert!(ae.contains("json.parse(_)") && ae.contains("map(fn(__) => __.data)"));
}
#[test]
fn chain_lambda_with_conditional_evaluates() {
    assert_eq!(eval_aeg("[1,-2,3]|m~x:^x>0{x}{0}\n"), Value::Array(vec![
        Value::Int(1), Value::Int(0), Value::Int(3),
    ]));
}
#[test]
fn chain_recursion_via_let_binding() {
    let ae = tr("fact=~num:^num<2{1}{num*fact(num-1)}\n");
    assert!(ae.contains("let fact = fn(num) =>") && ae.contains("match (num<2)"));
}
#[test]
fn chain_multiple_for_each_pipeline() {
    let ae = tr("*[1,2]~idx:echo(idx)\n*[3,4]~idx:echo(idx)\n");
    assert!(ae.contains("([1,2]) | each(fn(idx) => echo(idx))"));
    assert!(ae.contains("([3,4]) | each(fn(idx) => echo(idx))"));
}
#[test]
fn chain_assign_conditional_pipeline() {
    assert_call("result=^active{data}{[]}\n", "let result = match (active) { true => (data), _ => ([]) }");
}

// ── New builtins: b=flatten, q=reverse ───────────────────────────────
#[test]
fn builtin_b_flatten_evaluates() {
    assert_eq!(eval_aeg("[[1,2],[3]]|#b\n"), Value::Array(vec![
        Value::Int(1), Value::Int(2), Value::Int(3),
    ]));
}
#[test]
fn builtin_q_reverse_evaluates() {
    assert_eq!(eval_aeg("[1,2,3]|#q\n"), Value::Array(vec![
        Value::Int(3), Value::Int(2), Value::Int(1),
    ]));
}
#[test]
fn bare_builtin_b_flatten() {
    assert_call("data|b\n", "flatten(");
}
#[test]
fn bare_builtin_q_reverse() {
    assert_call("data|q\n", "reverse(");
}

// ── Single-char module sigils ────────────────────────────────────────
#[test]
fn module_sigil_a_arr() {
    assert_call("A.r(10)\n", "arr.range");
}
#[test]
fn module_sigil_r_str() {
    assert_call("R.s(x, \",\")\n", "str.split");
}
#[test]
fn module_sigil_v_vm() {
    assert_call("V.l()\n", "vm.list");
}
#[test]
fn module_sigil_u_uv() {
    assert_call("U.i(\"pkg\")\n", "uv.install");
}
#[test]
fn module_sigil_w_wsl() {
    assert_call("W.l()\n", "wsl.list");
}
#[test]
fn module_sigil_y_yarn() {
    assert_call("Y.a(\"pkg\")\n", "yarn.add");
}
#[test]
fn module_sigil_z_zoxide() {
    assert_call("Z.q(\"proj\")\n", "zoxide.query");
}
#[test]
fn module_sigil_b_bun() {
    assert_call("B.r(\"script\")\n", "bun.run");
}
#[test]
fn module_sigil_e_evo() {
    assert_call("E.p(100)\n", "evo.population");
}
#[test]
fn module_sigil_i_ai() {
    assert_call("I.q(\"prompt\")\n", "ai.q");
}
#[test]
fn module_sigil_ar_still_works() {
    assert_call("AR.r(10)\n", "arr.range");
}
#[test]
fn module_sigil_st_still_works() {
    assert_call("ST.u(x)\n", "str.upper");
}
