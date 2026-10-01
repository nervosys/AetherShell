use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;

fn fields(v: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(r) = v else {
        panic!("expected Record")
    };
    r
}
fn data(v: &Value) -> serde_json::Value {
    let Value::Str(s) = &fields(v)["_data"] else {
        panic!("expected encoding")
    };
    serde_json::from_str(s).unwrap()
}
fn encoded(v: serde_json::Value) -> Value {
    Value::Record(BTreeMap::from([(
        "_data".into(),
        Value::Str(v.to_string()),
    )]))
}
fn agent(env: &mut Env) -> Value {
    call(
        "rl_sarsa_agent",
        vec![Value::Str("test".into()), Value::Int(2), Value::Int(3)],
        env,
    )
    .unwrap()
}
fn transition() -> Vec<Value> {
    vec![
        Value::Int(0),
        Value::Int(1),
        Value::Int(2),
        Value::Int(1),
        Value::Int(0),
        Value::Bool(false),
    ]
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}

#[test]
fn creation_checks_dimensions_and_rejects_discarded_configuration() {
    let mut env = Env::default();
    for (states, actions, expected) in [
        (-1, 3, "E_BAD_ARG"),
        (2, 0, "E_BAD_ARG"),
        (2, 10001, "E_BUDGET_EXCEEDED"),
        (1001, 1000, "E_BUDGET_EXCEEDED"),
    ] {
        code(
            call(
                "rl_sarsa_agent",
                vec![
                    Value::Str("test".into()),
                    Value::Int(states),
                    Value::Int(actions),
                ],
                &mut env,
            ),
            expected,
        );
    }
    code(
        call(
            "rl_sarsa_agent",
            vec![
                Value::Str("test".into()),
                Value::Int(2),
                Value::Int(3),
                Value::Record(BTreeMap::new()),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn nonempty_tables_round_trip_and_sarsa_uses_the_supplied_next_action() {
    let mut env = Env::default();
    let original = agent(&mut env);
    let mut wire = data(&original);
    wire["learning_rate"] = serde_json::json!(1);
    wire["discount"] = serde_json::json!(1);
    wire["q_table"] = serde_json::json!([[1, 0, 5], [1, 2, 99]]);
    let input = encoded(wire);
    let mut direct = vec![input.clone()];
    direct.extend(transition());
    let result = call("rl_sarsa_update", direct, &mut env).unwrap();
    assert_eq!(
        data(&result)["q_table"],
        serde_json::json!([[0, 1, 7.0], [1, 0, 5.0], [1, 2, 99.0]])
    );
    assert_eq!(fields(&result)["steps"], Value::Int(1));
    assert_eq!(fields(&result)["epsilon"], fields(&original)["epsilon"]);
    assert_eq!(data(&original)["q_table"], serde_json::json!([]));
    let mut terminal = transition();
    terminal[5] = Value::Bool(true);
    let result = call_with_input("rl_sarsa_update", terminal, Some(result), &mut env).unwrap();
    assert_eq!(data(&result)["q_table"][0], serde_json::json!([0, 1, 2.0]));
    assert_eq!(fields(&result)["steps"], Value::Int(2));
}

#[test]
fn direct_and_pipeline_updates_refuse_invalid_indices_rewards_and_flags() {
    let mut env = Env::default();
    let input = agent(&mut env);
    for (index, value) in [
        (0, Value::Int(-1)),
        (1, Value::Int(3)),
        (2, Value::Float(f64::INFINITY)),
        (3, Value::Int(2)),
        (4, Value::Float(0.0)),
        (5, Value::Int(1)),
    ] {
        let mut args = transition();
        args[index] = value;
        code(
            call_with_input(
                "rl_sarsa_update",
                args.clone(),
                Some(input.clone()),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        args.insert(0, input.clone());
        code(call("rl_sarsa_update", args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn decoding_checks_metadata_and_unique_rows_but_accepts_legacy_empty_tables() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    let mut legacy = original.clone();
    legacy["q_table"] = serde_json::json!({});
    call_with_input(
        "rl_sarsa_update",
        transition(),
        Some(encoded(legacy)),
        &mut env,
    )
    .unwrap();
    for rows in [
        serde_json::json!([[0, 1, 2], [0, 1, 3]]),
        serde_json::json!([[2, 0, 1]]),
        serde_json::json!([[0, 3, 1]]),
        serde_json::json!([[0.0, 1, 2]]),
        serde_json::json!([[0, 1]]),
        serde_json::json!({"0,1": 2}),
    ] {
        let mut wire = original.clone();
        wire["q_table"] = rows;
        code(
            call_with_input(
                "rl_sarsa_update",
                transition(),
                Some(encoded(wire)),
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    for (key, value) in [
        ("state_size", 0),
        ("epsilon", 2),
        ("learning_rate", -1),
        ("discount", 2),
    ] {
        let mut wire = original.clone();
        wire[key] = serde_json::json!(value);
        code(
            call_with_input(
                "rl_sarsa_update",
                transition(),
                Some(encoded(wire)),
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    let invalid = Value::Record(BTreeMap::from([("_data".into(), Value::Str("{".into()))]));
    code(
        call_with_input("rl_sarsa_update", transition(), Some(invalid), &mut env),
        "E_BAD_ARG",
    );
}

#[test]
fn update_refuses_counter_and_numeric_overflow() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    let mut wire = original.clone();
    wire["steps"] = serde_json::json!(i64::MAX);
    code(
        call_with_input(
            "rl_sarsa_update",
            transition(),
            Some(encoded(wire)),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    let mut wire = original;
    wire["learning_rate"] = serde_json::json!(1);
    wire["q_table"] = serde_json::json!([[0, 1, -f64::MAX]]);
    let mut args = transition();
    args[2] = Value::Float(f64::MAX);
    args[5] = Value::Bool(true);
    code(
        call_with_input("rl_sarsa_update", args, Some(encoded(wire)), &mut env),
        "E_BAD_STATE",
    );
}
