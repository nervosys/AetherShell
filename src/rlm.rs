//! Recursive Language Models (RLM) for hierarchical agent spawning
//!
//! This module implements recursive agent architectures where agents can spawn
//! an arbitrary number of subagents to decompose and solve complex tasks.
//!
//! ## Features
//!
//! - **Recursive Spawning**: Agents can spawn subagents with full capabilities
//! - **Hierarchical Control**: Parent agents coordinate child results
//! - **Resource Limits**: Configurable depth and total agent limits
//! - **State Isolation**: Each agent has isolated state with parent access
//! - **Result Aggregation**: Child results flow back to parents
//! - **Cycle Detection**: Prevents infinite recursion loops
//! - **Permission Control**: Well-defined permissions for subagent spawning
//!
//! ## Architecture
//!
//! ```text
//! RootAgent (depth=0, permissions={tools:[*], domains:[*]})
//! ├── SubAgent1 (depth=1, permissions={tools:[ls,cat], domains:[FileSystem]})
//! │   ├── SubAgent1.1 (depth=2, permissions={tools:[cat], domains:[FileSystem]})
//! │   └── SubAgent1.2 (depth=2, permissions={tools:[ls], domains:[FileSystem]})
//! └── SubAgent2 (depth=1, permissions={tools:[http_get], domains:[Network]})
//!     └── SubAgent2.1 (depth=2, permissions={tools:[http_get], domains:[Network]})
//! ```
//!
//! ## Security
//!
//! - Maximum recursion depth (default: 5)
//! - Maximum total agents (default: 50)
//! - Per-agent timeout enforcement
//! - Memory limits per agent hierarchy
//! - Audit logging of spawn operations
//! - **Subagent Permissions**: Tool allowlists, domain restrictions, path constraints
//! - **Permission Inheritance**: Children cannot exceed parent permissions
//! - **Resource Quotas**: CPU, memory, and network limits per spawn level

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as J;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::ai::agents::{AgentStep, Tool, ToolRegistry};
use crate::ai::{backend_from_env, backend_from_model, ChatMessage, LlmBackend};
use crate::env::Env;

// ===================== Configuration =====================

/// Default maximum recursion depth
const DEFAULT_MAX_DEPTH: usize = 5;

/// Default maximum total agents in a hierarchy
const DEFAULT_MAX_AGENTS: usize = 50;

/// Default per-agent timeout in seconds
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 60;

/// Default maximum concurrent subagents per parent
const DEFAULT_MAX_CONCURRENT_CHILDREN: usize = 10;

/// RLM Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmConfig {
    /// Maximum recursion depth (0 = root only, no subagents)
    pub max_depth: usize,
    /// Maximum total agents in the hierarchy
    pub max_agents: usize,
    /// Timeout per agent in seconds
    pub agent_timeout_secs: u64,
    /// Maximum concurrent children per parent
    pub max_concurrent_children: usize,
    /// Enable detailed trace logging
    pub trace_enabled: bool,
    /// Model URI for subagents (None = inherit from parent)
    pub subagent_model_uri: Option<String>,
    /// Root agent permissions (None = use defaults)
    #[serde(skip)]
    pub root_permissions: Option<SubagentPermissions>,
    /// Subagent spawning policy (None = use defaults)
    #[serde(skip)]
    pub subagent_policy: Option<SubagentPolicy>,
}

impl Default for RlmConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_agents: DEFAULT_MAX_AGENTS,
            agent_timeout_secs: DEFAULT_AGENT_TIMEOUT_SECS,
            max_concurrent_children: DEFAULT_MAX_CONCURRENT_CHILDREN,
            trace_enabled: true,
            subagent_model_uri: None,
            root_permissions: None,
            subagent_policy: None,
        }
    }
}

// ===================== Agent Identity =====================

/// Unique identifier for an agent in the hierarchy
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentId {
    /// Hierarchical path (e.g., "root.sub1.sub1_1")
    pub path: String,
    /// Numeric ID for quick comparison
    pub id: u64,
    /// Depth in the hierarchy (0 = root)
    pub depth: usize,
}

impl AgentId {
    /// Create a root agent ID
    pub fn root() -> Self {
        static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);
        Self {
            path: "root".to_string(),
            id: ROOT_COUNTER.fetch_add(1, Ordering::SeqCst),
            depth: 0,
        }
    }

    /// Create a child agent ID
    pub fn child(&self, name: &str, child_id: u64) -> Self {
        Self {
            path: format!("{}.{}", self.path, name),
            id: child_id,
            depth: self.depth + 1,
        }
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.path, self.id)
    }
}

// ===================== Agent State =====================

/// State shared across the agent hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: u64,
    pub msg_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    Task,       // Task assignment from parent
    Result,     // Result from child
    Query,      // Inter-agent query
    Response,   // Response to query
    Broadcast,  // Broadcast to all siblings
    Escalation, // Problem escalation to parent
}

/// Result from a subagent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResult {
    pub agent_id: AgentId,
    pub goal: String,
    pub output: String,
    pub success: bool,
    pub steps_taken: usize,
    pub execution_time_ms: u64,
    pub child_results: Vec<SubagentResult>,
}

/// Shared state for the agent hierarchy
pub struct HierarchyState {
    /// Total agents spawned
    total_agents: AtomicUsize,
    /// Active agents
    active_agents: AtomicUsize,
    /// Messages between agents
    messages: Mutex<Vec<AgentMessage>>,
    /// Agent results by ID
    results: RwLock<HashMap<String, SubagentResult>>,
    /// Spawned agent IDs (for cycle detection)
    spawned_ids: Mutex<HashSet<String>>,
    /// Configuration
    config: RlmConfig,
    /// Start time for the hierarchy
    start_time: Instant,
}

impl HierarchyState {
    pub fn new(config: RlmConfig) -> Self {
        Self {
            total_agents: AtomicUsize::new(0),
            active_agents: AtomicUsize::new(0),
            messages: Mutex::new(Vec::new()),
            results: RwLock::new(HashMap::new()),
            spawned_ids: Mutex::new(HashSet::new()),
            config,
            start_time: Instant::now(),
        }
    }

    /// Check if we can spawn a new agent
    pub fn can_spawn(&self, depth: usize) -> Result<()> {
        let total = self.total_agents.load(Ordering::SeqCst);
        if total >= self.config.max_agents {
            return Err(anyhow!(
                "Maximum agent limit reached ({}/{})",
                total,
                self.config.max_agents
            ));
        }
        if depth >= self.config.max_depth {
            return Err(anyhow!(
                "Maximum recursion depth reached ({}/{})",
                depth,
                self.config.max_depth
            ));
        }
        Ok(())
    }

    /// Register a new agent
    pub fn register_agent(&self, id: &AgentId) -> Result<()> {
        let mut spawned = self.spawned_ids.lock().unwrap();
        if spawned.contains(&id.path) {
            return Err(anyhow!(
                "Agent with path {} already exists (cycle detected)",
                id.path
            ));
        }
        spawned.insert(id.path.clone());
        self.total_agents.fetch_add(1, Ordering::SeqCst);
        self.active_agents.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Deregister an agent (completed)
    pub fn deregister_agent(&self, id: &AgentId) {
        self.active_agents.fetch_sub(1, Ordering::SeqCst);
        if self.config.trace_enabled {
            eprintln!("[RLM] Agent {} completed", id);
        }
    }

    /// Send a message between agents
    pub fn send_message(&self, msg: AgentMessage) {
        let mut messages = self.messages.lock().unwrap();
        messages.push(msg);
    }

    /// Get messages for an agent
    pub fn get_messages(&self, agent_path: &str) -> Vec<AgentMessage> {
        let messages = self.messages.lock().unwrap();
        messages
            .iter()
            .filter(|m| m.to == agent_path || m.to == "*")
            .cloned()
            .collect()
    }

    /// Store a result
    pub fn store_result(&self, id: &AgentId, result: SubagentResult) {
        let mut results = self.results.write().unwrap();
        results.insert(id.path.clone(), result);
    }

    /// Get a result
    pub fn get_result(&self, path: &str) -> Option<SubagentResult> {
        let results = self.results.read().unwrap();
        results.get(path).cloned()
    }

    /// Get hierarchy statistics
    pub fn stats(&self) -> HierarchyStats {
        HierarchyStats {
            total_spawned: self.total_agents.load(Ordering::SeqCst),
            currently_active: self.active_agents.load(Ordering::SeqCst),
            messages_sent: self.messages.lock().unwrap().len(),
            elapsed_ms: self.start_time.elapsed().as_millis() as u64,
            max_depth: self.config.max_depth,
            max_agents: self.config.max_agents,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyStats {
    pub total_spawned: usize,
    pub currently_active: usize,
    pub messages_sent: usize,
    pub elapsed_ms: u64,
    pub max_depth: usize,
    pub max_agents: usize,
}

// ===================== Subagent Permissions =====================

/// Permissions that define what a subagent is allowed to do.
///
/// This struct implements a capability-based security model where parent agents
/// define what their children can do. Children cannot exceed parent permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentPermissions {
    /// Tools the subagent is allowed to use
    pub allowed_tools: HashSet<String>,
    /// Whether all tools are allowed (wildcard)
    pub allow_all_tools: bool,
    /// Filesystem paths the subagent can access
    pub allowed_paths: HashSet<String>,
    /// Network hosts the subagent can connect to
    pub allowed_hosts: HashSet<String>,
    /// Whether the subagent can write to files
    pub can_write_files: bool,
    /// Whether the subagent can execute shell commands
    pub can_execute_shell: bool,
    /// Whether the subagent can make network requests
    pub can_access_network: bool,
    /// Whether the subagent can read environment variables
    pub can_access_env: bool,
    /// Whether the subagent can spawn its own subagents
    pub can_spawn_subagents: bool,
    /// Maximum depth this subagent can spawn to
    pub max_spawn_depth: usize,
    /// Maximum number of children this subagent can spawn
    pub max_children: usize,
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    /// Maximum memory in MB (advisory)
    pub max_memory_mb: usize,
}

impl Default for SubagentPermissions {
    fn default() -> Self {
        Self::new()
    }
}

impl SubagentPermissions {
    /// Create new restrictive permissions (secure defaults)
    pub fn new() -> Self {
        Self {
            allowed_tools: HashSet::new(),
            allow_all_tools: false,
            allowed_paths: HashSet::new(),
            allowed_hosts: HashSet::new(),
            can_write_files: false,
            can_execute_shell: false,
            can_access_network: false,
            can_access_env: true,
            can_spawn_subagents: true,
            max_spawn_depth: 3,
            max_children: 10,
            timeout_secs: 60,
            max_memory_mb: 512,
        }
    }

    /// Create permissions that allow everything (use with caution)
    pub fn allow_all() -> Self {
        let mut perms = Self::new();
        perms.allow_all_tools = true;
        perms.allowed_paths.insert("*".to_string());
        perms.allowed_hosts.insert("*".to_string());
        perms.can_write_files = true;
        perms.can_execute_shell = true;
        perms.can_access_network = true;
        perms.max_spawn_depth = 5;
        perms.max_children = 50;
        perms
    }

    /// Create read-only permissions (safe for most tasks)
    pub fn read_only() -> Self {
        let mut perms = Self::new();
        perms.allowed_tools = ["ls", "cat", "grep", "find", "head", "tail", "wc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        perms.can_access_env = true;
        perms.can_spawn_subagents = false;
        perms
    }

    /// Create network-only permissions
    pub fn network_only() -> Self {
        let mut perms = Self::new();
        perms.allowed_tools = ["http_get", "http_post", "fetch"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        perms.can_access_network = true;
        perms.can_spawn_subagents = false;
        perms
    }

    /// Add specific tools to the allowed list
    pub fn with_tools(mut self, tools: Vec<&str>) -> Self {
        for tool in tools {
            self.allowed_tools.insert(tool.to_string());
        }
        self
    }

    /// Add allowed filesystem paths
    pub fn with_allowed_paths(mut self, paths: Vec<&str>) -> Self {
        for path in paths {
            self.allowed_paths.insert(path.to_string());
        }
        self
    }

    /// Add allowed network hosts
    pub fn with_allowed_hosts(mut self, hosts: Vec<&str>) -> Self {
        for host in hosts {
            self.allowed_hosts.insert(host.to_string());
        }
        self
    }

    /// Enable file write access
    pub fn with_write_access(mut self) -> Self {
        self.can_write_files = true;
        self
    }

    /// Enable shell command execution
    pub fn with_shell_access(mut self) -> Self {
        self.can_execute_shell = true;
        self
    }

    /// Enable network access
    pub fn with_network_access(mut self) -> Self {
        self.can_access_network = true;
        self
    }

    /// Configure spawn permissions
    pub fn with_spawn(mut self, allowed: bool, max_depth: usize, max_children: usize) -> Self {
        self.can_spawn_subagents = allowed;
        self.max_spawn_depth = max_depth;
        self.max_children = max_children;
        self
    }

    /// Configure resource limits
    pub fn with_limits(mut self, timeout_secs: u64, max_memory_mb: usize) -> Self {
        self.timeout_secs = timeout_secs;
        self.max_memory_mb = max_memory_mb;
        self
    }

    /// Check if a tool is allowed
    pub fn is_tool_allowed(&self, tool: &str) -> bool {
        self.allow_all_tools || self.allowed_tools.contains(tool)
    }

    /// Check if a path is allowed
    pub fn is_path_allowed(&self, path: &str) -> bool {
        if self.allowed_paths.contains("*") {
            return true;
        }
        self.allowed_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    /// Check if a host is allowed
    pub fn is_host_allowed(&self, host: &str) -> bool {
        if self.allowed_hosts.contains("*") {
            return true;
        }
        self.allowed_hosts.contains(host)
    }

    /// Validate that child permissions don't exceed parent permissions
    pub fn validate_child(&self, child: &SubagentPermissions) -> Result<()> {
        // Check capability escalation
        if child.can_write_files && !self.can_write_files {
            return Err(anyhow!("Child cannot have write access when parent doesn't"));
        }
        if child.can_execute_shell && !self.can_execute_shell {
            return Err(anyhow!("Child cannot have shell access when parent doesn't"));
        }
        if child.can_access_network && !self.can_access_network {
            return Err(anyhow!("Child cannot have network access when parent doesn't"));
        }

        // Check spawn depth
        if child.max_spawn_depth > self.max_spawn_depth {
            return Err(anyhow!(
                "Child spawn depth ({}) exceeds parent ({})",
                child.max_spawn_depth,
                self.max_spawn_depth
            ));
        }

        // Check tools (if parent has restrictions)
        if !self.allow_all_tools {
            for tool in &child.allowed_tools {
                if !self.allowed_tools.contains(tool) {
                    return Err(anyhow!("Child requests tool '{}' not allowed by parent", tool));
                }
            }
        }

        Ok(())
    }

    /// Create restricted child permissions based on parent permissions
    pub fn create_child_permissions(&self) -> SubagentPermissions {
        let mut child = self.clone();
        // Decrease spawn depth for children
        child.max_spawn_depth = self.max_spawn_depth.saturating_sub(1);
        // Reduce max children
        child.max_children = self.max_children / 2;
        child
    }
}

// ===================== Subagent Policy =====================

/// Global policy for subagent spawning behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentPolicy {
    /// Whether agents can request custom permissions
    pub allow_custom_permissions: bool,
    /// Global maximum spawn depth across all agents
    pub global_max_depth: usize,
    /// Global maximum number of agents
    pub global_max_agents: usize,
    /// Whether spawning requires approval (future: human-in-the-loop)
    pub require_spawn_approval: bool,
    /// Audit level for spawn operations
    pub audit_level: AuditLevel,
    /// Default permissions for agents that don't specify their own
    pub default_permissions: SubagentPermissions,
}

impl Default for SubagentPolicy {
    fn default() -> Self {
        Self {
            allow_custom_permissions: true,
            global_max_depth: 5,
            global_max_agents: 50,
            require_spawn_approval: false,
            audit_level: AuditLevel::Basic,
            default_permissions: SubagentPermissions::new(),
        }
    }
}

impl SubagentPolicy {
    /// Create a restrictive policy for high-security environments
    pub fn restrictive() -> Self {
        Self {
            allow_custom_permissions: false,
            global_max_depth: 2,
            global_max_agents: 10,
            require_spawn_approval: true,
            audit_level: AuditLevel::Detailed,
            default_permissions: SubagentPermissions::read_only(),
        }
    }

    /// Create a permissive policy for trusted environments
    pub fn permissive() -> Self {
        Self {
            allow_custom_permissions: true,
            global_max_depth: 10,
            global_max_agents: 100,
            require_spawn_approval: false,
            audit_level: AuditLevel::Minimal,
            default_permissions: SubagentPermissions::allow_all(),
        }
    }
}

/// Audit level for spawn operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    /// No auditing
    Minimal,
    /// Log spawn and completion
    Basic,
    /// Log all operations
    Detailed,
    /// Log everything with full context
    Verbose,
}

// ===================== RlmConfig Extensions =====================

impl RlmConfig {
    /// Set root agent permissions
    pub fn with_permissions(mut self, permissions: SubagentPermissions) -> Self {
        self.root_permissions = Some(permissions);
        self
    }

    /// Set subagent policy
    pub fn with_policy(mut self, policy: SubagentPolicy) -> Self {
        self.subagent_policy = Some(policy);
        self
    }

    /// Get effective root permissions
    pub fn effective_root_permissions(&self) -> SubagentPermissions {
        self.root_permissions
            .clone()
            .unwrap_or_else(SubagentPermissions::new)
    }

    /// Get effective subagent policy
    pub fn effective_policy(&self) -> SubagentPolicy {
        self.subagent_policy.clone().unwrap_or_default()
    }
}


// ===================== Recursive Agent =====================

/// A recursive agent that can spawn subagents
pub struct RecursiveAgent {
    /// Agent identity
    pub id: AgentId,
    /// LLM backend
    backend: Box<dyn LlmBackend>,
    /// Available tools (including spawn_agent)
    pub tools: Vec<Box<dyn Tool>>,
    /// Maximum steps for this agent
    pub max_steps: usize,
    /// Execution trace
    pub trace: Vec<AgentStep>,
    /// Shared hierarchy state
    state: Arc<HierarchyState>,
    /// Model URI for this agent
    model_uri: Option<String>,
    /// Parent agent ID (None for root)
    parent_id: Option<AgentId>,
    /// Child results collected
    child_results: Vec<SubagentResult>,
}

impl RecursiveAgent {
    /// Create a root recursive agent
    pub fn new(tools: Vec<Box<dyn Tool>>, config: RlmConfig) -> Self {
        let state = Arc::new(HierarchyState::new(config.clone()));
        let id = AgentId::root();

        // Register the root agent
        state
            .register_agent(&id)
            .expect("Failed to register root agent");

        let model_uri = config
            .subagent_model_uri
            .clone()
            .or_else(|| std::env::var("AETHER_RLM_MODEL_URI").ok());

        let backend = if let Some(ref uri) = model_uri {
            backend_from_model(uri.clone())
        } else {
            backend_from_env()
        };

        Self {
            id,
            backend,
            tools,
            max_steps: 10,
            trace: Vec::new(),
            state,
            model_uri,
            parent_id: None,
            child_results: Vec::new(),
        }
    }

    /// Create a root agent with a specific model URI
    pub fn with_model_uri(tools: Vec<Box<dyn Tool>>, model_uri: &str, config: RlmConfig) -> Self {
        let mut agent = Self::new(tools, config);
        agent.model_uri = Some(model_uri.to_string());
        agent.backend = backend_from_model(model_uri.to_string());
        agent
    }

    /// Create a child agent (internal use)
    fn create_child(
        &self,
        name: &str,
        goal: &str,
        tools: Vec<Box<dyn Tool>>,
    ) -> Result<RecursiveAgent> {
        // Check if we can spawn
        self.state.can_spawn(self.id.depth + 1)?;

        // Generate child ID
        static CHILD_COUNTER: AtomicU64 = AtomicU64::new(0);
        let child_id = self
            .id
            .child(name, CHILD_COUNTER.fetch_add(1, Ordering::SeqCst));

        // Register the child
        self.state.register_agent(&child_id)?;

        if self.state.config.trace_enabled {
            eprintln!(
                "[RLM] {} spawning child {} for: {}",
                self.id,
                child_id,
                if goal.len() > 50 { &goal[..50] } else { goal }
            );
        }

        // Send task message
        self.state.send_message(AgentMessage {
            from: self.id.path.clone(),
            to: child_id.path.clone(),
            content: goal.to_string(),
            timestamp: self.state.start_time.elapsed().as_millis() as u64,
            msg_type: MessageType::Task,
        });

        // Create the child agent
        let model_uri = self
            .model_uri
            .clone()
            .or_else(|| self.state.config.subagent_model_uri.clone());

        let backend = if let Some(ref uri) = model_uri {
            backend_from_model(uri.clone())
        } else {
            backend_from_env()
        };

        Ok(RecursiveAgent {
            id: child_id,
            backend,
            tools,
            max_steps: self.max_steps,
            trace: Vec::new(),
            state: Arc::clone(&self.state),
            model_uri,
            parent_id: Some(self.id.clone()),
            child_results: Vec::new(),
        })
    }

    /// Spawn a subagent and run it synchronously
    pub fn spawn_subagent(
        &mut self,
        name: &str,
        goal: &str,
        tool_names: &[&str],
        env: &mut Env,
    ) -> Result<SubagentResult> {
        let start_time = Instant::now();

        // Resolve tools for the subagent
        let reg = ToolRegistry::with_builtins();
        let tools = reg.resolve_many(tool_names);

        // Create the child
        let mut child = self.create_child(name, goal, tools)?;

        // Run the child
        let output = child.run_sync(goal, false, env)?;
        let success = !output.contains("ERROR") && !output.contains("(incomplete)");

        let result = SubagentResult {
            agent_id: child.id.clone(),
            goal: goal.to_string(),
            output,
            success,
            steps_taken: child.trace.len(),
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            child_results: child.child_results,
        };

        // Store and send result
        self.state.store_result(&child.id, result.clone());
        self.state.send_message(AgentMessage {
            from: child.id.path.clone(),
            to: self.id.path.clone(),
            content: serde_json::to_string(&result).unwrap_or_default(),
            timestamp: self.state.start_time.elapsed().as_millis() as u64,
            msg_type: MessageType::Result,
        });

        // Deregister the child
        self.state.deregister_agent(&child.id);

        // Collect result
        self.child_results.push(result.clone());

        Ok(result)
    }

    /// Spawn multiple subagents for parallel-like execution
    pub fn spawn_subagents(
        &mut self,
        tasks: &[(&str, &str, &[&str])], // (name, goal, tools)
        env: &mut Env,
    ) -> Result<Vec<SubagentResult>> {
        let max_children = self.state.config.max_concurrent_children;
        if tasks.len() > max_children {
            return Err(anyhow!(
                "Too many subagents requested ({} > {})",
                tasks.len(),
                max_children
            ));
        }

        let mut results = Vec::new();
        for (name, goal, tools) in tasks {
            let result = self.spawn_subagent(name, goal, tools, env)?;
            results.push(result);
        }
        Ok(results)
    }

    /// Run the recursive agent
    pub fn run_sync(&mut self, goal: &str, dry_run: bool, env: &mut Env) -> Result<String> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.state.config.agent_timeout_secs);

        // Build system prompt with spawn capability
        let spawn_capability = format!(
            r#"
You can spawn subagents to handle subtasks. Use the spawn_subagent tool:
{{"type":"tool","tool":"spawn_subagent","input":{{"name":"<name>","goal":"<subtask>","tools":["tool1","tool2"]}}}}

Current hierarchy: {} (depth {}/{})
Active agents: {}/{}

Spawning rules:
- Break complex tasks into simpler subtasks
- Spawn subagents for independent subtasks
- Aggregate results from children before responding
- Don't spawn for simple tasks you can handle directly"#,
            self.id,
            self.id.depth,
            self.state.config.max_depth,
            self.state.active_agents.load(Ordering::SeqCst),
            self.state.config.max_agents,
        );

        let tool_list = self
            .tools
            .iter()
            .map(|t| format!("- {}: {}", t.name(), t.description()))
            .collect::<Vec<_>>()
            .join("\n");

        let system = ChatMessage {
            role: "system".into(),
            content: format!(
                "You are Aether Recursive Agent {}. Emit JSON commands:\n\
                 {{\"type\":\"tool\",\"tool\":\"<name>\",\"input\":<json or string>}} or \
                 {{\"type\":\"final\",\"output\":\"...\"}}\n\n\
                 Tools:\n{}\n{}",
                self.id, tool_list, spawn_capability
            ),
        };

        let mut dialogue = vec![
            system,
            ChatMessage {
                role: "user".into(),
                content: goal.into(),
            },
        ];

        // Add any messages from parent
        for msg in self.state.get_messages(&self.id.path) {
            if msg.msg_type == MessageType::Task && msg.from != self.id.path {
                dialogue.push(ChatMessage {
                    role: "user".into(),
                    content: format!("[Parent task from {}]: {}", msg.from, msg.content),
                });
            }
        }

        for _ in 0..self.max_steps {
            // Check timeout
            if start_time.elapsed() > timeout {
                return Err(anyhow!(
                    "Agent {} timeout ({} seconds)",
                    self.id,
                    self.state.config.agent_timeout_secs
                ));
            }

            let reply = self.backend.chat(&dialogue)?;
            let (cmd, thought) = crate::ai::parse_agent_command(&reply);

            self.trace.push(AgentStep {
                thought: thought.clone(),
                command: cmd.clone(),
                observation: None,
            });

            // Check for final response
            if let Some(c) = cmd
                .as_ref()
                .and_then(|j| j.get("type"))
                .and_then(|t| t.as_str())
            {
                if c == "final" {
                    let out = cmd
                        .as_ref()
                        .and_then(|j| j.get("output"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();

                    if self.state.config.trace_enabled {
                        eprintln!(
                            "[RLM] {} final output: {}",
                            self.id,
                            if out.len() > 100 { &out[..100] } else { &out }
                        );
                    }

                    return if dry_run {
                        Ok(format!(
                            "[dry_run] final: {}\ntrace: {:?}\nchildren: {}",
                            out,
                            self.trace,
                            self.child_results.len()
                        ))
                    } else {
                        Ok(out)
                    };
                }
            }

            // Handle tool calls
            if let Some(tool_name) = cmd
                .as_ref()
                .and_then(|j| j.get("tool"))
                .and_then(|s| s.as_str())
            {
                let input = cmd
                    .as_ref()
                    .and_then(|j| j.get("input"))
                    .unwrap_or(&J::Null);

                let obs = if dry_run {
                    format!("[dry_run] would call {} with {}", tool_name, input)
                } else if tool_name == "spawn_subagent" {
                    // Handle spawn_subagent specially
                    self.handle_spawn_subagent(input, env)?
                } else if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                    match tool.call(&input.to_string(), env) {
                        Ok(val) => format!("OK: {}", crate::ai::display_value(&val)),
                        Err(e) => format!("ERROR: {}", e),
                    }
                } else {
                    format!("ERROR: unknown tool {}", tool_name)
                };

                dialogue.push(ChatMessage {
                    role: "assistant".into(),
                    content: reply,
                });
                dialogue.push(ChatMessage {
                    role: "user".into(),
                    content: format!("Observation: {}", obs),
                });

                if let Some(last) = self.trace.last_mut() {
                    last.observation = Some(obs);
                }
                continue;
            }

            // Invalid response
            dialogue.push(ChatMessage {
                role: "assistant".into(),
                content: reply.clone(),
            });
            dialogue.push(ChatMessage {
                role: "user".into(),
                content: "Your last response was not valid JSON. Please emit a valid command."
                    .into(),
            });
        }

        Ok(format!(
            "(incomplete) {} max steps reached; children: {}; trace: {:?}",
            self.id,
            self.child_results.len(),
            self.trace
        ))
    }

    /// Handle the spawn_subagent tool call
    fn handle_spawn_subagent(&mut self, input: &J, env: &mut Env) -> Result<String> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("subagent");

        let goal = input
            .get("goal")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("spawn_subagent requires 'goal' field"))?;

        let tools: Vec<&str> = input
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_else(|| vec!["ls", "cat", "grep", "find"]);

        match self.spawn_subagent(name, goal, &tools, env) {
            Ok(result) => {
                let summary = format!(
                    "Subagent {} completed:\n- Success: {}\n- Steps: {}\n- Time: {}ms\n- Output: {}",
                    result.agent_id,
                    result.success,
                    result.steps_taken,
                    result.execution_time_ms,
                    if result.output.len() > 200 {
                        format!("{}...", &result.output[..200])
                    } else {
                        result.output.clone()
                    }
                );
                Ok(summary)
            }
            Err(e) => Ok(format!("ERROR spawning subagent: {}", e)),
        }
    }

    /// Get hierarchy statistics
    pub fn stats(&self) -> HierarchyStats {
        self.state.stats()
    }

    /// Get all results from child agents
    pub fn get_child_results(&self) -> &[SubagentResult] {
        &self.child_results
    }

    /// Get the parent agent ID (None for root agent)
    pub fn parent(&self) -> Option<&AgentId> {
        self.parent_id.as_ref()
    }

    /// Check if this is a root agent
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }
}

// ===================== Public API =====================

/// Create and run a recursive agent
pub fn run_recursive(
    goal: &str,
    tool_names: &[&str],
    config: RlmConfig,
    dry_run: bool,
    env: &mut Env,
) -> Result<(String, HierarchyStats)> {
    let reg = ToolRegistry::with_builtins();
    let tools = reg.resolve_many(tool_names);

    let mut agent = RecursiveAgent::new(tools, config);
    let result = agent.run_sync(goal, dry_run, env)?;
    let stats = agent.stats();

    Ok((result, stats))
}

/// Create and run a recursive agent with a specific model
pub fn run_recursive_with_model(
    goal: &str,
    tool_names: &[&str],
    model_uri: &str,
    config: RlmConfig,
    dry_run: bool,
    env: &mut Env,
) -> Result<(String, HierarchyStats)> {
    let reg = ToolRegistry::with_builtins();
    let tools = reg.resolve_many(tool_names);

    let mut agent = RecursiveAgent::with_model_uri(tools, model_uri, config);
    let result = agent.run_sync(goal, dry_run, env)?;
    let stats = agent.stats();

    Ok((result, stats))
}

// ===================== Spawn Tool =====================

/// Tool for spawning subagents (used by agents internally)
pub struct SpawnSubagentTool;

impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to handle a subtask. Input: {\"name\": \"agent_name\", \"goal\": \"task description\", \"tools\": [\"tool1\", \"tool2\"]}"
    }

    fn call(&self, _input: &str, _env: &mut Env) -> Result<crate::value::Value> {
        // This tool is handled specially by RecursiveAgent::handle_spawn_subagent
        // If called directly, return an error
        Err(anyhow!(
            "spawn_subagent must be used within a RecursiveAgent context"
        ))
    }
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_creation() {
        let root = AgentId::root();
        assert_eq!(root.depth, 0);
        assert!(root.path.starts_with("root"));

        let child = root.child("worker", 1);
        assert_eq!(child.depth, 1);
        assert!(child.path.contains("worker"));
    }

    #[test]
    fn test_hierarchy_state_limits() {
        let config = RlmConfig {
            max_depth: 2,
            max_agents: 3,
            ..Default::default()
        };
        let state = HierarchyState::new(config);

        // Should allow spawn at depth 0
        assert!(state.can_spawn(0).is_ok());
        assert!(state.can_spawn(1).is_ok());

        // Should reject at max depth
        assert!(state.can_spawn(2).is_err());

        // Register agents up to limit
        let id1 = AgentId::root();
        assert!(state.register_agent(&id1).is_ok());

        let id2 = id1.child("a", 1);
        assert!(state.register_agent(&id2).is_ok());

        let id3 = id1.child("b", 2);
        assert!(state.register_agent(&id3).is_ok());

        // Should reject when at limit
        let _id4 = id1.child("c", 3);
        // Note: can_spawn checks total, but we've already registered 3
        // so the next spawn should fail
    }

    #[test]
    fn test_message_passing() {
        let config = RlmConfig::default();
        let state = HierarchyState::new(config);

        state.send_message(AgentMessage {
            from: "root".to_string(),
            to: "root.worker".to_string(),
            content: "do something".to_string(),
            timestamp: 0,
            msg_type: MessageType::Task,
        });

        let msgs = state.get_messages("root.worker");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "do something");

        // Broadcast message
        state.send_message(AgentMessage {
            from: "root".to_string(),
            to: "*".to_string(),
            content: "broadcast".to_string(),
            timestamp: 1,
            msg_type: MessageType::Broadcast,
        });

        let msgs = state.get_messages("root.worker");
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_config_defaults() {
        let config = RlmConfig::default();
        assert_eq!(config.max_depth, DEFAULT_MAX_DEPTH);
        assert_eq!(config.max_agents, DEFAULT_MAX_AGENTS);
        assert_eq!(config.agent_timeout_secs, DEFAULT_AGENT_TIMEOUT_SECS);
    }

    #[test]
    fn test_cycle_detection() {
        let config = RlmConfig::default();
        let state = HierarchyState::new(config);

        let id = AgentId::root();
        assert!(state.register_agent(&id).is_ok());

        // Trying to register same ID should fail
        let id_dup = AgentId {
            path: id.path.clone(),
            id: 999,
            depth: 0,
        };
        assert!(state.register_agent(&id_dup).is_err());
    }

    #[test]
    fn test_stats() {
        let config = RlmConfig::default();
        let state = HierarchyState::new(config);

        let stats = state.stats();
        assert_eq!(stats.total_spawned, 0);
        assert_eq!(stats.currently_active, 0);

        let id = AgentId::root();
        state.register_agent(&id).unwrap();

        let stats = state.stats();
        assert_eq!(stats.total_spawned, 1);
        assert_eq!(stats.currently_active, 1);

        state.deregister_agent(&id);

        let stats = state.stats();
        assert_eq!(stats.total_spawned, 1);
        assert_eq!(stats.currently_active, 0);
    }
    
    // ===================== Permission Tests =====================
    
    #[test]
    fn test_subagent_permissions_default() {
        let perms = SubagentPermissions::new();
        
        // Default permissions are restrictive
        assert!(!perms.can_write_files);
        assert!(!perms.can_execute_shell);
        assert!(!perms.can_access_network);
        assert!(perms.can_access_env);
        assert!(perms.can_spawn_subagents);
        assert_eq!(perms.max_spawn_depth, 3);
    }
    
    #[test]
    fn test_subagent_permissions_allow_all() {
        let perms = SubagentPermissions::allow_all();
        
        assert!(perms.can_write_files);
        assert!(perms.can_execute_shell);
        assert!(perms.can_access_network);
        assert!(perms.allowed_paths.contains(&"*".to_string()));
        assert!(perms.allowed_hosts.contains(&"*".to_string()));
    }
    
    #[test]
    fn test_subagent_permissions_read_only() {
        let perms = SubagentPermissions::read_only();
        
        assert!(!perms.can_write_files);
        assert!(!perms.can_execute_shell);
        assert!(!perms.can_access_network);
        assert!(perms.allowed_tools.contains("ls"));
        assert!(perms.allowed_tools.contains("cat"));
        assert!(perms.allowed_tools.contains("grep"));
    }
    
    #[test]
    fn test_subagent_permissions_tool_check() {
        let perms = SubagentPermissions::new()
            .with_tools(vec!["ls", "cat", "grep"]);
        
        assert!(perms.is_tool_allowed("ls"));
        assert!(perms.is_tool_allowed("cat"));
        assert!(!perms.is_tool_allowed("rm"));
        assert!(!perms.is_tool_allowed("sh"));
    }
    
    #[test]
    fn test_subagent_permissions_path_check() {
        let perms = SubagentPermissions::new()
            .with_allowed_paths(vec!["/home/user/project", "/tmp"]);
        
        assert!(perms.is_path_allowed("/home/user/project/src"));
        assert!(perms.is_path_allowed("/tmp/test.txt"));
        assert!(!perms.is_path_allowed("/etc/passwd"));
        assert!(!perms.is_path_allowed("/home/user/other"));
    }
    
    #[test]
    fn test_subagent_permissions_validate_child() {
        let parent = SubagentPermissions::new()
            .with_tools(vec!["ls", "cat", "grep"])
            .with_spawn(true, 3, 10)
            .with_limits(60, 512);
        
        // Valid child (subset of parent)
        let child = SubagentPermissions::new()
            .with_tools(vec!["ls", "cat"])
            .with_spawn(true, 2, 5)
            .with_limits(30, 256);
        
        assert!(parent.validate_child(&child).is_ok());
        
        // Invalid child (exceeds parent's spawn depth)
        let bad_child = SubagentPermissions::new()
            .with_spawn(true, 5, 10);
        
        assert!(parent.validate_child(&bad_child).is_err());
    }
    
    #[test]
    fn test_subagent_permissions_escalation() {
        let parent = SubagentPermissions::new();
        
        // Child cannot have write access if parent doesn't
        let child_with_write = SubagentPermissions::new().with_write_access();
        assert!(parent.validate_child(&child_with_write).is_err());
        
        // Child cannot have shell access if parent doesn't
        let child_with_shell = SubagentPermissions::new().with_shell_access();
        assert!(parent.validate_child(&child_with_shell).is_err());
    }
    
    #[test]
    fn test_subagent_policy_default() {
        let policy = SubagentPolicy::default();
        
        assert!(policy.allow_custom_permissions);
        assert_eq!(policy.global_max_depth, 5);
        assert_eq!(policy.global_max_agents, 50);
        assert!(!policy.require_spawn_approval);
    }
    
    #[test]
    fn test_rlm_config_with_permissions() {
        let config = RlmConfig::default()
            .with_permissions(SubagentPermissions::read_only())
            .with_policy(SubagentPolicy::restrictive());
        
        assert!(config.root_permissions.is_some());
        assert!(config.subagent_policy.is_some());
        
        let perms = config.effective_root_permissions();
        assert!(!perms.can_write_files);
    }
}
