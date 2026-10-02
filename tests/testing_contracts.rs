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

fn outcome_report(events: &[serde_json::Value]) -> String {
    events.iter().map(|v| format!("{v}\n")).collect()
}
fn empty_suite() -> String {
    outcome_report(&[
        serde_json::json!({"type":"suite", "event":"started", "test_count":0}),
        serde_json::json!({"type":"suite", "event":"ok", "passed":0, "failed":0, "ignored":0, "measured":0, "filtered_out":7}),
    ])
}
#[test]
fn lcov_summarizes_weighted_coverage_and_unions_repeated_source_records() {
    let report = concat!(
        "TN:first\nSF:z.rs\nFN:2,fun,with,commas\nFNDA:0,fun,with,commas\n",
        "DA:1,0,checksum1\nDA:2,3,checksum2\nBRDA:2,0,0,-\nBRDA:2,0,1,3\n",
        "LF:2\nLH:1\nFNF:1\nFNH:0\nBRF:2\nBRH:1\nend_of_record\n",
        "TN:second\nSF:z.rs\nFNDA:1,fun,with,commas\nFN:2,fun,with,commas\n",
        "DA:1,1,checksum1\nDA:2,0,checksum2\nBRDA:2,0,0,5\nBRDA:2,0,1,0\nend_of_record\n",
        "SF:C:\\not-a-real-path\\a.rs\nDA:9,0\nend_of_record\n"
    );
    for text in [report.to_string(), report.replace('\n', "\r\n")] {
        let result = run("test_coverage", vec![s(&text)]);
        let coverage = fields(&result);
        let lines = fields(&coverage["lines"]);
        assert_eq!(lines["found"], Value::Int(3));
        assert_eq!(lines["hit"], Value::Int(2));
        let Value::Float(percent) = lines["percent"] else {
            panic!()
        };
        assert!((percent - 200.0 / 3.0).abs() < 1e-10);
        assert_eq!(fields(&coverage["functions"])["hit"], Value::Int(1));
        assert_eq!(fields(&coverage["branches"])["hit"], Value::Int(2));
        let files = rows(&coverage["files"]);
        assert_eq!(fields(&files[0])["path"], s("C:\\not-a-real-path\\a.rs"));
        assert_eq!(fields(&fields(&files[1])["lines"])["hit"], Value::Int(2));
    }
    let empty = run("test_coverage", vec![s("SF:empty.rs\nend_of_record\n")]);
    assert_eq!(fields(&fields(&empty)["lines"])["percent"], Value::Null);
    let huge_count = run(
        "test_coverage",
        vec![s("SF:a\nDA:1,9223372036854775807\nend_of_record\n")],
    );
    assert_eq!(fields(&fields(&huge_count)["lines"])["hit"], Value::Int(1));
}
#[test]
fn lcov_rejects_partial_contradictory_duplicate_and_incompatible_records() {
    for report in [
        "",
        "a-file.info",
        "DA:1,1\n",
        "end_of_record\n",
        "SF:a\nDA:1,1\n",
        "SF:a\nSF:b\nend_of_record\n",
        "SF:a\nDA:0,1\nend_of_record\n",
        "SF:a\nDA:1,-1\nend_of_record\n",
        "SF:a\nDA:1,9223372036854775808\nend_of_record\n",
        "SF:a\nDA:1,1\nLH:0\nend_of_record\n",
        "SF:a\nLF:1\nend_of_record\n",
        "SF:a\nDA:1,1\nDA:1,0\nend_of_record\n",
        "SF:a\nLF:0\nLF:0\nend_of_record\n",
        "SF:a\nFN:1,f\nend_of_record\n",
        "SF:a\nFNDA:1,f\nend_of_record\n",
        "SF:a\nFN:1,f\nFNDA:1,g\nend_of_record\n",
        "SF:a\nBRDA:1,0,0,foo\nend_of_record\n",
        "SF:a\nBRDA:1,0,0,0\nBRDA:1,0,0,1\nend_of_record\n",
        "SF:a\nVER:2.0\nend_of_record\n",
        "SF:a\nDA:1,1,old\nend_of_record\nSF:a\nDA:1,1,new\nend_of_record\n",
        "SF:a\nFN:1,f\nFNDA:1,f\nend_of_record\nSF:a\nFN:2,f\nFNDA:1,f\nend_of_record\n",
        "SF:a\nDA:1,1,\nend_of_record\n",
        "SF:a\nTN:inside\nend_of_record\n",
    ] {
        error("test_coverage", vec![s(report)], "E_BAD_ARG");
    }
}
#[test]
fn libtest_reports_preserve_status_diagnostics_and_suite_identity() {
    use serde_json::json;
    let report = outcome_report(&[
        json!({"type":"suite","event":"started","test_count":4,"shuffle_seed":18446744073709551615u64}),
        json!({"type":"test","event":"started","name":"z"}),
        json!({"type":"test","event":"started","name":"a"}),
        json!({"type":"test","event":"timeout","name":"z"}),
        json!({"type":"test","event":"ignored","name":"later","message":"hardware unavailable"}),
        json!({"type":"test","event":"ok","name":"z","stdout":"hello\n☃","exec_time":0.2}),
        json!({"type":"test","event":"failed","name":"a","stdout":"panic","message":"bad result","reason":"time limit exceeded","exec_time":0}),
        json!({"type":"test","event":"started","name":"b"}),
        json!({"type":"test","event":"ok","name":"b"}),
        json!({"type":"suite","event":"failed","passed":2,"failed":1,"ignored":1,"measured":0,"filtered_out":9,"exec_time":0.3}),
        json!({"type":"report","total_time":0.3,"compilation_time":0.1}),
        json!({"type":"suite","event":"started","test_count":1}),
        json!({"type":"test","event":"started","name":"b"}),
        json!({"type":"test","event":"ok","name":"b"}),
        json!({"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0}),
    ]);
    let passed = run("test_passing", vec![s(&report)]);
    let passed = rows(&passed);
    assert_eq!(passed.len(), 3);
    assert_eq!(fields(&passed[0])["name"], s("b"));
    assert_eq!(fields(&passed[0])["suite"], Value::Int(1));
    assert_eq!(fields(&passed[0])["stdout"], Value::Null);
    assert_eq!(fields(&passed[1])["stdout"], s("hello\n☃"));
    assert_eq!(fields(&passed[2])["suite"], Value::Int(2));
    let failed = run("test_failing", vec![s(&report)]);
    let failure = fields(&rows(&failed)[0]);
    assert_eq!(failure["name"], s("a"));
    assert_eq!(failure["reason"], s("time limit exceeded"));
    assert_eq!(failure["exec_time"], Value::Float(0.0));
    let skipped = run("test_skipped", vec![s(&report)]);
    assert_eq!(rows(&skipped).len(), 1);
    assert_eq!(
        fields(&rows(&skipped)[0])["message"],
        s("hardware unavailable")
    );
}
#[test]
fn libtest_reports_refuse_missing_events_forged_counts_and_duplicate_fields() {
    use serde_json::json;
    let base = vec![
        json!({"type":"suite","event":"started","test_count":1}),
        json!({"type":"test","event":"started","name":"x"}),
        json!({"type":"test","event":"ok","name":"x"}),
        json!({"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0}),
    ];
    let mut bad_reports = vec![
        String::new(),
        "test result: ok".into(),
        outcome_report(&base[..3]),
        outcome_report(&base[1..]),
        "{\"type\":\"suite\",\"type\":\"test\",\"event\":\"started\",\"test_count\":0}".into(),
    ];
    let mut changes = Vec::new();
    let mut report = base.clone();
    report.remove(1);
    changes.push(report);
    let mut report = base.clone();
    report.insert(2, report[1].clone());
    changes.push(report);
    let mut report = base.clone();
    report.insert(3, report[2].clone());
    changes.push(report);
    for (i, key, value) in [
        (0, "test_count", json!(2)),
        (0, "test_count", json!(1.0)),
        (3, "passed", json!(0)),
        (3, "event", json!("failed")),
        (3, "measured", json!(1)),
        (2, "exec_time", json!(-1)),
        (2, "stdout", json!(null)),
        (2, "reason", json!({"nested":true})),
        (2, "extra", json!(1)),
        (3, "filtered_out", json!(-1)),
        (1, "event", json!("timeout")),
        (2, "name", json!("other")),
    ] {
        let mut report = base.clone();
        report[i][key] = value;
        changes.push(report);
    }
    bad_reports.extend(changes.iter().map(|v| outcome_report(v)));
    bad_reports.push(format!(
        "{}\n{{\"type\":\"suite\",\"event\":\"started\",\"test_count\":1}}",
        empty_suite()
    ));
    bad_reports.push(format!(
        "{}\n{{\"type\":\"bench\",\"name\":\"bench\",\"median\":1}}",
        empty_suite()
    ));
    bad_reports.push("{\"type\":\"suite\",\"event\":\"discovery\"}".into());
    bad_reports.push(format!("{} trailing", empty_suite()));
    for report in bad_reports {
        for name in ["test_passing", "test_failing", "test_skipped"] {
            error(name, vec![s(&report)], "E_BAD_ARG");
        }
    }
    for name in ["test_passing", "test_failing", "test_skipped"] {
        assert!(rows(&run(name, vec![s(&empty_suite())])).is_empty());
    }
}
#[test]
fn report_builtins_bound_work_and_refuse_discarded_arguments() {
    for (name, report) in [
        ("test_coverage", "SF:a\nend_of_record\n".to_string()),
        ("test_passing", empty_suite()),
        ("test_failing", empty_suite()),
        ("test_skipped", empty_suite()),
    ] {
        error(name, vec![], "E_BAD_ARG");
        error(name, vec![Value::Null], "E_BAD_ARG");
        error(name, vec![s(&report), s("discarded")], "E_BAD_ARG");
        let err = call_with_input(
            name,
            vec![s(&report)],
            Some(Value::Null),
            &mut Env::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("E_BAD_ARG"));
        for oversized in ["x".repeat(1048577), "x".repeat(65537), "\n".repeat(25001)] {
            error(name, vec![s(&oversized)], "E_BUDGET_EXCEEDED");
        }
    }
    error(
        "test_coverage",
        vec![s(&"SF:a\nend_of_record\n".repeat(1025))],
        "E_BUDGET_EXCEEDED",
    );
    let mut entries = String::from("SF:a\n");
    for line in 1..=10001 {
        entries.push_str(&format!("DA:{line},0\n"));
    }
    entries.push_str("end_of_record\n");
    error("test_coverage", vec![s(&entries)], "E_BUDGET_EXCEEDED");
    for name in ["test_passing", "test_failing", "test_skipped"] {
        error(
            name,
            vec![s(&empty_suite().repeat(129))],
            "E_BUDGET_EXCEEDED",
        );
        error(
            name,
            vec![s(
                "{\"type\":\"suite\",\"event\":\"started\",\"test_count\":10001}",
            )],
            "E_BUDGET_EXCEEDED",
        );
    }
}
#[test]
fn outcome_reporters_parse_actual_rust_harness_pass_failure_ignore_and_filtering() {
    let scratch = Scratch::new();
    let source = scratch.0.join("report_tests.rs");
    let executable = scratch.0.join(if cfg!(windows) {
        "report_tests.exe"
    } else {
        "report_tests"
    });
    std::fs::write(
        &source,
        r#"#![deny(warnings)]
#[test] fn fixture_passed() { println!("hello\n☃"); }
#[test] fn fixture_failed() { panic!("fixture_failure"); }
#[test] #[ignore = "hardware unavailable"] fn fixture_skipped() {}
#[test] fn excluded_by_filter() {}
"#,
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = std::process::Command::new(rustc)
        .args(["--test", "--edition=2021"])
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    // Enable only this owned child harness's unstable JSON reporter. No global
    // environment or production compiler options are changed.
    let output = std::process::Command::new(executable)
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "-Z",
            "unstable-options",
            "--format=json",
            "--show-output",
            "--test-threads=1",
            "fixture_",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let passed = run("test_passing", vec![s(&text)]);
    assert_eq!(rows(&passed).len(), 1);
    assert_eq!(fields(&rows(&passed)[0])["name"], s("fixture_passed"));
    assert_eq!(fields(&rows(&passed)[0])["stdout"], s("hello\n☃\n"));
    let failed = run("test_failing", vec![s(&text)]);
    assert_eq!(rows(&failed).len(), 1);
    assert_eq!(fields(&rows(&failed)[0])["name"], s("fixture_failed"));
    let Value::Str(stdout) = &fields(&rows(&failed)[0])["stdout"] else {
        panic!()
    };
    assert!(stdout.contains("fixture_failure"));
    let skipped = run("test_skipped", vec![s(&text)]);
    assert_eq!(rows(&skipped).len(), 1);
    assert_eq!(fields(&rows(&skipped)[0])["name"], s("fixture_skipped"));
    assert_eq!(
        fields(&rows(&skipped)[0])["message"],
        s("hardware unavailable")
    );
}
#[test]
fn report_declaration_examples_and_module_aliases_execute_as_pure_operations() {
    for (name, alias) in [
        ("test_coverage", "test.coverage"),
        ("test_passing", "test.passing"),
        ("test_failing", "test.failing"),
        ("test_skipped", "test.skipped"),
    ] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::Pure
        );
        let signature = aethershell::signature::signature_of(name).unwrap();
        for (code, expected) in signature.examples {
            for source in [code.to_string(), code.replacen(name, alias, 1)] {
                let statements = aethershell::parser::parse_program(&source).unwrap();
                let mut env = Env::default();
                env.register_module("test", aethershell::modules::test_module());
                let result = aethershell::eval::eval_program(&statements, &mut env).unwrap();
                assert_eq!(result.to_display_string(), *expected, "{source}");
            }
        }
    }
}
