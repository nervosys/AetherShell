use aethershell::{
    ast::Expr,
    builtins::{call, call_with_input},
    env::Env,
    value::{Lambda, Value},
};
use std::{collections::BTreeMap, path::PathBuf};
fn s(value: &str) -> Value {
    Value::Str(value.into())
}
fn record(fields: &[(&str, Value)]) -> Value {
    Value::Record(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
fn fields(value: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(r) = value else {
        panic!("{value:?}")
    };
    r
}
fn rows(value: &Value) -> &[Value] {
    let Value::Array(r) = value else {
        panic!("{value:?}")
    };
    r
}
fn run(name: &str, args: Vec<Value>) -> Value {
    call(name, args, &mut Env::default()).unwrap()
}
fn error(name: &str, args: Vec<Value>, code: &str) {
    let error = call(name, args, &mut Env::default()).unwrap_err();
    assert!(error.to_string().contains(code), "{error}");
}
fn case(arguments: &[&str], expected: &str) -> Value {
    record(&[
        (
            "arguments",
            Value::Array(arguments.iter().map(|s0| s(s0)).collect()),
        ),
        ("expected", s(expected)),
    ])
}
#[test]
fn snapshots_preserve_types_and_report_sorted_pointer_differences_and_absence() {
    let actual = record(&[
        ("a/b~c", Value::Int(1)),
        ("array", Value::Array(vec![Value::Int(7), Value::Int(8)])),
        ("added", Value::Null),
        ("typed", Value::Float(1.0)),
    ]);
    let expected = record(&[
        ("a/b~c", Value::Int(2)),
        ("array", Value::Array(vec![Value::Int(7)])),
        ("removed", Value::Null),
        ("typed", Value::Int(1)),
    ]);
    let result = run("test_snapshot", vec![actual, expected]);
    let result = fields(&result);
    assert_eq!(result["matched"], Value::Bool(false));
    assert_eq!(result["mismatches"], Value::Int(5));
    assert_eq!(result["truncated"], Value::Bool(false));
    let changes = rows(&result["differences"]);
    let expected = [
        ("/a~1b~0c", "changed"),
        ("/added", "added"),
        ("/array/1", "added"),
        ("/removed", "removed"),
        ("/typed", "changed"),
    ];
    for (change, (path, kind)) in changes.iter().zip(expected) {
        assert_eq!(fields(change)["path"], s(path));
        assert_eq!(fields(change)["kind"], s(kind));
    }
    assert_eq!(fields(&changes[3])["expected"], Value::Null);
    assert_eq!(fields(&changes[4])["actual"], Value::Float(1.0));
    let exact = record(&[(
        "x",
        Value::Array(vec![
            s("α"),
            Value::Bool(true),
            Value::Null,
            Value::Float(1.5),
        ]),
    )]);
    let result = run("test_snapshot", vec![exact.clone(), exact]);
    assert_eq!(fields(&result)["matched"], Value::Bool(true));
    assert!(rows(&fields(&result)["differences"]).is_empty());
    let result = run("test_snapshot", vec![Value::Float(-0.0), Value::Float(0.0)]);
    assert_eq!(fields(&result)["matched"], Value::Bool(true));
    let result = run("test_snapshot", vec![Value::Bool(true), Value::Int(1)]);
    assert_eq!(
        fields(&rows(&fields(&result)["differences"])[0])["path"],
        s("")
    );
}
#[test]
fn snapshots_cap_output_but_count_all_changes_and_validate_both_inputs_first() {
    let actual = Value::Array((0..200).map(Value::Int).collect());
    let expected = Value::Array((200..400).map(Value::Int).collect());
    let result = run("test_snapshot", vec![actual, expected]);
    assert_eq!(fields(&result)["mismatches"], Value::Int(200));
    assert_eq!(fields(&result)["truncated"], Value::Bool(true));
    assert_eq!(rows(&fields(&result)["differences"]).len(), 128);
    for opaque in [
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::Uri("https://example.com".into()),
        Value::Lambda(Lambda {
            params: vec!["x".into()],
            body: Box::new(Expr::Ident("x".into())),
            captured: BTreeMap::new(),
        }),
    ] {
        error("test_snapshot", vec![Value::Null, opaque], "E_BAD_ARG");
    }
    let mut nested = Value::Int(0);
    for _ in 0..33 {
        nested = Value::Array(vec![nested])
    }
    error(
        "test_snapshot",
        vec![nested, Value::Null],
        "E_BUDGET_EXCEEDED",
    );
    error(
        "test_snapshot",
        vec![Value::Array(vec![Value::Null; 10000]), Value::Null],
        "E_BUDGET_EXCEEDED",
    );
    error(
        "test_snapshot",
        vec![s(&"a".repeat(1048577)), Value::Null],
        "E_BUDGET_EXCEEDED",
    );
    let long_key = "x".repeat(257);
    error(
        "test_snapshot",
        vec![record(&[(&long_key, Value::Null)]), Value::Null],
        "E_BUDGET_EXCEEDED",
    );
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("ae-generated-tests-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.canonicalize().is_ok_and(|p| p == self.0) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
fn compile_and_run(generated: &str) -> std::process::Output {
    let scratch = Scratch::new();
    let source = scratch.0.join("unit.rs");
    let executable = scratch.0.join(if cfg!(windows) {
        "unit_tests.exe"
    } else {
        "unit_tests"
    });
    std::fs::write(
        &source,
        format!("#![deny(warnings)]\nfn add(a:i32,b:i32)->i32{{a+b}}\n{generated}"),
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(rustc)
        .args(["--test", "--edition=2021"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::process::Command::new(&executable)
        .args(["--color", "never"])
        .output()
        .unwrap()
}
#[test]
fn generated_rust_tests_compile_run_and_detect_wrong_oracles() {
    let cases = Value::Array(vec![case(&["1", "2"], "3"), case(&["5", "-2"], "3")]);
    let Value::Str(generated) = run("test_generate", vec![s("crate::add"), cases]) else {
        panic!()
    };
    syn::parse_file(&generated).unwrap();
    assert!(generated.contains("case_000"));
    assert!(generated.contains("case_001"));
    let output = compile_and_run(&generated);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("2 passed; 0 failed"));
    let Value::Str(wrong) = run(
        "test_generate",
        vec![s("add"), Value::Array(vec![case(&["1", "2"], "4")])],
    ) else {
        panic!()
    };
    let output = compile_and_run(&wrong);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("case_000 ... FAILED"));
}
#[test]
fn test_generation_preserves_literal_contents_and_rejects_invalid_or_oversized_cases() {
    let Value::Str(generated) = run(
        "test_generate",
        vec![s("length"), Value::Array(vec![case(&[r#""a  b""#], "4")])],
    ) else {
        panic!()
    };
    let parsed = syn::parse_file(&generated).unwrap();
    assert_eq!(parsed.items.len(), 1);
    assert!(generated.contains(r#""a  b""#));
    error(
        "test_generate",
        vec![s("add; arbitrary()"), Value::Array(vec![case(&[], "0")])],
        "E_BAD_ARG",
    );
    error(
        "test_generate",
        vec![s("add"), Value::Array(vec![])],
        "E_BAD_ARG",
    );
    error(
        "test_generate",
        vec![s("add"), Value::Array(vec![case(&[], "0"); 129])],
        "E_BAD_ARG",
    );
    for bad_case in [
        case(&["("], "1"),
        case(&["1"], "1; 2"),
        record(&[("arguments", s("1")), ("expected", s("2"))]),
        record(&[
            ("arguments", Value::Array(vec![])),
            ("expected", s("1")),
            ("discarded", Value::Int(1)),
        ]),
    ] {
        error(
            "test_generate",
            vec![s("add"), Value::Array(vec![bad_case])],
            "E_BAD_ARG",
        );
    }
    let long = "x".repeat(16385);
    error(
        "test_generate",
        vec![s("add"), Value::Array(vec![case(&[&long], "1")])],
        "E_BUDGET_EXCEEDED",
    );
    let nested = format!("{}1{}", "(".repeat(65), ")".repeat(65));
    error(
        "test_generate",
        vec![s("add"), Value::Array(vec![case(&[&nested], "1")])],
        "E_BUDGET_EXCEEDED",
    );
    let literal = format!("\"{}\"", "a".repeat(8192));
    error(
        "test_generate",
        vec![s("add"), Value::Array(vec![case(&[&literal], &literal); 5])],
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn testing_contracts_reject_surplus_arguments_and_pipeline_input() {
    for (name, args) in [
        ("test_snapshot", vec![Value::Null, Value::Null]),
        (
            "test_generate",
            vec![s("add"), Value::Array(vec![case(&[], "0")])],
        ),
    ] {
        error(name, vec![], "E_BAD_ARG");
        let mut extra = args.clone();
        extra.push(Value::Int(1));
        error(name, extra, "E_BAD_ARG");
        let e = call_with_input(name, args, Some(s("discarded")), &mut Env::default()).unwrap_err();
        assert!(e.to_string().contains("E_BAD_ARG"), "{e}");
    }
}

#[test]
fn declared_testing_examples_run_through_the_language() {
    for name in ["test_snapshot", "test_generate"] {
        let sig = aethershell::signature::signature_of(name).unwrap();
        for (code, expected) in sig.examples {
            let statements = aethershell::parser::parse_program(code).unwrap();
            let result = aethershell::eval::eval_program(&statements, &mut Env::default()).unwrap();
            assert_eq!(result.to_display_string(), *expected, "{code}");
        }
    }
}
