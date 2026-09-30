use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;

fn numbers(values: &[i64]) -> Value {
    Value::Array(values.iter().copied().map(Value::Int).collect())
}

fn network(env: &mut Env, sizes: &[i64]) -> Value {
    call(
        "nn_create",
        vec![
            Value::Str("example".into()),
            numbers(sizes),
            Value::Str("relu".into()),
            Value::Str("linear".into()),
        ],
        env,
    )
    .unwrap()
}

fn encoded(data: serde_json::Value) -> Value {
    Value::Record(BTreeMap::from([(
        "_data".into(),
        Value::Str(data.to_string()),
    )]))
}

fn bad_argument(name: &str, args: Vec<Value>, env: &mut Env) {
    let error = call(name, args, env).unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
}

#[test]
fn creation_refuses_invalid_sizes_and_activations_before_allocation() {
    let mut env = Env::new();
    for sizes in [vec![-1, 2], vec![0, 2], vec![2], vec![]] {
        bad_argument(
            "nn_create",
            vec![Value::Str("test".into()), numbers(&sizes)],
            &mut env,
        );
    }
    for sizes in [[i64::MAX, i64::MAX], [1000, 1001]] {
        let error = call(
            "nn_create",
            vec![Value::Str("test".into()), numbers(&sizes)],
            &mut env,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("E_BAD_ARG")
                || error.to_string().contains("E_BUDGET_EXCEEDED"),
            "{error}"
        );
    }
    for activation in [
        Value::Int(1),
        Value::Str("unknown".into()),
        Value::Uri("relu".into()),
    ] {
        bad_argument(
            "nn_create",
            vec![
                Value::Str("test".into()),
                numbers(&[2, 1]),
                activation.clone(),
            ],
            &mut env,
        );
        bad_argument(
            "nn_create",
            vec![
                Value::Str("test".into()),
                numbers(&[2, 1]),
                Value::Str("relu".into()),
                activation,
            ],
            &mut env,
        );
    }
    let Value::Record(net) = network(&mut env, &[2, 3, 1]) else {
        unreachable!()
    };
    assert_eq!(net["param_count"], Value::Int(13));
}

#[test]
fn setting_parameters_is_exact_and_forward_never_pads_or_discards_input() {
    let mut env = Env::new();
    let net = network(&mut env, &[2, 1]);
    let set = call(
        "nn_set_params",
        vec![net.clone(), numbers(&[2, 3, 4])],
        &mut env,
    )
    .unwrap();
    let expected = Value::Array(vec![Value::Float(12.0)]);
    assert_eq!(
        call("nn_forward", vec![set.clone(), numbers(&[1, 2])], &mut env).unwrap(),
        expected
    );
    assert_eq!(
        call_with_input(
            "nn_forward",
            vec![numbers(&[1, 2])],
            Some(set.clone()),
            &mut env
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        call("nn_params", vec![set.clone()], &mut env).unwrap(),
        Value::Array(vec![
            Value::Float(2.0),
            Value::Float(3.0),
            Value::Float(4.0)
        ])
    );
    for params in [
        numbers(&[]),
        numbers(&[1, 2]),
        numbers(&[1, 2, 3, 4]),
        Value::Array(vec![Value::Int(1), Value::Float(f64::NAN), Value::Int(3)]),
    ] {
        bad_argument("nn_set_params", vec![set.clone(), params], &mut env);
    }
    for input in [
        numbers(&[]),
        numbers(&[1]),
        numbers(&[1, 2, 3]),
        Value::Array(vec![Value::Float(f64::INFINITY), Value::Int(1)]),
        Value::Array(vec![Value::Bool(true), Value::Int(1)]),
    ] {
        bad_argument("nn_forward", vec![set.clone(), input], &mut env);
    }
    assert_eq!(
        call("nn_forward", vec![set, numbers(&[1, 2])], &mut env).unwrap(),
        expected
    );
    for name in ["nn_params", "nn_layers", "nn_info"] {
        bad_argument(name, vec![net.clone(), Value::Bool(true)], &mut env);
    }
}

#[test]
fn malformed_serialized_networks_are_coded_refusals_in_all_consumers() {
    let layer = serde_json::json!({"input_size":2,"output_size":1,"weights":[[2.0,3.0]],"biases":[4.0],"activation":"Linear"});
    let mut missing_bias = layer.clone();
    missing_bias["biases"] = serde_json::json!([]);
    let mut ragged = layer.clone();
    ragged["weights"] = serde_json::json!([[1.0]]);
    let mut oversized = layer.clone();
    oversized["input_size"] = serde_json::json!(usize::MAX);
    let inputs = [
        Value::Record(BTreeMap::from([(
            "_data".into(),
            Value::Str("invalid JSON".into()),
        )])),
        encoded(serde_json::json!({"name":"invalid","layers":[]})),
        encoded(serde_json::json!({"name":"invalid","layers":[missing_bias]})),
        encoded(serde_json::json!({"name":"invalid","layers":[ragged]})),
        encoded(serde_json::json!({"name":"invalid","layers":[layer.clone(),layer]})),
        encoded(serde_json::json!({"name":"invalid","layers":[oversized]})),
    ];
    let mut env = Env::new();
    for net in inputs {
        for name in ["nn_info", "nn_layers", "nn_params", "nn_mutate"] {
            let error = call(name, vec![net.clone()], &mut env).unwrap_err();
            assert!(
                error.to_string().contains("E_BAD_ARG")
                    || error.to_string().contains("E_BUDGET_EXCEEDED"),
                "{name}: {error}"
            );
        }
        bad_argument("nn_forward", vec![net.clone(), numbers(&[1, 2])], &mut env);
        bad_argument("nn_set_params", vec![net, numbers(&[1, 2, 3])], &mut env);
    }
}

#[test]
fn crossover_requires_matching_architectures_even_when_parameter_counts_match() {
    let mut env = Env::new();
    let a = network(&mut env, &[3, 1]);
    let b = network(&mut env, &[1, 2]);
    bad_argument("nn_crossover", vec![a.clone(), b], &mut env);
    let child = call("nn_crossover", vec![a.clone(), a.clone()], &mut env).unwrap();
    assert_eq!(
        call("nn_params", vec![child], &mut env).unwrap(),
        call("nn_params", vec![a], &mut env).unwrap()
    );
}

#[test]
fn mutation_options_are_finite_and_zero_mutation_preserves_parameters() {
    let mut env = Env::new();
    let net = network(&mut env, &[2, 1]);
    for (rate, strength) in [
        (-0.1, 0.3),
        (1.1, 0.3),
        (f64::NAN, 0.3),
        (0.1, -0.3),
        (0.1, f64::INFINITY),
    ] {
        bad_argument(
            "nn_mutate",
            vec![net.clone(), Value::Float(rate), Value::Float(strength)],
            &mut env,
        );
    }
    let copy = call_with_input(
        "nn_mutate",
        vec![Value::Int(0)],
        Some(net.clone()),
        &mut env,
    )
    .unwrap();
    assert_eq!(
        call("nn_params", vec![copy], &mut env).unwrap(),
        call("nn_params", vec![net], &mut env).unwrap()
    );
}

#[test]
fn softmax_uses_logits_once_and_handles_large_offsets() {
    let mut env = Env::new();
    let net = call(
        "nn_create",
        vec![
            Value::Str("softmax".into()),
            numbers(&[1, 2]),
            Value::Str("relu".into()),
            Value::Str("softmax".into()),
        ],
        &mut env,
    )
    .unwrap();
    for (biases, expected) in [
        ([0.0, 3.0_f64.ln()], [0.25, 0.75]),
        (
            [1000.0, 1001.0],
            [1.0 / (1.0 + 1.0_f64.exp()), 1.0 / (1.0 + (-1.0_f64).exp())],
        ),
    ] {
        let params = Value::Array(vec![
            Value::Int(0),
            Value::Int(0),
            Value::Float(biases[0]),
            Value::Float(biases[1]),
        ]);
        let set = call("nn_set_params", vec![net.clone(), params], &mut env).unwrap();
        let Value::Array(output) = call("nn_forward", vec![set, numbers(&[0])], &mut env).unwrap()
        else {
            unreachable!()
        };
        for (got, want) in output.iter().zip(expected) {
            let Value::Float(got) = got else {
                panic!("expected Float")
            };
            assert!((got - want).abs() < 1e-12, "{got} != {want}");
        }
    }
}

#[test]
fn non_finite_forward_results_are_refused() {
    let mut env = Env::new();
    let net = network(&mut env, &[2, 1]);
    let net = call(
        "nn_set_params",
        vec![
            net,
            Value::Array(vec![
                Value::Float(f64::MAX),
                Value::Float(f64::MAX),
                Value::Int(0),
            ]),
        ],
        &mut env,
    )
    .unwrap();
    let error = call("nn_forward", vec![net, numbers(&[2, 2])], &mut env).unwrap_err();
    assert!(error.to_string().contains("E_BAD_STATE"), "{error}");
}

#[test]
fn strategy_descriptors_refuse_unused_parameters_and_invalid_domains() {
    let mut env = Env::new();
    for name in ["roulette", "rank"] {
        bad_argument(
            "selection_strategy",
            vec![Value::Str(name.into()), Value::Int(1)],
            &mut env,
        );
    }
    for name in ["single_point", "singlepoint", "two_point", "twopoint"] {
        bad_argument(
            "crossover_strategy",
            vec![Value::Str(name.into()), Value::Float(0.5)],
            &mut env,
        );
    }
    for (builtin, strategy, value) in [
        ("selection_strategy", "tournament", Value::Int(-1)),
        ("selection_strategy", "elite", Value::Int(0)),
        ("selection_strategy", "tournament", Value::Float(2.5)),
        ("selection_strategy", "truncation", Value::Float(f64::NAN)),
        ("selection_strategy", "truncation", Value::Float(1.1)),
        ("crossover_strategy", "uniform", Value::Float(-0.1)),
        ("crossover_strategy", "blend", Value::Float(-0.1)),
        ("crossover_strategy", "sbx", Value::Int(0)),
        ("crossover_strategy", "sbx", Value::Float(f64::INFINITY)),
    ] {
        bad_argument(builtin, vec![Value::Str(strategy.into()), value], &mut env);
    }
    bad_argument(
        "activation",
        vec![Value::Str("leaky_relu".into()), Value::Float(0.2)],
        &mut env,
    );
    bad_argument("activation", vec![Value::Str("unknown".into())], &mut env);
    bad_argument(
        "evolution_stats",
        vec![Value::Record(BTreeMap::new())],
        &mut env,
    );
}

fn consensus_network(env: &mut Env) -> Value {
    call(
        "consensus_net",
        vec![
            Value::Str("test".into()),
            Value::Int(2),
            Value::Int(2),
            Value::Int(2),
            Value::Int(2),
        ],
        env,
    )
    .unwrap()
}

#[test]
fn consensus_creation_checks_sizes_and_aggregate_budget() {
    let mut env = Env::new();
    for position in 1..5 {
        for invalid in [-1, 0, i64::MAX] {
            let mut args = vec![
                Value::Str("test".into()),
                Value::Int(2),
                Value::Int(2),
                Value::Int(2),
                Value::Int(2),
            ];
            args[position] = Value::Int(invalid);
            let error = call("consensus_net", args, &mut env).unwrap_err();
            assert!(
                error.to_string().contains("E_BAD_ARG")
                    || error.to_string().contains("E_BUDGET_EXCEEDED"),
                "{error}"
            );
        }
    }
    // Each component fits individually, but their combined parameters do not.
    let error = call(
        "consensus_net",
        vec![
            Value::Str("test".into()),
            Value::Int(100),
            Value::Int(100),
            Value::Int(100),
            Value::Int(2),
        ],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BUDGET_EXCEEDED"), "{error}");
}

#[test]
fn consensus_vote_requires_exact_finite_inputs_and_returns_normalized_decisions() {
    let mut env = Env::new();
    let network = consensus_network(&mut env);
    for input in [
        Value::Array(vec![]),
        Value::Array(vec![numbers(&[0, 0])]),
        Value::Array(vec![numbers(&[0]), numbers(&[0, 0])]),
        Value::Array(vec![numbers(&[0, 0, 0]), numbers(&[0, 0])]),
        Value::Array(vec![
            Value::Array(vec![Value::Float(f64::NAN), Value::Int(0)]),
            numbers(&[0, 0]),
        ]),
    ] {
        bad_argument(
            "consensus_vote",
            vec![network.clone(), input.clone()],
            &mut env,
        );
        let error = call_with_input(
            "consensus_vote",
            vec![input],
            Some(network.clone()),
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    let result = call(
        "consensus_vote",
        vec![
            network,
            Value::Array(vec![numbers(&[0, 0]), numbers(&[0, 0])]),
        ],
        &mut env,
    )
    .unwrap();
    let Value::Record(result) = result else {
        panic!("expected Record")
    };
    assert_eq!(result["confidence"], Value::Float(1.0));
    let Value::Array(decisions) = &result["decisions"] else {
        panic!("expected decisions")
    };
    assert_eq!(decisions.len(), 2);
    for decision in decisions {
        let Value::Array(row) = decision else {
            panic!("expected row")
        };
        let sum: f64 = row
            .iter()
            .map(|value| match value {
                Value::Float(n) => *n,
                _ => panic!("expected Float"),
            })
            .sum();
        assert!((sum - 1.0).abs() < 1e-12);
    }
}

#[test]
fn consensus_vote_validates_serialized_components_and_round_budget() {
    let mut env = Env::new();
    let Value::Record(network) = consensus_network(&mut env) else {
        panic!("expected Record")
    };
    let Value::Str(data) = &network["_data"] else {
        panic!("expected encoding")
    };
    let original: serde_json::Value = serde_json::from_str(data).unwrap();
    for mutation in 0..5 {
        let mut data = original.clone();
        match mutation {
            0 => data["local_encoder"]["layers"][0]["weights"] = serde_json::json!([]),
            1 => data["agent_networks"] = serde_json::json!([]),
            2 => data["state_dim"] = serde_json::json!(3),
            3 => data["message_dim"] = serde_json::json!(3),
            _ => data["decision_network"]["layers"] = serde_json::json!([]),
        }
        bad_argument(
            "consensus_vote",
            vec![
                encoded(data),
                Value::Array(vec![numbers(&[0, 0]), numbers(&[0, 0])]),
            ],
            &mut env,
        );
    }
    let mut data = original;
    data["num_rounds"] = serde_json::json!(17);
    let error = call(
        "consensus_vote",
        vec![
            encoded(data),
            Value::Array(vec![numbers(&[0, 0]), numbers(&[0, 0])]),
        ],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BUDGET_EXCEEDED"), "{error}");
    bad_argument(
        "consensus_vote",
        vec![
            Value::Record(BTreeMap::from([(
                "_data".into(),
                Value::Str("invalid".into()),
            )])),
            Value::Array(vec![]),
        ],
        &mut env,
    );
}

#[test]
fn consensus_vote_refuses_non_finite_computation_results() {
    let mut env = Env::new();
    let Value::Record(network) = consensus_network(&mut env) else {
        panic!("expected Record")
    };
    let Value::Str(data) = &network["_data"] else {
        panic!("expected encoding")
    };
    let mut data: serde_json::Value = serde_json::from_str(data).unwrap();
    let layers = data["decision_network"]["layers"].as_array_mut().unwrap();
    let last = layers.last_mut().unwrap();
    last["activation"] = serde_json::json!("Linear");
    last["weights"] = serde_json::json!([[0.0, 0.0], [0.0, 0.0]]);
    last["biases"] = serde_json::json!([f64::MAX, f64::MAX]);
    // Each individual output is finite, but summing two decisions overflows.
    let error = call(
        "consensus_vote",
        vec![
            encoded(data),
            Value::Array(vec![numbers(&[0, 0]), numbers(&[0, 0])]),
        ],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_STATE"), "{error}");
}
