# A2A & A2UI Optimization Proposal

> **Date:** January 26, 2026  
> **Status:** Proposal  
> **Author:** AetherShell Team

## Executive Summary

This document proposes optimizations and new interfaces for Agent-to-Agent (A2A) and Agent-to-User Interface (A2UI) protocols in AetherShell. The goal is to improve performance, enable richer interactions, and provide a more intuitive developer experience.

---

## Current State Analysis

### A2A Protocol (Implemented)
- **Message Bus**: Mutex-based with per-agent mailboxes
- **Message Types**: 8 types (DirectMessage, Broadcast, Delegate, etc.)
- **Agent Abstraction**: Basic send/receive/broadcast capabilities
- **Limitations**:
  - Synchronous message delivery only
  - No message prioritization
  - No message expiration/TTL
  - Linear mailbox scaling
  - No typed message payloads

### A2UI Protocol (Documented, Not Implemented)
- Referenced in README with examples (`a2ui_notify`, `a2ui_prompt`, etc.)
- No actual implementation in `src/` directory
- No builtins for A2UI operations

---

## Proposed Optimizations

### 1. A2A Message Bus Optimizations

#### 1.1 Lock-Free Message Queue
Replace `Mutex<Vec<>>` with a lock-free concurrent queue for better throughput:

```rust
use crossbeam_channel::{bounded, Sender, Receiver};

pub struct A2AMessageBus {
    // Per-agent channels for lock-free delivery
    agent_channels: DashMap<String, (Sender<A2AMessage>, Receiver<A2AMessage>)>,
    // Broadcast channel for pub/sub
    broadcast_tx: broadcast::Sender<A2AMessage>,
}
```

**Benefits:**
- 10-100x throughput improvement under contention
- Non-blocking sends
- Better cache locality

#### 1.2 Message Prioritization

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

pub struct A2AMessage {
    // ... existing fields ...
    pub priority: MessagePriority,
    pub ttl: Option<Duration>,  // Time-to-live
    pub correlation_id: Option<Uuid>,  // For request/response pairing
}
```

#### 1.3 Typed Message Payloads

```rust
use serde::de::DeserializeOwned;

pub trait A2APayload: Serialize + DeserializeOwned + Send + Sync + 'static {}

pub struct TypedMessage<T: A2APayload> {
    pub header: A2AMessageHeader,
    pub payload: T,
}

// Example typed payloads
#[derive(Serialize, Deserialize)]
pub struct CodeReviewRequest {
    pub files: Vec<String>,
    pub review_type: ReviewType,
    pub urgency: MessagePriority,
}

#[derive(Serialize, Deserialize)]
pub struct CodeReviewResponse {
    pub findings: Vec<Finding>,
    pub summary: String,
    pub confidence: f64,
}
```

#### 1.4 Async Message Patterns

```rust
impl A2AAgent {
    /// Send with acknowledgment (returns when recipient confirms receipt)
    pub async fn send_ack(&self, to: &str, msg: A2AMessage) -> Result<Ack>;
    
    /// Request-response pattern with timeout
    pub async fn request<Req, Resp>(&self, to: &str, req: Req, timeout: Duration) 
        -> Result<Resp>
    where
        Req: A2APayload,
        Resp: A2APayload;
    
    /// Subscribe to message stream with filter
    pub fn subscribe(&self, filter: MessageFilter) -> impl Stream<Item = A2AMessage>;
}
```

---

### 2. A2A Routing Optimizations

#### 2.1 Topic-Based Routing

```rust
pub struct TopicRouter {
    subscriptions: DashMap<String, Vec<String>>,  // topic -> [agent_ids]
}

impl A2AMessageBus {
    /// Publish to topic (all subscribers receive)
    pub fn publish(&self, topic: &str, msg: A2AMessage) -> Result<usize>;
    
    /// Subscribe agent to topic
    pub fn subscribe(&self, agent_id: &str, topic: &str) -> Result<()>;
    
    /// Pattern-based subscription (e.g., "security.*")
    pub fn subscribe_pattern(&self, agent_id: &str, pattern: &str) -> Result<()>;
}
```

#### 2.2 Capability-Based Routing

```rust
pub struct CapabilityRouter {
    capabilities: DashMap<String, Vec<String>>,  // capability -> [agent_ids]
}

impl A2AMessageBus {
    /// Route to any agent with capability
    pub fn route_by_capability(&self, capability: &str, msg: A2AMessage) -> Result<String>;
    
    /// Route to best agent (load-balanced among capable)
    pub fn route_optimal(&self, capability: &str, msg: A2AMessage) -> Result<String>;
}
```

---

### 3. A2UI Protocol Implementation

#### 3.1 Core A2UI Module (`src/ai/a2ui.rs`)

```rust
//! A2UI (Agent-to-User Interface) Protocol
//! Rich agent-to-user interaction patterns

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

/// A2UI Event types that agents can emit to users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum A2UIEvent {
    /// Notification with optional actions
    Notification {
        title: String,
        body: String,
        level: NotificationLevel,
        actions: Vec<A2UIAction>,
    },
    
    /// Progress indicator
    Progress {
        id: Uuid,
        label: String,
        current: u64,
        total: u64,
        status: ProgressStatus,
    },
    
    /// Interactive prompt
    Prompt {
        id: Uuid,
        message: String,
        prompt_type: PromptType,
        default: Option<String>,
    },
    
    /// Structured data render request
    Render {
        content_type: RenderType,
        data: serde_json::Value,
        interactive: bool,
    },
    
    /// Agent status update
    StatusUpdate {
        agent_id: String,
        status: AgentStatusInfo,
        context: Option<String>,
    },
    
    /// Confirmation request (blocks agent until user responds)
    Confirmation {
        id: Uuid,
        message: String,
        severity: ConfirmationSeverity,
        timeout: Option<Duration>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2UIAction {
    pub id: String,
    pub label: String,
    pub action_type: ActionType,
    pub keyboard_shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptType {
    Text { validation: Option<String> },
    Number { min: Option<f64>, max: Option<f64> },
    Select { options: Vec<SelectOption> },
    MultiSelect { options: Vec<SelectOption>, min: usize, max: usize },
    Confirm,
    Password,
    FilePath { filter: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderType {
    Table { columns: Vec<ColumnDef>, sortable: bool },
    Tree { expandable: bool },
    Chart { chart_type: ChartType },
    Markdown,
    Code { language: String, line_numbers: bool },
    Diff { context_lines: usize },
    Image { width: Option<u32>, height: Option<u32> },
}

/// A2UI Channel for bidirectional agent-user communication
pub struct A2UIChannel {
    /// Events from agent to user
    event_tx: mpsc::Sender<A2UIEvent>,
    /// Responses from user to agent  
    response_rx: mpsc::Receiver<A2UIResponse>,
    /// Agent identifier
    agent_id: String,
}

impl A2UIChannel {
    /// Send notification (fire-and-forget)
    pub async fn notify(&self, title: &str, body: &str, level: NotificationLevel) -> Result<()>;
    
    /// Prompt user and wait for response
    pub async fn prompt<T: FromUserInput>(&self, msg: &str, prompt_type: PromptType) -> Result<T>;
    
    /// Request confirmation (blocking)
    pub async fn confirm(&self, msg: &str, severity: ConfirmationSeverity) -> Result<bool>;
    
    /// Update progress indicator
    pub async fn progress(&self, id: Uuid, current: u64, total: u64) -> Result<()>;
    
    /// Render structured data
    pub async fn render(&self, render_type: RenderType, data: impl Serialize) -> Result<()>;
    
    /// Stream events (for long-running operations)
    pub fn stream(&self) -> impl Stream<Item = A2UIEvent>;
}
```

#### 3.2 A2UI Builtins

```ae
# Notifications
a2ui_notify(title: String, body: String, opts?: Record) -> Unit
a2ui_toast(message: String, duration?: Int) -> Unit

# Interactive Prompts
a2ui_prompt(message: String, opts?: Record) -> String
a2ui_confirm(message: String, opts?: Record) -> Bool
a2ui_select(message: String, options: Array<String>, opts?: Record) -> String
a2ui_multiselect(message: String, options: Array<String>, opts?: Record) -> Array<String>

# Progress Indicators
a2ui_progress_start(label: String, total: Int) -> String  # returns task_id
a2ui_progress_update(task_id: String, current: Int) -> Unit
a2ui_progress_complete(task_id: String, message?: String) -> Unit
a2ui_spinner(label: String) -> String  # indeterminate progress

# Data Rendering
a2ui_table(data: Array<Record>, opts?: Record) -> Unit
a2ui_tree(data: Record, opts?: Record) -> Unit
a2ui_chart(data: Array, chart_type: String, opts?: Record) -> Unit
a2ui_code(code: String, language: String, opts?: Record) -> Unit
a2ui_diff(before: String, after: String, opts?: Record) -> Unit
a2ui_markdown(content: String) -> Unit

# Agent Status
a2ui_status(status: String, context?: String) -> Unit
a2ui_thinking(message?: String) -> String  # returns thinking_id
a2ui_thinking_done(thinking_id: String) -> Unit
```

---

### 4. Unified Agent Communication Interface

#### 4.1 AgentContext - Unified Interface

```rust
/// Unified context providing both A2A and A2UI capabilities
pub struct AgentContext {
    pub id: String,
    pub capabilities: Vec<String>,
    
    // A2A communication
    pub a2a: A2AHandle,
    
    // A2UI communication  
    pub ui: A2UIChannel,
    
    // Shared state
    pub blackboard: Arc<Blackboard>,
    
    // Configuration
    pub config: AgentConfig,
}

impl AgentContext {
    /// Send to another agent
    pub async fn send(&self, to: &str, msg: impl Into<A2AMessage>) -> Result<()> {
        self.a2a.send(to, msg.into()).await
    }
    
    /// Notify user
    pub async fn notify(&self, msg: &str) -> Result<()> {
        self.ui.notify("Agent Update", msg, NotificationLevel::Info).await
    }
    
    /// Request user input
    pub async fn ask(&self, question: &str) -> Result<String> {
        self.ui.prompt(question, PromptType::Text { validation: None }).await
    }
    
    /// Delegate to best capable agent
    pub async fn delegate(&self, capability: &str, task: impl Into<A2AMessage>) -> Result<A2AMessage> {
        self.a2a.route_by_capability(capability, task.into()).await
    }
}
```

#### 4.2 AetherShell Builtin Syntax

```ae
# Agent creation with unified context
let analyzer = agent("code-analyzer", {
    capabilities: ["code-review", "security-scan"],
    model: "openai:gpt-4o",
    tools: ["read_file", "grep", "lint"]
})

# A2A: Send to specific agent
analyzer.send("deployer", {
    type: "ready_for_deploy",
    files: reviewed_files
})

# A2A: Broadcast to all agents
analyzer.broadcast({
    type: "analysis_complete",
    summary: results
})

# A2A: Request from capable agent
response = analyzer.request("database-expert", {
    query: "Optimize this SQL",
    sql: slow_query
}, timeout: 30s)

# A2UI: User notifications
analyzer.notify("Found 3 security issues", {level: "warning"})

# A2UI: User prompts
proceed = analyzer.confirm("Deploy to production?", {severity: "high"})

# A2UI: Progress updates
task = analyzer.progress_start("Analyzing files", total: file_count)
for file in files {
    analyze(file)
    analyzer.progress_update(task, current: idx)
}
analyzer.progress_complete(task, "Analysis complete!")

# A2UI: Rich rendering
analyzer.render_table(findings, {
    columns: ["File", "Issue", "Severity", "Line"],
    sortable: true
})
```

---

### 5. Performance Optimizations

#### 5.1 Message Batching

```rust
impl A2AMessageBus {
    /// Batch multiple messages for efficient delivery
    pub fn send_batch(&self, messages: Vec<A2AMessage>) -> Result<BatchResult>;
    
    /// Receive with batching (reduces syscall overhead)
    pub fn receive_batch(&self, agent_id: &str, max: usize, timeout: Duration) 
        -> Result<Vec<A2AMessage>>;
}
```

#### 5.2 Message Compression

```rust
#[derive(Clone, Copy)]
pub enum Compression {
    None,
    Lz4,    // Fast, good for small messages
    Zstd,   // Better ratio for large messages
}

impl A2AMessage {
    pub fn compress(&self, method: Compression) -> Result<CompressedMessage>;
    pub fn decompress(compressed: &CompressedMessage) -> Result<Self>;
}
```

#### 5.3 Connection Pooling (for distributed A2A)

```rust
pub struct DistributedA2ABus {
    local_bus: A2AMessageBus,
    remote_connections: ConnectionPool,
    routing_table: RoutingTable,
}

impl DistributedA2ABus {
    /// Send to agent (local or remote)
    pub async fn send(&self, to: &str, msg: A2AMessage) -> Result<()> {
        if self.routing_table.is_local(to) {
            self.local_bus.send(msg)
        } else {
            let conn = self.remote_connections.get(to).await?;
            conn.send(msg).await
        }
    }
}
```

---

### 6. Observability & Debugging

#### 6.1 Message Tracing

```rust
#[derive(Debug, Clone)]
pub struct MessageTrace {
    pub message_id: Uuid,
    pub correlation_id: Option<Uuid>,
    pub hops: Vec<TraceHop>,
    pub total_latency: Duration,
}

#[derive(Debug, Clone)]
pub struct TraceHop {
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub action: TraceAction,
    pub duration: Duration,
}

impl A2AMessageBus {
    /// Enable tracing for debugging
    pub fn enable_tracing(&self, config: TracingConfig);
    
    /// Get trace for message
    pub fn get_trace(&self, message_id: Uuid) -> Option<MessageTrace>;
}
```

#### 6.2 Builtin Debugging Commands

```ae
# View message bus state
a2a_debug_bus()

# Trace specific message
a2a_trace(message_id)

# View agent mailbox
a2a_debug_mailbox("analyzer")

# View A2UI event queue
a2ui_debug_events()

# Performance metrics
a2a_metrics()  # throughput, latency, queue depths
```

---

## Implementation Roadmap

### Phase 1: A2UI Implementation (1-2 weeks)
1. Create `src/ai/a2ui.rs` with core types
2. Implement A2UI channel with TUI integration
3. Add A2UI builtins to `builtins.rs`
4. Write tests for A2UI protocol

### Phase 2: A2A Optimizations (1-2 weeks)
1. Replace Mutex with crossbeam channels
2. Add message prioritization and TTL
3. Implement topic-based routing
4. Add typed message payloads

### Phase 3: Unified Interface (1 week)
1. Create `AgentContext` unified interface
2. Update agent builtin to use new context
3. Add convenience methods for common patterns
4. Update documentation

### Phase 4: Advanced Features (2 weeks)
1. Implement distributed A2A bus
2. Add message batching and compression
3. Implement tracing and observability
4. Performance benchmarks and tuning

---

## API Changes Summary

### New Modules
- `src/ai/a2ui.rs` - A2UI protocol implementation
- `src/ai/a2a_router.rs` - Advanced routing strategies
- `src/ai/agent_context.rs` - Unified agent interface

### New Builtins (24 total)
- **A2UI (15)**: `a2ui_notify`, `a2ui_toast`, `a2ui_prompt`, `a2ui_confirm`, `a2ui_select`, `a2ui_multiselect`, `a2ui_progress_start`, `a2ui_progress_update`, `a2ui_progress_complete`, `a2ui_spinner`, `a2ui_table`, `a2ui_tree`, `a2ui_chart`, `a2ui_code`, `a2ui_markdown`
- **A2A Enhanced (5)**: `a2a_publish`, `a2a_subscribe`, `a2a_request`, `a2a_route`, `a2a_trace`
- **Debug (4)**: `a2a_debug_bus`, `a2a_debug_mailbox`, `a2ui_debug_events`, `a2a_metrics`

### Breaking Changes
- None (all additions are backwards compatible)

---

## Alternatives Considered

1. **gRPC for A2A**: More overhead, overkill for in-process communication
2. **Actor Model (Actix)**: Good fit but large dependency
3. **Shared Memory Only**: Too limited for distributed scenarios

---

## References

- [Google A2A Protocol Spec](https://github.com/google/A2A) (inspiration)
- [Model Context Protocol](https://modelcontextprotocol.io/)
- [Crossbeam Channels](https://docs.rs/crossbeam-channel)
- [Tokio Broadcast](https://docs.rs/tokio/latest/tokio/sync/broadcast/)
