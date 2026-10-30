use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;
fn data(v: &Value) -> serde_json::Value {
    let Value::Record(r) = v else {
        panic!("expected Record")
    };
    let Value::Str(s) = &r["_data"] else {
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
        "rl_dqn_agent",
        vec![
            Value::Str("test".into()),
            Value::Int(1),
            Value::Int(2),
            Value::Array(vec![]),
        ],
        env,
    )
    .unwrap()
}
fn args() -> Vec<Value> {
    vec![
        Value::Array(vec![Value::Int(1)]),
        Value::Int(0),
        Value::Int(2),
        Value::Array(vec![Value::Int(2)]),
        Value::Bool(false),
    ]
}
fn step(v: Value, env: &mut Env) -> anyhow::Result<Value> {
    call_with_input("rl_dqn_step", args(), Some(v), env)
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn construction_checks_both_networks_before_allocation_and_hidden_sizes_are_strict() {
    let mut env = Env::default();
    for (states, actions, expected) in [
        (-1, 2, "E_BAD_ARG"),
        (1, 0, "E_BAD_ARG"),
        (1, 10001, "E_BUDGET_EXCEEDED"),
        (1000, 500, "E_BUDGET_EXCEEDED"),
    ] {
        code(
            call(
                "rl_dqn_agent",
                vec![
                    Value::Str("test".into()),
                    Value::Int(states),
                    Value::Int(actions),
                    Value::Array(vec![]),
                ],
                &mut env,
            ),
            expected,
        );
    }
    for hidden in [
        Value::Int(4),
        Value::Array(vec![Value::Int(0)]),
        Value::Array(vec![Value::Int(-1)]),
        Value::Array(vec![Value::Float(1.0)]),
    ] {
        code(
            call(
                "rl_dqn_agent",
                vec![
                    Value::Str("test".into()),
                    Value::Int(1),
                    Value::Int(2),
                    hidden,
                ],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    code(
        call(
            "rl_dqn_agent",
            vec![
                Value::Str("test".into()),
                Value::Int(1),
                Value::Int(2),
                Value::Array(vec![Value::Int(1); 127]),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(
        data(&agent(&mut env))["network"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn storage_does_not_train_until_threshold_then_updates_policy_weights() {
    let mut env = Env::default();
    let original = agent(&mut env);
    let mut current = original.clone();
    for _ in 0..31 {
        current = step(current, &mut env).unwrap();
    }
    let before = data(&current);
    assert_eq!(before["steps"], serde_json::json!(31));
    assert_eq!(
        before["replay_buffer"]["buffer"].as_array().unwrap().len(),
        31
    );
    assert_eq!(before["network"], data(&original)["network"]);
    assert_eq!(data(&original)["steps"], serde_json::json!(0));
    assert!((before["epsilon"].as_f64().unwrap() - 0.995_f64.powi(31)).abs() < 1e-12);
    let trained = step(current.clone(), &mut env).unwrap();
    let after = data(&trained);
    assert_eq!(after["steps"], serde_json::json!(32));
    assert_ne!(after["network"], before["network"]);
    assert_eq!(after["target_network"], before["target_network"]);
    assert_eq!(data(&current), before);
    let mut direct = vec![current];
    direct.extend(args());
    let direct = data(&call("rl_dqn_step", direct, &mut env).unwrap());
    // Every stored transition is identical, so sampling cannot change the update.
    assert_eq!(direct["network"], after["network"]);
}
#[test]
fn circular_storage_target_sync_and_epsilon_threshold_match_source_behavior() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["replay_buffer"]["capacity"] = serde_json::json!(2);
    wire["steps"] = serde_json::json!(99);
    wire["epsilon"] = serde_json::json!(0.01);
    wire["network"]["layers"][0]["weights"] = serde_json::json!([[2], [3]]);
    wire["target_network"]["layers"][0]["weights"] = serde_json::json!([[4], [5]]);
    let first = step(encoded(wire), &mut env).unwrap();
    assert_eq!(data(&first)["network"], data(&first)["target_network"]);
    assert_eq!(data(&first)["epsilon"], serde_json::json!(0.01));
    assert_eq!(data(&first)["steps"], serde_json::json!(100));
    let second = step(first, &mut env).unwrap();
    assert_eq!(
        data(&second)["replay_buffer"]["position"],
        serde_json::json!(0)
    );
    let mut values = args();
    values[2] = Value::Int(3);
    let third = call_with_input("rl_dqn_step", values, Some(second), &mut env).unwrap();
    assert_eq!(
        data(&third)["replay_buffer"]["position"],
        serde_json::json!(1)
    );
    assert_eq!(
        data(&third)["replay_buffer"]["buffer"][0]["reward"],
        serde_json::json!(3.0)
    );
    assert_eq!(
        data(&third)["replay_buffer"]["buffer"][1]["reward"],
        serde_json::json!(2.0)
    );
}
#[test]
fn direct_and_pipeline_arguments_are_validated_including_terminal_next_state() {
    let mut env = Env::default();
    let original = agent(&mut env);
    for (index, value) in [
        (0, Value::Array(vec![])),
        (0, Value::Array(vec![Value::Float(f64::NAN)])),
        (1, Value::Int(-1)),
        (1, Value::Int(2)),
        (1, Value::Float(0.0)),
        (2, Value::Float(f64::INFINITY)),
        (3, Value::Array(vec![Value::Int(1), Value::Int(2)])),
        (4, Value::Int(1)),
    ] {
        let mut values = args();
        values[4] = Value::Bool(true);
        values[index] = value;
        code(
            call_with_input(
                "rl_dqn_step",
                values.clone(),
                Some(original.clone()),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        values.insert(0, original.clone());
        code(call("rl_dqn_step", values, &mut env), "E_BAD_ARG");
    }
}
#[test]
fn decoded_networks_rates_replay_entries_and_counters_are_validated() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    for (field, value) in [
        ("state_dim", 0),
        ("epsilon", 2),
        ("epsilon_decay", -1),
        ("epsilon_min", 2),
        ("learning_rate", -1),
        ("discount", 2),
        ("batch_size", 0),
        ("target_update_freq", 0),
    ] {
        let mut wire = original.clone();
        wire[field] = serde_json::json!(value);
        code(step(encoded(wire), &mut env), "E_BAD_ARG");
    }
    let mut wire = original.clone();
    wire["steps"] = serde_json::json!(i64::MAX);
    code(step(encoded(wire), &mut env), "E_BAD_ARG");
    for field in ["network", "target_network"] {
        let mut wire = original.clone();
        wire[field]["layers"][0]["weights"] = serde_json::json!([[0]]);
        code(step(encoded(wire), &mut env), "E_BAD_ARG");
        let mut wire = original.clone();
        wire[field]["layers"][0]["input_size"] = serde_json::json!(2);
        wire[field]["layers"][0]["weights"] = serde_json::json!([[0, 0], [0, 0]]);
        code(step(encoded(wire), &mut env), "E_BAD_ARG");
    }
    for (capacity, position, expected) in [
        (0, 0, "E_BAD_ARG"),
        (2, 1, "E_BAD_ARG"),
        (10001, 0, "E_BUDGET_EXCEEDED"),
    ] {
        let mut wire = original.clone();
        wire["replay_buffer"]["capacity"] = serde_json::json!(capacity);
        wire["replay_buffer"]["position"] = serde_json::json!(position);
        code(step(encoded(wire), &mut env), expected);
    }
    let stored = data(&step(agent(&mut env), &mut env).unwrap());
    for (field, value) in [
        ("state", serde_json::json!([])),
        ("next_state", serde_json::json!([0, 1])),
        ("action", serde_json::json!(2)),
        ("reward", serde_json::json!(null)),
    ] {
        let mut wire = stored.clone();
        wire["replay_buffer"]["buffer"][0][field] = value;
        code(step(encoded(wire), &mut env), "E_BAD_ARG");
    }
}

#[test]
fn replay_state_budget_accepts_the_boundary_and_refuses_growth() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["state_dim"] = serde_json::json!(1000);
    wire["action_size"] = serde_json::json!(1);
    wire["batch_size"] = serde_json::json!(1000);
    wire["network"]["layers"][0]["input_size"] = serde_json::json!(1000);
    wire["network"]["layers"][0]["output_size"] = serde_json::json!(1);
    wire["network"]["layers"][0]["weights"] = serde_json::json!([vec![0; 1000]]);
    wire["network"]["layers"][0]["biases"] = serde_json::json!([0]);
    wire["target_network"] = wire["network"].clone();
    let experience = serde_json::json!({
        "state": vec![0;1000], "next_state": vec![0;1000],
        "action": 0, "reward": 0, "done": false
    });
    wire["replay_buffer"]["buffer"] = serde_json::json!(vec![experience; 499]);
    wire["replay_buffer"]["position"] = serde_json::json!(499);
    let values = vec![
        Value::Array(vec![Value::Int(0); 1000]),
        Value::Int(0),
        Value::Int(0),
        Value::Array(vec![Value::Int(0); 1000]),
        Value::Bool(false),
    ];
    let at_limit =
        call_with_input("rl_dqn_step", values.clone(), Some(encoded(wire)), &mut env).unwrap();
    assert_eq!(
        data(&at_limit)["replay_buffer"]["buffer"]
            .as_array()
            .unwrap()
            .len(),
        500
    );
    code(
        call_with_input("rl_dqn_step", values, Some(at_limit), &mut env),
        "E_BUDGET_EXCEEDED",
    );
}

#[test]
fn training_reduces_terminal_td_error_and_syncs_trained_weights() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["batch_size"] = serde_json::json!(1);
    wire["steps"] = serde_json::json!(99);
    wire["learning_rate"] = serde_json::json!(0.1);
    wire["network"]["layers"][0]["weights"] = serde_json::json!([[0], [3]]);
    wire["network"]["layers"][0]["biases"] = serde_json::json!([0, 0]);
    wire["target_network"] = wire["network"].clone();
    let mut values = args();
    values[4] = Value::Bool(true);
    let updated =
        data(&call_with_input("rl_dqn_step", values, Some(encoded(wire)), &mut env).unwrap());
    assert_eq!(updated["steps"], serde_json::json!(100));
    assert_eq!(updated["network"], updated["target_network"]);
    let layer = &updated["network"]["layers"][0];
    assert!((layer["weights"][0][0].as_f64().unwrap() - 0.2).abs() < 1e-12);
    assert!((layer["biases"][0].as_f64().unwrap() - 0.2).abs() < 1e-12);
    assert_eq!(layer["weights"][1][0], serde_json::json!(3.0));
    assert_eq!(layer["biases"][1], serde_json::json!(0.0));
}

#[test]
fn overflowing_training_and_excessive_work_are_coded_and_preserve_input() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["batch_size"] = serde_json::json!(1);
    wire["network"]["layers"][0]["weights"] = serde_json::json!([[1], [0]]);
    wire["network"]["layers"][0]["biases"] = serde_json::json!([0, 0]);
    let original = encoded(wire);
    let values = vec![
        Value::Array(vec![Value::Float(f64::MAX)]),
        Value::Int(0),
        Value::Float(-f64::MAX),
        Value::Array(vec![Value::Int(0)]),
        Value::Bool(true),
    ];
    code(
        call_with_input("rl_dqn_step", values, Some(original.clone()), &mut env),
        "E_BAD_STATE",
    );
    assert_eq!(data(&original)["steps"], serde_json::json!(0));
    let mut wire = data(
        &call(
            "rl_dqn_agent",
            vec![
                Value::Str("budget".into()),
                Value::Int(1000),
                Value::Int(2),
                Value::Array(vec![Value::Int(128)]),
            ],
            &mut env,
        )
        .unwrap(),
    );
    let experience = serde_json::json!({"state":vec![0;1000],"next_state":vec![0;1000],"action":0,"reward":0,"done":true});
    wire["replay_buffer"]["buffer"] = serde_json::json!(vec![experience; 31]);
    wire["replay_buffer"]["position"] = serde_json::json!(31);
    let original = encoded(wire);
    let values = vec![
        Value::Array(vec![Value::Int(0); 1000]),
        Value::Int(0),
        Value::Int(0),
        Value::Array(vec![Value::Int(0); 1000]),
        Value::Bool(true),
    ];
    code(
        call_with_input("rl_dqn_step", values, Some(original.clone()), &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(
        data(&original)["replay_buffer"]["buffer"]
            .as_array()
            .unwrap()
            .len(),
        31
    );
}
