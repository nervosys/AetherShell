use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::{BTreeMap, BTreeSet};

fn lambda(source: &str) -> Value {
    let program = aethershell::parser::parse_program(source).unwrap();
    let result = aethershell::eval::eval_program(&program, &mut Env::new()).unwrap();
    assert!(
        matches!(result, Value::Lambda(_)),
        "expected Lambda from {source}"
    );
    result
}

fn population(genome: &str, env: &mut Env) -> Value {
    call(
        "population",
        vec![
            Value::Int(3),
            Value::Str(genome.into()),
            Value::Record(BTreeMap::from([
                ("elitism".into(), Value::Int(0)),
                ("mutation_rate".into(), Value::Int(0)),
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

fn individuals(value: &Value) -> serde_json::Value {
    let Value::Str(encoded) = &fields(value)["_individuals"] else {
        panic!("expected individuals")
    };
    serde_json::from_str(encoded).unwrap()
}

fn changed(value: &Value, key: &str, change: impl FnOnce(&mut serde_json::Value)) -> Value {
    let mut result = fields(value).clone();
    let Value::Str(encoded) = &result[key] else {
        panic!("expected encoding")
    };
    let mut data = serde_json::from_str(encoded).unwrap();
    change(&mut data);
    result.insert(key.into(), Value::Str(data.to_string()));
    Value::Record(result)
}

fn error_code(result: anyhow::Result<Value>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn callback_failures_and_invalid_scores_are_never_zero_fitness() {
    let mut env = Env::new();
    for genome in ["nn", "vec"] {
        let pop = population(genome, &mut env);
        for name in ["fitness", "evolve_step", "evolve"] {
            let error = call(
                name,
                vec![pop.clone(), lambda("fn(g) => missing_fitness_callback()")],
                &mut env,
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("missing_fitness_callback"),
                "{error}"
            );
            error_code(
                call(
                    name,
                    vec![pop.clone(), lambda("fn(g) => \"bad\"")],
                    &mut env,
                ),
                "E_BAD_ARG",
            );
            error_code(
                call(name, vec![pop.clone(), lambda("fn(a, b) => 1")], &mut env),
                "E_BAD_ARG",
            );
        }
    }
}

struct CallbackFile(std::path::PathBuf);
impl Drop for CallbackFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn invalid_callback_score_stops_after_the_first_call() {
    let mut env = Env::new();
    for name in ["fitness", "evolve_step", "evolve"] {
        let path = std::env::temp_dir().join(format!("ae-fitness-{}.txt", uuid::Uuid::new_v4()));
        drop(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap(),
        );
        let fixture = CallbackFile(path);
        let quoted_path = serde_json::to_string(fixture.0.to_str().unwrap()).unwrap();
        let callback = lambda(&format!("fn(g) => file_append({quoted_path}, \"x\")"));
        let pop = population("vec", &mut env);
        error_code(call(name, vec![pop, callback], &mut env), "E_BAD_ARG");
        assert_eq!(std::fs::read_to_string(&fixture.0).unwrap(), "x");
    }
}

#[test]
fn fitness_caches_scores_and_preserves_history_across_direct_and_pipeline_calls() {
    let mut env = Env::new();
    for genome in ["nn", "vec"] {
        let initial = population(genome, &mut env);
        let scored = call(
            "fitness",
            vec![initial.clone(), lambda("fn(g) => 7")],
            &mut env,
        )
        .unwrap();
        assert_eq!(
            fields(&fields(&scored)["stats"])["best_fitness"],
            Value::Float(7.0)
        );
        assert_eq!(individuals(&initial)[0]["fitness"], serde_json::Value::Null);
        let cached = call_with_input(
            "fitness",
            vec![lambda("fn(g) => missing_fitness_callback()")],
            Some(scored),
            &mut env,
        )
        .unwrap();
        assert_eq!(
            fields(&fields(&cached)["stats"])["history"],
            Value::Array(vec![Value::Float(7.0), Value::Float(7.0)])
        );
        let best = call_with_input("best_individual", vec![], Some(cached), &mut env).unwrap();
        assert_eq!(fields(&best)["fitness"], Value::Float(7.0));
        assert_eq!(fields(&best)["id"], Value::Int(2));
    }
}

#[test]
fn generations_preserve_history_and_never_reuse_current_individual_ids() {
    let mut env = Env::new();
    for genome in ["nn", "vec"] {
        let initial = population(genome, &mut env);
        let first = call(
            "evolve_step",
            vec![initial.clone(), lambda("fn(g) => 1")],
            &mut env,
        )
        .unwrap();
        let second = call_with_input(
            "evolve_step",
            vec![lambda("fn(g) => 1")],
            Some(first),
            &mut env,
        )
        .unwrap();
        assert_eq!(fields(&second)["generation"], Value::Int(2));
        assert_eq!(fields(&second)["next_id"], Value::Int(9));
        assert_eq!(
            fields(&fields(&second)["stats"])["history"],
            Value::Array(vec![Value::Float(1.0), Value::Float(1.0)])
        );
        let data = individuals(&second);
        let ids: BTreeSet<_> = data
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, BTreeSet::from([6, 7, 8]));
        assert!(data
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["fitness"].is_null()));
        error_code(
            call("best_individual", vec![second.clone()], &mut env),
            "E_BAD_STATE",
        );
        let scored = call("fitness", vec![second, lambda("fn(g) => 2")], &mut env).unwrap();
        assert_eq!(
            fields(&fields(&scored)["stats"])["generation"],
            Value::Int(2)
        );
        assert_eq!(
            fields(&call("best_individual", vec![scored], &mut env).unwrap())["fitness"],
            Value::Float(2.0)
        );
        let multiple = call(
            "evolve",
            vec![initial, lambda("fn(g) => 1"), Value::Int(2)],
            &mut env,
        )
        .unwrap();
        assert_eq!(fields(&multiple)["next_id"], Value::Int(9));
    }
}

#[test]
fn zero_generations_and_legacy_next_id_are_supported_but_bad_limits_are_refused() {
    let mut env = Env::new();
    let initial = population("vec", &mut env);
    let result = call(
        "evolve",
        vec![
            initial.clone(),
            lambda("fn(g) => missing_fitness_callback()"),
            Value::Int(0),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(result, initial);
    for count in [Value::Int(-1), Value::Float(1.0)] {
        error_code(
            call(
                "evolve",
                vec![initial.clone(), lambda("fn(g) => 1"), count],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    error_code(
        call(
            "evolve",
            vec![
                initial.clone(),
                lambda("fn(g) => missing_fitness_callback()"),
                Value::Int(10001),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    let mut old = fields(&initial).clone();
    old.remove("next_id");
    let evolved = call(
        "evolve_step",
        vec![Value::Record(old), lambda("fn(g) => 1")],
        &mut env,
    )
    .unwrap();
    assert_eq!(fields(&evolved)["next_id"], Value::Int(6));
    let mut overflow = fields(&initial).clone();
    overflow.insert("next_id".into(), Value::Int(i64::MAX));
    error_code(
        call(
            "evolve_step",
            vec![
                Value::Record(overflow),
                lambda("fn(g) => missing_fitness_callback()"),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn malformed_population_state_is_refused_before_any_callback() {
    let mut env = Env::new();
    let pop = population("vec", &mut env);
    let cases = [
        changed(&pop, "_individuals", |d| *d = serde_json::json!([])),
        changed(&pop, "_individuals", |d| d[1]["id"] = d[0]["id"].clone()),
        changed(&pop, "_individuals", |d| {
            d[0]["genome"] = serde_json::json!([])
        }),
        changed(&pop, "_individuals", |d| {
            d[0]["genome"] = serde_json::json!([1.0])
        }),
        changed(&pop, "_individuals", |d| {
            d[0]["generation"] = serde_json::json!(1)
        }),
        changed(&pop, "_config", |d| {
            d["mutation_rate"] = serde_json::json!(2)
        }),
        changed(&pop, "_config", |d| {
            d["population_size"] = serde_json::json!(4)
        }),
        changed(&pop, "_config", |d| {
            d["selection"] = serde_json::json!({"Tournament": 1000000})
        }),
    ];
    for name in ["fitness", "evolve_step", "evolve", "best_individual"] {
        for value in &cases {
            let mut args = vec![value.clone()];
            if name != "best_individual" {
                args.push(lambda("fn(g) => missing_fitness_callback()"));
            }
            error_code(call(name, args, &mut env), "E_BAD_ARG");
        }
    }
    let nn = population("nn", &mut env);
    let ragged = changed(&nn, "_individuals", |d| {
        d[0]["genome"]["layers"][0]["weights"] = serde_json::json!([])
    });
    error_code(
        call(
            "fitness",
            vec![ragged, lambda("fn(g) => missing_fitness_callback()")],
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn computed_statistics_overflow_is_a_coded_refusal() {
    let mut env = Env::new();
    let pop = population("vec", &mut env);
    let numeric_callback = |score| {
        Value::Lambda(aethershell::value::Lambda {
            params: vec!["g".into()],
            body: Box::new(aethershell::ast::Expr::LitFloat(score)),
            captured: BTreeMap::new(),
        })
    };
    for name in ["fitness", "evolve_step", "evolve"] {
        for score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            error_code(
                call(name, vec![pop.clone(), numeric_callback(score)], &mut env),
                "E_BAD_ARG",
            );
        }
    }
    let callback = numeric_callback(f64::MAX);
    error_code(
        call("fitness", vec![pop, callback], &mut env),
        "E_BAD_STATE",
    );
}

#[test]
fn work_and_history_budgets_are_checked_before_callbacks() {
    let mut env = Env::new();
    let large = call(
        "population",
        vec![Value::Int(101), Value::Str("vec".into())],
        &mut env,
    )
    .unwrap();
    error_code(
        call(
            "evolve",
            vec![
                large,
                lambda("fn(g) => missing_fitness_callback()"),
                Value::Int(1000),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    let initial = population("vec", &mut env);
    let wide = changed(&initial, "_individuals", |data| {
        for individual in data.as_array_mut().unwrap() {
            individual["genome"] = serde_json::json!(vec![1.0; 1000]);
        }
    });
    error_code(
        call(
            "evolve",
            vec![
                wide,
                lambda("fn(g) => missing_fitness_callback()"),
                Value::Int(4000),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    let mut exhausted = fields(&initial).clone();
    let Value::Record(stats) = exhausted.get_mut("stats").unwrap() else {
        panic!("expected stats")
    };
    stats.insert(
        "history".into(),
        Value::Array(vec![Value::Float(1.0); 10000]),
    );
    stats.insert("best_fitness".into(), Value::Float(1.0));
    stats.insert("worst_fitness".into(), Value::Float(1.0));
    error_code(
        call(
            "fitness",
            vec![
                Value::Record(exhausted),
                lambda("fn(g) => missing_fitness_callback()"),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}
