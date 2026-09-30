use aethershell::{builtins::call, env::Env, value::Value};
use std::collections::BTreeMap;

fn options(fields: &[(&str, Value)]) -> Value {
    Value::Record(
        fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
    )
}

fn population(
    size: i64,
    genome: Value,
    config: Option<Value>,
    env: &mut Env,
) -> anyhow::Result<Value> {
    let mut args = vec![Value::Int(size), genome];
    args.extend(config);
    call("population", args, env)
}

fn error_code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn population_refuses_invalid_sizes_and_genome_types_before_allocation() {
    let mut env = Env::new();
    for size in [-1, 0] {
        error_code(
            population(size, Value::Str("nn".into()), None, &mut env),
            "E_BAD_ARG",
        );
    }
    error_code(
        population(10001, Value::Str("nn".into()), None, &mut env),
        "E_BUDGET_EXCEEDED",
    );
    let error = population(i64::MAX, Value::Str("nn".into()), None, &mut env).unwrap_err();
    assert!(
        error.to_string().contains("E_BAD_ARG") || error.to_string().contains("E_BUDGET_EXCEEDED"),
        "{error}"
    );
    for genome in [
        Value::Str("unknown".into()),
        Value::Uri("vec".into()),
        Value::Int(1),
    ] {
        error_code(population(1, genome, None, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn numeric_settings_are_checked_consistently_by_both_calls() {
    let mut env = Env::new();
    let invalid = [
        ("mutation_rate", Value::Float(-0.1)),
        ("mutation_rate", Value::Float(1.1)),
        ("mutation_rate", Value::Float(f64::NAN)),
        ("mutation_rate", Value::Str("0.1".into())),
        ("mutation_strength", Value::Float(-1.0)),
        ("mutation_strength", Value::Float(f64::INFINITY)),
        ("crossover_rate", Value::Int(2)),
        ("crossover_rate", Value::Float(f64::NEG_INFINITY)),
        ("generations", Value::Int(-1)),
        ("generations", Value::Float(1.0)),
        ("elitism", Value::Int(-1)),
        ("elitism", Value::Float(1.0)),
        ("elitism", Value::Int(4)),
        ("seed", Value::Int(1)),
        ("selection", Value::Str("rank".into())),
        ("type", Value::Str("other".into())),
    ];
    for (key, value) in invalid {
        let config = options(&[("population_size", Value::Int(3)), (key, value)]);
        error_code(
            call("evolution_config", vec![config.clone()], &mut env),
            "E_BAD_ARG",
        );
        error_code(
            population(3, Value::Str("vec".into()), Some(config), &mut env),
            "E_BAD_ARG",
        );
    }
    for key in ["population_size", "generations"] {
        error_code(
            call(
                "evolution_config",
                vec![options(&[(key, Value::Int(10001))])],
                &mut env,
            ),
            "E_BUDGET_EXCEEDED",
        );
    }
    error_code(
        call(
            "evolution_config",
            vec![options(&[("population_size", Value::Int(0))])],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    error_code(
        population(
            3,
            Value::Str("vec".into()),
            Some(options(&[("generations", Value::Int(10001))])),
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}

#[test]
fn returned_configuration_round_trips_and_mismatched_size_is_refused() {
    let mut env = Env::new();
    let config = call(
        "evolution_config",
        vec![options(&[
            ("population_size", Value::Int(1)),
            ("generations", Value::Int(0)),
            ("mutation_rate", Value::Int(0)),
            ("mutation_strength", Value::Int(1)),
            ("crossover_rate", Value::Int(1)),
        ])],
        &mut env,
    )
    .unwrap();
    let Value::Record(fields) = &config else {
        panic!("expected Record")
    };
    assert_eq!(fields["elitism"], Value::Int(1));
    assert_eq!(
        call("evolution_config", vec![config.clone()], &mut env).unwrap(),
        config
    );
    let result = population(1, Value::Str("vec".into()), Some(config.clone()), &mut env).unwrap();
    let Value::Record(fields) = result else {
        panic!("expected Record")
    };
    let Value::Str(serialized) = &fields["_config"] else {
        panic!("expected config")
    };
    let config: serde_json::Value = serde_json::from_str(serialized).unwrap();
    assert_eq!(config["mutation_rate"], 0.0);
    assert_eq!(config["mutation_strength"], 1.0);
    assert_eq!(config["crossover_rate"], 1.0);
    assert_eq!(config["generations"], 0);
    assert_eq!(config["elitism"], 1);
    assert_eq!(fields["size"], Value::Int(1));
    error_code(
        population(
            2,
            Value::Str("vec".into()),
            Some(options(&[("population_size", Value::Int(1))])),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn created_population_genomes_have_the_documented_shapes() {
    let mut env = Env::new();
    for name in ["nn", "vec", "vector"] {
        let Value::Record(pop) = population(3, Value::Str(name.into()), None, &mut env).unwrap()
        else {
            panic!("expected Record")
        };
        assert_eq!(pop["generation"], Value::Int(0));
        let Value::Str(data) = &pop["_individuals"] else {
            panic!("expected serialized individuals")
        };
        let individuals: serde_json::Value = serde_json::from_str(data).unwrap();
        let individuals = individuals.as_array().unwrap();
        assert_eq!(individuals.len(), 3);
        for (id, individual) in individuals.iter().enumerate() {
            assert_eq!(individual["id"], id);
            assert!(individual["fitness"].is_null());
            if name == "nn" {
                let network: aethershell::neural::NeuralNetwork =
                    serde_json::from_value(individual["genome"].clone()).unwrap();
                assert_eq!(network.param_count(), 76);
                assert_eq!(network.layers[0].input_size, 4);
                assert_eq!(network.layers[0].output_size, 8);
                assert_eq!(network.layers[1].output_size, 4);
            } else {
                let vector = individual["genome"].as_array().unwrap();
                assert_eq!(vector.len(), 10);
                assert!(vector
                    .iter()
                    .all(|value| value.as_f64().unwrap().is_finite()));
            }
        }
    }
}

#[test]
fn wrong_outer_options_are_refused_instead_of_selecting_defaults() {
    let mut env = Env::new();
    for config in [
        Value::Int(1),
        Value::Null,
        Value::Array(vec![]),
        Value::Table(aethershell::value::Table {
            rows: vec![],
            schema: vec![],
        }),
    ] {
        error_code(
            call("evolution_config", vec![config.clone()], &mut env),
            "E_BAD_ARG",
        );
        error_code(
            population(1, Value::Str("vec".into()), Some(config), &mut env),
            "E_BAD_ARG",
        );
    }
    // Also reject invalid values nested in an otherwise valid Record.
    error_code(
        call(
            "evolution_config",
            vec![Value::Record(BTreeMap::from([(
                "mutation_rate".into(),
                Value::Null,
            )]))],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}
