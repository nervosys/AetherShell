//! A2A (Agent-to-Agent) Communication Protocol
//! Direct messaging and delegation between agents in a swarm

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// A2A Message Types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum A2AMessageType {
    /// Direct message to specific agent
    DirectMessage { to: String, content: String },

    /// Broadcast to all agents
    Broadcast { content: String },

    /// Request delegation of task
    Delegate {
        to: String,
        task: String,
        context: serde_json::Value,
    },

    /// Response to delegation
    DelegateResponse {
        from: String,
        result: String,
        success: bool,
    },

    /// Query other agent's capabilities
    QueryCapabilities { to: String },

    /// Response with capabilities
    CapabilitiesResponse {
        from: String,
        capabilities: Vec<String>,
    },

    /// Request for assistance
    AssistRequest { to: String, problem: String },

    /// Offer assistance
    AssistOffer {
        from: String,
        solution: Option<String>,
    },
}

/// A2A Message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    pub id: Uuid,
    pub from: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub msg_type: A2AMessageType,
}

impl A2AMessage {
    pub fn new(from: String, msg_type: A2AMessageType) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            timestamp: chrono::Utc::now(),
            msg_type,
        }
    }

    /// Get recipient(s) of this message
    pub fn recipients(&self) -> Vec<String> {
        match &self.msg_type {
            A2AMessageType::DirectMessage { to, .. }
            | A2AMessageType::Delegate { to, .. }
            | A2AMessageType::QueryCapabilities { to }
            | A2AMessageType::AssistRequest { to, .. } => vec![to.clone()],
            A2AMessageType::Broadcast { .. } => vec![], // Broadcast to all
            A2AMessageType::DelegateResponse { .. }
            | A2AMessageType::CapabilitiesResponse { .. }
            | A2AMessageType::AssistOffer { .. } => vec![], // Response messages don't route
        }
    }

    /// Check if this is a response message
    pub fn is_response(&self) -> bool {
        matches!(
            self.msg_type,
            A2AMessageType::DelegateResponse { .. }
                | A2AMessageType::CapabilitiesResponse { .. }
                | A2AMessageType::AssistOffer { .. }
        )
    }
}

/// A2A Message Bus for agent communication
#[derive(Clone)]
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
        // Store in global message log
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        {
            let mut messages = self
                .messages
                .lock()
                .map_err(|e| anyhow!("Failed to acquire messages lock: {}", e))?;
            messages.push(message.clone());
        }

        // Route to recipient mailboxes
        let recipients = message.recipients();

        if recipients.is_empty() && matches!(message.msg_type, A2AMessageType::Broadcast { .. }) {
            // Broadcast to all mailboxes
            let mut mailboxes = self
                .agent_mailboxes
                .lock()
                .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
            for messages in mailboxes.values_mut() {
                messages.push(message.clone());
            }
        } else {
            // Route to specific recipients
            let mut mailboxes = self
                .agent_mailboxes
                .lock()
                .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
            for recipient in recipients {
                mailboxes
                    .entry(recipient)
                    .or_insert_with(Vec::new)
                    .push(message.clone());
            }
        }

        Ok(())
    }

    /// Receive messages for specific agent
    pub fn receive(&self, agent_id: &str) -> Result<Vec<A2AMessage>> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let mut mailboxes = self
            .agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
        Ok(mailboxes
            .entry(agent_id.to_string())
            .or_insert_with(Vec::new)
            .drain(..)
            .collect())
    }

    /// Peek at messages without removing them
    pub fn peek(&self, agent_id: &str) -> Result<Vec<A2AMessage>> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let mailboxes = self
            .agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
        Ok(mailboxes
            .get(agent_id)
            .map(|msgs| msgs.clone())
            .unwrap_or_default())
    }

    /// Get all messages (for monitoring/debugging)
    pub fn get_all_messages(&self) -> Result<Vec<A2AMessage>> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let messages = self
            .messages
            .lock()
            .map_err(|e| anyhow!("Failed to acquire messages lock: {}", e))?;
        Ok(messages.clone())
    }

    /// Clear all messages
    pub fn clear(&self) -> Result<()> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        self.messages
            .lock()
            .map_err(|e| anyhow!("Failed to acquire messages lock: {}", e))?
            .clear();
        self.agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?
            .clear();
        Ok(())
    }

    /// Get message count for agent
    pub fn message_count(&self, agent_id: &str) -> Result<usize> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let mailboxes = self
            .agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
        Ok(mailboxes.get(agent_id).map(|msgs| msgs.len()).unwrap_or(0))
    }

    /// Register an agent (creates empty mailbox)
    pub fn register_agent(&self, agent_id: &str) -> Result<()> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let mut mailboxes = self
            .agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
        mailboxes
            .entry(agent_id.to_string())
            .or_insert_with(Vec::new);
        Ok(())
    }

    /// Get list of registered agents
    pub fn registered_agents(&self) -> Result<Vec<String>> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let mailboxes = self
            .agent_mailboxes
            .lock()
            .map_err(|e| anyhow!("Failed to acquire mailboxes lock: {}", e))?;
        Ok(mailboxes.keys().cloned().collect())
    }
}

impl Default for A2AMessageBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent with A2A capabilities
pub struct A2AAgent {
    pub id: String,
    pub capabilities: Vec<String>,
    pub message_bus: Arc<A2AMessageBus>,
}

impl A2AAgent {
    pub fn new(id: String, capabilities: Vec<String>, bus: Arc<A2AMessageBus>) -> Result<Self> {
        // Register with the bus
        bus.register_agent(&id)?;

        Ok(Self {
            id,
            capabilities,
            message_bus: bus,
        })
    }

    /// Send direct message to another agent
    pub fn send_message(&self, to: &str, content: String) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::DirectMessage {
                to: to.to_string(),
                content,
            },
        );
        self.message_bus.send(msg)
    }

    /// Delegate task to another agent
    pub fn delegate_task(&self, to: &str, task: String, context: serde_json::Value) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::Delegate {
                to: to.to_string(),
                task,
                context,
            },
        );
        self.message_bus.send(msg)
    }

    /// Query another agent's capabilities
    pub fn query_capabilities(&self, to: &str) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::QueryCapabilities { to: to.to_string() },
        );
        self.message_bus.send(msg)
    }

    /// Respond with capabilities
    pub fn respond_capabilities(&self) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::CapabilitiesResponse {
                from: self.id.clone(),
                capabilities: self.capabilities.clone(),
            },
        );
        self.message_bus.send(msg)
    }

    /// Broadcast to all agents
    pub fn broadcast(&self, content: String) -> Result<()> {
        let msg = A2AMessage::new(self.id.clone(), A2AMessageType::Broadcast { content });
        self.message_bus.send(msg)
    }

    /// Request assistance
    pub fn request_assistance(&self, to: &str, problem: String) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::AssistRequest {
                to: to.to_string(),
                problem,
            },
        );
        self.message_bus.send(msg)
    }

    /// Offer assistance
    pub fn offer_assistance(&self, solution: Option<String>) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::AssistOffer {
                from: self.id.clone(),
                solution,
            },
        );
        self.message_bus.send(msg)
    }

    /// Send delegation response
    pub fn respond_to_delegation(&self, result: String, success: bool) -> Result<()> {
        let msg = A2AMessage::new(
            self.id.clone(),
            A2AMessageType::DelegateResponse {
                from: self.id.clone(),
                result,
                success,
            },
        );
        self.message_bus.send(msg)
    }

    /// Process incoming messages
    pub fn receive_messages(&self) -> Result<Vec<A2AMessage>> {
        self.message_bus.receive(&self.id)
    }

    /// Peek at messages without consuming
    pub fn peek_messages(&self) -> Result<Vec<A2AMessage>> {
        self.message_bus.peek(&self.id)
    }

    /// Get pending message count
    pub fn pending_message_count(&self) -> Result<usize> {
        self.message_bus.message_count(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = A2AMessage::new(
            "agent1".to_string(),
            A2AMessageType::DirectMessage {
                to: "agent2".to_string(),
                content: "Hello".to_string(),
            },
        );
        assert_eq!(msg.from, "agent1");
        assert!(!msg.id.is_nil());
    }

    #[test]
    fn test_message_recipients() {
        let msg = A2AMessage::new(
            "agent1".to_string(),
            A2AMessageType::DirectMessage {
                to: "agent2".to_string(),
                content: "Test".to_string(),
            },
        );
        assert_eq!(msg.recipients(), vec!["agent2".to_string()]);
    }

    #[test]
    fn test_broadcast_recipients() {
        let msg = A2AMessage::new(
            "agent1".to_string(),
            A2AMessageType::Broadcast {
                content: "Broadcast".to_string(),
            },
        );
        assert!(msg.recipients().is_empty()); // Empty means broadcast to all
    }
}
