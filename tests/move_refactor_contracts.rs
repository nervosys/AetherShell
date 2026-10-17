use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use quote::ToTokens;
fn s(text: &str) -> Value {
    Value::Str(text.into())
}
fn run(source: &str, item: &str, destination: &str) -> (String, String) {
    let Value::Record(record) = call(
        "refactor_move_to_file",
        vec![s(source), s(item), s(destination)],
        &mut Env::default(),
    )
    .unwrap() else {
        panic!("expected record")
    };
    let text = |key: &str| match record.get(key) {
        Some(Value::Str(text)) => text.clone(),
        other => panic!("expected string {key}, got {other:?}"),
    };
    (text("source"), text("destination"))
}
fn error(args: Vec<Value>, code: &str) {
    let error = call("refactor_move_to_file", args, &mut Env::default()).unwrap_err();
    assert!(error.to_string().contains(code), "{error}");
}
fn s3(source: &str, item: &str, destination: &str) -> Vec<Value> {
    vec![s(source), s(item), s(destination)]
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-move-{}", uuid::Uuid::new_v4()));
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
fn items(source: &str) -> Vec<syn::Item> {
    syn::parse_str::<syn::File>(source).unwrap().items
}
fn names(source: &str) -> Vec<String> {
    items(source)
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
            syn::Item::Struct(s) => Some(s.ident.to_string()),
            _ => None,
        })
        .collect()
}
#[test]
fn moved_fn_and_struct_leave_source_and_compile_together() {
    let source = "pub struct P{pub x:i32} pub fn make()->P{P{x:7}} pub fn keep()->i32{1}";
    let (source, destination) = run(source, "make", "pub fn other()->i32{2}");
    assert!(!source.contains("make"));
    assert!(destination.contains("make"));
    let (source, destination) = run(&source, "P", &destination);
    assert_eq!(names(&source), ["keep"]);
    assert_eq!(names(&destination), ["other", "make", "P"]);
    // Concatenated, the two outputs form one program that behaves as before.
    compile_and_run(
        &format!("{source}\n{destination}"),
        "assert_eq!(make().x,7); assert_eq!(keep(),1); assert_eq!(other(),2);",
    );
}
#[test]
fn doc_comments_survive_as_doc_attributes_and_attributes_are_kept() {
    let (_, destination) = run(
        "/// documented\n#[derive(Debug)]\npub struct D; pub fn k()->i32{1}",
        "D",
        "",
    );
    let item = items(&destination).remove(0);
    let syn::Item::Struct(item) = item else {
        panic!("expected struct")
    };
    let attributes: Vec<String> = item
        .attrs
        .iter()
        .map(|attribute| attribute.to_token_stream().to_string())
        .collect();
    assert!(attributes
        .iter()
        .any(|a| a.contains("doc") && a.contains("documented")));
    assert!(attributes.iter().any(|a| a.contains("derive")));
}
#[test]
fn raw_identifiers_and_inline_modules_move() {
    let (source, destination) = run(
        "pub fn r#type()->i32{1} pub mod m { pub fn n()->i32{2} }",
        "r#type",
        "",
    );
    assert_eq!(names(&source), Vec::<String>::new());
    assert!(destination.contains("type"));
    let (source, destination) = run(&source, "m", &destination);
    assert!(!source.contains("mod m"));
    assert!(destination.contains("mod m"));
    compile_and_run(
        &destination,
        "assert_eq!(r#type(),1); assert_eq!(m::n(),2);",
    );
}
#[test]
fn moved_item_referenced_by_source_is_still_moved_and_other_items_untouched() {
    let (source, destination) = run("fn a()->i32{1} fn b()->i32{a()}", "a", "");
    assert_eq!(names(&source), ["b"]);
    assert_eq!(names(&destination), ["a"]);
    assert!(!destination.contains("b"));
}
#[test]
fn refusals_for_absent_duplicate_and_colliding_items() {
    for (source, item, destination) in [
        ("fn a(){}", "missing", ""),
        ("fn a(){} fn a(){}", "a", ""),
        ("fn a(){} struct a;", "a", ""),
        ("fn a(){}", "a", "fn a(){}"),
        ("fn a(){}", "a", "struct a;"),
        ("fn a(){}", "a", "use x::a;"),
        ("fn a(){}", "a", "use x::b as a;"),
        ("fn a(){}", "a", "use x::a as b;"),
        ("fn a(){}", "a", "use x::{y, a};"),
        ("mod a;", "a", ""),
        ("fn a( {", "a", ""),
        ("fn a(){}", "a", "fn ("),
    ] {
        error(s3(source, item, destination), "E_BAD_ARG");
    }
}
#[test]
fn refusals_for_impls_macros_and_disallowed_attributes() {
    for (source, item) in [
        ("struct S; impl S { fn f(){} }", "S"),
        ("struct S; impl T for S {}", "S"),
        ("pub mod a { macro_rules! m { () => {} } }", "a"),
        ("pub fn a(){ macro_rules! m { () => {} } }", "a"),
        ("#[macro_export] pub fn a(){}", "a"),
        ("#[no_mangle] pub fn a(){}", "a"),
        ("#[path = \"x\"] pub mod a {}", "a"),
        ("#[cfg_attr(unix, inline)] pub fn a(){}", "a"),
        ("pub mod a { #[no_mangle] pub fn n(){} }", "a"),
    ] {
        error(s3(source, item, ""), "E_BAD_ARG");
    }
}
#[test]
fn bad_identifiers_shapes_and_pipeline_input_refuse() {
    for item in ["bad-name", "fn", "a b", "_", "1x"] {
        error(s3("fn a(){}", item, ""), "E_BAD_ARG");
    }
    for args in [
        vec![],
        vec![s("fn a(){}"), s("a")],
        vec![s("fn a(){}"), s("a"), s(""), s("extra")],
        vec![Value::Int(1), s("a"), s("")],
        vec![s("fn a(){}"), Value::Int(1), s("")],
        vec![s("fn a(){}"), s("a"), Value::Int(1)],
    ] {
        error(args, "E_BAD_ARG");
    }
    assert!(call_with_input(
        "refactor_move_to_file",
        s3("fn a(){}", "a", ""),
        Some(s("input")),
        &mut Env::default()
    )
    .is_err());
}
#[test]
fn oversized_inputs_refuse_with_budget_errors() {
    error(s3(&"a".repeat(262_145), "a", ""), "E_BUDGET");
    error(
        s3(&format!("fn a(){{}} //{}", "!".repeat(4097)), "a", ""),
        "E_BUDGET",
    );
    error(s3("fn a(){}", &"a".repeat(257), ""), "E_BUDGET");
    error(
        s3("fn a(){}", &format!("a{}", "!".repeat(33)), ""),
        "E_BUDGET",
    );
    error(s3("fn a(){}", "a", &"a".repeat(262_145)), "E_BUDGET");
    error(
        s3("fn a(){}", "a", &format!("// {}", "!".repeat(4097))),
        "E_BUDGET",
    );
}
#[test]
fn move_never_touches_the_filesystem() {
    let scratch = Scratch::new();
    let before = std::fs::read_dir(&scratch.0).unwrap().count();
    // Path-like text is Rust text here: it is refused, not read.
    let path = scratch.0.join("source.rs");
    std::fs::write(&path, "fn a(){}").unwrap();
    let text = path.display().to_string();
    error(s3(&text, "a", ""), "E_BAD_ARG");
    let _ = run("fn a(){}", "a", "");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn a(){}");
    assert_eq!(
        std::fs::read_dir(&scratch.0).unwrap().count(),
        before + 1,
        "only the fixture file may exist"
    );
    assert_eq!(
        aethershell::safety::effect_of("refactor_move_to_file"),
        aethershell::safety::Effect::Pure
    );
}
#[test]
fn new_examples_and_aliases_execute_with_pure_effects() {
    let name = "refactor_move_to_file";
    let alias = "refactor.move_to_file";
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
