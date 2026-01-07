# AI Features Implementation & Testing Plan

## Overview
This document outlines the comprehensive implementation of MCP, A2A, and NANDA protocols, plus extensive testing coverage for AetherShell's AI capabilities.

## ✅ Completed

### 1. Comprehensive Agent Tests (`tests/ai_agents_comprehensive.rs`)
- ✅ Basic agent execution (10+ tests)
- ✅ Model selection and URIs (stub, openai, ollama formats)
- ✅ Error handling (invalid tools, empty goals, long inputs)
- ✅ Tool registry (listing, resolving, deduplication)
- ✅ Agent construction (default, custom models)
- ✅ Execution traces (capturing steps, thoughts)
- ✅ Integration tests (real builtin calls)
- ✅ Performance tests (timing, parallel execution)
- ✅ Edge cases (zero steps, Unicode, special characters)
- ✅ Tool call simulation (dry_run vs wet_run)

**Total: 40+ agent tests**

## 🚧 To Implement

### 2. Comprehensive Swarm Tests (`tests/ai_swarm_comprehensive.rs`)

#### Swarm Coordination Tests
- Multi-agent swarm creation with different policies
- Round-robin coordinator behavior
- Router coordinator (intelligent routing)
- Blackboard communication between agents
- Agent task distribution
- Swarm completion conditions

#### Policy Tests
- RoundRobin policy: equal distribution
- Router policy: intelligent agent selection
- Custom coordination strategies
- Load balancing across agents
- Specialized agent routing

#### Blackboard Tests
- Message posting and retrieval
- Message kinds (note, thought, final, observation)
- Message ordering and history
- Agent-to-agent communication via blackboard
- Concurrent message handling

#### Tool Usage in Swarms
- Tools per agent (different tool sets)
- Shared tools across agents
- Tool conflict resolution
- Tool execution coordination

#### Model Selection in Swarms
- Different models per agent
- Model URI per agent configuration
- Fallback to environment models
- Mixed model swarms (GPT + Llama)

**Estimated: 50+ swarm tests**

### 3. MCP (Model Context Protocol) Full Implementation

#### Current Status
```rust
// src/ai.rs line ~1254
pub mod mcp {
    pub struct McpClient { endpoint: String }
    // TODO: Implement real MCP protocol
    pub struct McpToolResolver { endpoint: String }
    // Stub: returns empty tool lists
}
```

#### Required Implementation (`src/ai/mcp.rs` or extend `src/ai.rs`)

```rust
pub mod mcp {
    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// MCP Protocol Version
    const MCP_VERSION: &str = "1.0";

    /// MCP Tool Schema
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpTool {
        pub name: String,
        pub description: String,
        pub input_schema: serde_json::Value,
        pub output_schema: Option<serde_json::Value>,
    }

    /// MCP Server Connection
    pub struct McpClient {
        endpoint: String,
        client: reqwest::blocking::Client,
        tools_cache: HashMap<String, McpTool>,
    }

    impl McpClient {
        pub fn new(endpoint: &str) -> Self {
            Self {
                endpoint: endpoint.to_string(),
                client: reqwest::blocking::Client::new(),
                tools_cache: HashMap::new(),
            }
        }

        /// Discover available tools from MCP server
        pub fn discover_tools(&mut self) -> Result<Vec<McpTool>> {
            let url = format!("{}/mcp/v1/tools", self.endpoint);
            let response: Vec<McpTool> = self.client
                .get(&url)
                .send()?
                .error_for_status()?
                .json()?;
            
            // Cache discovered tools
            for tool in &response {
                self.tools_cache.insert(tool.name.clone(), tool.clone());
            }
            
            Ok(response)
        }

        /// Execute a tool via MCP
        pub fn execute_tool(&self, name: &str, input: serde_json::Value) -> Result<serde_json::Value> {
            let url = format!("{}/mcp/v1/tools/{}/execute", self.endpoint, name);
            let response: serde_json::Value = self.client
                .post(&url)
                .json(&input)
                .send()?
                .error_for_status()?
                .json()?;
            
            Ok(response)
        }

        /// Validate tool input against schema
        pub fn validate_input(&self, tool_name: &str, input: &serde_json::Value) -> Result<()> {
            if let Some(tool) = self.tools_cache.get(tool_name) {
                // Use jsonschema crate for validation
                // Implementation depends on input_schema format
                Ok(())
            } else {
                Err(anyhow!("Tool {} not found in cache", tool_name))
            }
        }

        /// Health check for MCP server
        pub fn health_check(&self) -> Result<bool> {
            let url = format!("{}/health", self.endpoint);
            let response = self.client.get(&url).send()?;
            Ok(response.status().is_success())
        }
    }

    /// MCP Tool Resolver for Agent ToolRegistry
    pub struct McpToolResolver {
        client: McpClient,
    }

    impl McpToolResolver {
        pub fn new(endpoint: &str) -> Self {
            Self {
                client: McpClient::new(endpoint),
            }
        }
    }

    impl super::agents::ToolResolver for McpToolResolver {
        fn list(&self) -> Vec<String> {
            // Try to discover tools, return cached if fail
            let mut client = self.client.clone(); // Need Arc<Mutex<>> for real impl
            client.discover_tools()
                .map(|tools| tools.iter().map(|t| t.name.clone()).collect())
                .unwrap_or_default()
        }

        fn get(&self, name: &str) -> Option<Box<dyn super::agents::Tool>> {
            // Return MCP tool wrapper
            Some(Box::new(McpToolWrapper {
                name: name.to_string(),
                client: self.client.clone(),
            }))
        }
    }

    /// Wrapper to make MCP tools compatible with Agent Tool trait
    struct McpToolWrapper {
        name: String,
        client: McpClient,
    }

    impl super::agents::Tool for McpToolWrapper {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            // Get from cached tool schema
            "MCP tool"
        }

        fn call(&self, input: &str, _env: &mut crate::env::Env) -> Result<crate::value::Value> {
            let input_json: serde_json::Value = serde_json::from_str(input)?;
            let result = self.client.execute_tool(&self.name, input_json)?;
            
            // Convert result to AetherShell Value
            Ok(json_to_value(&result))
        }
    }

    fn json_to_value(v: &serde_json::Value) -> crate::value::Value {
        // Convert JSON to AetherShell Value
        // Implementation similar to existing json_to_value
        crate::value::Value::Str(v.to_string())
    }
}
```

#### MCP Tests (`tests/ai_mcp.rs`)
- Tool discovery from MCP server
- Tool execution via MCP protocol
- Input schema validation
- Error handling (server down, invalid tools)
- Tool caching
- Health checks
- Integration with Agent ToolRegistry

**Estimated: 20+ MCP tests**

### 4. A2A (Agent-to-Agent) Communication Protocol

#### Implementation (`src/ai/a2a.rs`)

```rust
//! Agent-to-Agent (A2A) Communication Protocol
//! Direct messaging and delegation between agents in a swarm

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// A2A Message Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum A2AMessageType {
    /// Direct message to specific agent
    DirectMessage { to: String, content: String },
    
    /// Broadcast to all agents
    Broadcast { content: String },
    
    /// Request delegation of task
    Delegate { to: String, task: String, context: serde_json::Value },
    
    /// Response to delegation
    DelegateResponse { from: String, result: String, success: bool },
    
    /// Query other agent's capabilities
    QueryCapabilities { to: String },
    
    /// Response with capabilities
    CapabilitiesResponse { from: String, capabilities: Vec<String> },
    
    /// Request for assistance
    AssistRequest { to: String, problem: String },
    
    /// Offer assistance
    AssistOffer { from: String, solution: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    pub id: Uuid,
    pub from: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub msg_type: A2AMessageType,
}

/// A2A Message Bus for agent communication
pub struct A2AMessageBus {
    messages: Arc<Mutex<Vec<A2AMessage>>>,
    agent_mailboxes: Arc<Mutex<HashMap<String, Vec<A2AMessage>>>>,
}

impl A2AMessageBus {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            agent_mailboxes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Send A2A message
    pub fn send(&self, message: A2AMessage) -> Result<()> {
        let mut messages = self.messages.lock().unwrap();
        messages.push(message.clone());

        // Route to recipient's mailbox
        match &message.msg_type {
            A2AMessageType::DirectMessage { to, .. } |
            A2AMessageType::Delegate { to, .. } |
            A2AMessageType::QueryCapabilities { to } |
            A2AMessageType::AssistRequest { to, .. } => {
                let mut mailboxes = self.agent_mailboxes.lock().unwrap();
                mailboxes.entry(to.clone())
                    .or_insert_with(Vec::new)
                    .push(message);
            }
            A2AMessageType::Broadcast { .. } => {
                // Add to all mailboxes
                let mut mailboxes = self.agent_mailboxes.lock().unwrap();
                for messages in mailboxes.values_mut() {
                    messages.push(message.clone());
                }
            }
            _ => {} // Response types don't route
        }

        Ok(())
    }

    /// Receive messages for specific agent
    pub fn receive(&self, agent_id: &str) -> Vec<A2AMessage> {
        let mut mailboxes = self.agent_mailboxes.lock().unwrap();
        mailboxes.entry(agent_id.to_string())
            .or_insert_with(Vec::new)
            .drain(..)
            .collect()
    }

    /// Get all messages (for monitoring)
    pub fn get_all_messages(&self) -> Vec<A2AMessage> {
        self.messages.lock().unwrap().clone()
    }

    /// Clear all messages
    pub fn clear(&self) {
        self.messages.lock().unwrap().clear();
        self.agent_mailboxes.lock().unwrap().clear();
    }
}

/// Agent with A2A capabilities
pub struct A2AAgent {
    pub id: String,
    pub capabilities: Vec<String>,
    pub message_bus: Arc<A2AMessageBus>,
}

impl A2AAgent {
    pub fn new(id: String, capabilities: Vec<String>, bus: Arc<A2AMessageBus>) -> Self {
        Self {
            id,
            capabilities,
            message_bus: bus,
        }
    }

    /// Send message to another agent
    pub fn send_message(&self, to: &str, content: String) -> Result<()> {
        let msg = A2AMessage {
            id: Uuid::new_v4(),
            from: self.id.clone(),
            timestamp: chrono::Utc::now(),
            msg_type: A2AMessageType::DirectMessage { to: to.to_string(), content },
        };
        self.message_bus.send(msg)
    }

    /// Delegate task to another agent
    pub fn delegate_task(&self, to: &str, task: String, context: serde_json::Value) -> Result<()> {
        let msg = A2AMessage {
            id: Uuid::new_v4(),
            from: self.id.clone(),
            timestamp: chrono::Utc::now(),
            msg_type: A2AMessageType::Delegate { to: to.to_string(), task, context },
        };
        self.message_bus.send(msg)
    }

    /// Query another agent's capabilities
    pub fn query_capabilities(&self, to: &str) -> Result<()> {
        let msg = A2AMessage {
            id: Uuid::new_v4(),
            from: self.id.clone(),
            timestamp: chrono::Utc::now(),
            msg_type: A2AMessageType::QueryCapabilities { to: to.to_string() },
        };
        self.message_bus.send(msg)
    }

    /// Broadcast to all agents
    pub fn broadcast(&self, content: String) -> Result<()> {
        let msg = A2AMessage {
            id: Uuid::new_v4(),
            from: self.id.clone(),
            timestamp: chrono::Utc::now(),
            msg_type: A2AMessageType::Broadcast { content },
        };
        self.message_bus.send(msg)
    }

    /// Process incoming messages
    pub fn process_messages(&self) -> Vec<A2AMessage> {
        self.message_bus.receive(&self.id)
    }
}
```

#### A2A Tests (`tests/ai_a2a.rs`)
- Direct messaging between agents
- Broadcast messages
- Task delegation
- Capability queries
- Message routing
- Mailbox management
- Concurrent message handling
- Message ordering and timestamps

**Estimated: 30+ A2A tests**

### 5. NANDA (Name And Negotiate Dynamic Agents) Framework

#### Implementation (`src/ai/nanda.rs`)

```rust
//! NANDA: Name And Negotiate Dynamic Agents
//! Framework for dynamic agent negotiation, task allocation, and consensus

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// NANDA Negotiation Protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NandaProposal {
    /// Propose task allocation
    TaskAllocation { task_id: Uuid, agent_id: String, priority: f32, rationale: String },
    
    /// Propose resource allocation
    ResourceAllocation { resource: String, agent_id: String, amount: f32 },
    
    /// Propose coordination strategy
    CoordinationStrategy { strategy: String, parameters: HashMap<String, serde_json::Value> },
    
    /// Propose consensus threshold
    ConsensusThreshold { threshold: f32, quorum: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NandaVote {
    Accept,
    Reject { reason: String },
    Abstain,
    CounterProposal { proposal: Box<NandaProposal> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NandaNegotiation {
    pub id: Uuid,
    pub proposal: NandaProposal,
    pub proposer: String,
    pub votes: HashMap<String, NandaVote>,
    pub status: NegotiationStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NegotiationStatus {
    Open,
    Voting,
    Accepted,
    Rejected,
    Modified,
}

/// NANDA Coordinator for negotiations
pub struct NandaCoordinator {
    agents: Vec<String>,
    negotiations: Vec<NandaNegotiation>,
    consensus_threshold: f32,
    quorum: usize,
}

impl NandaCoordinator {
    pub fn new(agents: Vec<String>, consensus_threshold: f32, quorum: usize) -> Self {
        Self {
            agents,
            negotiations: Vec::new(),
            consensus_threshold,
            quorum,
        }
    }

    /// Propose new negotiation
    pub fn propose(&mut self, proposer: String, proposal: NandaProposal) -> Uuid {
        let negotiation = NandaNegotiation {
            id: Uuid::new_v4(),
            proposal,
            proposer,
            votes: HashMap::new(),
            status: NegotiationStatus::Open,
            timestamp: chrono::Utc::now(),
        };
        let id = negotiation.id;
        self.negotiations.push(negotiation);
        id
    }

    /// Submit vote for negotiation
    pub fn vote(&mut self, negotiation_id: Uuid, agent_id: String, vote: NandaVote) -> Result<()> {
        if let Some(neg) = self.negotiations.iter_mut().find(|n| n.id == negotiation_id) {
            neg.votes.insert(agent_id, vote);
            
            // Check if we have quorum and can decide
            if neg.votes.len() >= self.quorum {
                self.evaluate_negotiation(negotiation_id)?;
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Negotiation {} not found", negotiation_id))
        }
    }

    /// Evaluate negotiation status based on votes
    fn evaluate_negotiation(&mut self, id: Uuid) -> Result<()> {
        if let Some(neg) = self.negotiations.iter_mut().find(|n| n.id == id) {
            let total_votes = neg.votes.len();
            let accept_votes = neg.votes.values().filter(|v| matches!(v, NandaVote::Accept)).count();
            let reject_votes = neg.votes.values().filter(|v| matches!(v, NandaVote::Reject { .. })).count();
            
            let accept_ratio = accept_votes as f32 / total_votes as f32;
            
            if accept_ratio >= self.consensus_threshold {
                neg.status = NegotiationStatus::Accepted;
            } else if reject_votes as f32 / total_votes as f32 > (1.0 - self.consensus_threshold) {
                neg.status = NegotiationStatus::Rejected;
            } else {
                // Check for counter-proposals
                let counter_proposals: Vec<_> = neg.votes.values()
                    .filter_map(|v| {
                        if let NandaVote::CounterProposal { proposal } = v {
                            Some(proposal.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                
                if !counter_proposals.is_empty() {
                    neg.status = NegotiationStatus::Modified;
                }
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Negotiation {} not found", id))
        }
    }

    /// Get negotiation status
    pub fn get_status(&self, id: Uuid) -> Option<NegotiationStatus> {
        self.negotiations.iter()
            .find(|n| n.id == id)
            .map(|n| n.status.clone())
    }

    /// Get all active negotiations
    pub fn get_active_negotiations(&self) -> Vec<&NandaNegotiation> {
        self.negotiations.iter()
            .filter(|n| n.status == NegotiationStatus::Open || n.status == NegotiationStatus::Voting)
            .collect()
    }

    /// Resolve conflicts between proposals
    pub fn resolve_conflicts(&self, proposals: Vec<NandaProposal>) -> Result<NandaProposal> {
        // Implement conflict resolution strategy
        // For now, return first proposal
        proposals.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No proposals to resolve"))
    }
}

/// Task Allocator using NANDA
pub struct NandaTaskAllocator {
    coordinator: NandaCoordinator,
    task_queue: Vec<Task>,
    allocations: HashMap<Uuid, String>, // task_id -> agent_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub priority: f32,
    pub required_capabilities: Vec<String>,
    pub estimated_effort: f32,
}

impl NandaTaskAllocator {
    pub fn new(coordinator: NandaCoordinator) -> Self {
        Self {
            coordinator,
            task_queue: Vec::new(),
            allocations: HashMap::new(),
        }
    }

    /// Add task to queue
    pub fn add_task(&mut self, task: Task) {
        self.task_queue.push(task);
    }

    /// Allocate tasks via negotiation
    pub fn allocate_tasks(&mut self, agent_capabilities: HashMap<String, Vec<String>>) -> Result<()> {
        for task in &self.task_queue {
            // Find capable agents
            let capable_agents: Vec<String> = agent_capabilities.iter()
                .filter(|(_, caps)| {
                    task.required_capabilities.iter().all(|req| caps.contains(req))
                })
                .map(|(id, _)| id.clone())
                .collect();

            if capable_agents.is_empty() {
                continue;
            }

            // Each capable agent proposes themselves
            for agent in capable_agents {
                let proposal = NandaProposal::TaskAllocation {
                    task_id: task.id,
                    agent_id: agent.clone(),
                    priority: task.priority,
                    rationale: format!("Agent {} has required capabilities", agent),
                };
                self.coordinator.propose(agent.clone(), proposal);
            }
        }

        Ok(())
    }

    /// Get task allocation
    pub fn get_allocation(&self, task_id: Uuid) -> Option<&String> {
        self.allocations.get(&task_id)
    }
}
```

#### NANDA Tests (`tests/ai_nanda.rs`)
- Proposal creation and submission
- Voting mechanisms (accept, reject, abstain, counter-propose)
- Consensus calculation
- Quorum requirements
- Task allocation via negotiation
- Resource negotiation
- Strategy negotiation
- Conflict resolution
- Multi-round negotiations
- Negotiation timeout and expiry

**Estimated: 40+ NANDA tests**

## Testing Summary

| Component          | Test File                    | Estimated Tests | Status           |
| ------------------ | ---------------------------- | --------------- | ---------------- |
| Single Agents      | `ai_agents_comprehensive.rs` | 40+             | ✅ Complete       |
| Multi-Agent Swarms | `ai_swarm_comprehensive.rs`  | 50+             | 🚧 To Implement   |
| MCP Protocol       | `ai_mcp.rs`                  | 20+             | 🚧 To Implement   |
| A2A Communication  | `ai_a2a.rs`                  | 30+             | 🚧 To Implement   |
| NANDA Framework    | `ai_nanda.rs`                | 40+             | 🚧 To Implement   |
| **TOTAL**          |                              | **180+**        | **25% Complete** |

## Implementation Priority

1. ✅ **Complete Agent Tests** - Done
2. **Swarm Tests** - High priority (extends existing swarm code)
3. **MCP Implementation** - Medium priority (expand stub)
4. **A2A Protocol** - Medium priority (new communication layer)
5. **NANDA Framework** - Advanced priority (complex negotiation)

## Running Tests

```bash
# Run all AI tests
cargo test ai_

# Run specific test suite
cargo test --test ai_agents_comprehensive
cargo test --test ai_swarm_comprehensive
cargo test --test ai_mcp
cargo test --test ai_a2a
cargo test --test ai_nanda

# Run with output
cargo test ai_ -- --nocapture

# Run specific test
cargo test test_agent_basic_execution -- --nocapture
```

## Dependencies Needed

Add to `Cargo.toml`:
```toml
[dependencies]
# Existing...
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
# For MCP schema validation
jsonschema = "0.17"
# For async operations (if needed)
tokio = { version = "1.0", features = ["full"] }
```

## Next Steps

1. Run current agent tests: `cargo test ai_agents_comprehensive`
2. Implement swarm tests based on existing swarm infrastructure
3. Expand MCP stub into full protocol implementation
4. Add A2A communication layer
5. Implement NANDA negotiation framework
6. Integration tests combining all features

---

**Status**: Phase 1 (Agent Tests) Complete ✅  
**Next**: Phase 2 (Swarm Tests) 🚧  
**Goal**: 180+ comprehensive AI feature tests
