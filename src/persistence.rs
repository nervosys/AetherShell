//! Agent Persistence and State Management
//!
//! This module provides comprehensive persistence capabilities for AI agents:
//!
//! - **State Snapshots**: Serialize and restore complete agent state
//! - **Conversation History**: Store and retrieve conversation threads
//! - **Checkpointing**: Automatic and manual checkpoints for recovery
//! - **State Versioning**: Track state changes over time
//! - **Storage Backends**: SQLite, file-based, and in-memory options
//! - **Migration Support**: Handle schema changes between versions

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::value::Value;

// =============================================================================
// Core Types
// =============================================================================

/// Unique identifier for agent instances
pub type AgentId = String;

/// Unique identifier for conversations
pub type ConversationId = String;

/// Unique identifier for checkpoints
pub type CheckpointId = String;

/// Agent state that can be persisted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Agent identifier
    pub agent_id: AgentId,
    /// Agent name
    pub name: String,
    /// Agent type/model
    pub agent_type: String,
    /// System prompt/instructions
    pub system_prompt: Option<String>,
    /// Agent capabilities
    pub capabilities: Vec<String>,
    /// Configuration parameters
    pub config: HashMap<String, Value>,
    /// Custom state data
    pub custom_state: HashMap<String, Value>,
    /// Active conversation IDs
    pub active_conversations: Vec<ConversationId>,
    /// Creation timestamp
    pub created_at: u64,
    /// Last modified timestamp
    pub modified_at: u64,
    /// State version for migrations
    pub version: u32,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl AgentState {
    /// Create a new agent state
    pub fn new(agent_id: AgentId, name: &str, agent_type: &str) -> Self {
        let now = current_timestamp();
        Self {
            agent_id,
            name: name.to_string(),
            agent_type: agent_type.to_string(),
            system_prompt: None,
            capabilities: Vec::new(),
            config: HashMap::new(),
            custom_state: HashMap::new(),
            active_conversations: Vec::new(),
            created_at: now,
            modified_at: now,
            version: 1,
            metadata: HashMap::new(),
        }
    }

    /// Update the modified timestamp
    pub fn touch(&mut self) {
        self.modified_at = current_timestamp();
    }

    /// Set a custom state value
    pub fn set_state(&mut self, key: &str, value: Value) {
        self.custom_state.insert(key.to_string(), value);
        self.touch();
    }

    /// Get a custom state value
    pub fn get_state(&self, key: &str) -> Option<&Value> {
        self.custom_state.get(key)
    }
}

// =============================================================================
// Conversation Types
// =============================================================================

/// A conversation thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Conversation identifier
    pub id: ConversationId,
    /// Agent that owns this conversation
    pub agent_id: AgentId,
    /// Conversation title
    pub title: Option<String>,
    /// Messages in chronological order
    pub messages: Vec<Message>,
    /// Conversation metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: u64,
    /// Last message timestamp
    pub last_message_at: u64,
    /// Is the conversation archived
    pub archived: bool,
    /// Tags for organization
    pub tags: Vec<String>,
}

impl Conversation {
    /// Create a new conversation
    pub fn new(agent_id: AgentId) -> Self {
        let now = current_timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            agent_id,
            title: None,
            messages: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            last_message_at: now,
            archived: false,
            tags: Vec::new(),
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: Message) {
        self.last_message_at = current_timestamp();
        self.messages.push(message);
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Generate a title from the first user message
    pub fn auto_title(&mut self) {
        if self.title.is_none() {
            if let Some(first_user_msg) = self.messages.iter().find(|m| m.role == MessageRole::User)
            {
                let content = &first_user_msg.content;
                let title = if content.len() > 50 {
                    format!("{}...", &content[..47])
                } else {
                    content.clone()
                };
                self.title = Some(title);
            }
        }
    }
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message identifier
    pub id: String,
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: u64,
    /// Token count (if available)
    pub tokens: Option<u32>,
    /// Attachments (images, files, etc.)
    pub attachments: Vec<Attachment>,
    /// Tool calls made in this message
    pub tool_calls: Vec<ToolCall>,
    /// Parent message ID (for branching conversations)
    pub parent_id: Option<String>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl Message {
    /// Create a new user message
    pub fn user(content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: current_timestamp(),
            tokens: None,
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: current_timestamp(),
            tokens: None,
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new system message
    pub fn system(content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: current_timestamp(),
            tokens: None,
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new tool result message
    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("tool_call_id".to_string(), tool_call_id.to_string());

        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Tool,
            content: content.to_string(),
            timestamp: current_timestamp(),
            tokens: None,
            attachments: Vec::new(),
            tool_calls: Vec::new(),
            parent_id: None,
            metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// An attachment to a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub attachment_type: AttachmentType,
    pub name: String,
    pub mime_type: String,
    /// Content (base64 encoded for binary)
    pub content: Option<String>,
    /// External URL reference
    pub url: Option<String>,
    /// Size in bytes
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttachmentType {
    Image,
    Audio,
    Video,
    File,
    Code,
}

/// A tool call made by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: HashMap<String, Value>,
    pub result: Option<String>,
    pub status: ToolCallStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

// =============================================================================
// Checkpoint Types
// =============================================================================

/// A checkpoint for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint identifier
    pub id: CheckpointId,
    /// Agent this checkpoint belongs to
    pub agent_id: AgentId,
    /// Checkpoint name/label
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Serialized agent state
    pub agent_state: AgentState,
    /// Serialized conversation states
    pub conversations: Vec<Conversation>,
    /// Checkpoint type
    pub checkpoint_type: CheckpointType,
    /// Creation timestamp
    pub created_at: u64,
    /// Size in bytes
    pub size_bytes: u64,
    /// Checksum for integrity
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CheckpointType {
    /// Automatic checkpoint (scheduled)
    Automatic,
    /// Manual checkpoint (user-triggered)
    Manual,
    /// Pre-operation checkpoint (before risky operation)
    PreOperation,
    /// Recovery checkpoint (after error recovery)
    Recovery,
}

// =============================================================================
// Storage Backend Trait
// =============================================================================

/// Storage backend for persistence
#[async_trait::async_trait]
pub trait PersistenceBackend: Send + Sync {
    /// Initialize the storage backend
    async fn initialize(&self) -> Result<()>;

    /// Save agent state
    async fn save_agent_state(&self, state: &AgentState) -> Result<()>;

    /// Load agent state
    async fn load_agent_state(&self, agent_id: &AgentId) -> Result<Option<AgentState>>;

    /// Delete agent state
    async fn delete_agent_state(&self, agent_id: &AgentId) -> Result<()>;

    /// List all agent IDs
    async fn list_agents(&self) -> Result<Vec<AgentId>>;

    /// Save conversation
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()>;

    /// Load conversation
    async fn load_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>>;

    /// Delete conversation
    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()>;

    /// List conversations for an agent
    async fn list_conversations(&self, agent_id: &AgentId) -> Result<Vec<ConversationId>>;

    /// Save checkpoint
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;

    /// Load checkpoint
    async fn load_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<Option<Checkpoint>>;

    /// Delete checkpoint
    async fn delete_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<()>;

    /// List checkpoints for an agent
    async fn list_checkpoints(&self, agent_id: &AgentId) -> Result<Vec<CheckpointId>>;

    /// Search conversations by content
    async fn search_conversations(
        &self,
        agent_id: &AgentId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationId>>;

    /// Get storage statistics
    async fn get_stats(&self) -> Result<StorageStats>;
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub agent_count: usize,
    pub conversation_count: usize,
    pub checkpoint_count: usize,
    pub total_messages: usize,
    pub total_size_bytes: u64,
}

// =============================================================================
// File-based Storage Backend
// =============================================================================

/// File-based persistence backend
pub struct FileBackend {
    base_path: PathBuf,
}

impl FileBackend {
    /// Create a new file backend
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    fn agents_path(&self) -> PathBuf {
        self.base_path.join("agents")
    }

    fn conversations_path(&self) -> PathBuf {
        self.base_path.join("conversations")
    }

    fn checkpoints_path(&self) -> PathBuf {
        self.base_path.join("checkpoints")
    }

    fn agent_file(&self, agent_id: &str) -> PathBuf {
        self.agents_path().join(format!("{}.json", agent_id))
    }

    fn conversation_file(&self, conversation_id: &str) -> PathBuf {
        self.conversations_path()
            .join(format!("{}.json", conversation_id))
    }

    fn checkpoint_file(&self, checkpoint_id: &str) -> PathBuf {
        self.checkpoints_path()
            .join(format!("{}.json", checkpoint_id))
    }
}

#[async_trait::async_trait]
impl PersistenceBackend for FileBackend {
    async fn initialize(&self) -> Result<()> {
        tokio::fs::create_dir_all(self.agents_path()).await?;
        tokio::fs::create_dir_all(self.conversations_path()).await?;
        tokio::fs::create_dir_all(self.checkpoints_path()).await?;
        Ok(())
    }

    async fn save_agent_state(&self, state: &AgentState) -> Result<()> {
        let path = self.agent_file(&state.agent_id);
        let json = serde_json::to_string_pretty(state)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn load_agent_state(&self, agent_id: &AgentId) -> Result<Option<AgentState>> {
        let path = self.agent_file(agent_id);
        if path.exists() {
            let json = tokio::fs::read_to_string(path).await?;
            let state: AgentState = serde_json::from_str(&json)?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    async fn delete_agent_state(&self, agent_id: &AgentId) -> Result<()> {
        let path = self.agent_file(agent_id);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentId>> {
        let mut agents = Vec::new();
        let path = self.agents_path();

        if path.exists() {
            let mut dir = tokio::fs::read_dir(path).await?;
            while let Some(entry) = dir.next_entry().await? {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        agents.push(name.trim_end_matches(".json").to_string());
                    }
                }
            }
        }

        Ok(agents)
    }

    async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        let path = self.conversation_file(&conversation.id);
        let json = serde_json::to_string_pretty(conversation)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn load_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>> {
        let path = self.conversation_file(conversation_id);
        if path.exists() {
            let json = tokio::fs::read_to_string(path).await?;
            let conv: Conversation = serde_json::from_str(&json)?;
            Ok(Some(conv))
        } else {
            Ok(None)
        }
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        let path = self.conversation_file(conversation_id);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list_conversations(&self, agent_id: &AgentId) -> Result<Vec<ConversationId>> {
        let mut conversations = Vec::new();
        let path = self.conversations_path();

        if path.exists() {
            let mut dir = tokio::fs::read_dir(path).await?;
            while let Some(entry) = dir.next_entry().await? {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let conv_id = name.trim_end_matches(".json").to_string();
                        // Load and check if it belongs to this agent
                        if let Ok(Some(conv)) = self.load_conversation(&conv_id).await {
                            if conv.agent_id == *agent_id {
                                conversations.push(conv_id);
                            }
                        }
                    }
                }
            }
        }

        Ok(conversations)
    }

    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let path = self.checkpoint_file(&checkpoint.id);
        let json = serde_json::to_string_pretty(checkpoint)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn load_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<Option<Checkpoint>> {
        let path = self.checkpoint_file(checkpoint_id);
        if path.exists() {
            let json = tokio::fs::read_to_string(path).await?;
            let checkpoint: Checkpoint = serde_json::from_str(&json)?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    async fn delete_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<()> {
        let path = self.checkpoint_file(checkpoint_id);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list_checkpoints(&self, agent_id: &AgentId) -> Result<Vec<CheckpointId>> {
        let mut checkpoints = Vec::new();
        let path = self.checkpoints_path();

        if path.exists() {
            let mut dir = tokio::fs::read_dir(path).await?;
            while let Some(entry) = dir.next_entry().await? {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let cp_id = name.trim_end_matches(".json").to_string();
                        if let Ok(Some(cp)) = self.load_checkpoint(&cp_id).await {
                            if cp.agent_id == *agent_id {
                                checkpoints.push(cp_id);
                            }
                        }
                    }
                }
            }
        }

        Ok(checkpoints)
    }

    async fn search_conversations(
        &self,
        agent_id: &AgentId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationId>> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        let conversations = self.list_conversations(agent_id).await?;
        for conv_id in conversations {
            if results.len() >= limit {
                break;
            }

            if let Ok(Some(conv)) = self.load_conversation(&conv_id).await {
                let matches = conv
                    .messages
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&query_lower));

                if matches {
                    results.push(conv_id);
                }
            }
        }

        Ok(results)
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let agents = self.list_agents().await?;
        let mut conversation_count = 0;
        let mut checkpoint_count = 0;
        let mut total_messages = 0;
        let mut total_size = 0u64;

        for agent_id in &agents {
            let conversations = self.list_conversations(agent_id).await?;
            conversation_count += conversations.len();

            for conv_id in conversations {
                if let Ok(Some(conv)) = self.load_conversation(&conv_id).await {
                    total_messages += conv.messages.len();
                }
                let path = self.conversation_file(&conv_id);
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    total_size += meta.len();
                }
            }

            let checkpoints = self.list_checkpoints(agent_id).await?;
            checkpoint_count += checkpoints.len();

            for cp_id in checkpoints {
                let path = self.checkpoint_file(&cp_id);
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    total_size += meta.len();
                }
            }

            let agent_path = self.agent_file(agent_id);
            if let Ok(meta) = tokio::fs::metadata(&agent_path).await {
                total_size += meta.len();
            }
        }

        Ok(StorageStats {
            agent_count: agents.len(),
            conversation_count,
            checkpoint_count,
            total_messages,
            total_size_bytes: total_size,
        })
    }
}

// =============================================================================
// In-Memory Storage Backend
// =============================================================================

/// In-memory persistence backend (for testing)
pub struct MemoryBackend {
    agents: Arc<RwLock<HashMap<AgentId, AgentState>>>,
    conversations: Arc<RwLock<HashMap<ConversationId, Conversation>>>,
    checkpoints: Arc<RwLock<HashMap<CheckpointId, Checkpoint>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            conversations: Arc::new(RwLock::new(HashMap::new())),
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PersistenceBackend for MemoryBackend {
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }

    async fn save_agent_state(&self, state: &AgentState) -> Result<()> {
        self.agents
            .write()
            .await
            .insert(state.agent_id.clone(), state.clone());
        Ok(())
    }

    async fn load_agent_state(&self, agent_id: &AgentId) -> Result<Option<AgentState>> {
        Ok(self.agents.read().await.get(agent_id).cloned())
    }

    async fn delete_agent_state(&self, agent_id: &AgentId) -> Result<()> {
        self.agents.write().await.remove(agent_id);
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentId>> {
        Ok(self.agents.read().await.keys().cloned().collect())
    }

    async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        self.conversations
            .write()
            .await
            .insert(conversation.id.clone(), conversation.clone());
        Ok(())
    }

    async fn load_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>> {
        Ok(self
            .conversations
            .read()
            .await
            .get(conversation_id)
            .cloned())
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        self.conversations.write().await.remove(conversation_id);
        Ok(())
    }

    async fn list_conversations(&self, agent_id: &AgentId) -> Result<Vec<ConversationId>> {
        Ok(self
            .conversations
            .read()
            .await
            .values()
            .filter(|c| c.agent_id == *agent_id)
            .map(|c| c.id.clone())
            .collect())
    }

    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        self.checkpoints
            .write()
            .await
            .insert(checkpoint.id.clone(), checkpoint.clone());
        Ok(())
    }

    async fn load_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<Option<Checkpoint>> {
        Ok(self.checkpoints.read().await.get(checkpoint_id).cloned())
    }

    async fn delete_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<()> {
        self.checkpoints.write().await.remove(checkpoint_id);
        Ok(())
    }

    async fn list_checkpoints(&self, agent_id: &AgentId) -> Result<Vec<CheckpointId>> {
        Ok(self
            .checkpoints
            .read()
            .await
            .values()
            .filter(|c| c.agent_id == *agent_id)
            .map(|c| c.id.clone())
            .collect())
    }

    async fn search_conversations(
        &self,
        agent_id: &AgentId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationId>> {
        let query_lower = query.to_lowercase();

        Ok(self
            .conversations
            .read()
            .await
            .values()
            .filter(|c| c.agent_id == *agent_id)
            .filter(|c| {
                c.messages
                    .iter()
                    .any(|m| m.content.to_lowercase().contains(&query_lower))
            })
            .take(limit)
            .map(|c| c.id.clone())
            .collect())
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let agents = self.agents.read().await;
        let conversations = self.conversations.read().await;
        let checkpoints = self.checkpoints.read().await;

        let total_messages: usize = conversations.values().map(|c| c.messages.len()).sum();

        // Estimate size
        let estimated_size = (agents.len() * 1000
            + conversations.len() * 500
            + total_messages * 200
            + checkpoints.len() * 5000) as u64;

        Ok(StorageStats {
            agent_count: agents.len(),
            conversation_count: conversations.len(),
            checkpoint_count: checkpoints.len(),
            total_messages,
            total_size_bytes: estimated_size,
        })
    }
}

// =============================================================================
// Persistence Manager
// =============================================================================

/// Configuration for automatic checkpointing
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Enable automatic checkpoints
    pub enabled: bool,
    /// Interval between automatic checkpoints (seconds)
    pub interval_secs: u64,
    /// Maximum checkpoints to keep per agent
    pub max_checkpoints: usize,
    /// Create checkpoint before risky operations
    pub checkpoint_before_operations: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 300, // 5 minutes
            max_checkpoints: 10,
            checkpoint_before_operations: true,
        }
    }
}

/// Main persistence manager
pub struct PersistenceManager {
    backend: Arc<dyn PersistenceBackend>,
    checkpoint_config: CheckpointConfig,
    running: Arc<RwLock<bool>>,
}

impl PersistenceManager {
    /// Create a new persistence manager with the given backend
    pub fn new(backend: Arc<dyn PersistenceBackend>) -> Self {
        Self {
            backend,
            checkpoint_config: CheckpointConfig::default(),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with file backend
    pub fn with_file_backend(path: impl AsRef<Path>) -> Self {
        Self::new(Arc::new(FileBackend::new(path)))
    }

    /// Create with memory backend
    pub fn with_memory_backend() -> Self {
        Self::new(Arc::new(MemoryBackend::new()))
    }

    /// Set checkpoint configuration
    pub fn set_checkpoint_config(&mut self, config: CheckpointConfig) {
        self.checkpoint_config = config;
    }

    /// Initialize the persistence system
    pub async fn initialize(&self) -> Result<()> {
        self.backend.initialize().await
    }

    // === Agent State Management ===

    /// Save agent state
    pub async fn save_agent(&self, state: &AgentState) -> Result<()> {
        self.backend.save_agent_state(state).await
    }

    /// Load agent state
    pub async fn load_agent(&self, agent_id: &AgentId) -> Result<Option<AgentState>> {
        self.backend.load_agent_state(agent_id).await
    }

    /// Delete agent and all associated data
    pub async fn delete_agent(&self, agent_id: &AgentId) -> Result<()> {
        // Delete conversations
        let conversations = self.backend.list_conversations(agent_id).await?;
        for conv_id in conversations {
            self.backend.delete_conversation(&conv_id).await?;
        }

        // Delete checkpoints
        let checkpoints = self.backend.list_checkpoints(agent_id).await?;
        for cp_id in checkpoints {
            self.backend.delete_checkpoint(&cp_id).await?;
        }

        // Delete agent state
        self.backend.delete_agent_state(agent_id).await
    }

    /// List all agents
    pub async fn list_agents(&self) -> Result<Vec<AgentId>> {
        self.backend.list_agents().await
    }

    // === Conversation Management ===

    /// Save conversation
    pub async fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        self.backend.save_conversation(conversation).await
    }

    /// Load conversation
    pub async fn load_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Conversation>> {
        self.backend.load_conversation(conversation_id).await
    }

    /// Delete conversation
    pub async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        self.backend.delete_conversation(conversation_id).await
    }

    /// List conversations for an agent
    pub async fn list_conversations(&self, agent_id: &AgentId) -> Result<Vec<ConversationId>> {
        self.backend.list_conversations(agent_id).await
    }

    /// Search conversations
    pub async fn search_conversations(
        &self,
        agent_id: &AgentId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationId>> {
        self.backend
            .search_conversations(agent_id, query, limit)
            .await
    }

    /// Add a message to a conversation
    pub async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: Message,
    ) -> Result<()> {
        let mut conversation = self
            .backend
            .load_conversation(conversation_id)
            .await?
            .ok_or_else(|| anyhow!("Conversation not found"))?;

        conversation.add_message(message);
        self.backend.save_conversation(&conversation).await
    }

    // === Checkpoint Management ===

    /// Create a checkpoint
    pub async fn create_checkpoint(
        &self,
        agent_id: &AgentId,
        name: &str,
        checkpoint_type: CheckpointType,
    ) -> Result<CheckpointId> {
        let agent_state = self
            .backend
            .load_agent_state(agent_id)
            .await?
            .ok_or_else(|| anyhow!("Agent not found"))?;

        let conversation_ids = self.backend.list_conversations(agent_id).await?;
        let mut conversations = Vec::new();
        for conv_id in conversation_ids {
            if let Some(conv) = self.backend.load_conversation(&conv_id).await? {
                conversations.push(conv);
            }
        }

        let checkpoint_id = Uuid::new_v4().to_string();
        let json = serde_json::to_string(&(&agent_state, &conversations))?;
        let checksum = integrity_checksum(&json);
        let size_bytes = json.len() as u64;

        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            agent_id: agent_id.clone(),
            name: name.to_string(),
            description: None,
            agent_state,
            conversations,
            checkpoint_type,
            created_at: current_timestamp(),
            size_bytes,
            checksum,
        };

        self.backend.save_checkpoint(&checkpoint).await?;

        // Cleanup old checkpoints if needed
        self.cleanup_old_checkpoints(agent_id).await?;

        Ok(checkpoint_id)
    }

    /// Restore from a checkpoint
    pub async fn restore_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<AgentId> {
        let checkpoint = self
            .backend
            .load_checkpoint(checkpoint_id)
            .await?
            .ok_or_else(|| anyhow!("Checkpoint not found"))?;

        // Verify checksum
        let json = serde_json::to_string(&(&checkpoint.agent_state, &checkpoint.conversations))?;
        if !verify_integrity(&json, &checkpoint.checksum) {
            return Err(anyhow!("Checkpoint integrity check failed"));
        }

        // Restore agent state
        self.backend
            .save_agent_state(&checkpoint.agent_state)
            .await?;

        // Restore conversations
        for conversation in &checkpoint.conversations {
            self.backend.save_conversation(conversation).await?;
        }

        Ok(checkpoint.agent_id)
    }

    /// List checkpoints for an agent
    pub async fn list_checkpoints(&self, agent_id: &AgentId) -> Result<Vec<CheckpointId>> {
        self.backend.list_checkpoints(agent_id).await
    }

    /// Delete a checkpoint
    pub async fn delete_checkpoint(&self, checkpoint_id: &CheckpointId) -> Result<()> {
        self.backend.delete_checkpoint(checkpoint_id).await
    }

    /// Cleanup old checkpoints, keeping only the most recent ones
    async fn cleanup_old_checkpoints(&self, agent_id: &AgentId) -> Result<()> {
        let checkpoint_ids = self.backend.list_checkpoints(agent_id).await?;

        if checkpoint_ids.len() <= self.checkpoint_config.max_checkpoints {
            return Ok(());
        }

        // Load all checkpoints and sort by creation time
        let mut checkpoints: Vec<Checkpoint> = Vec::new();
        for cp_id in checkpoint_ids {
            if let Some(cp) = self.backend.load_checkpoint(&cp_id).await? {
                checkpoints.push(cp);
            }
        }

        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Delete excess checkpoints
        for checkpoint in checkpoints
            .iter()
            .skip(self.checkpoint_config.max_checkpoints)
        {
            self.backend.delete_checkpoint(&checkpoint.id).await?;
        }

        Ok(())
    }

    // === Automatic Checkpointing ===

    /// Start automatic checkpoint scheduler
    pub async fn start_auto_checkpoint(&self, agent_ids: Vec<AgentId>) {
        if !self.checkpoint_config.enabled {
            return;
        }

        *self.running.write().await = true;

        let backend = Arc::clone(&self.backend);
        let running = Arc::clone(&self.running);
        let interval = Duration::from_secs(self.checkpoint_config.interval_secs);
        let _max_checkpoints = self.checkpoint_config.max_checkpoints;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            while *running.read().await {
                ticker.tick().await;

                for agent_id in &agent_ids {
                    // Create automatic checkpoint
                    if let Ok(Some(agent_state)) = backend.load_agent_state(agent_id).await {
                        let conversation_ids = backend
                            .list_conversations(agent_id)
                            .await
                            .unwrap_or_default();
                        let mut conversations = Vec::new();
                        for conv_id in conversation_ids {
                            if let Ok(Some(conv)) = backend.load_conversation(&conv_id).await {
                                conversations.push(conv);
                            }
                        }

                        let checkpoint_id = Uuid::new_v4().to_string();
                        let json = serde_json::to_string(&(&agent_state, &conversations))
                            .unwrap_or_default();
                        let checksum = integrity_checksum(&json);
                        let size_bytes = json.len() as u64;

                        let checkpoint = Checkpoint {
                            id: checkpoint_id,
                            agent_id: agent_id.clone(),
                            name: format!(
                                "Auto checkpoint {}",
                                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                            ),
                            description: Some("Automatic checkpoint".to_string()),
                            agent_state,
                            conversations,
                            checkpoint_type: CheckpointType::Automatic,
                            created_at: current_timestamp(),
                            size_bytes,
                            checksum,
                        };

                        let _ = backend.save_checkpoint(&checkpoint).await;
                    }
                }
            }
        });
    }

    /// Stop automatic checkpointing
    pub async fn stop_auto_checkpoint(&self) {
        *self.running.write().await = false;
    }

    // === Statistics ===

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<StorageStats> {
        self.backend.get_stats().await
    }
}

// =============================================================================
// Builtin Functions
// =============================================================================

/// Create builtin functions for persistence
pub fn persistence_builtins() -> Vec<(&'static str, &'static str)> {
    vec![
        ("agent_save", "Save agent state to persistent storage"),
        ("agent_load", "Load agent state from persistent storage"),
        ("agent_delete", "Delete agent and all associated data"),
        ("agent_list", "List all persisted agents"),
        ("conversation_save", "Save a conversation"),
        ("conversation_load", "Load a conversation"),
        ("conversation_delete", "Delete a conversation"),
        ("conversation_list", "List conversations for an agent"),
        ("conversation_search", "Search conversations by content"),
        ("checkpoint_create", "Create a checkpoint for recovery"),
        ("checkpoint_restore", "Restore from a checkpoint"),
        ("checkpoint_list", "List checkpoints for an agent"),
        ("checkpoint_delete", "Delete a checkpoint"),
        ("persistence_stats", "Get storage statistics"),
    ]
}

// =============================================================================
// Helper Functions
// =============================================================================

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Compute the integrity checksum for persisted data — **SHA-256** (FIPS-approved),
/// hex-encoded. Replaces the legacy MD5 checksum, which is collision-broken and
/// therefore forgeable as an integrity guard.
fn integrity_checksum(data: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Verify `data` against a `stored` checksum. New checkpoints carry a 64-hex
/// SHA-256 digest; **legacy** checkpoints written before the migration carry a
/// 32-hex MD5 digest, which is still accepted on read (by length) so existing
/// state validates and re-saves forward to SHA-256.
fn verify_integrity(data: &str, stored: &str) -> bool {
    match stored.len() {
        64 => integrity_checksum(data) == stored,
        32 => format!("{:x}", md5::compute(data)) == stored,
        _ => false,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_creation() {
        let state = AgentState::new("agent-1".to_string(), "Test Agent", "gpt-4");
        assert_eq!(state.agent_id, "agent-1");
        assert_eq!(state.name, "Test Agent");
        assert_eq!(state.agent_type, "gpt-4");
        assert_eq!(state.version, 1);
    }

    #[test]
    fn test_agent_state_custom_state() {
        let mut state = AgentState::new("agent-1".to_string(), "Test", "gpt-4");
        state.set_state("counter", Value::Int(42));

        assert_eq!(state.get_state("counter"), Some(&Value::Int(42)));
        assert_eq!(state.get_state("nonexistent"), None);
    }

    #[test]
    fn test_conversation_creation() {
        let conv = Conversation::new("agent-1".to_string());
        assert!(!conv.id.is_empty());
        assert_eq!(conv.agent_id, "agent-1");
        assert!(conv.messages.is_empty());
        assert!(!conv.archived);
    }

    #[test]
    fn test_conversation_add_message() {
        let mut conv = Conversation::new("agent-1".to_string());
        conv.add_message(Message::user("Hello"));
        conv.add_message(Message::assistant("Hi there!"));

        assert_eq!(conv.message_count(), 2);
    }

    #[test]
    fn test_conversation_auto_title() {
        let mut conv = Conversation::new("agent-1".to_string());
        conv.add_message(Message::user("What is the weather like today?"));
        conv.auto_title();

        assert_eq!(
            conv.title,
            Some("What is the weather like today?".to_string())
        );
    }

    #[test]
    fn test_conversation_auto_title_truncation() {
        let mut conv = Conversation::new("agent-1".to_string());
        conv.add_message(Message::user(
            "This is a very long message that should be truncated when used as a title",
        ));
        conv.auto_title();

        assert!(conv.title.as_ref().unwrap().len() <= 50);
        assert!(conv.title.as_ref().unwrap().ends_with("..."));
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there!");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, MessageRole::System);
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("call-123", "Result: 42");
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(
            msg.metadata.get("tool_call_id"),
            Some(&"call-123".to_string())
        );
    }

    #[test]
    fn test_checkpoint_config_default() {
        let config = CheckpointConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 300);
        assert_eq!(config.max_checkpoints, 10);
    }

    #[tokio::test]
    async fn test_memory_backend_agent_operations() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        let state = AgentState::new("agent-1".to_string(), "Test", "gpt-4");
        backend.save_agent_state(&state).await.unwrap();

        let loaded = backend
            .load_agent_state(&"agent-1".to_string())
            .await
            .unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "Test");

        let agents = backend.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1);

        backend
            .delete_agent_state(&"agent-1".to_string())
            .await
            .unwrap();
        let loaded = backend
            .load_agent_state(&"agent-1".to_string())
            .await
            .unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_memory_backend_conversation_operations() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        let mut conv = Conversation::new("agent-1".to_string());
        conv.add_message(Message::user("Hello"));
        let conv_id = conv.id.clone();

        backend.save_conversation(&conv).await.unwrap();

        let loaded = backend.load_conversation(&conv_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().messages.len(), 1);

        let convs = backend
            .list_conversations(&"agent-1".to_string())
            .await
            .unwrap();
        assert_eq!(convs.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_backend_search() {
        let backend = MemoryBackend::new();
        backend.initialize().await.unwrap();

        let mut conv1 = Conversation::new("agent-1".to_string());
        conv1.add_message(Message::user("Hello world"));
        backend.save_conversation(&conv1).await.unwrap();

        let mut conv2 = Conversation::new("agent-1".to_string());
        conv2.add_message(Message::user("Goodbye universe"));
        backend.save_conversation(&conv2).await.unwrap();

        let results = backend
            .search_conversations(&"agent-1".to_string(), "world", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], conv1.id);
    }

    #[tokio::test]
    async fn test_persistence_manager_basic() {
        let manager = PersistenceManager::with_memory_backend();
        manager.initialize().await.unwrap();

        let state = AgentState::new("agent-1".to_string(), "Test", "gpt-4");
        manager.save_agent(&state).await.unwrap();

        let loaded = manager.load_agent(&"agent-1".to_string()).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_persistence_manager_checkpoint() {
        let manager = PersistenceManager::with_memory_backend();
        manager.initialize().await.unwrap();

        let state = AgentState::new("agent-1".to_string(), "Test", "gpt-4");
        manager.save_agent(&state).await.unwrap();

        let checkpoint_id = manager
            .create_checkpoint(
                &"agent-1".to_string(),
                "Test checkpoint",
                CheckpointType::Manual,
            )
            .await
            .unwrap();

        assert!(!checkpoint_id.is_empty());

        let checkpoints = manager
            .list_checkpoints(&"agent-1".to_string())
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_persistence_manager_stats() {
        let manager = PersistenceManager::with_memory_backend();
        manager.initialize().await.unwrap();

        let state = AgentState::new("agent-1".to_string(), "Test", "gpt-4");
        manager.save_agent(&state).await.unwrap();

        let stats = manager.get_stats().await.unwrap();
        assert_eq!(stats.agent_count, 1);
    }

    #[test]
    fn test_persistence_builtins() {
        let builtins = persistence_builtins();
        assert!(builtins.len() >= 10);
        assert!(builtins.iter().any(|(name, _)| *name == "agent_save"));
        assert!(builtins
            .iter()
            .any(|(name, _)| *name == "checkpoint_create"));
    }

    #[test]
    fn test_message_role_equality() {
        assert_eq!(MessageRole::User, MessageRole::User);
        assert_ne!(MessageRole::User, MessageRole::Assistant);
    }

    #[test]
    fn test_checkpoint_type_equality() {
        assert_eq!(CheckpointType::Manual, CheckpointType::Manual);
        assert_ne!(CheckpointType::Automatic, CheckpointType::Manual);
    }

    #[test]
    fn test_tool_call_status_equality() {
        assert_eq!(ToolCallStatus::Pending, ToolCallStatus::Pending);
        assert_ne!(ToolCallStatus::Running, ToolCallStatus::Completed);
    }

    #[test]
    fn test_attachment_type_equality() {
        assert_eq!(AttachmentType::Image, AttachmentType::Image);
        assert_ne!(AttachmentType::Audio, AttachmentType::Video);
    }
}
