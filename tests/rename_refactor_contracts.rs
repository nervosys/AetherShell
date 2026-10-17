use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
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
        let root = std::env::temp_dir().join(format!("ae-rename-{}", uuid::Uuid::new_v4()));
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
fn renamed_function_compiles_and_runs_at_every_use() {
    let output = run(
        "refactor_rename",
        vec![
            s("pub fn helper(x:i32)->i32 { x*2 } pub fn run()->i32 { helper(3) + helper(1) }"),
            s("helper"),
            s("assist"),
        ],
    );
    assert!(!output.contains("helper"), "{output}");
    compile_and_run(&output, "assert_eq!(run(),8); assert_eq!(assist(5),10);");
}
#[test]
fn renamed_field_and_local_stay_consistent_and_compile() {
    let output = run(
        "refactor_rename",
        vec![
            s("pub struct P { pub count: i32 } pub fn bump(p: &mut P) -> i32 { let count = p.count + 1; p.count = count; count }"),
            s("count"),
            s("total"),
        ],
    );
    assert!(output.contains("pub total : i32"), "{output}");
    compile_and_run(
        &output,
        "let mut p = P { total: 1 }; assert_eq!(bump(&mut p), 2); assert_eq!(p.total, 2);",
    );
}
#[test]
fn lifetimes_are_not_renamed_and_raw_names_compile() {
    let output = run(
        "refactor_rename",
        vec![
            s("pub fn pick<'a>(a: &'a str, b: &'a str) -> &'a str { let x = a; if x.is_empty() { b } else { x } }"),
            s("x"),
            s("r#type"),
        ],
    );
    assert!(output.contains("'a"), "{output}");
    compile_and_run(
        &output,
        "assert_eq!(pick(\"\", \"b\"), \"b\"); assert_eq!(pick(\"a\", \"b\"), \"a\");",
    );
    let raw = run(
        "refactor_rename",
        vec![s("pub fn id(x:i32)->i32 { x }"), s("r#x"), s("r#type")],
    );
    compile_and_run(&raw, "assert_eq!(id(7), 7);");
}
#[test]
fn raw_identifier_targets_compile_and_run() {
    let output = run(
        "refactor_rename",
        vec![
            s("pub fn f(x:i32)->i32 { x + 1 } pub fn g()->i32 { f(4) }"),
            s("f"),
            s("r#type"),
        ],
    );
    compile_and_run(&output, "assert_eq!(g(),5); assert_eq!(r#type(1),2);");
}
#[test]
fn refusals_are_coded_bad_arg() {
    for (source, old, new) in [
        // new already occurs
        ("pub fn a()->i32 { 1 } pub fn b()->i32 { 2 }", "a", "b"),
        // old absent
        ("pub fn a()->i32 { 1 }", "missing", "b"),
        // old equals new, including raw normalization
        ("pub fn a()->i32 { 1 }", "a", "a"),
        ("pub fn a()->i32 { 1 }", "r#a", "a"),
        // old inside a macro invocation
        ("pub fn f(x:i32) { println!(\"{}\", x); }", "x", "y"),
        // old inside a format-capture string
        ("pub fn f(x:i32) { println!(\"{x}\"); }", "x", "y"),
        // reserved names
        ("pub fn a()->i32 { 1 }", "self", "b"),
        ("pub fn a()->i32 { 1 }", "a", "Self"),
        // not a Rust identifier
        ("pub fn a()->i32 { 1 }", "bad-name", "b"),
        ("pub fn a()->i32 { 1 }", "a", "type"),
        // not a Rust source file
        ("pub fn a( {", "a", "b"),
    ] {
        error(
            "refactor_rename",
            vec![s(source), s(old), s(new)],
            "E_BAD_ARG",
        );
    }
    for args in [
        vec![],
        vec![s("pub fn a()->i32 { 1 }")],
        vec![s("pub fn a()->i32 { 1 }"), s("a")],
        vec![s("pub fn a()->i32 { 1 }"), s("a"), s("b"), s("c")],
        vec![Value::Int(1), s("a"), s("b")],
        vec![s("pub fn a()->i32 { 1 }"), Value::Int(1), s("b")],
        vec![s("pub fn a()->i32 { 1 }"), s("a"), Value::Int(1)],
    ] {
        error("refactor_rename", args, "E_BAD_ARG");
    }
    assert!(call_with_input(
        "refactor_rename",
        vec![s("pub fn a()->i32 { 1 }"), s("a"), s("b")],
        Some(s("input")),
        &mut Env::default()
    )
    .is_err());
}
#[test]
fn oversize_source_and_names_are_budget_errors() {
    error(
        "refactor_rename",
        vec![s(&"a".repeat(262145)), s("a"), s("b")],
        "E_BUDGET",
    );
    error(
        "refactor_rename",
        vec![
            s(&format!("fn a(){{}} //{}", "!".repeat(4097))),
            s("a"),
            s("b"),
        ],
        "E_BUDGET",
    );
    error(
        "refactor_rename",
        vec![s("pub fn a()->i32 { 1 }"), s(&"a".repeat(257)), s("b")],
        "E_BUDGET",
    );
    error(
        "refactor_rename",
        vec![s("pub fn a()->i32 { 1 }"), s("a"), s(&"!".repeat(33))],
        "E_BUDGET",
    );
}
#[test]
fn new_example_and_alias_execute_with_pure_effects() {
    for (name, alias) in [("refactor_rename", "refactor.rename")] {
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
#[test]
fn string_literals_and_static_lifetimes_keep_their_text() {
    let output = run(
        "refactor_rename",
        vec![
            s("pub fn f(x:i32)->&'static str { let _ = x; \"x x\" }"),
            s("x"),
            s("y"),
        ],
    );
    assert!(output.contains("\"x x\""), "{output}");
    assert!(output.contains("'static"), "{output}");
    compile_and_run(&output, "assert_eq!(f(1), \"x x\");");
}
