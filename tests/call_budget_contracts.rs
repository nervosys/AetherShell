use aethershell::{
    builtins::{budget_value, call, call_with_input},
    env::Env,
    value::{Table, Value},
};
use std::collections::BTreeMap;
fn fields(v: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(r) = v else {
        panic!("expected Record")
    };
    r
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn dynamic_calls_forward_names_aliases_arguments_and_pipeline_inputs() {
    let mut env = Env::default();
    assert_eq!(
        call(
            "call",
            vec![Value::Uri("lower".into()), Value::Str("ABC".into())],
            &mut env
        )
        .unwrap(),
        Value::Str("abc".into())
    );
    assert_eq!(
        call(
            "call",
            vec![Value::Str("type_of".into()), Value::Int(3)],
            &mut env
        )
        .unwrap(),
        Value::Str("Int".into())
    );
    assert_eq!(
        call(
            "call",
            vec![
                Value::Str("call".into()),
                Value::Str("lower".into()),
                Value::Str("ABC".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("abc".into())
    );
    assert_eq!(
        call_with_input(
            "call",
            vec![Value::Str("len".into())],
            Some(Value::Array(vec![Value::Int(1), Value::Int(2)])),
            &mut env
        )
        .unwrap(),
        Value::Int(2)
    );
    let default = call("call", vec![Value::Str("rl_gridworld".into())], &mut env).unwrap();
    assert_eq!(fields(&default)["width"], Value::Int(5));
}
#[test]
fn dynamic_calls_preserve_target_signature_errors_and_unknown_names() {
    let mut env = Env::default();
    for args in [vec![], vec![Value::Int(1)]] {
        code(call("call", args, &mut env), "E_BAD_ARG");
    }
    let direct = call("lower", vec![Value::Int(1)], &mut env)
        .unwrap_err()
        .to_string();
    let dynamic = call(
        "call",
        vec![Value::Str("lower".into()), Value::Int(1)],
        &mut env,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(dynamic, direct);
    let e = call(
        "call",
        vec![Value::Str("not_a_builtin_ae_contract_test".into())],
        &mut env,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("unknown builtin"), "{e}");
}
#[test]
fn dynamic_dispatch_keeps_the_target_effect_gate() {
    struct Restore {
        vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
        workspace: std::path::PathBuf,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in &self.vars {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
            let _ = std::fs::remove_file(self.workspace.join("audit.log"));
            let _ = std::fs::remove_dir(&self.workspace);
        }
    }
    let workspace = std::env::temp_dir().join(format!("ae-call-contract-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&workspace).unwrap();
    let keys = [
        "AETHER_MODE",
        "AETHER_POLICY",
        "AETHER_WORKSPACE",
        "AETHER_AUDIT_LOG",
        "AETHER_AUDIT_SINK",
        "AETHER_AUDIT_KEY",
        "AETHER_AUDIT_KEY_FILE",
        "AETHER_APPROVE",
        "AETHER_APPROVE_ALL",
    ];
    let _restore = Restore {
        vars: keys
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect(),
        workspace: workspace.clone(),
    };
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    std::env::set_var("AETHER_AUDIT_LOG", workspace.join("audit.log"));
    std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
    for key in [
        "AETHER_AUDIT_SINK",
        "AETHER_AUDIT_KEY_FILE",
        "AETHER_APPROVE",
        "AETHER_APPROVE_ALL",
    ] {
        std::env::remove_var(key);
    }
    let mut env = Env::default();
    // The requested shell command only prints if the guard ever regresses.
    code(
        call(
            "call",
            vec![
                Value::Str("shell".into()),
                Value::Str("echo".into()),
                Value::Str("ae_contract_test".into()),
            ],
            &mut env,
        ),
        "E_NEEDS_APPROVAL",
    );
}
#[test]
fn budget_pages_arrays_and_tables_without_changing_rendering_or_input() {
    let mut env = Env::default();
    let rows: Vec<_> = (0..3)
        .map(|n| BTreeMap::from([("n".into(), Value::Int(n))]))
        .collect();
    let array = Value::Array(rows.iter().cloned().map(Value::Record).collect());
    let table = Value::Table(Table {
        schema: vec!["n".into()],
        rows,
    });
    let expected = call(
        "aecon",
        vec![Value::Array(vec![
            Value::Record(BTreeMap::from([("n".into(), Value::Int(1))])),
            Value::Record(BTreeMap::from([("n".into(), Value::Int(2))])),
        ])],
        &mut env,
    )
    .unwrap();
    for value in [array.clone(), table] {
        let result = call(
            "budget",
            vec![value.clone(), Value::Int(1000), Value::Int(1)],
            &mut env,
        )
        .unwrap();
        let piped = call_with_input(
            "budget",
            vec![Value::Int(1000), Value::Int(1)],
            Some(value.clone()),
            &mut env,
        )
        .unwrap();
        assert_eq!(piped, result);
        assert_eq!(fields(&result)["page"], expected);
        assert_eq!(fields(&result)["from"], Value::Int(1));
        assert_eq!(fields(&result)["shown"], Value::Int(2));
        assert_eq!(fields(&result)["total"], Value::Int(3));
        assert_eq!(fields(&result)["elided"], Value::Int(0));
        assert_eq!(fields(&result)["next_cursor"], Value::Null);
        assert_eq!(fields(&result)["truncated"], Value::Bool(true));
    }
    assert_eq!(
        fields(&budget_value(&array, 1000, 0))["shown"],
        Value::Int(3)
    );
}
#[test]
fn paging_makes_progress_on_oversized_rows_and_clamps_empty_or_exhausted_pages() {
    let oversized = Value::Array(vec![
        Value::Str("oversized row ".repeat(1000)),
        Value::Int(2),
    ]);
    let page = budget_value(&oversized, 1, 0);
    assert_eq!(fields(&page)["shown"], Value::Int(1));
    assert_eq!(fields(&page)["next_cursor"], Value::Int(1));
    assert_eq!(fields(&page)["elided"], Value::Int(1));
    let Value::Int(tokens) = fields(&page)["page_tokens"] else {
        panic!("expected count")
    };
    assert!(tokens > 1);
    let exhausted = budget_value(&oversized, 100, usize::MAX);
    assert_eq!(fields(&exhausted)["from"], Value::Int(2));
    assert_eq!(fields(&exhausted)["shown"], Value::Int(0));
    assert_eq!(fields(&exhausted)["next_cursor"], Value::Null);
    let empty = budget_value(&Value::Array(vec![]), 1, 0);
    assert_eq!(fields(&empty)["truncated"], Value::Bool(false));
}
#[test]
fn scalar_elision_is_explicit_and_invalid_cursors_and_budgets_are_refused() {
    let mut env = Env::default();
    let input = Value::Str("界🙂".repeat(100));
    let result = call("budget", vec![input.clone(), Value::Int(1)], &mut env).unwrap();
    assert_eq!(fields(&result)["truncated"], Value::Bool(true));
    let Value::Str(page) = &fields(&result)["page"] else {
        panic!("expected page")
    };
    assert!(page.contains("more chars elided"), "{page}");
    for max in [Value::Int(0), Value::Int(-1), Value::Float(1.0)] {
        code(
            call("budget", vec![input.clone(), max], &mut env),
            "E_BAD_ARG",
        );
    }
    for cursor in [Value::Int(-1), Value::Float(0.0), Value::Str("x".into())] {
        code(
            call_with_input(
                "budget",
                vec![Value::Int(1), cursor],
                Some(Value::Array(vec![])),
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    code(
        call(
            "budget",
            vec![input, Value::Int(1), Value::Int(1)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}
