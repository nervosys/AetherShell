//! Workflow Templates and Patterns
//!
//! This module provides pre-built workflow patterns for common distributed
//! computing scenarios:
//!
//! - **Map-Reduce**: Parallel data processing with aggregation
//! - **Fan-Out/Fan-In**: Parallel execution with result collection
//! - **Saga**: Distributed transactions with compensating actions
//! - **Pipeline**: Sequential processing stages
//! - **Scatter-Gather**: Broadcast to multiple workers, collect responses
//! - **Circuit Breaker**: Fault-tolerant execution with automatic recovery
//! - **Retry**: Automatic retry with exponential backoff
//! - **Choreography**: Event-driven workflow coordination
//!
//! # Status
//!
//! The engine executes. It did not until the change recorded in CHANGELOG.md:
//! every leaf step returned a
//! formatted string describing what it would have done — an `Execute` step
//! answered `"Executed {fn} with {args}"` without calling anything, an `Http`
//! step answered a hard-coded `status: 200` without making a request, and
//! `evaluate_condition` returned `true` for every guard, so a workflow could
//! not branch. Eighteen tests passed over all of it, because the only one that
//! called [`WorkflowEngine::execute`] used an empty pipeline and so never ran
//! a step.
//!
//! What the steps do now:
//!
//! - `Execute` calls the named builtin through [`crate::builtins`], on the
//!   blocking pool, so a workflow step is subject to the same effect gate,
//!   workspace jail and audit chain as a call typed at the prompt.
//! - `Agent` goes through the `agent` builtin, for the same reason.
//! - `Http` issues the request, through the same egress allowlist, SSRF
//!   validation and hardened client the `http_get` builtin uses.
//! - `EmitEvent` and `WaitForEvent` carry a payload over the engine's
//!   broadcast channel, with an already-emitted event visible to a later wait
//!   in the same workflow.
//! - `SubWorkflow` re-enters the engine, bounded by
//!   [`MAX_SUBWORKFLOW_DEPTH`].
//! - `input_mapping`, `output_mapping` and `timeout_ms` are honoured, so a
//!   template's stages actually pass data to each other.
//!
//! **Not exposed to the shell.** [`workflow_builtins`] lists sixteen names and
//! nothing registers them, so `workflow_create` at the prompt still answers
//! `unknown builtin`. This module is reachable as library API only. Wiring the
//! names into the dispatcher is a separate decision about the shell's surface,
//! and the documentation must not describe them as available until it is made.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::env::Env;
use crate::value::Value;

// =============================================================================
// Core Workflow Types
// =============================================================================

/// Unique identifier for workflow instances
pub type WorkflowId = String;

/// Unique identifier for workflow steps
pub type StepId = String;

/// Workflow execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
// Same reasoning as `WorkflowEvent`: construct one with `new` or `default` and
// set what you need, so a future field is additive.
#[non_exhaustive]
pub struct WorkflowContext {
    /// Workflow instance ID
    pub workflow_id: WorkflowId,
    /// Current step being executed
    pub current_step: Option<StepId>,
    /// Variables available to all steps
    pub variables: HashMap<String, Value>,
    /// Accumulated results from steps
    pub results: HashMap<StepId, StepResult>,
    /// Workflow metadata
    pub metadata: HashMap<String, String>,
    /// Start time
    pub started_at: Option<u64>,
    /// End time (if completed)
    pub ended_at: Option<u64>,
    /// How many sub-workflow levels deep this context is. A `SubWorkflow` step
    /// executes a whole other template, and a template that names itself would
    /// otherwise recurse until the stack ran out.
    #[serde(default)]
    pub depth: u32,
}

impl WorkflowContext {
    /// A fresh context for one workflow instance.
    ///
    /// The struct is `#[non_exhaustive]`, so this and [`Default`] are how one
    /// is built from outside the crate.
    pub fn new(workflow_id: impl Into<WorkflowId>) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            ..Default::default()
        }
    }
}

/// How many levels of `SubWorkflow` nesting are allowed before a workflow is
/// refused. Deep nesting is far more often a cycle than a design.
pub const MAX_SUBWORKFLOW_DEPTH: u32 = 8;

impl Default for WorkflowContext {
    fn default() -> Self {
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            current_step: None,
            variables: HashMap::new(),
            results: HashMap::new(),
            metadata: HashMap::new(),
            started_at: None,
            ended_at: None,
            depth: 0,
        }
    }
}

/// Result of a workflow step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: StepId,
    pub status: StepStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub retries: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Compensated,
}

/// Overall workflow status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Compensating,
}

/// Workflow event for monitoring and choreography
#[derive(Debug, Clone, Serialize, Deserialize)]
// Marked non-exhaustive so the *next* variant is not a breaking change. It does
// not rescue this one: downstream code that matched exhaustively against 11.0.1
// stops compiling either way, because `Custom` is already there.
#[non_exhaustive]
pub enum WorkflowEvent {
    Started {
        workflow_id: WorkflowId,
        template: String,
    },
    StepStarted {
        workflow_id: WorkflowId,
        step_id: StepId,
    },
    StepCompleted {
        workflow_id: WorkflowId,
        step_id: StepId,
        result: StepResult,
    },
    StepFailed {
        workflow_id: WorkflowId,
        step_id: StepId,
        error: String,
    },
    Completed {
        workflow_id: WorkflowId,
        result: Value,
    },
    Failed {
        workflow_id: WorkflowId,
        error: String,
    },
    Compensating {
        workflow_id: WorkflowId,
        step_id: StepId,
    },
    /// Emitted by a `StepType::EmitEvent` step, and the only kind a
    /// `StepType::WaitForEvent` step waits for.
    Custom {
        workflow_id: WorkflowId,
        event_type: String,
        payload: Value,
    },
}

// =============================================================================
// Workflow Step Definition
// =============================================================================

/// A single step in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step identifier
    pub id: StepId,
    /// Human-readable name
    pub name: String,
    /// Step type (determines execution behavior)
    pub step_type: StepType,
    /// Input transformation (optional)
    pub input_mapping: Option<String>,
    /// Output transformation (optional)
    pub output_mapping: Option<String>,
    /// Retry configuration
    pub retry_config: Option<RetryConfig>,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Condition for execution (optional expression)
    pub condition: Option<String>,
    /// Compensating action for saga pattern
    pub compensate: Option<Box<WorkflowStep>>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepType {
    /// Execute a function/builtin
    Execute { function: String, args: Vec<Value> },
    /// Call an AI agent
    Agent { agent_id: String, prompt: String },
    /// HTTP request
    Http {
        method: String,
        url: String,
        body: Option<Value>,
    },
    /// Parallel execution of sub-steps
    Parallel { steps: Vec<WorkflowStep> },
    /// Conditional branching
    Branch {
        conditions: Vec<(String, WorkflowStep)>,
        default: Option<Box<WorkflowStep>>,
    },
    /// Wait for an event
    WaitForEvent {
        event_type: String,
        timeout_ms: Option<u64>,
    },
    /// Emit an event
    EmitEvent { event_type: String, payload: Value },
    /// Delay execution
    Delay { duration_ms: u64 },
    /// Sub-workflow
    SubWorkflow {
        template_id: String,
        inputs: HashMap<String, Value>,
    },
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Jitter (0.0 to 1.0)
    pub jitter: f64,
    /// Retryable error patterns
    pub retry_on: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            jitter: 0.1,
            retry_on: vec!["timeout".to_string(), "connection_error".to_string()],
        }
    }
}

// =============================================================================
// Workflow Template
// =============================================================================

/// A workflow template that can be instantiated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    /// Template identifier
    pub id: String,
    /// Template name
    pub name: String,
    /// Description
    pub description: String,
    /// Template version
    pub version: String,
    /// Input parameters schema
    pub inputs: Vec<ParameterDef>,
    /// Output schema
    pub outputs: Vec<ParameterDef>,
    /// Root steps (can be sequential or parallel)
    pub steps: Vec<WorkflowStep>,
    /// Template metadata
    pub metadata: HashMap<String, String>,
    /// Pattern type for this template
    pub pattern: WorkflowPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub default: Option<Value>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowPattern {
    Sequential,
    Parallel,
    MapReduce,
    FanOutFanIn,
    Saga,
    Pipeline,
    ScatterGather,
    Choreography,
}

// =============================================================================
// Pre-built Template Factories
// =============================================================================

/// Factory for creating pre-built workflow templates
pub struct WorkflowTemplateFactory;

impl WorkflowTemplateFactory {
    /// Create a Map-Reduce template
    pub fn map_reduce(
        name: &str,
        description: &str,
        mapper: WorkflowStep,
        reducer: WorkflowStep,
    ) -> WorkflowTemplate {
        WorkflowTemplate {
            id: format!("map-reduce-{}", Uuid::new_v4()),
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            inputs: vec![
                ParameterDef {
                    name: "data".to_string(),
                    param_type: "array".to_string(),
                    required: true,
                    default: None,
                    description: "Input data array to process".to_string(),
                },
                ParameterDef {
                    name: "parallelism".to_string(),
                    param_type: "int".to_string(),
                    required: false,
                    default: Some(Value::Int(4)),
                    description: "Number of parallel mappers".to_string(),
                },
            ],
            outputs: vec![ParameterDef {
                name: "result".to_string(),
                param_type: "any".to_string(),
                required: true,
                default: None,
                description: "Reduced result".to_string(),
            }],
            steps: vec![
                WorkflowStep {
                    id: "map".to_string(),
                    name: "Map Phase".to_string(),
                    step_type: StepType::Parallel {
                        steps: vec![mapper],
                    },
                    input_mapping: Some("$.data".to_string()),
                    output_mapping: Some("$.mapped_results".to_string()),
                    retry_config: Some(RetryConfig::default()),
                    timeout_ms: Some(60000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
                WorkflowStep {
                    id: "reduce".to_string(),
                    name: "Reduce Phase".to_string(),
                    step_type: reducer.step_type,
                    input_mapping: Some("$.mapped_results".to_string()),
                    output_mapping: Some("$.result".to_string()),
                    retry_config: Some(RetryConfig::default()),
                    timeout_ms: Some(30000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
            ],
            metadata: HashMap::from([("pattern".to_string(), "map-reduce".to_string())]),
            pattern: WorkflowPattern::MapReduce,
        }
    }

    /// Create a Fan-Out/Fan-In template
    pub fn fan_out_fan_in(
        name: &str,
        description: &str,
        workers: Vec<WorkflowStep>,
        aggregator: WorkflowStep,
    ) -> WorkflowTemplate {
        WorkflowTemplate {
            id: format!("fan-out-fan-in-{}", Uuid::new_v4()),
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            inputs: vec![ParameterDef {
                name: "input".to_string(),
                param_type: "any".to_string(),
                required: true,
                default: None,
                description: "Input to fan out".to_string(),
            }],
            outputs: vec![ParameterDef {
                name: "aggregated".to_string(),
                param_type: "any".to_string(),
                required: true,
                default: None,
                description: "Aggregated result".to_string(),
            }],
            steps: vec![
                WorkflowStep {
                    id: "fan-out".to_string(),
                    name: "Fan Out".to_string(),
                    step_type: StepType::Parallel { steps: workers },
                    input_mapping: Some("$.input".to_string()),
                    output_mapping: Some("$.worker_results".to_string()),
                    retry_config: None,
                    timeout_ms: Some(120000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
                WorkflowStep {
                    id: "fan-in".to_string(),
                    name: "Fan In".to_string(),
                    step_type: aggregator.step_type,
                    input_mapping: Some("$.worker_results".to_string()),
                    output_mapping: Some("$.aggregated".to_string()),
                    retry_config: Some(RetryConfig::default()),
                    timeout_ms: Some(30000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
            ],
            metadata: HashMap::from([("pattern".to_string(), "fan-out-fan-in".to_string())]),
            pattern: WorkflowPattern::FanOutFanIn,
        }
    }

    /// Create a Saga template (distributed transaction with compensations)
    pub fn saga(
        name: &str,
        description: &str,
        transactions: Vec<(WorkflowStep, WorkflowStep)>, // (action, compensation)
    ) -> WorkflowTemplate {
        let steps: Vec<WorkflowStep> = transactions
            .into_iter()
            .enumerate()
            .map(|(i, (mut action, compensation))| {
                action.id = format!("saga-step-{}", i);
                action.compensate = Some(Box::new(compensation));
                action
            })
            .collect();

        WorkflowTemplate {
            id: format!("saga-{}", Uuid::new_v4()),
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            inputs: vec![ParameterDef {
                name: "transaction_id".to_string(),
                param_type: "string".to_string(),
                required: false,
                default: None,
                description: "Transaction correlation ID".to_string(),
            }],
            outputs: vec![ParameterDef {
                name: "success".to_string(),
                param_type: "bool".to_string(),
                required: true,
                default: None,
                description: "Whether the saga completed successfully".to_string(),
            }],
            steps,
            metadata: HashMap::from([("pattern".to_string(), "saga".to_string())]),
            pattern: WorkflowPattern::Saga,
        }
    }

    /// Create a Pipeline template (sequential processing stages)
    pub fn pipeline(name: &str, description: &str, stages: Vec<WorkflowStep>) -> WorkflowTemplate {
        WorkflowTemplate {
            id: format!("pipeline-{}", Uuid::new_v4()),
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            inputs: vec![ParameterDef {
                name: "input".to_string(),
                param_type: "any".to_string(),
                required: true,
                default: None,
                description: "Pipeline input".to_string(),
            }],
            outputs: vec![ParameterDef {
                name: "output".to_string(),
                param_type: "any".to_string(),
                required: true,
                default: None,
                description: "Pipeline output".to_string(),
            }],
            steps: stages,
            metadata: HashMap::from([("pattern".to_string(), "pipeline".to_string())]),
            pattern: WorkflowPattern::Pipeline,
        }
    }

    /// Create a Scatter-Gather template
    pub fn scatter_gather(
        name: &str,
        description: &str,
        scatter_targets: Vec<String>, // Agent IDs or endpoints
        gather_strategy: GatherStrategy,
    ) -> WorkflowTemplate {
        let workers: Vec<WorkflowStep> = scatter_targets
            .iter()
            .enumerate()
            .map(|(i, target)| WorkflowStep {
                id: format!("scatter-{}", i),
                name: format!("Scatter to {}", target),
                step_type: StepType::Agent {
                    agent_id: target.clone(),
                    prompt: "$.prompt".to_string(),
                },
                input_mapping: Some("$.input".to_string()),
                output_mapping: None,
                retry_config: Some(RetryConfig::default()),
                timeout_ms: Some(30000),
                condition: None,
                compensate: None,
                metadata: HashMap::new(),
            })
            .collect();

        WorkflowTemplate {
            id: format!("scatter-gather-{}", Uuid::new_v4()),
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            inputs: vec![
                ParameterDef {
                    name: "input".to_string(),
                    param_type: "any".to_string(),
                    required: true,
                    default: None,
                    description: "Input to scatter".to_string(),
                },
                ParameterDef {
                    name: "timeout_ms".to_string(),
                    param_type: "int".to_string(),
                    required: false,
                    default: Some(Value::Int(30000)),
                    description: "Gather timeout".to_string(),
                },
            ],
            outputs: vec![ParameterDef {
                name: "gathered".to_string(),
                param_type: "array".to_string(),
                required: true,
                default: None,
                description: "Gathered results".to_string(),
            }],
            steps: vec![
                WorkflowStep {
                    id: "scatter".to_string(),
                    name: "Scatter Phase".to_string(),
                    step_type: StepType::Parallel { steps: workers },
                    input_mapping: Some("$.input".to_string()),
                    output_mapping: Some("$.scattered".to_string()),
                    retry_config: None,
                    timeout_ms: Some(60000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
                WorkflowStep {
                    id: "gather".to_string(),
                    name: "Gather Phase".to_string(),
                    step_type: StepType::Execute {
                        function: "workflow_gather".to_string(),
                        args: vec![Value::Str(format!("{:?}", gather_strategy))],
                    },
                    input_mapping: Some("$.scattered".to_string()),
                    output_mapping: Some("$.gathered".to_string()),
                    retry_config: None,
                    timeout_ms: Some(10000),
                    condition: None,
                    compensate: None,
                    metadata: HashMap::new(),
                },
            ],
            metadata: HashMap::from([
                ("pattern".to_string(), "scatter-gather".to_string()),
                (
                    "gather_strategy".to_string(),
                    format!("{:?}", gather_strategy),
                ),
            ]),
            pattern: WorkflowPattern::ScatterGather,
        }
    }
}

/// Strategy for gathering scattered results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GatherStrategy {
    /// Wait for all responses
    WaitAll,
    /// Return after first N responses
    FirstN(usize),
    /// Return after timeout, with partial results
    BestEffort { timeout_ms: u64 },
    /// Return first successful response
    FirstSuccess,
    /// Consensus (majority must agree)
    Consensus { threshold: f64 },
}

// =============================================================================
// Circuit Breaker
// =============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for fault tolerance
#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of successes needed to close from half-open
    pub success_threshold: u32,
    /// Time to wait before transitioning from open to half-open
    pub reset_timeout: Duration,
    /// Number of requests allowed in half-open state
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout: Duration::from_secs(30),
            half_open_max_requests: 3,
        }
    }
}

impl CircuitBreaker {
    pub fn new(name: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.to_string(),
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Check if request is allowed
    pub async fn allow_request(&self) -> bool {
        let mut state = self.state.write().await;

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.reset_timeout {
                        *state = CircuitState::HalfOpen;
                        *self.success_count.write().await = 0;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => {
                // Allow limited requests
                *self.success_count.read().await < self.config.half_open_max_requests
            }
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match *state {
            CircuitState::Closed => {
                *self.failure_count.write().await = 0;
            }
            CircuitState::HalfOpen => {
                let mut count = self.success_count.write().await;
                *count += 1;
                if *count >= self.config.success_threshold {
                    *state = CircuitState::Closed;
                    *self.failure_count.write().await = 0;
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;

        match *state {
            CircuitState::Closed => {
                let mut count = self.failure_count.write().await;
                *count += 1;
                if *count >= self.config.failure_threshold {
                    *state = CircuitState::Open;
                    *self.last_failure_time.write().await = Some(Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                *state = CircuitState::Open;
                *self.last_failure_time.write().await = Some(Instant::now());
            }
            CircuitState::Open => {}
        }
    }

    /// Get current state
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }

    /// Get circuit breaker name
    pub fn name(&self) -> &str {
        &self.name
    }
}

// =============================================================================
// Step execution helpers
// =============================================================================

/// Is this value a "yes" for a step condition or a branch arm?
///
/// Deliberately the same rule the interpreter uses for `if`: only `false`,
/// `null`, zero, an empty string and an empty collection are falsey. A step
/// condition that evaluated to a record was previously "true" because the
/// evaluator returned `true` for everything.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) | Value::Uri(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Record(r) => !r.is_empty(),
        Value::Table(tbl) => !tbl.rows.is_empty(),
        Value::Error(_) => false,
        Value::Lambda(_) | Value::AsyncLambda(_) | Value::Future(_) | Value::Builtin(_) => true,
    }
}

/// Read `$.a.b` out of a workflow's variables.
///
/// `$` alone is the whole variable map as a record. A path that does not
/// resolve is `None` rather than an error: a step whose `input_mapping` names
/// a variable no earlier step produced simply receives no input.
fn resolve_path(context: &WorkflowContext, path: &str) -> Option<Value> {
    let path = path.trim();
    if path == "$" {
        return Some(Value::Record(
            context
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ));
    }
    let rest = path.strip_prefix("$.").unwrap_or(path);
    let mut parts = rest.split('.');
    let mut cur = context.variables.get(parts.next()?)?.clone();
    for part in parts {
        cur = match cur {
            Value::Record(ref r) => r.get(part)?.clone(),
            _ => return None,
        };
    }
    Some(cur)
}

/// Write a step's output back to `$.name`.
///
/// Only a single segment is written; `$.a.b` stores under `a.b` verbatim
/// rather than reaching into a record, because a partial write into a value a
/// parallel branch also holds is a race the engine has no way to resolve.
fn store_path(context: &mut WorkflowContext, path: &str, value: Value) {
    let path = path.trim();
    let name = path.strip_prefix("$.").unwrap_or(path);
    if name.is_empty() || name == "$" {
        return;
    }
    context.variables.insert(name.to_string(), value);
}

/// An environment seeded with the workflow's variables, for evaluating a
/// condition or calling a builtin.
fn context_env(context: &WorkflowContext) -> Env {
    let mut env = Env::new();
    for (k, v) in &context.variables {
        let _ = env.set_var(k.clone(), v.clone());
    }
    env
}

/// Substitute a whole-string `$.path` reference from the workflow's variables.
///
/// Only a whole-string reference is substituted, never an embedded one. The
/// templates are written that way -- `prompt: "$.prompt"` in `scatter_gather`
/// -- and a rule that also rewrote text inside a string would make any prompt
/// containing a dollar sign unpredictable.
fn resolve_string(context: &WorkflowContext, s: &str) -> String {
    if !s.starts_with("$.") && s != "$" {
        return s.to_string();
    }
    match resolve_path(context, s) {
        Some(Value::Str(v)) | Some(Value::Uri(v)) => v,
        Some(other) => other.to_string(),
        None => s.to_string(),
    }
}

/// The same substitution for a value: a string that is a `$.path` becomes what
/// the path holds, with its type intact. Everything else is passed through.
fn resolve_value(context: &WorkflowContext, v: &Value) -> Value {
    match v {
        Value::Str(s) if s.starts_with("$.") || s == "$" => {
            resolve_path(context, s).unwrap_or_else(|| v.clone())
        }
        _ => v.clone(),
    }
}

/// The key under which a workflow remembers the events it has emitted.
const SEEN_EVENTS: &str = "__emitted_events";

/// Remember an emitted event so a later `WaitForEvent` in the same workflow can
/// see it.
fn record_seen_event(context: &mut WorkflowContext, event_type: &str, payload: Value) {
    let entry = Value::Record(BTreeMap::from([
        ("event".to_string(), Value::Str(event_type.to_string())),
        ("payload".to_string(), payload),
    ]));
    match context.variables.get_mut(SEEN_EVENTS) {
        Some(Value::Array(seen)) => seen.push(entry),
        _ => {
            context
                .variables
                .insert(SEEN_EVENTS.to_string(), Value::Array(vec![entry]));
        }
    }
}

/// Consume the earliest remembered event of this type, if there is one.
fn take_seen_event(context: &mut WorkflowContext, event_type: &str) -> Option<Value> {
    let Some(Value::Array(seen)) = context.variables.get_mut(SEEN_EVENTS) else {
        return None;
    };
    let idx = seen.iter().position(|e| {
        matches!(e, Value::Record(r)
            if matches!(r.get("event"), Some(Value::Str(t)) if t == event_type))
    })?;
    let entry = seen.remove(idx);
    match entry {
        Value::Record(mut r) => r.remove("payload"),
        other => Some(other),
    }
}

/// Fold a parallel branch's variables back into the parent context.
///
/// Keys the parent already holds are left alone: the branch started from a
/// copy of them, so re-inserting an unchanged value is noise, and a branch
/// overwriting an input another branch also read is the race this whole
/// arrangement exists to avoid. Events the branch emitted are appended.
fn merge_branch_variables(parent: &mut WorkflowContext, branch: WorkflowContext) {
    // Step results are keyed by step id and every step in a template has a
    // distinct one, so this cannot collide. Without it a nested Parallel step's
    // per-branch outcomes died with the branch context and `get_instance`
    // reported nothing about them.
    for (step_id, result) in branch.results {
        parent.results.entry(step_id).or_insert(result);
    }
    for (k, v) in branch.variables {
        if k == SEEN_EVENTS {
            if let Value::Array(events) = v {
                match parent.variables.get_mut(SEEN_EVENTS) {
                    Some(Value::Array(existing)) => {
                        for e in events {
                            if !existing.contains(&e) {
                                existing.push(e);
                            }
                        }
                    }
                    _ => {
                        parent
                            .variables
                            .insert(SEEN_EVENTS.to_string(), Value::Array(events));
                    }
                }
            }
            continue;
        }
        parent.variables.entry(k).or_insert(v);
    }
}

/// Perform an HTTP request for a `StepType::Http`.
///
/// `http_get` is the only HTTP builtin, so anything other than GET is issued
/// here -- through the same egress allowlist (`guard_network`), the same SSRF
/// validation (`validate_http_url`) and the same hardened client the builtin
/// uses. A workflow step must not be a way around the network policy.
async fn http_step(method: String, url: String, body: Option<Value>) -> Result<Value> {
    tokio::task::spawn_blocking(move || {
        crate::builtins::guard_network("workflow_http", &url)?;
        let url = crate::security::validate_http_url(&url)
            .context("workflow http step: URL validation failed")?;
        let client = crate::security::create_secure_http_client()
            .context("workflow http step: failed to create HTTP client")?;

        let mut req = match method.as_str() {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            "HEAD" => client.head(&url),
            other => return Err(anyhow!("workflow http step: unsupported method `{other}`")),
        };
        if let Some(body) = body {
            req = match body {
                Value::Str(s) => req.body(s),
                other => req
                    .header("content-type", "application/json")
                    .body(other.to_json().to_string()),
            };
        }

        let resp = req.send()?;
        let status = resp.status().as_u16() as i64;
        let mut headers = BTreeMap::<String, Value>::new();
        for (k, v) in resp.headers().iter() {
            headers.insert(
                k.to_string(),
                Value::Str(v.to_str().unwrap_or("").to_string()),
            );
        }
        let body_text = resp.text().unwrap_or_default();

        Ok(Value::Record(BTreeMap::from([
            ("url".to_string(), Value::Str(url)),
            ("method".to_string(), Value::Str(method)),
            ("status".to_string(), Value::Int(status)),
            ("headers".to_string(), Value::Record(headers)),
            ("body".to_string(), Value::Str(body_text)),
        ])))
    })
    .await
    .map_err(|e| anyhow!("workflow http step panicked: {e}"))?
}

/// Call a builtin off the async runtime.
///
/// Builtins are synchronous and some of them block for a long time -- an HTTP
/// request, a spawned process, a model round-trip -- so they run on the
/// blocking pool rather than stalling a runtime worker. `Value` and `Env` are
/// both `Send`, which is what makes this possible.
async fn call_builtin_off_runtime(
    name: String,
    args: Vec<Value>,
    input: Option<Value>,
    vars: HashMap<String, Value>,
) -> Result<Value> {
    tokio::task::spawn_blocking(move || {
        let mut env = Env::new();
        for (k, v) in vars {
            let _ = env.set_var(k, v);
        }
        crate::builtins::call_with_input(&name, args, input, &mut env)
    })
    .await
    .map_err(|e| anyhow!("workflow step panicked: {e}"))?
}

// =============================================================================
// Workflow Engine
// =============================================================================

/// Workflow execution engine
#[derive(Clone)]
pub struct WorkflowEngine {
    templates: Arc<RwLock<HashMap<String, WorkflowTemplate>>>,
    instances: Arc<RwLock<HashMap<WorkflowId, WorkflowInstance>>>,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    event_tx: broadcast::Sender<WorkflowEvent>,
}

/// A running workflow instance
pub struct WorkflowInstance {
    pub id: WorkflowId,
    pub template_id: String,
    pub context: WorkflowContext,
    pub status: WorkflowStatus,
    pub created_at: Instant,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Register a workflow template
    pub async fn register_template(&self, template: WorkflowTemplate) {
        self.templates
            .write()
            .await
            .insert(template.id.clone(), template);
    }

    /// Get a registered template
    pub async fn get_template(&self, template_id: &str) -> Option<WorkflowTemplate> {
        self.templates.read().await.get(template_id).cloned()
    }

    /// List all registered templates
    pub async fn list_templates(&self) -> Vec<WorkflowTemplate> {
        self.templates.read().await.values().cloned().collect()
    }

    /// Create a workflow instance from a template
    pub async fn create_instance(
        &self,
        template_id: &str,
        inputs: HashMap<String, Value>,
    ) -> Result<WorkflowId> {
        let template = self
            .templates
            .read()
            .await
            .get(template_id)
            .cloned()
            .ok_or_else(|| anyhow!("Template not found: {}", template_id))?;

        // Validate required inputs
        for param in &template.inputs {
            if param.required && !inputs.contains_key(&param.name) {
                return Err(anyhow!("Missing required input: {}", param.name));
            }
        }

        let workflow_id = Uuid::new_v4().to_string();
        let mut context = WorkflowContext::default();
        context.workflow_id = workflow_id.clone();
        context.variables = inputs;
        context.started_at = Some(current_timestamp());

        let instance = WorkflowInstance {
            id: workflow_id.clone(),
            template_id: template_id.to_string(),
            context,
            status: WorkflowStatus::Created,
            created_at: Instant::now(),
        };

        self.instances
            .write()
            .await
            .insert(workflow_id.clone(), instance);

        let _ = self.event_tx.send(WorkflowEvent::Started {
            workflow_id: workflow_id.clone(),
            template: template_id.to_string(),
        });

        Ok(workflow_id)
    }

    /// Execute a workflow instance
    pub async fn execute(&self, workflow_id: &WorkflowId) -> Result<Value> {
        let (template, mut instance) = {
            let instances = self.instances.read().await;
            let instance = instances
                .get(workflow_id)
                .ok_or_else(|| anyhow!("Workflow instance not found: {}", workflow_id))?
                .clone_minimal();

            let templates = self.templates.read().await;
            let template = templates
                .get(&instance.template_id)
                .cloned()
                .ok_or_else(|| anyhow!("Template not found"))?;

            (template, instance)
        };

        instance.status = WorkflowStatus::Running;

        // Execute based on pattern
        let result = match template.pattern {
            WorkflowPattern::Sequential | WorkflowPattern::Pipeline => {
                self.execute_sequential(&template.steps, &mut instance.context)
                    .await
            }
            WorkflowPattern::Parallel => {
                self.execute_parallel(&template.steps, &mut instance.context)
                    .await
            }
            // Fan-out/fan-in and scatter/gather are two-stage *sequences*: a
            // Parallel step that fans out, then an aggregator that reads what
            // it published. Running the template's steps in parallel ran the
            // aggregator at the same time as the fan-out, so it read a variable
            // that did not exist yet -- the aggregator failed with "requires
            // input" and the concurrency was in the wrong place entirely.
            WorkflowPattern::FanOutFanIn | WorkflowPattern::ScatterGather => {
                self.execute_sequential(&template.steps, &mut instance.context)
                    .await
            }
            WorkflowPattern::MapReduce => {
                self.execute_map_reduce(&template.steps, &mut instance.context)
                    .await
            }
            WorkflowPattern::Saga => {
                self.execute_saga(&template.steps, &mut instance.context)
                    .await
            }
            WorkflowPattern::Choreography => {
                self.execute_choreography(&template.steps, &mut instance.context)
                    .await
            }
        };

        // Update instance status
        {
            let mut instances = self.instances.write().await;
            if let Some(inst) = instances.get_mut(workflow_id) {
                inst.status = match &result {
                    Ok(_) => WorkflowStatus::Completed,
                    Err(_) => WorkflowStatus::Failed,
                };
                // The run mutated a clone of the context. Without this the
                // variables every step wrote and the per-step results were
                // thrown away, and `get_instance` answered with the state from
                // before the workflow ran.
                instance.context.ended_at = Some(current_timestamp());
                inst.context = instance.context.clone();
            }
        }

        match result {
            Ok(value) => {
                let _ = self.event_tx.send(WorkflowEvent::Completed {
                    workflow_id: workflow_id.clone(),
                    result: value.clone(),
                });
                Ok(value)
            }
            Err(e) => {
                let _ = self.event_tx.send(WorkflowEvent::Failed {
                    workflow_id: workflow_id.clone(),
                    error: e.to_string(),
                });
                Err(e)
            }
        }
    }

    async fn execute_sequential(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        let mut last_result = Value::Null;

        for step in steps {
            context.current_step = Some(step.id.clone());

            let _ = self.event_tx.send(WorkflowEvent::StepStarted {
                workflow_id: context.workflow_id.clone(),
                step_id: step.id.clone(),
            });

            let start = Instant::now();
            let result = self.execute_step(step, context).await;
            let duration = start.elapsed();

            match result {
                Ok(output) => {
                    let step_result = StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Completed,
                        output: Some(output.clone()),
                        error: None,
                        duration_ms: duration.as_millis() as u64,
                        retries: 0,
                    };
                    context.results.insert(step.id.clone(), step_result.clone());

                    let _ = self.event_tx.send(WorkflowEvent::StepCompleted {
                        workflow_id: context.workflow_id.clone(),
                        step_id: step.id.clone(),
                        result: step_result,
                    });

                    last_result = output;
                }
                Err(e) => {
                    let _ = self.event_tx.send(WorkflowEvent::StepFailed {
                        workflow_id: context.workflow_id.clone(),
                        step_id: step.id.clone(),
                        error: e.to_string(),
                    });
                    return Err(e);
                }
            }
        }

        Ok(last_result)
    }

    /// Run steps concurrently and collect their results.
    ///
    /// Each branch gets its own clone of the context, because the alternative
    /// — sharing one — is a write race the engine has no way to resolve. Their
    /// `output_mapping` writes are merged back in step order once every branch
    /// has finished, so the result does not depend on which one won.
    ///
    /// This previously had a second, smaller copy of the step executor inline,
    /// which returned `"Executed: {function}"` for an execute step and
    /// `"Step: {id}"` for everything else — so a parallel branch never reached
    /// the real executor at all, even after that executor existed.
    async fn execute_parallel(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        let mut handles = Vec::new();

        for step in steps {
            let engine = self.clone();
            let step = step.clone();
            let mut branch_ctx = context.clone();
            let event_tx = self.event_tx.clone();
            let workflow_id = context.workflow_id.clone();

            handles.push(tokio::spawn(async move {
                let _ = event_tx.send(WorkflowEvent::StepStarted {
                    workflow_id,
                    step_id: step.id.clone(),
                });
                let start = Instant::now();
                let result = engine.execute_step(&step, &mut branch_ctx).await;
                (step, result, start.elapsed(), branch_ctx)
            }));
        }

        let mut outputs = Vec::new();
        let mut first_error: Option<anyhow::Error> = None;

        for handle in handles {
            let (step, result, duration, branch_ctx) = match handle.await {
                Ok(joined) => joined,
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(anyhow!("parallel branch panicked: {e}"));
                    }
                    continue;
                }
            };

            match result {
                Ok(value) => {
                    // Anything the branch emitted or stored belongs to the
                    // parent too; its own output lands under output_mapping.
                    merge_branch_variables(context, branch_ctx);
                    if let Some(path) = &step.output_mapping {
                        store_path(context, path, value.clone());
                    }
                    let step_result = StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Completed,
                        output: Some(value.clone()),
                        error: None,
                        duration_ms: duration.as_millis() as u64,
                        retries: 0,
                    };
                    context.results.insert(step.id.clone(), step_result.clone());
                    let _ = self.event_tx.send(WorkflowEvent::StepCompleted {
                        workflow_id: context.workflow_id.clone(),
                        step_id: step.id.clone(),
                        result: step_result,
                    });
                    outputs.push(value);
                }
                Err(e) => {
                    let message = e.to_string();
                    context.results.insert(
                        step.id.clone(),
                        StepResult {
                            step_id: step.id.clone(),
                            status: StepStatus::Failed,
                            output: None,
                            error: Some(message.clone()),
                            duration_ms: duration.as_millis() as u64,
                            retries: 0,
                        },
                    );
                    let _ = self.event_tx.send(WorkflowEvent::StepFailed {
                        workflow_id: context.workflow_id.clone(),
                        step_id: step.id.clone(),
                        error: message,
                    });
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        // Every branch is awaited before failing, so a fan-out reports each
        // branch's outcome rather than abandoning the rest at the first error.
        match first_error {
            Some(e) => Err(e),
            None => Ok(Value::Array(outputs)),
        }
    }

    async fn execute_map_reduce(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        // Map-reduce expects at least 2 steps: map and reduce
        if steps.len() < 2 {
            return Err(anyhow!("Map-reduce requires at least 2 steps"));
        }

        // Execute map phase (parallel)
        let map_step = &steps[0];
        if let StepType::Parallel { steps: map_steps } = &map_step.step_type {
            let map_result = self.execute_parallel(map_steps, context).await?;
            context
                .variables
                .insert("mapped_results".to_string(), map_result);
        }

        // Execute reduce phase (sequential)
        let reduce_step = &steps[1];
        self.execute_step(reduce_step, context).await
    }

    async fn execute_saga(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        let mut completed_steps: Vec<&WorkflowStep> = Vec::new();

        for step in steps {
            context.current_step = Some(step.id.clone());

            let result = self.execute_step(step, context).await;

            match result {
                Ok(output) => {
                    context.results.insert(
                        step.id.clone(),
                        StepResult {
                            step_id: step.id.clone(),
                            status: StepStatus::Completed,
                            output: Some(output),
                            error: None,
                            duration_ms: 0,
                            retries: 0,
                        },
                    );
                    completed_steps.push(step);
                }
                Err(e) => {
                    // Saga failure - compensate in reverse order
                    let _ = self.event_tx.send(WorkflowEvent::StepFailed {
                        workflow_id: context.workflow_id.clone(),
                        step_id: step.id.clone(),
                        error: e.to_string(),
                    });

                    // Execute compensating actions
                    for completed_step in completed_steps.iter().rev() {
                        if let Some(compensate) = &completed_step.compensate {
                            let _ = self.event_tx.send(WorkflowEvent::Compensating {
                                workflow_id: context.workflow_id.clone(),
                                step_id: completed_step.id.clone(),
                            });

                            let _ = self.execute_step(compensate, context).await;

                            if let Some(result) = context.results.get_mut(&completed_step.id) {
                                result.status = StepStatus::Compensated;
                            }
                        }
                    }

                    return Err(anyhow!("Saga failed at step {}: {}", step.id, e));
                }
            }
        }

        Ok(Value::Record(BTreeMap::from([(
            "success".to_string(),
            Value::Str("true".to_string()),
        )])))
    }

    async fn execute_choreography(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        // Choreography executes steps based on events
        // For now, execute sequentially but emit events
        self.execute_sequential(steps, context).await
    }

    /// Run one step: guard, map its input, execute with a timeout, retry on
    /// failure, and store its output where the template said to.
    ///
    /// `input_mapping`, `output_mapping` and `timeout_ms` are set on every step
    /// the template factory builds and were read by nothing, so map-reduce
    /// never handed the mapper's results to the reducer and a step could run
    /// forever. They are honoured here.
    fn execute_step<'a>(
        &'a self,
        step: &'a WorkflowStep,
        context: &'a mut WorkflowContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            // Check condition if present
            if let Some(condition) = &step.condition {
                if !self.evaluate_condition(condition, context) {
                    return Ok(Value::Null);
                }
            }

            // What this step is fed: the variable its input_mapping names.
            let input = step
                .input_mapping
                .as_deref()
                .and_then(|path| resolve_path(context, path));

            // Apply retry logic if configured
            let mut retries = 0;
            let max_retries = step
                .retry_config
                .as_ref()
                .map(|c| c.max_retries)
                .unwrap_or(0);

            loop {
                let attempt = self.execute_step_type(&step.step_type, input.clone(), context);
                let result = match step.timeout_ms {
                    Some(ms) => {
                        match tokio::time::timeout(Duration::from_millis(ms), attempt).await {
                            Ok(r) => r,
                            Err(_) => {
                                Err(anyhow!("step `{}` exceeded its {}ms timeout", step.id, ms))
                            }
                        }
                    }
                    None => attempt.await,
                };

                match result {
                    Ok(value) => {
                        if let Some(path) = &step.output_mapping {
                            store_path(context, path, value.clone());
                        }
                        return Ok(value);
                    }
                    Err(_e) if retries < max_retries => {
                        retries += 1;

                        // Calculate backoff delay
                        if let Some(config) = &step.retry_config {
                            let delay = config.initial_delay_ms as f64
                                * config.backoff_multiplier.powi(retries as i32);
                            let delay = delay.min(config.max_delay_ms as f64);
                            let jitter = delay * config.jitter * rand::random::<f64>();
                            tokio::time::sleep(Duration::from_millis((delay + jitter) as u64))
                                .await;
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        })
    }

    /// Run one step and return its value.
    ///
    /// Boxed rather than a plain `async fn` because `SubWorkflow` re-enters the
    /// engine, and an `async fn` cannot be recursive.
    fn execute_step_type<'a>(
        &'a self,
        step_type: &'a StepType,
        input: Option<Value>,
        context: &'a mut WorkflowContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            match step_type {
                StepType::Execute { function, args } => {
                    let function = resolve_string(context, function);
                    let args = args.iter().map(|a| resolve_value(context, a)).collect();
                    call_builtin_off_runtime(function, args, input, context.variables.clone()).await
                }

                StepType::Agent { agent_id, prompt } => {
                    // Routed through the `agent` builtin rather than calling the
                    // agent loop directly, so a workflow step is subject to the
                    // same effect gate, command allowlist and rate limit as an
                    // agent call typed at the prompt. A workflow must not be a
                    // way around them.
                    let goal = resolve_string(context, prompt);
                    let mut args = vec![Value::Str(goal)];
                    if let Some(Value::Array(tools)) = context.variables.get("tools") {
                        args.push(Value::Array(tools.clone()));
                    }
                    let out = call_builtin_off_runtime(
                        "agent".to_string(),
                        args,
                        input,
                        context.variables.clone(),
                    )
                    .await?;
                    // `agent_id` names the worker a scatter-gather step was
                    // addressed to; it is carried on the result so a fan-in
                    // stage can tell the branches apart.
                    Ok(Value::Record(BTreeMap::from([
                        ("agent".to_string(), Value::Str(agent_id.clone())),
                        ("output".to_string(), out),
                    ])))
                }

                StepType::Http { method, url, body } => {
                    let method = resolve_string(context, method).to_uppercase();
                    let url = resolve_string(context, url);
                    let body = body.as_ref().map(|b| resolve_value(context, b));
                    http_step(method, url, body).await
                }

                StepType::Parallel { steps } => self.execute_parallel(steps, context).await,

                StepType::Branch {
                    conditions,
                    default,
                } => {
                    for (condition, step) in conditions {
                        if self.evaluate_condition(condition, context) {
                            return self.execute_step(step, context).await;
                        }
                    }
                    if let Some(default_step) = default {
                        self.execute_step(default_step, context).await
                    } else {
                        Ok(Value::Null)
                    }
                }

                StepType::WaitForEvent {
                    event_type,
                    timeout_ms,
                } => {
                    let wanted = resolve_string(context, event_type);

                    // An event this workflow already emitted counts. Without
                    // this, an emit step followed by a wait step in the same
                    // sequential workflow would block until the timeout: the
                    // broadcast went out before the receiver subscribed.
                    if let Some(seen) = take_seen_event(context, &wanted) {
                        return Ok(seen);
                    }

                    let mut rx = self.event_tx.subscribe();
                    let workflow_id = context.workflow_id.clone();
                    let wait = async {
                        loop {
                            match rx.recv().await {
                                Ok(WorkflowEvent::Custom {
                                    workflow_id: id,
                                    event_type: ty,
                                    payload,
                                }) if id == workflow_id && ty == wanted => return Ok(payload),
                                Ok(_) => continue,
                                Err(e) => return Err(anyhow!("event stream closed: {e}")),
                            }
                        }
                    };

                    match timeout_ms {
                        Some(ms) => {
                            match tokio::time::timeout(Duration::from_millis(*ms), wait).await {
                                Ok(v) => v,
                                Err(_) => Err(anyhow!(
                                    "timed out after {}ms waiting for event `{}`",
                                    ms,
                                    wanted
                                )),
                            }
                        }
                        None => wait.await,
                    }
                }

                StepType::EmitEvent {
                    event_type,
                    payload,
                } => {
                    let ty = resolve_string(context, event_type);
                    let payload = resolve_value(context, payload);
                    record_seen_event(context, &ty, payload.clone());
                    let _ = self.event_tx.send(WorkflowEvent::Custom {
                        workflow_id: context.workflow_id.clone(),
                        event_type: ty,
                        payload: payload.clone(),
                    });
                    Ok(payload)
                }

                StepType::Delay { duration_ms } => {
                    tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                    Ok(Value::Null)
                }

                StepType::SubWorkflow {
                    template_id,
                    inputs,
                } => {
                    if context.depth >= MAX_SUBWORKFLOW_DEPTH {
                        return Err(anyhow!(
                            "sub-workflow nesting exceeded {} levels at template `{}`; that is \
                             almost always a cycle",
                            MAX_SUBWORKFLOW_DEPTH,
                            template_id
                        ));
                    }

                    // The child starts from the parent's variables, so a
                    // template can be reused as a stage, with its declared
                    // inputs layered on top.
                    let mut child_inputs = context.variables.clone();
                    for (k, v) in inputs {
                        child_inputs.insert(k.clone(), resolve_value(context, v));
                    }
                    if let Some(input) = input {
                        child_inputs.insert("input".to_string(), input);
                    }

                    let child_id = self.create_instance(template_id, child_inputs).await?;
                    self.set_instance_depth(&child_id, context.depth + 1).await;
                    self.execute(&child_id).await
                }
            }
        })
    }

    /// Evaluate a step condition or branch guard against the workflow's
    /// variables.
    ///
    /// The condition is AetherShell source, parsed and evaluated by the same
    /// interpreter the shell uses, in an environment holding the workflow's
    /// variables. Previously this returned `true` unconditionally, which meant
    /// a conditional step always ran and a `Branch` always took its first arm
    /// — a workflow could not branch at all, and nothing said so.
    ///
    /// A condition that fails to parse or evaluate is `false`, not an error:
    /// a guard that cannot be understood must not be treated as satisfied.
    fn evaluate_condition(&self, condition: &str, context: &WorkflowContext) -> bool {
        let condition = condition.trim();
        if condition.is_empty() {
            return true;
        }
        let Ok(stmts) = crate::parser::parse_program(condition) else {
            return false;
        };
        let mut env = context_env(context);
        match crate::eval::eval_program(&stmts, &mut env) {
            Ok(v) => truthy(&v),
            Err(_) => false,
        }
    }

    /// Record the nesting level of a freshly created sub-workflow instance.
    async fn set_instance_depth(&self, workflow_id: &WorkflowId, depth: u32) {
        if let Some(inst) = self.instances.write().await.get_mut(workflow_id) {
            inst.context.depth = depth;
        }
    }
    /// Get a workflow instance
    pub async fn get_instance(&self, workflow_id: &WorkflowId) -> Option<WorkflowInstanceInfo> {
        self.instances
            .read()
            .await
            .get(workflow_id)
            .map(|i| WorkflowInstanceInfo {
                id: i.id.clone(),
                template_id: i.template_id.clone(),
                status: i.status.clone(),
                context: i.context.clone(),
            })
    }

    /// List all workflow instances
    pub async fn list_instances(&self) -> Vec<WorkflowInstanceInfo> {
        self.instances
            .read()
            .await
            .values()
            .map(|i| WorkflowInstanceInfo {
                id: i.id.clone(),
                template_id: i.template_id.clone(),
                status: i.status.clone(),
                context: i.context.clone(),
            })
            .collect()
    }

    /// Cancel a workflow instance
    pub async fn cancel(&self, workflow_id: &WorkflowId) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(workflow_id) {
            instance.status = WorkflowStatus::Cancelled;
            Ok(())
        } else {
            Err(anyhow!("Workflow not found"))
        }
    }

    /// Pause a workflow instance
    pub async fn pause(&self, workflow_id: &WorkflowId) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(workflow_id) {
            if instance.status == WorkflowStatus::Running {
                instance.status = WorkflowStatus::Paused;
            }
            Ok(())
        } else {
            Err(anyhow!("Workflow not found"))
        }
    }

    /// Resume a paused workflow instance
    pub async fn resume(&self, workflow_id: &WorkflowId) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(workflow_id) {
            if instance.status == WorkflowStatus::Paused {
                instance.status = WorkflowStatus::Running;
            }
            Ok(())
        } else {
            Err(anyhow!("Workflow not found"))
        }
    }

    /// Subscribe to workflow events
    pub fn subscribe(&self) -> broadcast::Receiver<WorkflowEvent> {
        self.event_tx.subscribe()
    }

    /// Register a circuit breaker
    pub async fn register_circuit_breaker(&self, name: &str, config: CircuitBreakerConfig) {
        let breaker = CircuitBreaker::new(name, config);
        self.circuit_breakers
            .write()
            .await
            .insert(name.to_string(), breaker);
    }

    /// Get circuit breaker status
    pub async fn get_circuit_breaker_status(&self, name: &str) -> Option<CircuitState> {
        if let Some(breaker) = self.circuit_breakers.read().await.get(name) {
            Some(breaker.get_state().await)
        } else {
            None
        }
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal info for cloning without full context
impl WorkflowInstance {
    fn clone_minimal(&self) -> Self {
        Self {
            id: self.id.clone(),
            template_id: self.template_id.clone(),
            context: self.context.clone(),
            status: self.status.clone(),
            created_at: self.created_at,
        }
    }
}

/// Public workflow instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstanceInfo {
    pub id: WorkflowId,
    pub template_id: String,
    pub status: WorkflowStatus,
    pub context: WorkflowContext,
}

// =============================================================================
// Builtin Functions for Workflows
// =============================================================================

/// Create builtin functions for workflow management
pub fn workflow_builtins() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "workflow_create",
            "Create a new workflow instance from a template",
        ),
        ("workflow_execute", "Execute a workflow instance"),
        ("workflow_status", "Get workflow instance status"),
        ("workflow_cancel", "Cancel a running workflow"),
        ("workflow_pause", "Pause a running workflow"),
        ("workflow_resume", "Resume a paused workflow"),
        ("workflow_list", "List all workflow instances"),
        ("workflow_templates", "List registered workflow templates"),
        ("workflow_register", "Register a new workflow template"),
        ("workflow_map_reduce", "Create a map-reduce workflow"),
        ("workflow_pipeline", "Create a pipeline workflow"),
        (
            "workflow_saga",
            "Create a saga (distributed transaction) workflow",
        ),
        ("workflow_fan_out", "Create a fan-out/fan-in workflow"),
        (
            "workflow_scatter_gather",
            "Create a scatter-gather workflow",
        ),
        ("circuit_breaker_create", "Create a circuit breaker"),
        ("circuit_breaker_status", "Get circuit breaker status"),
    ]
}

// =============================================================================
// Helper Functions
// =============================================================================

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_context_default() {
        let ctx = WorkflowContext::default();
        assert!(!ctx.workflow_id.is_empty());
        assert!(ctx.variables.is_empty());
        assert!(ctx.results.is_empty());
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_workflow_step_serialization() {
        let step = WorkflowStep {
            id: "test-step".to_string(),
            name: "Test Step".to_string(),
            step_type: StepType::Execute {
                function: "test_fn".to_string(),
                args: vec![Value::Int(42)],
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: Some(5000),
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&step).unwrap();
        let deserialized: WorkflowStep = serde_json::from_str(&json).unwrap();
        assert_eq!(step.id, deserialized.id);
    }

    #[test]
    fn test_map_reduce_template_creation() {
        let mapper = WorkflowStep {
            id: "mapper".to_string(),
            name: "Mapper".to_string(),
            step_type: StepType::Execute {
                function: "map_fn".to_string(),
                args: vec![],
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };

        let reducer = WorkflowStep {
            id: "reducer".to_string(),
            name: "Reducer".to_string(),
            step_type: StepType::Execute {
                function: "reduce_fn".to_string(),
                args: vec![],
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };

        let template = WorkflowTemplateFactory::map_reduce(
            "Test MapReduce",
            "Test description",
            mapper,
            reducer,
        );

        assert_eq!(template.pattern, WorkflowPattern::MapReduce);
        assert_eq!(template.steps.len(), 2);
    }

    #[test]
    fn test_pipeline_template_creation() {
        let stages = vec![
            WorkflowStep {
                id: "stage1".to_string(),
                name: "Stage 1".to_string(),
                step_type: StepType::Execute {
                    function: "stage1_fn".to_string(),
                    args: vec![],
                },
                input_mapping: None,
                output_mapping: None,
                retry_config: None,
                timeout_ms: None,
                condition: None,
                compensate: None,
                metadata: HashMap::new(),
            },
            WorkflowStep {
                id: "stage2".to_string(),
                name: "Stage 2".to_string(),
                step_type: StepType::Execute {
                    function: "stage2_fn".to_string(),
                    args: vec![],
                },
                input_mapping: None,
                output_mapping: None,
                retry_config: None,
                timeout_ms: None,
                condition: None,
                compensate: None,
                metadata: HashMap::new(),
            },
        ];

        let template =
            WorkflowTemplateFactory::pipeline("Test Pipeline", "Test description", stages);

        assert_eq!(template.pattern, WorkflowPattern::Pipeline);
        assert_eq!(template.steps.len(), 2);
    }

    #[test]
    fn test_saga_template_creation() {
        let transactions = vec![(
            WorkflowStep {
                id: "action1".to_string(),
                name: "Action 1".to_string(),
                step_type: StepType::Execute {
                    function: "action1".to_string(),
                    args: vec![],
                },
                input_mapping: None,
                output_mapping: None,
                retry_config: None,
                timeout_ms: None,
                condition: None,
                compensate: None,
                metadata: HashMap::new(),
            },
            WorkflowStep {
                id: "compensate1".to_string(),
                name: "Compensate 1".to_string(),
                step_type: StepType::Execute {
                    function: "undo_action1".to_string(),
                    args: vec![],
                },
                input_mapping: None,
                output_mapping: None,
                retry_config: None,
                timeout_ms: None,
                condition: None,
                compensate: None,
                metadata: HashMap::new(),
            },
        )];

        let template = WorkflowTemplateFactory::saga("Test Saga", "Test description", transactions);

        assert_eq!(template.pattern, WorkflowPattern::Saga);
        assert!(template.steps[0].compensate.is_some());
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new("test", config);

        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test", config);

        breaker.record_failure().await;
        assert_eq!(breaker.get_state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);
        assert!(!breaker.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_success_resets_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("test", config);

        breaker.record_failure().await;
        breaker.record_failure().await;
        breaker.record_success().await;

        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_workflow_engine_creation() {
        let engine = WorkflowEngine::new();
        let templates = engine.list_templates().await;
        assert!(templates.is_empty());
    }

    #[tokio::test]
    async fn test_workflow_engine_register_template() {
        let engine = WorkflowEngine::new();

        let template = WorkflowTemplateFactory::pipeline("Test", "Test pipeline", vec![]);
        let template_id = template.id.clone();

        engine.register_template(template).await;

        let retrieved = engine.get_template(&template_id).await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_workflow_engine_create_instance() {
        let engine = WorkflowEngine::new();

        let template = WorkflowTemplateFactory::pipeline("Test", "Test pipeline", vec![]);
        let template_id = template.id.clone();

        engine.register_template(template).await;

        let inputs = HashMap::from([("input".to_string(), Value::Str("test".to_string()))]);

        let workflow_id = engine.create_instance(&template_id, inputs).await.unwrap();
        assert!(!workflow_id.is_empty());

        let instance = engine.get_instance(&workflow_id).await;
        assert!(instance.is_some());
    }

    #[tokio::test]
    async fn test_workflow_engine_execute_empty_pipeline() {
        let engine = WorkflowEngine::new();

        let template = WorkflowTemplateFactory::pipeline("Test", "Empty pipeline", vec![]);
        let template_id = template.id.clone();

        engine.register_template(template).await;

        // Pipeline template requires 'input' parameter
        let inputs = HashMap::from([("input".to_string(), Value::Str("test".to_string()))]);
        let workflow_id = engine.create_instance(&template_id, inputs).await.unwrap();
        let result = engine.execute(&workflow_id).await.unwrap();

        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn test_workflow_engine_cancel() {
        let engine = WorkflowEngine::new();

        let template = WorkflowTemplateFactory::pipeline("Test", "Test", vec![]);
        let template_id = template.id.clone();
        engine.register_template(template).await;

        // Pipeline template requires 'input' parameter
        let inputs = HashMap::from([("input".to_string(), Value::Str("test".to_string()))]);
        let workflow_id = engine.create_instance(&template_id, inputs).await.unwrap();
        engine.cancel(&workflow_id).await.unwrap();

        let instance = engine.get_instance(&workflow_id).await.unwrap();
        assert_eq!(instance.status, WorkflowStatus::Cancelled);
    }

    #[test]
    fn test_gather_strategy_variants() {
        let _ = GatherStrategy::WaitAll;
        let _ = GatherStrategy::FirstN(3);
        let _ = GatherStrategy::BestEffort { timeout_ms: 5000 };
        let _ = GatherStrategy::FirstSuccess;
        let _ = GatherStrategy::Consensus { threshold: 0.5 };
    }

    #[test]
    fn test_workflow_pattern_equality() {
        assert_eq!(WorkflowPattern::MapReduce, WorkflowPattern::MapReduce);
        assert_ne!(WorkflowPattern::MapReduce, WorkflowPattern::Saga);
    }

    #[test]
    fn test_step_status_variants() {
        assert_eq!(StepStatus::Pending, StepStatus::Pending);
        assert_ne!(StepStatus::Running, StepStatus::Completed);
    }

    #[test]
    fn test_workflow_builtins() {
        let builtins = workflow_builtins();
        assert!(builtins.len() >= 10);
        assert!(builtins.iter().any(|(name, _)| *name == "workflow_create"));
        assert!(builtins
            .iter()
            .any(|(name, _)| *name == "workflow_map_reduce"));
        assert!(builtins
            .iter()
            .any(|(name, _)| *name == "circuit_breaker_create"));
    }

    // =========================================================================
    // Engine execution
    //
    // Until these existed, every step type returned a placeholder: an Execute
    // step answered "Executed {fn} with {args}", an Http step answered a
    // hard-coded `status: 200` without making a request, and a parallel branch
    // answered "Step: {id}". Eighteen tests passed over all of it, because the
    // only one that called `execute()` used an empty pipeline and so never ran
    // a step.
    //
    // Each test below asserts on a value the engine could only produce by
    // actually doing the work.
    // =========================================================================

    fn exec_step(id: &str, function: &str, args: Vec<Value>) -> WorkflowStep {
        WorkflowStep {
            id: id.to_string(),
            name: id.to_string(),
            step_type: StepType::Execute {
                function: function.to_string(),
                args,
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        }
    }

    async fn run(engine: &WorkflowEngine, template: WorkflowTemplate) -> Result<Value> {
        let id = template.id.clone();
        engine.register_template(template).await;
        let inputs = HashMap::from([("input".to_string(), Value::Null)]);
        let workflow_id = engine.create_instance(&id, inputs).await?;
        engine.execute(&workflow_id).await
    }

    #[tokio::test]
    async fn an_execute_step_calls_the_real_builtin() {
        let engine = WorkflowEngine::new();
        let step = exec_step("upper", "upper", vec![Value::Str("hello".into())]);
        let template = WorkflowTemplateFactory::pipeline("Upper", "", vec![step]);

        let out = run(&engine, template).await.expect("pipeline runs");
        assert_eq!(
            out,
            Value::Str("HELLO".into()),
            "the step must call the builtin, not describe the call"
        );
    }

    #[tokio::test]
    async fn a_step_result_is_never_a_placeholder_string() {
        let engine = WorkflowEngine::new();
        let step = exec_step(
            "sum",
            "sum",
            vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
        );
        let template = WorkflowTemplateFactory::pipeline("Sum", "", vec![step]);

        let out = run(&engine, template).await.expect("pipeline runs");
        assert_eq!(out, Value::Int(3));
        if let Value::Str(s) = &out {
            assert!(
                !s.starts_with("Executed"),
                "the stubbed executor is back: {s}"
            );
        }
    }

    #[tokio::test]
    async fn a_failing_builtin_fails_the_workflow() {
        let engine = WorkflowEngine::new();
        let step = exec_step("nope", "definitely_not_a_builtin", vec![]);
        let template = WorkflowTemplateFactory::pipeline("Bad", "", vec![step]);

        let err = run(&engine, template).await.expect_err("must not succeed");
        assert!(
            err.to_string().contains("definitely_not_a_builtin"),
            "the builtin's own error must reach the caller, got: {err}"
        );
    }

    #[tokio::test]
    async fn output_mapping_feeds_the_next_step() {
        let engine = WorkflowEngine::new();

        let mut first = exec_step("first", "upper", vec![Value::Str("ab".into())]);
        first.output_mapping = Some("$.shouted".to_string());

        let mut second = exec_step("second", "len", vec![]);
        second.input_mapping = Some("$.shouted".to_string());

        let template = WorkflowTemplateFactory::pipeline("Chain", "", vec![first, second]);
        let out = run(&engine, template).await.expect("pipeline runs");

        assert_eq!(
            out,
            Value::Int(2),
            "the second step must receive the first step's output through the mapping"
        );
    }

    #[tokio::test]
    async fn a_false_condition_skips_the_step() {
        let engine = WorkflowEngine::new();

        let mut yes = exec_step("yes", "upper", vec![Value::Str("run".into())]);
        yes.condition = Some("1 == 1".to_string());
        let mut no = exec_step("no", "upper", vec![Value::Str("skipped".into())]);
        no.condition = Some("1 == 2".to_string());

        let template = WorkflowTemplateFactory::pipeline("Guarded", "", vec![yes, no]);
        let out = run(&engine, template).await.expect("pipeline runs");

        assert_eq!(
            out,
            Value::Null,
            "the guarded step must not run; a condition that always passed was \
             the old behaviour"
        );
    }

    #[tokio::test]
    async fn a_condition_reads_the_workflow_variables() {
        let engine = WorkflowEngine::new();
        let mut step = exec_step("guarded", "upper", vec![Value::Str("ok".into())]);
        step.condition = Some("threshold > 5".to_string());
        let template = WorkflowTemplateFactory::pipeline("Vars", "", vec![step]);
        let id = template.id.clone();
        engine.register_template(template).await;

        let over = engine
            .create_instance(
                &id,
                HashMap::from([
                    ("input".to_string(), Value::Null),
                    ("threshold".to_string(), Value::Int(10)),
                ]),
            )
            .await
            .unwrap();
        assert_eq!(
            engine.execute(&over).await.unwrap(),
            Value::Str("OK".into())
        );

        let under = engine
            .create_instance(
                &id,
                HashMap::from([
                    ("input".to_string(), Value::Null),
                    ("threshold".to_string(), Value::Int(1)),
                ]),
            )
            .await
            .unwrap();
        assert_eq!(engine.execute(&under).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn an_unparseable_condition_is_false_not_true() {
        let engine = WorkflowEngine::new();
        let ctx = WorkflowContext::default();
        assert!(
            !engine.evaluate_condition("this is ( not [ syntax", &ctx),
            "a guard that cannot be understood must not be treated as satisfied"
        );
        assert!(
            engine.evaluate_condition("", &ctx),
            "an empty condition is no condition at all"
        );
    }

    #[tokio::test]
    async fn a_branch_takes_the_arm_whose_condition_holds() {
        let engine = WorkflowEngine::new();
        let step = WorkflowStep {
            id: "branch".to_string(),
            name: "branch".to_string(),
            step_type: StepType::Branch {
                conditions: vec![
                    (
                        "1 == 2".to_string(),
                        exec_step("wrong", "upper", vec![Value::Str("wrong".into())]),
                    ),
                    (
                        "2 == 2".to_string(),
                        exec_step("right", "upper", vec![Value::Str("right".into())]),
                    ),
                ],
                default: Some(Box::new(exec_step(
                    "fallback",
                    "upper",
                    vec![Value::Str("fallback".into())],
                ))),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };

        let template = WorkflowTemplateFactory::pipeline("Branch", "", vec![step]);
        let out = run(&engine, template).await.expect("pipeline runs");
        assert_eq!(
            out,
            Value::Str("RIGHT".into()),
            "a branch must pick by condition, not always take the first arm"
        );
    }

    #[tokio::test]
    async fn parallel_branches_run_the_real_executor() {
        let engine = WorkflowEngine::new();
        let steps = vec![
            exec_step("a", "upper", vec![Value::Str("a".into())]),
            exec_step("b", "upper", vec![Value::Str("b".into())]),
            exec_step("c", "upper", vec![Value::Str("c".into())]),
        ];
        let template = WorkflowTemplateFactory::fan_out_fan_in(
            "Fan",
            "",
            steps,
            exec_step("agg", "len", vec![]),
        );

        let id = template.id.clone();
        engine.register_template(template).await;
        let workflow_id = engine
            .create_instance(&id, HashMap::from([("input".to_string(), Value::Null)]))
            .await
            .unwrap();
        engine.execute(&workflow_id).await.expect("fan-out runs");

        let ctx = engine.get_instance(&workflow_id).await.unwrap().context;
        for id in ["a", "b", "c"] {
            let output = ctx.results.get(id).and_then(|r| r.output.clone());
            assert_eq!(
                output,
                Some(Value::Str(id.to_uppercase())),
                "branch {id} did not reach the real executor"
            );
        }
    }

    #[tokio::test]
    async fn a_failing_parallel_branch_still_lets_the_others_finish() {
        let engine = WorkflowEngine::new();
        let steps = vec![
            exec_step("ok", "upper", vec![Value::Str("ok".into())]),
            exec_step("bad", "definitely_not_a_builtin", vec![]),
        ];
        let template = WorkflowTemplateFactory::fan_out_fan_in(
            "Fan",
            "",
            steps,
            exec_step("agg", "len", vec![]),
        );

        let id = template.id.clone();
        engine.register_template(template).await;
        let workflow_id = engine
            .create_instance(&id, HashMap::from([("input".to_string(), Value::Null)]))
            .await
            .unwrap();
        let result = engine.execute(&workflow_id).await;
        assert!(result.is_err(), "a failed branch must fail the workflow");

        let ctx = engine.get_instance(&workflow_id).await.unwrap().context;
        assert_eq!(
            ctx.results.get("ok").map(|r| r.status.clone()),
            Some(StepStatus::Completed),
            "the healthy branch must still be awaited and recorded"
        );
        assert_eq!(
            ctx.results.get("bad").map(|r| r.status.clone()),
            Some(StepStatus::Failed)
        );
    }

    #[tokio::test]
    async fn emitting_an_event_delivers_its_payload_to_a_waiting_step() {
        let engine = WorkflowEngine::new();
        let emit = WorkflowStep {
            id: "emit".to_string(),
            name: "emit".to_string(),
            step_type: StepType::EmitEvent {
                event_type: "ready".to_string(),
                payload: Value::Int(7),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };
        let wait = WorkflowStep {
            id: "wait".to_string(),
            name: "wait".to_string(),
            step_type: StepType::WaitForEvent {
                event_type: "ready".to_string(),
                timeout_ms: Some(2000),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };

        let template = WorkflowTemplateFactory::pipeline("Events", "", vec![emit, wait]);
        let out = run(&engine, template).await.expect("pipeline runs");
        assert_eq!(
            out,
            Value::Int(7),
            "the waiting step must receive the emitted payload"
        );
    }

    #[tokio::test]
    async fn waiting_for_an_event_nobody_emits_times_out() {
        let engine = WorkflowEngine::new();
        let wait = WorkflowStep {
            id: "wait".to_string(),
            name: "wait".to_string(),
            step_type: StepType::WaitForEvent {
                event_type: "never".to_string(),
                timeout_ms: Some(50),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };
        let template = WorkflowTemplateFactory::pipeline("Waiting", "", vec![wait]);
        let err = run(&engine, template).await.expect_err("must time out");
        assert!(
            err.to_string().contains("timed out"),
            "expected a timeout, got: {err}"
        );
    }

    #[tokio::test]
    async fn a_step_that_overruns_its_timeout_fails() {
        let engine = WorkflowEngine::new();
        let mut slow = WorkflowStep {
            id: "slow".to_string(),
            name: "slow".to_string(),
            step_type: StepType::Delay { duration_ms: 5000 },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: Some(50),
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };
        slow.retry_config = None;

        let template = WorkflowTemplateFactory::pipeline("Slow", "", vec![slow]);
        let started = Instant::now();
        let err = run(&engine, template).await.expect_err("must time out");
        assert!(
            err.to_string().contains("timeout"),
            "expected a step timeout, got: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the timeout did not actually cut the step short"
        );
    }

    #[tokio::test]
    async fn a_sub_workflow_runs_its_template() {
        let engine = WorkflowEngine::new();

        let child = WorkflowTemplateFactory::pipeline(
            "Child",
            "",
            vec![exec_step(
                "inner",
                "upper",
                vec![Value::Str("child".into())],
            )],
        );
        let child_id = child.id.clone();
        engine.register_template(child).await;

        let parent_step = WorkflowStep {
            id: "call-child".to_string(),
            name: "call-child".to_string(),
            step_type: StepType::SubWorkflow {
                template_id: child_id,
                inputs: HashMap::new(),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        };
        let parent = WorkflowTemplateFactory::pipeline("Parent", "", vec![parent_step]);

        let out = run(&engine, parent).await.expect("parent runs");
        assert_eq!(out, Value::Str("CHILD".into()));
    }

    #[tokio::test]
    async fn a_self_referential_sub_workflow_is_stopped_not_hung() {
        let engine = WorkflowEngine::new();

        // A template whose only step calls itself.
        let mut looping = WorkflowTemplateFactory::pipeline("Loop", "", vec![]);
        let looping_id = looping.id.clone();
        looping.steps = vec![WorkflowStep {
            id: "recurse".to_string(),
            name: "recurse".to_string(),
            step_type: StepType::SubWorkflow {
                template_id: looping_id.clone(),
                inputs: HashMap::new(),
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        }];
        engine.register_template(looping).await;

        let workflow_id = engine
            .create_instance(
                &looping_id,
                HashMap::from([("input".to_string(), Value::Null)]),
            )
            .await
            .unwrap();
        let err = engine
            .execute(&workflow_id)
            .await
            .expect_err("a cycle must be refused, not run until the stack ends");
        assert!(
            err.to_string().contains("nesting exceeded"),
            "expected the depth guard, got: {err}"
        );
    }

    #[tokio::test]
    async fn the_context_a_run_produced_survives_the_run() {
        let engine = WorkflowEngine::new();
        let mut step = exec_step("keep", "upper", vec![Value::Str("kept".into())]);
        step.output_mapping = Some("$.kept".to_string());
        let template = WorkflowTemplateFactory::pipeline("Keep", "", vec![step]);

        let id = template.id.clone();
        engine.register_template(template).await;
        let workflow_id = engine
            .create_instance(&id, HashMap::from([("input".to_string(), Value::Null)]))
            .await
            .unwrap();
        engine.execute(&workflow_id).await.unwrap();

        let info = engine.get_instance(&workflow_id).await.unwrap();
        assert_eq!(
            info.context.variables.get("kept"),
            Some(&Value::Str("KEPT".into())),
            "execute() worked on a clone and discarded it; the variables a run \
             produced must be readable afterwards"
        );
        assert!(
            info.context.results.contains_key("keep"),
            "per-step results were discarded too"
        );
    }

    #[tokio::test]
    async fn map_reduce_hands_the_mapped_results_to_the_reducer() {
        let engine = WorkflowEngine::new();

        let mapper = exec_step("map-one", "upper", vec![Value::Str("x".into())]);
        let reducer = exec_step("reduce", "len", vec![]);
        let template = WorkflowTemplateFactory::map_reduce("MR", "", mapper, reducer);

        let id = template.id.clone();
        engine.register_template(template).await;
        let workflow_id = engine
            .create_instance(
                &id,
                HashMap::from([(
                    "data".to_string(),
                    Value::Array(vec![Value::Int(1), Value::Int(2)]),
                )]),
            )
            .await
            .unwrap();

        engine.execute(&workflow_id).await.expect("map-reduce runs");
        let ctx = engine.get_instance(&workflow_id).await.unwrap().context;
        assert!(
            ctx.variables.contains_key("mapped_results"),
            "the map phase must publish through its output_mapping"
        );
        assert!(
            ctx.variables.contains_key("result"),
            "the reduce phase must publish through its output_mapping"
        );
    }

    #[test]
    fn path_resolution_reads_and_writes_the_variables() {
        let mut ctx = WorkflowContext::default();
        ctx.variables.insert(
            "outer".to_string(),
            Value::Record(BTreeMap::from([("inner".to_string(), Value::Int(4))])),
        );

        assert_eq!(resolve_path(&ctx, "$.outer.inner"), Some(Value::Int(4)));
        assert_eq!(resolve_path(&ctx, "$.missing"), None);
        assert!(matches!(resolve_path(&ctx, "$"), Some(Value::Record(_))));

        store_path(&mut ctx, "$.written", Value::Int(9));
        assert_eq!(ctx.variables.get("written"), Some(&Value::Int(9)));
    }

    #[test]
    fn truthiness_matches_the_interpreter() {
        assert!(!truthy(&Value::Null));
        assert!(!truthy(&Value::Bool(false)));
        assert!(!truthy(&Value::Int(0)));
        assert!(!truthy(&Value::Str(String::new())));
        assert!(!truthy(&Value::Array(vec![])));
        assert!(truthy(&Value::Bool(true)));
        assert!(truthy(&Value::Int(1)));
        assert!(truthy(&Value::Str("x".into())));
        assert!(truthy(&Value::Array(vec![Value::Null])));
    }
    fn http_step_with(method: &str, url: &str) -> WorkflowStep {
        WorkflowStep {
            id: "http".to_string(),
            name: "http".to_string(),
            step_type: StepType::Http {
                method: method.to_string(),
                url: url.to_string(),
                body: None,
            },
            input_mapping: None,
            output_mapping: None,
            retry_config: None,
            timeout_ms: None,
            condition: None,
            compensate: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn an_http_step_refuses_a_method_it_cannot_issue() {
        let engine = WorkflowEngine::new();
        let template = WorkflowTemplateFactory::pipeline(
            "Http",
            "",
            vec![http_step_with("TRACE", "https://example.com/")],
        );
        let err = run(&engine, template).await.expect_err("must be refused");
        assert!(
            err.to_string().contains("unsupported method"),
            "expected the method to be named, got: {err}"
        );
    }

    #[tokio::test]
    async fn an_http_step_goes_through_the_network_guard() {
        // A URL beginning with `-` is what `guard_network` exists to reject:
        // it lands in a positional slot and is read as an option. If the step
        // ever stops calling the guard, this URL reaches the client instead and
        // the error changes.
        let engine = WorkflowEngine::new();
        let template =
            WorkflowTemplateFactory::pipeline("Http", "", vec![http_step_with("GET", "-oevil")]);
        let err = run(&engine, template).await.expect_err("must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("option") || msg.contains("E_OPTION") || msg.contains("-oevil"),
            "the network guard did not run on this path: {msg}"
        );
    }

    #[tokio::test]
    async fn a_saga_compensates_the_steps_that_already_ran() {
        let engine = WorkflowEngine::new();

        let charge = exec_step("charge", "upper", vec![Value::Str("charged".into())]);
        let mut refund = exec_step("refund", "upper", vec![Value::Str("refunded".into())]);
        refund.output_mapping = Some("$.compensated".to_string());

        let ship = exec_step("ship", "definitely_not_a_builtin", vec![]);
        let unship = exec_step("unship", "upper", vec![Value::Str("unshipped".into())]);

        // saga() takes (action, compensation) pairs and rewrites the ids.
        let template =
            WorkflowTemplateFactory::saga("Order", "", vec![(charge, refund), (ship, unship)]);
        let id = template.id.clone();
        engine.register_template(template).await;
        let workflow_id = engine
            .create_instance(&id, HashMap::from([("input".to_string(), Value::Null)]))
            .await
            .unwrap();

        let result = engine.execute(&workflow_id).await;
        assert!(result.is_err(), "the saga must fail when a step fails");

        let ctx = engine.get_instance(&workflow_id).await.unwrap().context;
        assert_eq!(
            ctx.variables.get("compensated"),
            Some(&Value::Str("REFUNDED".into())),
            "the compensation for the completed step must actually have run; \
             before the executor was implemented it returned a string and did \
             nothing"
        );
    }
}
