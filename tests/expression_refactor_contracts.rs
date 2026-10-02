use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use quote::ToTokens;
fn s(text: &str) -> Value {
    Value::Str(text.into())
}
fn run(name: &str, args: Vec<Value>) -> String {
    let Value::Str(source) = call(name, args, &mut Env::default()).unwrap() else {
        panic!("expected source")
    };
    source
}
fn error(name: &str, args: Vec<Value>, code: &str) {
    let error = call(name, args, &mut Env::default()).unwrap_err();
    assert!(error.to_string().contains(code), "{error}");
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-refactor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.canonicalize().is_ok_and(|path| path == self.0) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
fn compile_and_run(source: &str, assertion: &str) {
    let scratch = Scratch::new();
    let file = scratch.0.join("unit.rs");
    let executable = scratch
        .0
        .join(if cfg!(windows) { "unit.exe" } else { "unit" });
    std::fs::write(
        &file,
        format!("#![deny(warnings)]\n{source}\n#[test] fn witness() {{ {assertion} }}"),
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = std::process::Command::new(rustc)
        .args(["--test", "--edition=2021"])
        .arg(file)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = std::process::Command::new(executable).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
}
#[test]
fn variable_extraction_compiles_preserves_order_and_evaluates_once() {
    let original =
        "pub fn increment(n:&mut i32)->i32 { let before = *n; { *n += 1; before + *n } }";
    let output = run("refactor_extract_variable", vec![s(original), s("result")]);
    let before = syn::parse_str::<syn::ItemFn>(original).unwrap();
    let after = syn::parse_str::<syn::ItemFn>(&output).unwrap();
    assert_eq!(
        before.sig.to_token_stream().to_string(),
        after.sig.to_token_stream().to_string()
    );
    assert_eq!(
        before.block.stmts[0].to_token_stream().to_string(),
        after.block.stmts[0].to_token_stream().to_string()
    );
    compile_and_run(
        &output,
        "let mut n=3; assert_eq!(increment(&mut n),7); assert_eq!(n,4);",
    );
    let borrowed = run(
        "refactor_extract_variable",
        vec![s("fn borrow(s:&str)->&str { s }"), s("r#type")],
    );
    compile_and_run(&borrowed, "assert_eq!(borrow(\"hello\"),\"hello\");");
}
#[test]
fn explicit_typed_constant_extraction_compiles_and_runs() {
    let output = run(
        "refactor_extract_constant",
        vec![s("pub fn answer()->i32 { 6*7 }"), s("ANSWER"), s("i32")],
    );
    assert!(output.contains("const ANSWER : i32 = 6 * 7"));
    compile_and_run(&output, "assert_eq!(answer(),42);");
    let original = "/// documentation\nasync unsafe fn f<T>(a:T)->T { a }";
    let output = run("refactor_extract_variable", vec![s(original), s("result")]);
    let before = syn::parse_str::<syn::ItemFn>(original).unwrap();
    let after = syn::parse_str::<syn::ItemFn>(&output).unwrap();
    assert_eq!(
        before
            .attrs
            .iter()
            .map(|a| a.to_token_stream().to_string())
            .collect::<Vec<_>>(),
        after
            .attrs
            .iter()
            .map(|a| a.to_token_stream().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        before.sig.to_token_stream().to_string(),
        after.sig.to_token_stream().to_string()
    );
}
#[test]
fn const_evaluability_is_left_to_caller_compiler_not_falsely_claimed() {
    let output = run(
        "refactor_extract_constant",
        vec![s("fn f(x:i32)->i32 { x+1 }"), s("ANSWER"), s("i32")],
    );
    let scratch = Scratch::new();
    let file = scratch.0.join("invalid.rs");
    std::fs::write(&file, output).unwrap();
    let result =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--crate-type=lib", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(scratch.0.join("invalid.rmeta"))
            .output()
            .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("E0435"));
}
#[test]
fn collisions_and_opaque_syntax_refuse() {
    for source in [
        "fn f(result:i32)->i32 { result }",
        "fn result()->i32 { 1 }",
        "fn f()->i32 { let result=1; result }",
        "fn f()->i32 { foo.result }",
        "fn f()->i32 { println!(\"{result}\"); 1 }",
        "#[inline] fn f()->i32 { 1 }",
        "fn f()->i32 { const X:i32=1; X }",
        "fn f()->i32 { #[allow(unused)] let x=1; x }",
    ] {
        for name in ["refactor_extract_variable", "refactor_extract_constant"] {
            let mut args = vec![s(source), s("result")];
            if name.ends_with("constant") {
                args.push(s("i32"));
            }
            error(name, args, "E_BAD_ARG");
        }
    }
    error(
        "refactor_extract_variable",
        vec![s("fn f(r#type:i32)->i32 { 1 }"), s("r#type")],
        "E_BAD_ARG",
    );
    error(
        "refactor_extract_constant",
        vec![s("fn f()->i32 { 1 }"), s("ANSWER"), s("some!()")],
        "E_BAD_ARG",
    );
}
#[test]
fn invalid_shapes_tail_syntax_and_work_limits_refuse() {
    for source in [
        "fn f() {}",
        "fn f() { let x=1; }",
        "fn f() { 1; }",
        "fn f()->i32 {1} fn g() {}",
        "impl X {}",
        "fn f(&self)->i32 {1}",
        "extern \"C\" fn f(x:i32,...)->i32 {1}",
    ] {
        error(
            "refactor_extract_variable",
            vec![s(source), s("result")],
            "E_BAD_ARG",
        );
    }
    for args in [
        vec![],
        vec![s("fn f()->i32 {1}")],
        vec![Value::Int(1), s("result")],
        vec![s("fn f()->i32 {1}"), s("bad-name")],
    ] {
        error("refactor_extract_variable", args, "E_BAD_ARG");
    }
    error(
        "refactor_extract_constant",
        vec![s("fn f()->i32 {1}"), s("ANSWER"), s("not a type")],
        "E_BAD_ARG",
    );
    error(
        "refactor_extract_variable",
        vec![s(&"a".repeat(65537)), s("result")],
        "E_BUDGET",
    );
    error(
        "refactor_extract_variable",
        vec![s("fn f()->i32 {1}"), s(&"a".repeat(257))],
        "E_BUDGET",
    );
    error(
        "refactor_extract_variable",
        vec![
            s(&format!("fn f()->i32 {{1}} //{}", "!".repeat(257))),
            s("result"),
        ],
        "E_BUDGET",
    );
    error(
        "refactor_extract_constant",
        vec![s("fn f()->i32 {1}"), s("ANSWER"), s(&"a".repeat(16385))],
        "E_BUDGET",
    );
    assert!(call_with_input(
        "refactor_extract_variable",
        vec![s("fn f()->i32 {1}"), s("result")],
        Some(s("input")),
        &mut Env::default()
    )
    .is_err());
}
#[test]
fn new_examples_and_aliases_execute_with_pure_effects() {
    for (name, alias) in [
        ("refactor_extract_variable", "refactor.extract_variable"),
        ("refactor_extract_constant", "refactor.extract_constant"),
    ] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::Pure
        );
        for (code, expected) in aethershell::signature::signature_of(name).unwrap().examples {
            for source in [code.to_string(), code.replacen(name, alias, 1)] {
                let mut env = Env::default();
                env.register_module("refactor", aethershell::modules::refactor_module());
                let statements = aethershell::parser::parse_program(&source).unwrap();
                assert_eq!(
                    aethershell::eval::eval_program(&statements, &mut env)
                        .unwrap()
                        .to_display_string(),
                    *expected
                );
            }
        }
    }
}
