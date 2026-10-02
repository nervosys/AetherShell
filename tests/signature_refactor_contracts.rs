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
fn parameter_addition_changes_real_call_shapes_and_preserves_bodies() {
    let original = "pub fn calc(a:i32,b:i32)->i32 { a-b }";
    for (position, assertion) in [
        (None, "assert_eq!(calc(7,2,999),5);"),
        (Some(0), "assert_eq!(calc(999,7,2),5);"),
        (Some(1), "assert_eq!(calc(7,999,2),5);"),
    ] {
        let mut args = vec![s(original), s("_extra:i32")];
        if let Some(position) = position {
            args.push(Value::Int(position));
        }
        let output = run("refactor_add_parameter", args);
        let before = syn::parse_str::<syn::ItemFn>(original).unwrap();
        let after = syn::parse_str::<syn::ItemFn>(&output).unwrap();
        assert_eq!(
            before.block.to_token_stream().to_string(),
            after.block.to_token_stream().to_string()
        );
        compile_and_run(&output, assertion);
    }
}
#[test]
fn signature_reordering_and_unused_parameter_removal_compile_and_run() {
    let original = "pub fn calc(a:i32,b:i32)->i32 { a-b }";
    let reordered = run(
        "refactor_change_signature",
        vec![s(original), s("fn calc(b:i32,a:i32)->i32")],
    );
    compile_and_run(&reordered, "assert_eq!(calc(2,7),5);");
    let removed = run(
        "refactor_remove_parameter",
        vec![
            s("pub fn calc(a:i32,r#extra:i32,b:i32)->i32 { a-b }"),
            s("extra"),
        ],
    );
    compile_and_run(&removed, "assert_eq!(calc(7,2),5);");
    let removed_by_change = run(
        "refactor_change_signature",
        vec![
            s("pub fn calc(a:i32,_unused:i32,b:i32)->i32 { a-b }"),
            s("fn calc(a:i32,b:i32)->i32"),
        ],
    );
    compile_and_run(&removed_by_change, "assert_eq!(calc(7,2),5);");
}
#[test]
fn edits_preserve_docs_qualifiers_generics_attributes_and_literal_tokens() {
    let source = "/// documentation\n#[allow(dead_code)] pub async unsafe fn f<'a,T:Copy>(x:&'a T)->T { let _s = r#\"a{b}\\c\"#; *x }";
    let output = run(
        "refactor_add_parameter",
        vec![s(source), s("_extra:i32"), Value::Int(0)],
    );
    let original = syn::parse_str::<syn::ItemFn>(source).unwrap();
    let parsed = syn::parse_str::<syn::ItemFn>(&output).unwrap();
    assert_eq!(original.attrs.len(), parsed.attrs.len());
    assert_eq!(
        original.sig.generics.to_token_stream().to_string(),
        parsed.sig.generics.to_token_stream().to_string()
    );
    assert!(parsed.sig.asyncness.is_some() && parsed.sig.unsafety.is_some());
    assert!(matches!(parsed.vis, syn::Visibility::Public(_)));
    assert_eq!(
        original.block.to_token_stream().to_string(),
        parsed.block.to_token_stream().to_string()
    );
    let output = run(
        "refactor_remove_parameter",
        vec![s("/// docs\npub fn f(_extra:i32)->i32 { 3 }"), s("_extra")],
    );
    compile_and_run(&output, "assert_eq!(f(),3);");
}
#[test]
fn removal_refuses_references_capture_macros_attributes_and_nested_items() {
    for source in [
        "fn f(x:i32)->i32 { x }",
        "fn f(x:i32)->i32 { let x=2; x }",
        "fn f(x:i32)->i32 { let r=R{x:2}; r.x }",
        "fn f(x:i32) { println!(\"{x}\"); }",
        "fn f(x:i32) { println!(\"unrelated\"); }",
        "#[allow(unused)] fn f(x:i32) {}",
        "fn f(x:i32) { #[allow(unused)] let y=2; }",
        "fn f(x:i32) { const Y:i32=2; }",
    ] {
        error(
            "refactor_remove_parameter",
            vec![s(source), s("x")],
            "E_BAD_ARG",
        );
        error(
            "refactor_change_signature",
            vec![s(source), s("fn f()")],
            "E_BAD_ARG",
        );
    }
}
#[test]
fn edits_reject_ambiguous_syntax_invalid_shapes_and_excessive_work() {
    let source = "fn f(a:i32)->i32 {a}";
    for invalid in [
        "",
        "file.rs",
        "fn f(){} fn g(){}",
        "impl X {fn f(){}}",
        "fn f(self){}",
        "fn f((a,b):(i32,i32)){}",
        "fn f(ref a:i32){}",
        "extern \"C\" fn f(a:i32,...){}",
        "fn f(a:i32,r#a:i32){}",
    ] {
        error(
            "refactor_add_parameter",
            vec![s(invalid), s("_extra:i32")],
            "E_BAD_ARG",
        );
    }
    for parameter in [
        "a:i32",
        "r#a:i32",
        "self",
        "(x,y):(i32,i32)",
        "ref x:i32",
        "x @ _:i32",
        "x",
        "x:i32,y:i32",
    ] {
        error(
            "refactor_add_parameter",
            vec![s(source), s(parameter)],
            "E_BAD_ARG",
        );
    }
    for position in [
        Value::Int(-1),
        Value::Int(2),
        Value::Int(i64::MAX),
        Value::Float(0.0),
        Value::Null,
    ] {
        error(
            "refactor_add_parameter",
            vec![s(source), s("_extra:i32"), position],
            "E_BAD_ARG",
        );
    }
    for signature in [
        "fn g(a:i32)->i32",
        "fn f(a:i32)->i32",
        "pub fn f(a:i32)->i32",
        "fn f(a:i32)->i32 {}",
        "fn f(a:i32,b:i32,b:i32)->i32",
    ] {
        error(
            "refactor_change_signature",
            vec![s(source), s(signature)],
            "E_BAD_ARG",
        );
    }
    error(
        "refactor_remove_parameter",
        vec![s(source), s("absent")],
        "E_BAD_ARG",
    );
    for (name, args) in [
        ("refactor_add_parameter", vec![s(source), s("_extra:i32")]),
        (
            "refactor_change_signature",
            vec![s(source), s("fn f(a:i64)->i64")],
        ),
        (
            "refactor_remove_parameter",
            vec![s("fn f(_extra:i32){}"), s("_extra")],
        ),
    ] {
        error(name, vec![], "E_BAD_ARG");
        error(name, vec![Value::Null, args[1].clone()], "E_BAD_ARG");
        let mut extra = args.clone();
        extra.extend([Value::Null, Value::Null]);
        error(name, extra, "E_BAD_ARG");
        let refusal = call_with_input(name, args.clone(), Some(Value::Null), &mut Env::default())
            .unwrap_err();
        assert!(refusal.to_string().contains("E_BAD_ARG"));
        error(
            name,
            vec![s(&"x".repeat(65537)), args[1].clone()],
            "E_BUDGET_EXCEEDED",
        );
        error(
            name,
            vec![s(&"(".repeat(257)), args[1].clone()],
            "E_BUDGET_EXCEEDED",
        );
    }
    let parameters = (0..64)
        .map(|n| format!("_p{n}:i32"))
        .collect::<Vec<_>>()
        .join(",");
    let function = format!("fn f({parameters}){{}}");
    error(
        "refactor_add_parameter",
        vec![s(&function), s("_extra:i32")],
        "E_BUDGET_EXCEEDED",
    );
    error(
        "refactor_add_parameter",
        vec![s(source), s(&"x".repeat(16385))],
        "E_BUDGET_EXCEEDED",
    );
    error(
        "refactor_remove_parameter",
        vec![s(source), s(&"x".repeat(257))],
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn refactor_examples_and_module_aliases_execute_without_effects() {
    for (name, alias) in [
        ("refactor_change_signature", "refactor.change_signature"),
        ("refactor_add_parameter", "refactor.add_parameter"),
        ("refactor_remove_parameter", "refactor.remove_parameter"),
    ] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::Pure
        );
        for (code, expected) in aethershell::signature::signature_of(name).unwrap().examples {
            for source in [code.to_string(), code.replacen(name, alias, 1)] {
                let statements = aethershell::parser::parse_program(&source).unwrap();
                let mut env = Env::default();
                env.register_module("refactor", aethershell::modules::refactor_module());
                let result = aethershell::eval::eval_program(&statements, &mut env).unwrap();
                assert_eq!(result.to_display_string(), *expected, "{source}");
            }
        }
    }
}
