use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
fn s(x: &str) -> Value {
    Value::Str(x.into())
}
fn codes(x: &[&str]) -> Value {
    Value::Array(x.iter().map(|x| s(x)).collect())
}
fn filter(report: &str, selectors: &[&str]) -> String {
    let Value::Str(result) = call(
        "diag_suppress",
        vec![s(report), codes(selectors)],
        &mut Env::default(),
    )
    .unwrap() else {
        panic!("expected String")
    };
    result
}
fn diagnostic(level: &str, code: Option<&str>) -> String {
    serde_json::json!({"$message_type":"diagnostic", "level":level,"message":"witness", "code":code.map(|code| serde_json::json!({"code":code,"explanation":null})), "children":[], "spans":[], "rendered":null}).to_string()
}
#[test]
fn exact_codes_preserve_errors_other_events_and_original_bytes() {
    let warning = diagnostic("warning", Some("unused_variables"));
    let error = diagnostic("error", Some("unused_variables"));
    let other = diagnostic("warning", Some("unused_variables_extra"));
    let summary = diagnostic("warning", None);
    let cargo = serde_json::json!({"reason":"compiler-message", "message":serde_json::from_str::<serde_json::Value>(&diagnostic("warning", Some("clippy::needless_return"))).unwrap()}).to_string();
    let event = "{\"reason\":\"build-finished\",\"success\":false}";
    let report = format!("{warning}\r\n{error}\r\n{cargo}\n{other}\n{summary}\n{event}");
    assert_eq!(
        filter(&report, &["unused_variables", "clippy::needless_return"]),
        format!("{error}\r\n{other}\n{summary}\n{event}")
    );
    assert_eq!(filter("", &["unused"]), "");
    assert_eq!(filter(&warning, &["unused_variables"]), "");
}
#[test]
fn malformed_or_ambiguous_reports_refuse_before_returning_output() {
    for report in [
        "warning: unused x",
        "[]",
        "{}",
        "\n",
        "{\"reason\":true}",
        "{\"reason\":\"compiler-message\",\"message\":null}",
        "{\"reason\":\"build-finished\",\"reason\":\"compiler-message\"}",
        "{\"reason\":\"build-finished\",\"nested\":{\"x\":1,\"x\":2}}",
    ] {
        assert!(call(
            "diag_suppress",
            vec![s(report), codes(&["x"])],
            &mut Env::default()
        )
        .unwrap_err()
        .to_string()
        .contains("E_BAD_ARG"));
    }
    let error = diagnostic("error", Some("E0308"));
    assert_eq!(filter(&error, &["E0308"]), error);
    for field in ["level", "message", "code"] {
        let mut report: serde_json::Value = serde_json::from_str(&error).unwrap();
        report.as_object_mut().unwrap().remove(field);
        assert!(call(
            "diag_suppress",
            vec![s(&report.to_string()), codes(&["E0308"])],
            &mut Env::default()
        )
        .is_err());
    }
}
#[test]
fn invalid_shapes_and_budgets_refuse() {
    for args in [
        vec![],
        vec![s("x")],
        vec![s("x"), codes(&[])],
        vec![s("x"), codes(&["x", "x"])],
        vec![s("x"), codes(&["bad code"])],
        vec![s("x"), codes(&[":bad"])],
        vec![s("x"), codes(&["bad:::code"])],
        vec![Value::Int(1), codes(&["x"])],
        vec![s("x"), Value::Array(vec![Value::Bool(true)])],
    ] {
        assert!(call("diag_suppress", args, &mut Env::default())
            .unwrap_err()
            .to_string()
            .contains("E_BAD_ARG"));
    }
    let short = "{\"reason\":\"build-finished\"}\n";
    for args in [
        vec![s(&"a".repeat(1048577)), codes(&["x"])],
        vec![s(&"a".repeat(65537)), codes(&["x"])],
        vec![s(&short.repeat(25001)), codes(&["x"])],
        vec![s(""), codes(&[&"x".repeat(129)])],
    ] {
        assert!(call("diag_suppress", args, &mut Env::default())
            .unwrap_err()
            .to_string()
            .contains("E_BUDGET"));
    }
    assert!(call_with_input(
        "diag_suppress",
        vec![s(""), codes(&["x"])],
        Some(s("input")),
        &mut Env::default()
    )
    .is_err());
}
#[test]
fn actual_rustc_json_warning_can_be_filtered() {
    let root = std::env::temp_dir().join(format!("ae-diag-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            if self.0.canonicalize().is_ok_and(|p| p == self.0) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
    let scratch = Scratch(root.canonicalize().unwrap());
    let source = scratch.0.join("unit.rs");
    std::fs::write(&source, "fn main() { let x = 1; }").unwrap();
    let output =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args([
                "--error-format=json",
                "--emit=metadata",
                "--crate-name",
                "diag_witness",
            ])
            .arg(&source)
            .arg("-o")
            .arg(scratch.0.join("unit.rmeta"))
            .output()
            .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stderr).unwrap();
    assert!(report.contains("unused_variables"));
    let kept = filter(&report, &["unused_variables"]);
    assert!(kept.len() < report.len());
    for line in kept.lines() {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_ne!(event["code"]["code"], "unused_variables");
    }
    assert_eq!(filter(&report, &["unrelated_code"]), report);
}
#[test]
fn declared_example_and_module_alias_are_pure_and_executable() {
    assert_eq!(
        aethershell::safety::effect_of("diag_suppress"),
        aethershell::safety::Effect::Pure
    );
    for (code, expected) in aethershell::signature::signature_of("diag_suppress")
        .unwrap()
        .examples
    {
        for source in [
            code.to_string(),
            code.replacen("diag_suppress", "diag.suppress", 1),
        ] {
            let mut env = Env::default();
            env.register_module("diag", aethershell::modules::diag_module());
            let statements = aethershell::parser::parse_program(&source).unwrap();
            let result = aethershell::eval::eval_program(&statements, &mut env).unwrap();
            assert_eq!(result.to_display_string(), *expected);
        }
    }
}
