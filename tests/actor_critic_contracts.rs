use aethershell::{
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
        "rl_ac_agent",
        vec![Value::Str("test".into()), Value::Int(1), Value::Int(2)],
        env,
    )
    .unwrap()
}
fn args(done: bool) -> Vec<Value> {
    vec![
        Value::Array(vec![Value::Int(1)]),
        Value::Int(0),
        Value::Int(2),
        Value::Array(vec![Value::Int(2)]),
        Value::Bool(done),
    ]
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn construction_checks_combined_allocation_and_rejects_invalid_contracts() {
    let mut env = Env::default();
    for (states, actions, expected) in [
        (-1, 2, "E_BAD_ARG"),
        (1, 0, "E_BAD_ARG"),
        (1, 10001, "E_BUDGET_EXCEEDED"),
        (1000, 1000, "E_BUDGET_EXCEEDED"),
    ] {
        code(
            call(
                "rl_ac_agent",
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
            "rl_ac_agent",
            vec![Value::Str("test".into()), Value::Float(1.0), Value::Int(2)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "rl_ac_agent",
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
fn updates_use_pre_update_values_for_both_actor_and_critic_and_preserve_input() {
    let mut env = Env::default();
    let mut wire = data(&agent(&mut env));
    wire["actor_weights"] = serde_json::json!([[0], [0]]);
    wire["critic_weights"] = serde_json::json!([1]);
    wire["actor_lr"] = serde_json::json!(1);
    wire["critic_lr"] = serde_json::json!(1);
    wire["discount"] = serde_json::json!(1);
    let original = encoded(wire.clone());
    // TD error: 2 + 1*2 - 1 = 3. Actor probabilities start at one half.
    let result = call_with_input(
        "rl_ac_update",
        args(false),
        Some(original.clone()),
        &mut env,
    )
    .unwrap();
    assert_eq!(data(&result)["critic_weights"], serde_json::json!([4.0]));
    assert_eq!(
        data(&result)["actor_weights"],
        serde_json::json!([[1.5], [-1.5]])
    );
    assert_eq!(data(&result)["steps"], serde_json::json!(1));
    assert_eq!(data(&original), wire);
    let mut direct = vec![original];
    direct.extend(args(true));
    let terminal = call("rl_ac_update", direct, &mut env).unwrap();
    assert_eq!(data(&terminal)["critic_weights"], serde_json::json!([2.0]));
    assert_eq!(
        data(&terminal)["actor_weights"],
        serde_json::json!([[0.5], [-0.5]])
    );
}
#[test]
fn direct_and_pipeline_updates_refuse_bad_state_action_reward_and_done() {
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
        (3, Value::Array(vec![Value::Str("x".into())])),
        (4, Value::Int(1)),
    ] {
        let mut values = args(true);
        values[index] = value;
        code(
            call_with_input(
                "rl_ac_update",
                values.clone(),
                Some(original.clone()),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        values.insert(0, original.clone());
        code(call("rl_ac_update", values, &mut env), "E_BAD_ARG");
    }
}
#[test]
fn malformed_metadata_and_shapes_are_refused_before_computation() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    for (field, value) in [
        ("state_dim", serde_json::json!(0)),
        ("action_size", serde_json::json!(0)),
        ("actor_lr", serde_json::json!(-1)),
        ("critic_lr", serde_json::json!(2)),
        ("discount", serde_json::json!(2)),
        ("actor_weights", serde_json::json!([[0]])),
        ("actor_weights", serde_json::json!([[0, 1], [0]])),
        ("critic_weights", serde_json::json!([])),
    ] {
        let mut wire = original.clone();
        wire[field] = value;
        code(
            call_with_input("rl_ac_update", args(false), Some(encoded(wire)), &mut env),
            "E_BAD_ARG",
        );
    }
    let invalid = Value::Record(BTreeMap::from([("_data".into(), Value::Str("{".into()))]));
    code(
        call_with_input("rl_ac_update", args(false), Some(invalid), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn counter_value_and_policy_numeric_overflow_are_refused() {
    let mut env = Env::default();
    let original = data(&agent(&mut env));
    let mut wire = original.clone();
    wire["steps"] = serde_json::json!(i64::MAX);
    code(
        call_with_input("rl_ac_update", args(true), Some(encoded(wire)), &mut env),
        "E_BAD_ARG",
    );
    let mut wire = original.clone();
    wire["critic_weights"] = serde_json::json!([f64::MAX]);
    code(
        call_with_input("rl_ac_update", args(false), Some(encoded(wire)), &mut env),
        "E_BAD_STATE",
    );
    let mut wire = original;
    wire["actor_weights"] = serde_json::json!([[f64::MAX], [f64::MAX]]);
    let mut values = args(true);
    values[0] = Value::Array(vec![Value::Int(2)]);
    code(
        call_with_input("rl_ac_update", values, Some(encoded(wire)), &mut env),
        "E_BAD_STATE",
    );
}
