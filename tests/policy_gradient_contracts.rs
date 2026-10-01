use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;

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
fn agent(env: &mut Env) -> Value {
    call(
        "rl_pg_agent",
        vec![Value::Str("test".into()), Value::Int(1), Value::Int(2)],
        env,
    )
    .unwrap()
}
fn step() -> Vec<Value> {
    vec![
        Value::Array(vec![Value::Int(1)]),
        Value::Int(0),
        Value::Int(2),
    ]
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}
fn end(value: Value, env: &mut Env) -> anyhow::Result<Value> {
    call_with_input("rl_pg_episode_end", vec![], Some(value), env)
}

#[test]
fn creation_bounds_allocation_and_refuses_invalid_dimensions_and_config() {
    let mut env = Env::default();
    for (states, actions, expected) in [
        (-1, 2, "E_BAD_ARG"),
        (1, 0, "E_BAD_ARG"),
        (1, 10001, "E_BUDGET_EXCEEDED"),
        (1001, 1000, "E_BUDGET_EXCEEDED"),
    ] {
        code(
            call(
                "rl_pg_agent",
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
            "rl_pg_agent",
            vec![Value::Str("test".into()), Value::Float(1.0), Value::Int(2)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "rl_pg_agent",
            vec![
                Value::Str("test".into()),
                Value::Int(1),
                Value::Int(2),
                Value::Record(BTreeMap::new()),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn recording_preserves_weights_and_original_and_checks_both_call_forms() {
    let mut env = Env::default();
    let original = agent(&mut env);
    let updated = call_with_input("rl_pg_step", step(), Some(original.clone()), &mut env).unwrap();
    assert_eq!(data(&updated)["weights"], data(&original)["weights"]);
    assert_eq!(data(&original)["episode_states"], serde_json::json!([]));
    assert_eq!(data(&updated)["episode_states"], serde_json::json!([[1.0]]));
    assert_eq!(data(&updated)["episode_actions"], serde_json::json!([0]));
    assert_eq!(data(&updated)["episode_rewards"], serde_json::json!([2.0]));
    assert_eq!(fields(&updated)["episodes"], Value::Int(0));
    for (index, value) in [
        (0, Value::Array(vec![])),
        (0, Value::Array(vec![Value::Float(f64::NAN)])),
        (0, Value::Array(vec![Value::Str("x".into())])),
        (1, Value::Int(-1)),
        (1, Value::Int(2)),
        (2, Value::Float(f64::INFINITY)),
    ] {
        let mut args = step();
        args[index] = value;
        code(
            call_with_input("rl_pg_step", args.clone(), Some(original.clone()), &mut env),
            "E_BAD_ARG",
        );
        args.insert(0, original.clone());
        code(call("rl_pg_step", args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn episode_end_preserves_empty_and_single_step_policy_and_updates_in_order() {
    let mut env = Env::default();
    let original = agent(&mut env);
    let empty = call("rl_pg_episode_end", vec![original.clone()], &mut env).unwrap();
    assert_eq!(data(&empty), data(&original));
    let single = call_with_input("rl_pg_step", step(), Some(original.clone()), &mut env).unwrap();
    let single = end(single, &mut env).unwrap();
    assert_eq!(data(&single)["weights"], data(&original)["weights"]);
    assert_eq!(fields(&single)["episodes"], Value::Int(1));
    let mut wire = data(&original);
    wire["weights"] = serde_json::json!([[0], [0]]);
    wire["learning_rate"] = serde_json::json!(1);
    wire["discount"] = serde_json::json!(0);
    wire["episode_states"] = serde_json::json!([[1], [1]]);
    wire["episode_actions"] = serde_json::json!([0, 1]);
    wire["episode_rewards"] = serde_json::json!([2, 0]);
    let result = end(encoded(wire), &mut env).unwrap();
    let result_data = data(&result);
    let expected = 0.5 + 1.0 / (1.0 + (-1.0_f64).exp());
    assert!((result_data["weights"][0][0].as_f64().unwrap() - expected).abs() < 1e-12);
    assert!((result_data["weights"][1][0].as_f64().unwrap() + expected).abs() < 1e-12);
    for field in ["episode_states", "episode_actions", "episode_rewards"] {
        assert_eq!(result_data[field], serde_json::json!([]));
    }
    assert_eq!(fields(&result)["episodes"], Value::Int(1));
}

#[test]
fn decoded_metadata_weights_and_episode_arrays_are_validated_before_indexing() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    for (field, value) in [
        ("state_dim", serde_json::json!(0)),
        ("action_size", serde_json::json!(0)),
        ("learning_rate", serde_json::json!(-1)),
        ("discount", serde_json::json!(2)),
        ("weights", serde_json::json!([[0]])),
        ("weights", serde_json::json!([[0, 1], [0]])),
        ("episode_states", serde_json::json!([[1]])),
        ("episode_actions", serde_json::json!([0])),
        ("episode_rewards", serde_json::json!([1])),
    ] {
        let mut wire = original.clone();
        wire[field] = value;
        code(end(encoded(wire), &mut env), "E_BAD_ARG");
    }
    for (states, actions) in [
        (serde_json::json!([[1, 2]]), serde_json::json!([0])),
        (serde_json::json!([[1]]), serde_json::json!([2])),
    ] {
        let mut wire = original.clone();
        wire["episode_states"] = states;
        wire["episode_actions"] = actions;
        wire["episode_rewards"] = serde_json::json!([1]);
        code(end(encoded(wire), &mut env), "E_BAD_ARG");
    }
    let invalid = Value::Record(BTreeMap::from([("_data".into(), Value::Str("{".into()))]));
    code(end(invalid, &mut env), "E_BAD_ARG");
}

#[test]
fn episode_storage_and_combined_update_work_are_bounded() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["episode_states"] = serde_json::json!(vec![vec![0]; 10000]);
    wire["episode_actions"] = serde_json::json!(vec![0; 10000]);
    wire["episode_rewards"] = serde_json::json!(vec![0; 10000]);
    code(
        call_with_input("rl_pg_step", step(), Some(encoded(wire)), &mut env),
        "E_BUDGET_EXCEEDED",
    );
    let mut wire = data(&agent(&mut env));
    wire["action_size"] = serde_json::json!(10000);
    wire["weights"] = serde_json::json!(vec![vec![0]; 10000]);
    wire["episode_states"] = serde_json::json!(vec![vec![0]; 1001]);
    wire["episode_actions"] = serde_json::json!(vec![0; 1001]);
    wire["episode_rewards"] = serde_json::json!(vec![0; 1001]);
    code(end(encoded(wire), &mut env), "E_BUDGET_EXCEEDED");
    let mut wire = data(&agent(&mut env));
    wire["state_dim"] = serde_json::json!(1000);
    wire["weights"] = serde_json::json!(vec![vec![0; 1000]; 2]);
    wire["episode_states"] = serde_json::json!(vec![vec![0; 1000]; 1001]);
    wire["episode_actions"] = serde_json::json!(vec![0; 1001]);
    wire["episode_rewards"] = serde_json::json!(vec![0; 1001]);
    code(end(encoded(wire), &mut env), "E_BUDGET_EXCEEDED");
}

#[test]
fn episode_counter_and_numeric_overflow_are_refused() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    let mut wire = original.clone();
    wire["episodes"] = serde_json::json!(i64::MAX);
    end(encoded(wire.clone()), &mut env).unwrap();
    wire["episode_states"] = serde_json::json!([[1]]);
    wire["episode_actions"] = serde_json::json!([0]);
    wire["episode_rewards"] = serde_json::json!([1]);
    code(end(encoded(wire), &mut env), "E_BAD_ARG");
    let mut wire = original.clone();
    wire["discount"] = serde_json::json!(1);
    wire["episode_states"] = serde_json::json!([[1], [1]]);
    wire["episode_actions"] = serde_json::json!([0, 1]);
    wire["episode_rewards"] = serde_json::json!([f64::MAX, f64::MAX]);
    code(end(encoded(wire), &mut env), "E_BAD_STATE");
    let mut wire = original;
    wire["weights"] = serde_json::json!([[f64::MAX], [f64::MAX]]);
    wire["episode_states"] = serde_json::json!([[f64::MAX]]);
    wire["episode_actions"] = serde_json::json!([0]);
    wire["episode_rewards"] = serde_json::json!([1]);
    code(end(encoded(wire), &mut env), "E_BAD_STATE");
}
