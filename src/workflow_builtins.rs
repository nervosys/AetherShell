//! The shell-facing surface of the workflow engine.
//!
//! [`crate::workflows`] is async and its API is Rust. This module is the
//! bridge: sixteen builtins that build templates from shell records, run them,
//! and report on them.
//!
//! Until this existed, `workflows::workflow_builtins()` listed those sixteen
//! names and nothing registered any of them, so `workflow_create` at the prompt
//! answered `unknown builtin` and the list's only caller was its own unit test
//! — which asserted the list had at least ten entries and so passed whether or
//! not the shell had ever heard of one.
//!
//! # Blocking on an async engine
//!
//! Builtins are synchronous. The engine is not. Which bridge is correct depends
//! on where the call came from, and getting it wrong is a panic rather than an
//! error:
//!
//! - No runtime running: drive the future on this module's own runtime.
//! - A multi-thread runtime already running (`ae mcp`, `ae agent serve`, and
//!   every workflow step, which runs on the blocking pool): hand the thread to
//!   `block_in_place` and reuse that runtime. Creating a second runtime and
//!   calling `block_on` inside the first panics.
//! - A current-thread runtime already running: there is no safe move. Blocking
//!   it would deadlock, so this reports rather than hangs.

use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::{Handle, Runtime, RuntimeFlavor};

use crate::value::Value;
use crate::workflows::{
    CircuitBreakerConfig, CircuitState, GatherStrategy, RetryConfig, StepType, WorkflowEngine,
    WorkflowPattern, WorkflowStatus, WorkflowStep, WorkflowTemplate, WorkflowTemplateFactory,
};

/// The engine every workflow builtin talks to.
///
/// One per process: a template registered by one call has to be visible to the
/// next, and the shell has no other place to keep it.
fn engine() -> &'static WorkflowEngine {
    static ENGINE: OnceLock<WorkflowEngine> = OnceLock::new();
    ENGINE.get_or_init(WorkflowEngine::new)
}

/// The runtime used when the caller has none of its own.
fn fallback_runtime() -> Result<&'static Runtime> {
    static RT: OnceLock<Option<Runtime>> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().ok())
        .as_ref()
        .ok_or_else(|| anyhow!("workflow: could not start an async runtime"))
}

/// Run an engine future to completion from synchronous builtin code.
fn block_on<F: Future<Output = Result<Value>>>(fut: F) -> Result<Value> {
    match Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            RuntimeFlavor::CurrentThread => Err(anyhow!(
                "workflow builtins cannot run inside a single-threaded async \
                 context: blocking it would deadlock. Use a SubWorkflow step to \
                 nest a workflow inside another one."
            )),
            _ => tokio::task::block_in_place(|| handle.block_on(fut)),
        },
        Err(_) => fallback_runtime()?.block_on(fut),
    }
}

// -----------------------------------------------------------------------------
// Shell records in, engine types out
// -----------------------------------------------------------------------------

fn as_str(v: Option<&Value>, what: &str) -> Result<String> {
    match v {
        Some(Value::Str(s)) | Some(Value::Uri(s)) => Ok(s.clone()),
        _ => Err(crate::safety::arg_err(what)),
    }
}

fn record(v: &Value, what: &str) -> Result<BTreeMap<String, Value>> {
    match v {
        Value::Record(r) => Ok(r.clone()),
        _ => Err(crate::safety::arg_err(what)),
    }
}

/// Build a workflow step from a shell record.
///
/// ```text
/// { run: "grep", args: ["TODO"], id: "scan",
///   input: "$.files", output: "$.hits",
///   when: "count > 0", timeout_ms: 5000, retries: 3 }
/// ```
///
/// `run` is required and names a builtin; everything else is optional. A record
/// with `agent:` instead of `run:` is an agent step, and one with `url:` is an
/// HTTP step.
fn step_from_value(v: &Value, index: usize) -> Result<WorkflowStep> {
    let r = record(v, "workflow step must be a record, e.g. {run: \"ls\"}")?;

    let step_type = if let Some(run) = r.get("run") {
        StepType::Execute {
            function: as_str(Some(run), "step `run` must be a builtin name")?,
            args: match r.get("args") {
                Some(Value::Array(a)) => a.clone(),
                Some(other) => vec![other.clone()],
                None => vec![],
            },
        }
    } else if let Some(agent) = r.get("agent") {
        StepType::Agent {
            agent_id: as_str(Some(agent), "step `agent` must be a name")?,
            prompt: as_str(r.get("prompt"), "an agent step needs a `prompt`")?,
        }
    } else if let Some(url) = r.get("url") {
        StepType::Http {
            method: r
                .get("method")
                .and_then(|m| match m {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "GET".to_string()),
            url: as_str(Some(url), "step `url` must be a string")?,
            body: r.get("body").cloned(),
        }
    } else if let Some(ms) = r.get("delay_ms") {
        StepType::Delay {
            duration_ms: match ms {
                Value::Int(n) if *n >= 0 => *n as u64,
                _ => {
                    return Err(crate::safety::arg_err(
                        "`delay_ms` must be a non-negative int",
                    ))
                }
            },
        }
    } else if let Some(emit) = r.get("emit") {
        StepType::EmitEvent {
            event_type: as_str(Some(emit), "step `emit` must be an event name")?,
            payload: r.get("payload").cloned().unwrap_or(Value::Null),
        }
    } else if let Some(wait) = r.get("wait") {
        StepType::WaitForEvent {
            event_type: as_str(Some(wait), "step `wait` must be an event name")?,
            timeout_ms: match r.get("timeout_ms") {
                Some(Value::Int(n)) if *n >= 0 => Some(*n as u64),
                _ => None,
            },
        }
    } else if let Some(sub) = r.get("workflow") {
        StepType::SubWorkflow {
            template_id: as_str(Some(sub), "step `workflow` must be a template id")?,
            inputs: match r.get("inputs") {
                Some(Value::Record(inputs)) => {
                    inputs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                }
                _ => HashMap::new(),
            },
        }
    } else {
        return Err(crate::safety::arg_err(
            "a step needs one of `run`, `agent`, `url`, `delay_ms`, `emit`, \
             `wait` or `workflow`",
        ));
    };

    let id = match r.get("id") {
        Some(Value::Str(s)) => s.clone(),
        _ => format!("step-{index}"),
    };

    let retries = match r.get("retries") {
        Some(Value::Int(n)) if *n > 0 => Some(RetryConfig {
            max_retries: *n as u32,
            ..RetryConfig::default()
        }),
        _ => None,
    };

    Ok(WorkflowStep {
        name: id.clone(),
        id,
        step_type,
        input_mapping: r.get("input").and_then(|v| match v {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }),
        output_mapping: r.get("output").and_then(|v| match v {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }),
        retry_config: retries,
        timeout_ms: match r.get("timeout_ms") {
            Some(Value::Int(n)) if *n >= 0 => Some(*n as u64),
            _ => None,
        },
        condition: r.get("when").and_then(|v| match v {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }),
        compensate: match r.get("compensate") {
            Some(step) => Some(Box::new(step_from_value(step, index)?)),
            None => None,
        },
        metadata: HashMap::new(),
    })
}

fn steps_from_value(v: Option<&Value>) -> Result<Vec<WorkflowStep>> {
    match v {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, s)| step_from_value(s, i))
            .collect(),
        _ => Err(crate::safety::arg_err("expected an array of step records")),
    }
}

fn status_str(s: &WorkflowStatus) -> &'static str {
    match s {
        WorkflowStatus::Created => "created",
        WorkflowStatus::Running => "running",
        WorkflowStatus::Paused => "paused",
        WorkflowStatus::Completed => "completed",
        WorkflowStatus::Failed => "failed",
        WorkflowStatus::Cancelled => "cancelled",
        WorkflowStatus::Compensating => "compensating",
    }
}

fn pattern_str(p: &WorkflowPattern) -> &'static str {
    match p {
        WorkflowPattern::Sequential => "sequential",
        WorkflowPattern::Parallel => "parallel",
        WorkflowPattern::MapReduce => "map-reduce",
        WorkflowPattern::FanOutFanIn => "fan-out-fan-in",
        WorkflowPattern::Saga => "saga",
        WorkflowPattern::Pipeline => "pipeline",
        WorkflowPattern::ScatterGather => "scatter-gather",
        WorkflowPattern::Choreography => "choreography",
    }
}

fn template_record(t: &WorkflowTemplate) -> Value {
    Value::Record(BTreeMap::from([
        ("id".to_string(), Value::Str(t.id.clone())),
        ("name".to_string(), Value::Str(t.name.clone())),
        ("description".to_string(), Value::Str(t.description.clone())),
        (
            "pattern".to_string(),
            Value::Str(pattern_str(&t.pattern).to_string()),
        ),
        ("steps".to_string(), Value::Int(t.steps.len() as i64)),
    ]))
}

async fn register(template: WorkflowTemplate) -> Result<Value> {
    let id = template.id.clone();
    engine().register_template(template).await;
    Ok(Value::Str(id))
}

// -----------------------------------------------------------------------------
// Dispatch
// -----------------------------------------------------------------------------

/// Every workflow builtin, paired with a one-line description.
///
/// This is the list `workflows::workflow_builtins()` always returned; the
/// difference is that each name now reaches an implementation.
pub fn names() -> Vec<(&'static str, &'static str)> {
    crate::workflows::workflow_builtins()
}

/// Serve a workflow builtin, or `None` if the name is not one.
pub fn call(name: &str, args: Vec<Value>, input: Option<Value>) -> Option<Result<Value>> {
    let served = names().iter().any(|(n, _)| *n == name);
    if !served {
        return None;
    }
    Some(dispatch(name, args, input))
}

fn dispatch(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let arg = |i: usize| {
        args.get(i)
            .cloned()
            .or_else(|| if i == 0 { input.clone() } else { None })
    };

    match name {
        // ---- Template construction -----------------------------------------
        "workflow_pipeline" => {
            let title = as_str(arg(0).as_ref(), "workflow_pipeline(name, steps)")?;
            let steps = steps_from_value(arg(1).as_ref())?;
            block_on(register(WorkflowTemplateFactory::pipeline(
                &title,
                "Created by workflow_pipeline",
                steps,
            )))
        }

        "workflow_map_reduce" => {
            let title = as_str(
                arg(0).as_ref(),
                "workflow_map_reduce(name, mapper, reducer)",
            )?;
            let mapper = step_from_value(
                arg(1).as_ref().ok_or_else(|| {
                    crate::safety::arg_err("workflow_map_reduce needs a mapper step")
                })?,
                0,
            )?;
            let reducer = step_from_value(
                arg(2).as_ref().ok_or_else(|| {
                    crate::safety::arg_err("workflow_map_reduce needs a reducer step")
                })?,
                1,
            )?;
            block_on(register(WorkflowTemplateFactory::map_reduce(
                &title,
                "Created by workflow_map_reduce",
                mapper,
                reducer,
            )))
        }

        "workflow_fan_out" => {
            let title = as_str(
                arg(0).as_ref(),
                "workflow_fan_out(name, workers, aggregator)",
            )?;
            let workers = steps_from_value(arg(1).as_ref())?;
            let aggregator = step_from_value(
                arg(2).as_ref().ok_or_else(|| {
                    crate::safety::arg_err("workflow_fan_out needs an aggregator")
                })?,
                0,
            )?;
            block_on(register(WorkflowTemplateFactory::fan_out_fan_in(
                &title,
                "Created by workflow_fan_out",
                workers,
                aggregator,
            )))
        }

        "workflow_scatter_gather" => {
            let title = as_str(
                arg(0).as_ref(),
                "workflow_scatter_gather(name, targets, strategy)",
            )?;
            let targets = match arg(1) {
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|v| as_str(Some(v), "each target must be a name"))
                    .collect::<Result<Vec<_>>>()?,
                _ => return Err(crate::safety::arg_err("expected an array of target names")),
            };
            // The gather half is a strategy, not a step: how many responses
            // to wait for, and what counts as enough.
            let strategy = match arg(2) {
                None | Some(Value::Null) => GatherStrategy::WaitAll,
                Some(Value::Str(s)) => match s.as_str() {
                    "all" => GatherStrategy::WaitAll,
                    "first" => GatherStrategy::FirstSuccess,
                    other => {
                        return Err(crate::safety::arg_err(&format!(
                            "unknown gather strategy `{other}`: expected \"all\", \"first\",                              {{first: n}}, {{timeout_ms: n}} or {{consensus: fraction}}"
                        )))
                    }
                },
                Some(Value::Record(r)) => {
                    if let Some(Value::Int(n)) = r.get("first") {
                        GatherStrategy::FirstN((*n).max(1) as usize)
                    } else if let Some(Value::Int(n)) = r.get("timeout_ms") {
                        GatherStrategy::BestEffort {
                            timeout_ms: (*n).max(0) as u64,
                        }
                    } else if let Some(Value::Float(f)) = r.get("consensus") {
                        GatherStrategy::Consensus { threshold: *f }
                    } else {
                        return Err(crate::safety::arg_err(
                            "gather record needs one of `first`, `timeout_ms` or `consensus`",
                        ));
                    }
                }
                Some(_) => {
                    return Err(crate::safety::arg_err(
                        "gather strategy must be a string or a record",
                    ))
                }
            };
            block_on(register(WorkflowTemplateFactory::scatter_gather(
                &title,
                "Created by workflow_scatter_gather",
                targets,
                strategy,
            )))
        }

        "workflow_saga" => {
            let title = as_str(arg(0).as_ref(), "workflow_saga(name, transactions)")?;
            let pairs = match arg(1) {
                Some(Value::Array(items)) => items,
                _ => {
                    return Err(crate::safety::arg_err(
                        "workflow_saga expects an array of [action, compensation] pairs",
                    ))
                }
            };
            let mut transactions = Vec::new();
            for (i, pair) in pairs.iter().enumerate() {
                match pair {
                    Value::Array(two) if two.len() == 2 => transactions
                        .push((step_from_value(&two[0], i)?, step_from_value(&two[1], i)?)),
                    _ => {
                        return Err(crate::safety::arg_err(
                            "each transaction must be a two-element [action, compensation] array",
                        ))
                    }
                }
            }
            block_on(register(WorkflowTemplateFactory::saga(
                &title,
                "Created by workflow_saga",
                transactions,
            )))
        }

        "workflow_register" => {
            let r = record(
                arg(0).as_ref().ok_or_else(|| {
                    crate::safety::arg_err("workflow_register({name, steps}) expects a record")
                })?,
                "workflow_register expects a record",
            )?;
            let title = as_str(r.get("name"), "the template needs a `name`")?;
            let steps = steps_from_value(r.get("steps"))?;
            block_on(register(WorkflowTemplateFactory::pipeline(
                &title,
                "Created by workflow_register",
                steps,
            )))
        }

        "workflow_templates" => block_on(async {
            let templates = engine().list_templates().await;
            Ok(Value::Array(
                templates.iter().map(template_record).collect(),
            ))
        }),

        // ---- Instances ------------------------------------------------------
        "workflow_create" => {
            let template_id = as_str(arg(0).as_ref(), "workflow_create(template_id, inputs)")?;
            let inputs = match arg(1) {
                Some(Value::Record(r)) => r.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                Some(other) => HashMap::from([("input".to_string(), other)]),
                None => HashMap::new(),
            };
            block_on(async move {
                engine()
                    .create_instance(&template_id, inputs)
                    .await
                    .map(Value::Str)
            })
        }

        "workflow_execute" => {
            let id = as_str(arg(0).as_ref(), "workflow_execute(workflow_id)")?;
            block_on(async move { engine().execute(&id).await })
        }

        "workflow_status" => {
            let id = as_str(arg(0).as_ref(), "workflow_status(workflow_id)")?;
            block_on(async move {
                let info = engine()
                    .get_instance(&id)
                    .await
                    .ok_or_else(|| anyhow!("no such workflow: {id}"))?;
                Ok(Value::Record(BTreeMap::from([
                    ("id".to_string(), Value::Str(info.id)),
                    ("template".to_string(), Value::Str(info.template_id)),
                    (
                        "status".to_string(),
                        Value::Str(status_str(&info.status).to_string()),
                    ),
                    (
                        "steps_completed".to_string(),
                        Value::Int(info.context.results.len() as i64),
                    ),
                    (
                        "variables".to_string(),
                        Value::Record(
                            info.context
                                .variables
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                        ),
                    ),
                ])))
            })
        }

        "workflow_list" => block_on(async {
            let instances = engine().list_instances().await;
            Ok(Value::Array(
                instances
                    .into_iter()
                    .map(|i| {
                        Value::Record(BTreeMap::from([
                            ("id".to_string(), Value::Str(i.id)),
                            ("template".to_string(), Value::Str(i.template_id)),
                            (
                                "status".to_string(),
                                Value::Str(status_str(&i.status).to_string()),
                            ),
                        ]))
                    })
                    .collect(),
            ))
        }),

        "workflow_cancel" | "workflow_pause" | "workflow_resume" => {
            let id = as_str(arg(0).as_ref(), "expects a workflow id")?;
            let verb = name.to_string();
            block_on(async move {
                match verb.as_str() {
                    "workflow_cancel" => engine().cancel(&id).await,
                    "workflow_pause" => engine().pause(&id).await,
                    _ => engine().resume(&id).await,
                }?;
                Ok(Value::Bool(true))
            })
        }

        // ---- Circuit breakers ------------------------------------------------
        "circuit_breaker_create" => {
            let breaker = as_str(arg(0).as_ref(), "circuit_breaker_create(name, [config])")?;
            let mut config = CircuitBreakerConfig::default();
            if let Some(Value::Record(r)) = arg(1) {
                if let Some(Value::Int(n)) = r.get("failure_threshold") {
                    config.failure_threshold = (*n).max(1) as u32;
                }
                if let Some(Value::Int(n)) = r.get("success_threshold") {
                    config.success_threshold = (*n).max(1) as u32;
                }
                if let Some(Value::Int(n)) = r.get("reset_timeout_ms") {
                    config.reset_timeout = Duration::from_millis((*n).max(0) as u64);
                }
            }
            block_on(async move {
                engine().register_circuit_breaker(&breaker, config).await;
                Ok(Value::Bool(true))
            })
        }

        "circuit_breaker_status" => {
            let breaker = as_str(arg(0).as_ref(), "circuit_breaker_status(name)")?;
            block_on(async move {
                let state = engine()
                    .get_circuit_breaker_status(&breaker)
                    .await
                    .ok_or_else(|| anyhow!("no such circuit breaker: {breaker}"))?;
                Ok(Value::Str(
                    match state {
                        CircuitState::Closed => "closed",
                        CircuitState::Open => "open",
                        CircuitState::HalfOpen => "half-open",
                    }
                    .to_string(),
                ))
            })
        }

        other => Err(anyhow!("workflow builtin not implemented: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_ok(name: &str, args: Vec<Value>) -> Value {
        call(name, args, None)
            .unwrap_or_else(|| panic!("{name} is not served"))
            .unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }

    #[test]
    fn every_declared_name_is_served() {
        for (name, _) in names() {
            assert!(
                call(name, vec![], None).is_some(),
                "{name} is declared by workflow_builtins() but nothing serves it"
            );
        }
        assert!(
            call("definitely_not_a_workflow_builtin", vec![], None).is_none(),
            "the dispatcher must decline names it does not serve"
        );
    }

    #[test]
    fn a_pipeline_runs_end_to_end_from_shell_values() {
        let template = call_ok(
            "workflow_pipeline",
            vec![
                Value::Str("shout".into()),
                Value::Array(vec![Value::Record(BTreeMap::from([
                    ("id".to_string(), Value::Str("up".into())),
                    ("run".to_string(), Value::Str("upper".into())),
                    (
                        "args".to_string(),
                        Value::Array(vec![Value::Str("hello".into())]),
                    ),
                ]))]),
            ],
        );
        let Value::Str(template_id) = template else {
            panic!("expected a template id");
        };

        let created = call_ok(
            "workflow_create",
            vec![
                Value::Str(template_id),
                Value::Record(BTreeMap::from([("input".to_string(), Value::Null)])),
            ],
        );
        let Value::Str(workflow_id) = created else {
            panic!("expected a workflow id");
        };

        let out = call_ok("workflow_execute", vec![Value::Str(workflow_id.clone())]);
        assert_eq!(out, Value::Str("HELLO".into()));

        let status = call_ok("workflow_status", vec![Value::Str(workflow_id)]);
        let Value::Record(r) = status else {
            panic!("expected a status record");
        };
        assert_eq!(r.get("status"), Some(&Value::Str("completed".into())));
    }

    #[test]
    fn steps_chain_through_their_mappings() {
        let template = call_ok(
            "workflow_pipeline",
            vec![
                Value::Str("chain".into()),
                Value::Array(vec![
                    Value::Record(BTreeMap::from([
                        ("id".to_string(), Value::Str("first".into())),
                        ("run".to_string(), Value::Str("upper".into())),
                        (
                            "args".to_string(),
                            Value::Array(vec![Value::Str("abc".into())]),
                        ),
                        ("output".to_string(), Value::Str("$.shouted".into())),
                    ])),
                    Value::Record(BTreeMap::from([
                        ("id".to_string(), Value::Str("second".into())),
                        ("run".to_string(), Value::Str("len".into())),
                        ("input".to_string(), Value::Str("$.shouted".into())),
                    ])),
                ]),
            ],
        );
        let Value::Str(template_id) = template else {
            panic!("expected a template id");
        };
        let Value::Str(workflow_id) = call_ok(
            "workflow_create",
            vec![
                Value::Str(template_id),
                Value::Record(BTreeMap::from([("input".to_string(), Value::Null)])),
            ],
        ) else {
            panic!("expected a workflow id");
        };
        assert_eq!(
            call_ok("workflow_execute", vec![Value::Str(workflow_id)]),
            Value::Int(3)
        );
    }

    #[test]
    fn a_step_record_without_an_action_is_refused() {
        let err = call(
            "workflow_pipeline",
            vec![
                Value::Str("bad".into()),
                Value::Array(vec![Value::Record(BTreeMap::from([(
                    "id".to_string(),
                    Value::Str("nothing".into()),
                )]))]),
            ],
            None,
        )
        .unwrap()
        .expect_err("a step with no action must be refused");
        assert!(
            err.to_string().contains("run"),
            "the error should name the field that was missing, got: {err}"
        );
    }

    #[test]
    fn a_circuit_breaker_reports_its_state() {
        call_ok(
            "circuit_breaker_create",
            vec![Value::Str("payments".into())],
        );
        assert_eq!(
            call_ok(
                "circuit_breaker_status",
                vec![Value::Str("payments".into())]
            ),
            Value::Str("closed".into())
        );
        assert!(
            call(
                "circuit_breaker_status",
                vec![Value::Str("absent".into())],
                None
            )
            .unwrap()
            .is_err(),
            "an unknown breaker must be an error, not a state"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_builtin_works_inside_a_multi_thread_runtime() {
        // `ae mcp` and `ae agent serve` both run inside one, and a naive
        // Runtime::new().block_on() there panics.
        let out = tokio::task::spawn_blocking(|| call_ok("workflow_templates", vec![]))
            .await
            .expect("no panic");
        assert!(matches!(out, Value::Array(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_single_threaded_runtime_is_refused_not_deadlocked() {
        let err = call("workflow_templates", vec![], None)
            .unwrap()
            .expect_err("must refuse rather than block the only thread");
        assert!(
            err.to_string().contains("single-threaded"),
            "the refusal should say why, got: {err}"
        );
    }
}
