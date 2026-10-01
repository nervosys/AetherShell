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
fn grid(env: &mut Env, width: i64, height: i64) -> Value {
    call(
        "rl_gridworld",
        vec![Value::Int(width), Value::Int(height)],
        env,
    )
    .unwrap()
}
fn set(value: &mut Value, key: &str, field: Value) {
    let Value::Record(fields) = value else {
        panic!("expected Record")
    };
    fields.insert(key.into(), field);
}
fn step(value: Value, action: i64, env: &mut Env) -> anyhow::Result<Value> {
    call_with_input("rl_env_step", vec![Value::Int(action)], Some(value), env)
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn constructors_honor_defaults_and_explicit_optional_arguments() {
    let mut env = Env::default();
    let default = call("rl_gridworld", vec![], &mut env).unwrap();
    assert_eq!(fields(&default)["state_size"], Value::Int(25));
    assert_eq!(fields(&default)["goal_x"], Value::Int(4));
    assert_eq!(fields(&default)["obstacles"], Value::Array(vec![]));
    let square = call("rl_gridworld", vec![Value::Int(3)], &mut env).unwrap();
    assert_eq!(fields(&square)["height"], Value::Int(3));
    assert_eq!(fields(&grid(&mut env, 3, 4))["state_size"], Value::Int(12));
    for capacity in [1, 10000] {
        let replay = call("rl_replay_buffer", vec![Value::Int(capacity)], &mut env).unwrap();
        assert_eq!(fields(&replay)["capacity"], Value::Int(capacity));
        let Value::Str(data) = &fields(&replay)["_data"] else {
            panic!("expected encoding")
        };
        let wire: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(wire["buffer"], serde_json::json!([]));
        assert_eq!(wire["position"], serde_json::json!(0));
    }
    let replay = call("rl_replay_buffer", vec![], &mut env).unwrap();
    assert_eq!(fields(&replay)["capacity"], Value::Int(10000));
}
#[test]
fn dimensions_and_replay_allocations_refuse_invalid_or_over_budget_arguments() {
    let mut env = Env::default();
    for n in [-1, 0] {
        code(
            call("rl_replay_buffer", vec![Value::Int(n)], &mut env),
            "E_BAD_ARG",
        );
        code(
            call("rl_gridworld", vec![Value::Int(n), Value::Int(2)], &mut env),
            "E_BAD_ARG",
        );
        code(
            call("rl_gridworld", vec![Value::Int(2), Value::Int(n)], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call("rl_replay_buffer", vec![Value::Int(10001)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call(
            "rl_gridworld",
            vec![Value::Int(1001), Value::Int(1000)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call(
            "rl_gridworld",
            vec![Value::Int(i64::MAX), Value::Int(i64::MAX)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    for name in ["rl_replay_buffer", "rl_gridworld"] {
        code(call(name, vec![Value::Float(1.0)], &mut env), "E_BAD_ARG");
    }
    let mut limit = grid(&mut env, 1_000_000, 1);
    set(&mut limit, "agent_x", Value::Int(999999));
    let result = step(limit, 3, &mut env).unwrap();
    assert_eq!(fields(&result)["state"], Value::Int(999999));
    assert_eq!(fields(&result)["reward"], Value::Float(1.0));
}
#[test]
fn steps_preserve_goals_obstacles_and_input_and_do_not_absorb_terminal_states() {
    let mut env = Env::default();
    let mut original = grid(&mut env, 3, 2);
    set(&mut original, "goal_x", Value::Int(1));
    set(&mut original, "goal_y", Value::Int(0));
    let obstacles = Value::Array(vec![
        Value::Array(vec![Value::Int(1), Value::Int(0)]),
        Value::Array(vec![Value::Int(1), Value::Int(1)]),
    ]);
    set(&mut original, "obstacles", obstacles.clone());
    let first = call(
        "rl_env_step",
        vec![original.clone(), Value::Int(3)],
        &mut env,
    )
    .unwrap();
    assert_eq!(fields(&first)["state"], Value::Int(1));
    assert_eq!(fields(&first)["reward"], Value::Float(1.0)); // Goal takes precedence over obstacle.
    assert_eq!(fields(&first)["done"], Value::Bool(true));
    let first_env = fields(&first)["env"].clone();
    assert_eq!(fields(&first_env)["goal_x"], Value::Int(1));
    assert_eq!(fields(&first_env)["obstacles"], obstacles);
    let second = step(first_env, 1, &mut env).unwrap();
    assert_eq!(fields(&second)["state"], Value::Int(4));
    assert_eq!(fields(&second)["reward"], Value::Float(-0.5));
    assert_eq!(fields(&second)["done"], Value::Bool(false));
    let third = step(fields(&second)["env"].clone(), 2, &mut env).unwrap();
    assert_eq!(fields(&third)["reward"], Value::Float(-0.01));
    assert_eq!(fields(&original)["agent_x"], Value::Int(0));
    let clamped = step(original, 0, &mut env).unwrap();
    assert_eq!(fields(&clamped)["state"], Value::Int(0));
}
#[test]
fn minimal_legacy_records_get_safe_defaults_and_single_cell_grids_are_valid() {
    let mut env = Env::default();
    let minimal = Value::Record(BTreeMap::from([
        ("width".into(), Value::Int(2)),
        ("height".into(), Value::Int(3)),
    ]));
    let result = step(minimal, 1, &mut env).unwrap();
    assert_eq!(fields(&result)["state"], Value::Int(2));
    let updated = &fields(&result)["env"];
    assert_eq!(fields(updated)["goal_x"], Value::Int(1));
    assert_eq!(fields(updated)["goal_y"], Value::Int(2));
    assert_eq!(fields(updated)["obstacles"], Value::Array(vec![]));
    let single = grid(&mut env, 1, 1);
    for action in 0..4 {
        let result = step(single.clone(), action, &mut env).unwrap();
        assert_eq!(fields(&result)["state"], Value::Int(0));
        assert_eq!(fields(&result)["done"], Value::Bool(true));
    }
}
#[test]
fn invalid_decoded_positions_metadata_obstacles_and_actions_are_refused() {
    let mut env = Env::default();
    let original = grid(&mut env, 2, 3);
    for (key, value) in [
        ("width", Value::Int(0)),
        ("height", Value::Int(-1)),
        ("agent_x", Value::Int(2)),
        ("agent_y", Value::Int(3)),
        ("goal_x", Value::Int(-1)),
        ("goal_y", Value::Float(1.0)),
        ("state_size", Value::Int(5)),
        ("action_size", Value::Int(3)),
        ("type", Value::Str("other".into())),
        ("obstacles", Value::Int(0)),
        ("obstacles", Value::Array(vec![Value::Int(0)])),
        (
            "obstacles",
            Value::Array(vec![Value::Array(vec![Value::Int(0)])]),
        ),
        (
            "obstacles",
            Value::Array(vec![Value::Array(vec![Value::Int(2), Value::Int(0)])]),
        ),
        (
            "obstacles",
            Value::Array(vec![Value::Array(vec![Value::Int(0), Value::Float(1.0)])]),
        ),
    ] {
        let mut value_record = original.clone();
        set(&mut value_record, key, value);
        code(step(value_record, 0, &mut env), "E_BAD_ARG");
    }
    for action in [Value::Int(-1), Value::Int(4), Value::Float(0.0)] {
        code(
            call_with_input(
                "rl_env_step",
                vec![action.clone()],
                Some(original.clone()),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call("rl_env_step", vec![original.clone(), action], &mut env),
            "E_BAD_ARG",
        );
    }
    let mut over = original;
    set(
        &mut over,
        "obstacles",
        Value::Array(vec![
            Value::Array(vec![Value::Int(0), Value::Int(0)]);
            10001
        ]),
    );
    code(step(over, 0, &mut env), "E_BUDGET_EXCEEDED");
}
