use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
fn s(text: &str) -> Value {
    Value::Str(text.into())
}
fn i(value: i64) -> Value {
    Value::Int(value)
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
fn extract(source: &str, name: &str, start: i64, end: i64) -> Vec<Value> {
    vec![s(source), s(name), i(start), i(end)]
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-extract-{}", uuid::Uuid::new_v4()));
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
fn tail_extraction_passes_typed_captures_in_first_use_order() {
    let original = "pub fn f(a: i32) -> i32 { let t: i32 = a * 3; let u: i32 = t + a; u * 2 }";
    let output = run("refactor_extract_function", extract(original, "tail", 1, 3));
    assert!(
        output.contains("fn tail (t : i32 , a : i32) -> i32"),
        "{output}"
    );
    assert!(output.contains("tail (t , a)"), "{output}");
    compile_and_run(original, "assert_eq!(f(2), 16); assert_eq!(f(0), 0);");
    compile_and_run(&output, "assert_eq!(f(2), 16); assert_eq!(f(0), 0);");
}
#[test]
fn statement_extraction_compiles_and_runs_with_unit_call() {
    let original =
        "pub fn g(a: i32) -> i32 { let t: i32 = a * 3; let _ = t > 0; let r = a + 1; r }";
    let output = run(
        "refactor_extract_function",
        extract(original, "check", 0, 2),
    );
    assert!(output.contains("check (a) ;"), "{output}");
    assert!(output.contains("fn check (a : i32)"), "{output}");
    compile_and_run(&output, "assert_eq!(g(2), 3);");
    let raw = run(
        "refactor_extract_function",
        extract(original, "r#type", 0, 2),
    );
    compile_and_run(&raw, "assert_eq!(g(0), 1);");
}
#[test]
fn refusals_for_scoping_capture_and_control_flow() {
    for (source, start, end) in [
        // shadowing: `a` bound by the parameter and by the let
        ("fn f(a: i32) -> i32 { let a = 1; a }", 0, 1),
        // untyped captured let
        ("fn f() -> i32 { let t = 2; t * 3 }", 1, 2),
        // ? inside range
        (
            "fn f(o: Option<i32>) -> Option<i32> { let t: i32 = o?; Some(t) }",
            0,
            1,
        ),
        // return inside range
        ("fn f(a: i32) -> i32 { if a > 0 { return a; } 0 }", 0, 1),
        // assignment to captured variable
        ("fn f() -> i32 { let mut t: i32 = 1; t = t + 1; t }", 1, 2),
        // compound assignment to captured variable
        ("fn f() -> i32 { let mut t: i32 = 1; t += 1; t }", 1, 2),
        // &mut of captured variable
        (
            "fn f() -> i32 { let mut x: i32 = 1; let t: &mut i32 = &mut x; *t }",
            1,
            2,
        ),
        // method call on captured variable
        (
            "fn f() -> usize { let v: Vec<i32> = Vec::new(); let t: usize = v.len(); t }",
            1,
            2,
        ),
        // range-bound name used after end
        (
            "fn f(a: i32) -> i32 { let t: i32 = a; let u: i32 = t * 2; u + 1 }",
            1,
            2,
        ),
        // await inside range
        (
            "fn f(a: i32) -> i32 { let t: i32 = a; let _ = (async { t }).await; t }",
            1,
            2,
        ),
        // macro anywhere in the function
        (
            "fn f(a: i32) -> i32 { let t: i32 = a; println!(\"{}\", t); t }",
            0,
            2,
        ),
        // nested item
        ("fn f(a: i32) -> i32 { fn g() {} let t: i32 = a; t }", 1, 2),
    ] {
        error(
            "refactor_extract_function",
            extract(source, "extracted", start, end),
            "E_BAD_ARG",
        );
    }
    // Refusal on name collision, including a raw name.
    error(
        "refactor_extract_function",
        extract("fn f(a: i32) -> i32 { a }", "a", 0, 1),
        "E_BAD_ARG",
    );
    error(
        "refactor_extract_function",
        extract("fn f(a: i32) -> i32 { let t: i32 = a; t }", "f", 1, 2),
        "E_BAD_ARG",
    );
    error(
        "refactor_extract_function",
        extract(
            "fn f(r#type: i32) -> i32 { let t: i32 = r#type; t }",
            "r#type",
            1,
            2,
        ),
        "E_BAD_ARG",
    );
}
#[test]
fn signature_and_range_shape_refusals() {
    for source in [
        "fn f<T>(a: T) -> T { a }",
        "fn f(a: i32) -> i32 where i32: Copy { a }",
        "fn f(&self) -> i32 { 1 }",
        "async fn f(a: i32) -> i32 { a }",
        "const fn f(a: i32) -> i32 { a }",
        "unsafe fn f(a: i32) -> i32 { a }",
        "extern \"C\" fn f(a: i32) -> i32 { a }",
        "extern \"C\" fn f(x: i32, ...) -> i32 { x }",
        "impl X {}",
        "fn f(a: i32) -> i32 { a } fn g() {}",
    ] {
        error(
            "refactor_extract_function",
            extract(source, "extracted", 0, 1),
            "E_BAD_ARG",
        );
    }
    let source = "fn f(a: i32) -> i32 { let t: i32 = a; t }";
    for (start, end) in [(0, 0), (1, 1), (2, 1), (0, 3), (-1, 1), (0, -1)] {
        error(
            "refactor_extract_function",
            extract(source, "extracted", start, end),
            "E_BAD_ARG",
        );
    }
    error(
        "refactor_extract_function",
        vec![s(source), s("extracted"), s("0"), i(1)],
        "E_BAD_ARG",
    );
    for args in [
        vec![],
        vec![s(source), s("extracted"), i(0)],
        vec![s(source), s("extracted"), i(0), i(1), i(1)],
        vec![Value::Int(1), s("extracted"), i(0), i(1)],
        vec![s(source), Value::Int(1), i(0), i(1)],
        vec![s(source), s("bad-name"), i(0), i(1)],
    ] {
        error("refactor_extract_function", args, "E_BAD_ARG");
    }
    assert!(call_with_input(
        "refactor_extract_function",
        extract(source, "extracted", 0, 1),
        Some(s("input")),
        &mut Env::default()
    )
    .is_err());
}
#[test]
fn work_limits_are_coded_budget_errors() {
    error(
        "refactor_extract_function",
        extract(&"a".repeat(65537), "extracted", 0, 1),
        "E_BUDGET",
    );
    error(
        "refactor_extract_function",
        extract(
            "fn f(a: i32) -> i32 { let t: i32 = a; t }",
            &"a".repeat(257),
            1,
            2,
        ),
        "E_BUDGET",
    );
    error(
        "refactor_extract_function",
        extract(
            &format!(
                "fn f(a: i32) -> i32 {{ let t: i32 = a; t }} //{}",
                "!".repeat(257)
            ),
            "extracted",
            1,
            2,
        ),
        "E_BUDGET",
    );
}
#[test]
fn new_examples_and_aliases_execute_with_pure_effects() {
    for (name, alias) in [("refactor_extract_function", "refactor.extract_function")] {
        assert_eq!(
            iron_shell::safety::effect_of(name),
            iron_shell::safety::Effect::Pure
        );
        for (code, expected) in iron_shell::signature::signature_of(name).unwrap().examples {
            for source in [code.to_string(), code.replacen(name, alias, 1)] {
                let mut env = Env::default();
                env.register_module("refactor", iron_shell::modules::refactor_module());
                let statements = iron_shell::parser::parse_program(&source).unwrap();
                assert_eq!(
                    iron_shell::eval::eval_program(&statements, &mut env)
                        .unwrap()
                        .to_display_string(),
                    *expected
                );
            }
        }
    }
}
