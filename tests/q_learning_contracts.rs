use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;

fn agent(env: &mut Env) -> Value {
    call(
        "rl_agent",
        vec![
            Value::Str("test".into()),
            Value::Int(2),
            Value::Int(3),
            Value::Record(BTreeMap::from([
                ("learning_rate".into(), Value::Int(1)),
                ("discount".into(), Value::Int(1)),
                ("epsilon".into(), Value::Int(0)),
            ])),
        ],
        env,
    )
    .unwrap()
}

fn fields(value: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(fields) = value else {
        panic!("expected Record")
    };
    fields
}

fn data(value: &Value) -> serde_json::Value {
    let Value::Str(data) = &fields(value)["_data"] else {
        panic!("expected encoding")
    };
    serde_json::from_str(data).unwrap()
}

fn encoded(data: serde_json::Value) -> Value {
    Value::Record(BTreeMap::from([(
        "_data".into(),
        Value::Str(data.to_string()),
    )]))
}

fn error_code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn creation_rejects_bad_dimensions_and_options() {
    let mut env = Env::new();
    for size in [-1, 0] {
        error_code(
            call(
                "rl_agent",
                vec![Value::Str("test".into()), Value::Int(size), Value::Int(1)],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        error_code(
            call(
                "rl_agent",
                vec![Value::Str("test".into()), Value::Int(1), Value::Int(size)],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    for (states, actions) in [(1, 10001), (1001, 1000)] {
        error_code(
            call(
                "rl_agent",
                vec![
                    Value::Str("test".into()),
                    Value::Int(states),
                    Value::Int(actions),
                ],
                &mut env,
            ),
            "E_BUDGET_EXCEEDED",
        );
    }
    for (key, value) in [
        ("epsilon", Value::Float(f64::NAN)),
        ("epsilon", Value::Int(2)),
        ("learning_rate", Value::Float(-0.1)),
        ("discount", Value::Str("0.9".into())),
        ("unknown", Value::Int(1)),
    ] {
        error_code(
            call(
                "rl_agent",
                vec![
                    Value::Str("test".into()),
                    Value::Int(2),
                    Value::Int(3),
                    Value::Record(BTreeMap::from([(key.into(), value)])),
                ],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    error_code(
        call(
            "rl_agent",
            vec![Value::Uri("test".into()), Value::Int(2), Value::Int(3)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn updates_round_trip_nonempty_tables_and_use_terminal_and_bootstrap_targets() {
    let mut env = Env::new();
    let original = agent(&mut env);
    assert_eq!(
        call("rl_action", vec![original.clone(), Value::Int(0)], &mut env).unwrap(),
        Value::Int(2)
    );
    let updated = call(
        "rl_update",
        vec![
            original.clone(),
            Value::Int(1),
            Value::Int(0),
            Value::Int(5),
            Value::Int(0),
            Value::Bool(true),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(data(&updated)["q_table"], serde_json::json!([[1, 0, 5.0]]));
    assert_eq!(data(&original)["q_table"], serde_json::json!([]));
    let updated = call_with_input(
        "rl_update",
        vec![
            Value::Int(0),
            Value::Int(1),
            Value::Int(2),
            Value::Int(1),
            Value::Bool(false),
        ],
        Some(updated),
        &mut env,
    )
    .unwrap();
    assert_eq!(
        data(&updated)["q_table"],
        serde_json::json!([[0, 1, 7.0], [1, 0, 5.0]])
    );
    assert_eq!(fields(&updated)["steps"], Value::Int(2));
    assert_eq!(
        call_with_input("rl_action", vec![Value::Int(0)], Some(updated), &mut env).unwrap(),
        Value::Int(1)
    );
}

#[test]
fn indices_and_rewards_are_checked_in_direct_and_pipeline_forms() {
    let mut env = Env::new();
    let agent = agent(&mut env);
    for state in [Value::Int(-1), Value::Int(2), Value::Float(1.0)] {
        error_code(
            call("rl_action", vec![agent.clone(), state.clone()], &mut env),
            "E_BAD_ARG",
        );
        error_code(
            call_with_input("rl_action", vec![state], Some(agent.clone()), &mut env),
            "E_BAD_ARG",
        );
    }
    for (position, value) in [
        (0, Value::Int(-1)),
        (1, Value::Int(3)),
        (2, Value::Float(f64::INFINITY)),
        (3, Value::Int(2)),
        (4, Value::Int(1)),
    ] {
        let mut args = vec![
            Value::Int(0),
            Value::Int(0),
            Value::Int(1),
            Value::Int(1),
            Value::Bool(true),
        ];
        args[position] = value;
        error_code(
            call_with_input("rl_update", args.clone(), Some(agent.clone()), &mut env),
            "E_BAD_ARG",
        );
        args.insert(0, agent.clone());
        error_code(call("rl_update", args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn encoded_tables_and_metadata_are_validated_and_legacy_empty_maps_still_work() {
    let mut env = Env::new();
    let original = agent(&mut env);
    let mut legacy = data(&original);
    legacy["q_table"] = serde_json::json!({});
    assert_eq!(
        call("rl_action", vec![encoded(legacy), Value::Int(0)], &mut env).unwrap(),
        Value::Int(2)
    );
    for table in [
        serde_json::json!([[0, 0, 1], [0, 0, 2]]),
        serde_json::json!([[2, 0, 1]]),
        serde_json::json!([[0, 3, 1]]),
        serde_json::json!([[0.0, 0, 1]]),
        serde_json::json!([[0, 0]]),
        serde_json::json!({"bad": 1}),
    ] {
        let mut invalid = data(&original);
        invalid["q_table"] = table;
        error_code(
            call("rl_action", vec![encoded(invalid), Value::Int(0)], &mut env),
            "E_BAD_ARG",
        );
    }
    for (key, value) in [
        ("state_size", serde_json::json!(0)),
        ("epsilon", serde_json::json!(2)),
        ("epsilon_decay", serde_json::json!(-1)),
        ("epsilon_min", serde_json::json!(2)),
    ] {
        let mut invalid = data(&original);
        invalid[key] = value;
        error_code(
            call("rl_action", vec![encoded(invalid), Value::Int(0)], &mut env),
            "E_BAD_ARG",
        );
    }
    error_code(
        call(
            "rl_action",
            vec![
                Value::Record(BTreeMap::from([(
                    "_data".into(),
                    Value::Str("invalid".into()),
                )])),
                Value::Int(0),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn update_refuses_counter_and_numeric_overflow() {
    let mut env = Env::new();
    let original = agent(&mut env);
    let mut counter = data(&original);
    counter["steps"] = serde_json::json!(i64::MAX);
    error_code(
        call(
            "rl_update",
            vec![
                encoded(counter),
                Value::Int(0),
                Value::Int(0),
                Value::Int(1),
                Value::Int(1),
                Value::Bool(true),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    let mut extreme = data(&original);
    extreme["q_table"] = serde_json::json!([[0, 0, -f64::MAX]]);
    error_code(
        call(
            "rl_update",
            vec![
                encoded(extreme),
                Value::Int(0),
                Value::Int(0),
                Value::Float(f64::MAX),
                Value::Int(1),
                Value::Bool(true),
            ],
            &mut env,
        ),
        "E_BAD_STATE",
    );
}
