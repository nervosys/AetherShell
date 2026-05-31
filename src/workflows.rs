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

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

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
}

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
// Workflow Engine
// =============================================================================

/// Workflow execution engine
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
            WorkflowPattern::Parallel | WorkflowPattern::FanOutFanIn => {
                self.execute_parallel(&template.steps, &mut instance.context)
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
            WorkflowPattern::ScatterGather => {
                self.execute_parallel(&template.steps, &mut instance.context)
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
                inst.context.ended_at = Some(current_timestamp());
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

    async fn execute_parallel(
        &self,
        steps: &[WorkflowStep],
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        let mut handles = Vec::new();
        let context_arc = Arc::new(RwLock::new(context.clone()));

        for step in steps {
            let step_clone = step.clone();
            let _ctx_clone = Arc::clone(&context_arc);
            let event_tx = self.event_tx.clone();
            let workflow_id = context.workflow_id.clone();

            let handle = tokio::spawn(async move {
                let _ = event_tx.send(WorkflowEvent::StepStarted {
                    workflow_id: workflow_id.clone(),
                    step_id: step_clone.id.clone(),
                });

                let start = Instant::now();

                // Simplified execution for parallel steps
                let result: Result<Value> = match &step_clone.step_type {
                    StepType::Execute { function, args } => Ok(Value::Str(format!(
                        "Executed: {} with {:?}",
                        function, args
                    ))),
                    StepType::Delay { duration_ms } => {
                        tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                        Ok(Value::Null)
                    }
                    _ => Ok(Value::Str(format!("Step: {}", step_clone.id))),
                };

                let duration = start.elapsed();
                (step_clone.id, result, duration)
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok((step_id, Ok(value), duration)) => {
                    let step_result = StepResult {
                        step_id: step_id.clone(),
                        status: StepStatus::Completed,
                        output: Some(value.clone()),
                        error: None,
                        duration_ms: duration.as_millis() as u64,
                        retries: 0,
                    };
                    context.results.insert(step_id, step_result);
                    results.push(value);
                }
                Ok((step_id, Err(e), _)) => {
                    return Err(anyhow!("Step {} failed: {}", step_id, e));
                }
                Err(e) => {
                    return Err(anyhow!("Task panicked: {}", e));
                }
            }
        }

        Ok(Value::Array(results))
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

            // Apply retry logic if configured
            let mut retries = 0;
            let max_retries = step
                .retry_config
                .as_ref()
                .map(|c| c.max_retries)
                .unwrap_or(0);

            loop {
                let result = self.execute_step_type(&step.step_type, context).await;

                match result {
                    Ok(value) => return Ok(value),
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

    async fn execute_step_type(
        &self,
        step_type: &StepType,
        context: &mut WorkflowContext,
    ) -> Result<Value> {
        match step_type {
            StepType::Execute { function, args } => {
                // In a real implementation, this would call the actual function
                Ok(Value::Str(format!("Executed {} with {:?}", function, args)))
            }
            StepType::Agent { agent_id, prompt } => {
                // Would call the agent API
                Ok(Value::Str(format!(
                    "Agent {} responded to: {}",
                    agent_id, prompt
                )))
            }
            StepType::Http {
                method,
                url,
                body: _,
            } => {
                // Would make HTTP request
                Ok(Value::Record(BTreeMap::from([
                    ("method".to_string(), Value::Str(method.clone())),
                    ("status".to_string(), Value::Int(200)),
                    ("url".to_string(), Value::Str(url.clone())),
                ])))
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
                timeout_ms: _,
            } => {
                // Would wait for an event
                Ok(Value::Str(format!("Received event: {}", event_type)))
            }
            StepType::EmitEvent {
                event_type,
                payload: _,
            } => {
                // Would emit an event
                Ok(Value::Record(BTreeMap::from([
                    ("emitted".to_string(), Value::Str("true".to_string())),
                    ("event".to_string(), Value::Str(event_type.clone())),
                ])))
            }
            StepType::Delay { duration_ms } => {
                tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                Ok(Value::Null)
            }
            StepType::SubWorkflow {
                template_id,
                inputs: _,
            } => {
                // Would execute sub-workflow
                Ok(Value::Str(format!(
                    "Sub-workflow {} completed",
                    template_id
                )))
            }
        }
    }

    fn evaluate_condition(&self, _condition: &str, _context: &WorkflowContext) -> bool {
        // Simplified condition evaluation
        // In a real implementation, would parse and evaluate the condition
        true
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
}
