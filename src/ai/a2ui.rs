//! A2UI (Agent-to-User Interface) Protocol
//!
//! Provides structured communication between AI agents and users through
//! rich UI interactions including notifications, prompts, progress indicators,
//! and content rendering.
//!
//! # Design Principles
//!
//! - **Non-blocking**: Events are queued for async consumption by UI layer
//! - **Typed Events**: Strongly typed event payloads for compile-time safety
//! - **TUI Integration**: Seamless integration with Ratatui-based terminal UI
//! - **Extensible**: Easy to add new event types and handlers
//!
//! # Example Usage
//!
//! ```aether
//! # Send a notification to the user
//! a2ui_notify "Processing complete!" { level: "success" }
//!
//! # Prompt user for input
//! response = a2ui_prompt "Choose an option" { options: ["A", "B", "C"] }
//!
//! # Show progress bar
//! for i in 1..100 {
//!     a2ui_progress "Downloading" { current: i, total: 100 }
//! }
//! ```

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use uuid::Uuid;

// ===================== Event Types =====================

/// Priority levels for A2UI events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum EventPriority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Notification severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotificationLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl std::str::FromStr for NotificationLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "info" => Ok(Self::Info),
            "success" => Ok(Self::Success),
            "warning" | "warn" => Ok(Self::Warning),
            "error" | "err" => Ok(Self::Error),
            _ => Err(anyhow!("Invalid notification level: {}", s)),
        }
    }
}

/// User input prompt types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptType {
    /// Simple text input
    Text { placeholder: Option<String> },
    /// Yes/No confirmation
    Confirm { default: Option<bool> },
    /// Selection from options
    Select {
        options: Vec<String>,
        default: Option<usize>,
    },
    /// Multiple selection
    MultiSelect {
        options: Vec<String>,
        defaults: Vec<usize>,
    },
    /// Password input (hidden)
    Password { placeholder: Option<String> },
    /// File path selection
    FilePath {
        filter: Option<String>,
        must_exist: bool,
    },
}

/// User response to a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptResponse {
    Text(String),
    Confirm(bool),
    Select(usize),
    MultiSelect(Vec<usize>),
    Cancelled,
}

/// Content types for rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderContent {
    /// Plain text
    Text(String),
    /// Markdown formatted text
    Markdown(String),
    /// JSON data (rendered as formatted)
    Json(serde_json::Value),
    /// Table data (rows of records)
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// Code block with syntax highlighting
    Code { language: String, content: String },
    /// Image (base64 or URL)
    Image { data: String, alt: Option<String> },
    /// Agent thinking/reasoning trace
    Thinking {
        steps: Vec<String>,
        final_answer: Option<String>,
    },
}

/// A2UI Event Types - structured payloads for UI interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum A2UIEventType {
    /// Display a notification to the user
    Notify {
        message: String,
        level: NotificationLevel,
        duration_ms: Option<u64>,
    },

    /// Request user input
    Prompt {
        id: Uuid,
        message: String,
        prompt_type: PromptType,
    },

    /// Update progress indicator
    Progress {
        id: Uuid,
        label: String,
        current: u64,
        total: u64,
        message: Option<String>,
    },

    /// Complete a progress indicator
    ProgressComplete { id: Uuid },

    /// Render content in the UI
    Render {
        target: Option<String>,
        content: RenderContent,
        replace: bool,
    },

    /// Clear content from a target area
    Clear { target: Option<String> },

    /// Highlight UI elements (for guidance)
    Highlight {
        element: String,
        message: Option<String>,
    },

    /// Update status bar
    Status {
        text: String,
        section: Option<String>,
    },

    /// Open a modal dialog
    Modal {
        id: Uuid,
        title: String,
        content: RenderContent,
        buttons: Vec<(String, String)>, // (label, action_id)
    },

    /// Close a modal dialog
    ModalClose { id: Uuid },

    /// Toast notification (auto-dismiss)
    Toast {
        message: String,
        level: NotificationLevel,
        duration_ms: u64,
    },

    /// Agent started working
    AgentStarted {
        agent_id: String,
        task: Option<String>,
    },

    /// Agent completed work
    AgentCompleted {
        agent_id: String,
        result: Option<String>,
        success: bool,
    },

    /// Agent is thinking (streaming thought process)
    AgentThinking {
        agent_id: String,
        thought: String,
        step: usize,
    },

    /// Request focus on a UI element
    Focus { element: String },

    /// Scroll to a specific location
    ScrollTo { element: String, position: String },
}

// ===================== Event Container =====================

/// A2UI Event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2UIEvent {
    pub id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub priority: EventPriority,
    pub event_type: A2UIEventType,
}

impl A2UIEvent {
    pub fn new(source: impl Into<String>, event_type: A2UIEventType) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            source: source.into(),
            priority: EventPriority::default(),
            event_type,
        }
    }

    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Create a notification event
    pub fn notify(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            source,
            A2UIEventType::Notify {
                message: message.into(),
                level: NotificationLevel::Info,
                duration_ms: None,
            },
        )
    }

    /// Create a notification with level
    pub fn notify_level(
        source: impl Into<String>,
        message: impl Into<String>,
        level: NotificationLevel,
    ) -> Self {
        Self::new(
            source,
            A2UIEventType::Notify {
                message: message.into(),
                level,
                duration_ms: None,
            },
        )
    }

    /// Create a progress event
    pub fn progress(
        source: impl Into<String>,
        id: Uuid,
        label: impl Into<String>,
        current: u64,
        total: u64,
    ) -> Self {
        Self::new(
            source,
            A2UIEventType::Progress {
                id,
                label: label.into(),
                current,
                total,
                message: None,
            },
        )
    }

    /// Create a text prompt event
    pub fn prompt_text(source: impl Into<String>, message: impl Into<String>) -> (Self, Uuid) {
        let prompt_id = Uuid::new_v4();
        (
            Self::new(
                source,
                A2UIEventType::Prompt {
                    id: prompt_id,
                    message: message.into(),
                    prompt_type: PromptType::Text { placeholder: None },
                },
            ),
            prompt_id,
        )
    }

    /// Create a confirmation prompt event
    pub fn prompt_confirm(source: impl Into<String>, message: impl Into<String>) -> (Self, Uuid) {
        let prompt_id = Uuid::new_v4();
        (
            Self::new(
                source,
                A2UIEventType::Prompt {
                    id: prompt_id,
                    message: message.into(),
                    prompt_type: PromptType::Confirm { default: None },
                },
            ),
            prompt_id,
        )
    }

    /// Create a select prompt event
    pub fn prompt_select(
        source: impl Into<String>,
        message: impl Into<String>,
        options: Vec<String>,
    ) -> (Self, Uuid) {
        let prompt_id = Uuid::new_v4();
        (
            Self::new(
                source,
                A2UIEventType::Prompt {
                    id: prompt_id,
                    message: message.into(),
                    prompt_type: PromptType::Select {
                        options,
                        default: None,
                    },
                },
            ),
            prompt_id,
        )
    }

    /// Create a render event
    pub fn render(source: impl Into<String>, content: RenderContent) -> Self {
        Self::new(
            source,
            A2UIEventType::Render {
                target: None,
                content,
                replace: false,
            },
        )
    }

    /// Create a toast notification
    pub fn toast(
        source: impl Into<String>,
        message: impl Into<String>,
        level: NotificationLevel,
        duration_ms: u64,
    ) -> Self {
        Self::new(
            source,
            A2UIEventType::Toast {
                message: message.into(),
                level,
                duration_ms,
            },
        )
    }

    /// Create an agent started event
    pub fn agent_started(source: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self::new(
            source,
            A2UIEventType::AgentStarted {
                agent_id: agent_id.into(),
                task: None,
            },
        )
    }

    /// Create an agent completed event
    pub fn agent_completed(
        source: impl Into<String>,
        agent_id: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::new(
            source,
            A2UIEventType::AgentCompleted {
                agent_id: agent_id.into(),
                result: None,
                success,
            },
        )
    }

    /// Create an agent thinking event
    pub fn agent_thinking(
        source: impl Into<String>,
        agent_id: impl Into<String>,
        thought: impl Into<String>,
        step: usize,
    ) -> Self {
        Self::new(
            source,
            A2UIEventType::AgentThinking {
                agent_id: agent_id.into(),
                thought: thought.into(),
                step,
            },
        )
    }

    /// Create a status bar update event
    pub fn status(source: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(
            source,
            A2UIEventType::Status {
                text: text.into(),
                section: None,
            },
        )
    }
}

// ===================== Event Channel =====================

/// Thread-safe event channel for A2UI communication
#[derive(Clone)]
pub struct A2UIChannel {
    /// Outgoing events (agent → UI)
    events: Arc<Mutex<VecDeque<A2UIEvent>>>,
    /// Pending prompt responses (UI → agent)
    responses: Arc<RwLock<std::collections::HashMap<Uuid, PromptResponse>>>,
    /// Condvar for blocking waits
    response_signal: Arc<(Mutex<bool>, Condvar)>,
    /// Max events to buffer
    max_events: usize,
}

impl A2UIChannel {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
            responses: Arc::new(RwLock::new(std::collections::HashMap::new())),
            response_signal: Arc::new((Mutex::new(false), Condvar::new())),
            max_events: 1000,
        }
    }

    pub fn with_capacity(max_events: usize) -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
            responses: Arc::new(RwLock::new(std::collections::HashMap::new())),
            response_signal: Arc::new((Mutex::new(false), Condvar::new())),
            max_events,
        }
    }

    // ---- Agent-side API ----

    /// Send an event to the UI
    pub fn send(&self, event: A2UIEvent) -> Result<()> {
        let mut events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;

        // Enforce max capacity (drop oldest if full)
        while events.len() >= self.max_events {
            events.pop_front();
        }

        events.push_back(event);
        Ok(())
    }

    /// Send a notification
    pub fn notify(&self, source: &str, message: &str) -> Result<()> {
        self.send(A2UIEvent::notify(source, message))
    }

    /// Send a notification with level
    pub fn notify_level(
        &self,
        source: &str,
        message: &str,
        level: NotificationLevel,
    ) -> Result<()> {
        self.send(A2UIEvent::notify_level(source, message, level))
    }

    /// Send a progress update
    pub fn progress(
        &self,
        source: &str,
        id: Uuid,
        label: &str,
        current: u64,
        total: u64,
    ) -> Result<()> {
        self.send(A2UIEvent::progress(source, id, label, current, total))
    }

    /// Complete a progress indicator
    pub fn progress_complete(&self, source: &str, id: Uuid) -> Result<()> {
        self.send(A2UIEvent::new(
            source,
            A2UIEventType::ProgressComplete { id },
        ))
    }

    /// Request text input from user (blocking)
    pub fn prompt_text(&self, source: &str, message: &str) -> Result<PromptResponse> {
        let (event, prompt_id) = A2UIEvent::prompt_text(source, message);
        self.send(event)?;
        self.wait_for_response(prompt_id)
    }

    /// Request confirmation from user (blocking)
    pub fn prompt_confirm(&self, source: &str, message: &str) -> Result<bool> {
        let (event, prompt_id) = A2UIEvent::prompt_confirm(source, message);
        self.send(event)?;
        match self.wait_for_response(prompt_id)? {
            PromptResponse::Confirm(v) => Ok(v),
            PromptResponse::Cancelled => Ok(false),
            _ => Err(anyhow!("Unexpected response type")),
        }
    }

    /// Request selection from user (blocking)
    pub fn prompt_select(
        &self,
        source: &str,
        message: &str,
        options: Vec<String>,
    ) -> Result<PromptResponse> {
        let (event, prompt_id) = A2UIEvent::prompt_select(source, message, options);
        self.send(event)?;
        self.wait_for_response(prompt_id)
    }

    /// Send content for rendering
    pub fn render(&self, source: &str, content: RenderContent) -> Result<()> {
        self.send(A2UIEvent::render(source, content))
    }

    /// Send a toast notification
    pub fn toast(
        &self,
        source: &str,
        message: &str,
        level: NotificationLevel,
        duration_ms: u64,
    ) -> Result<()> {
        self.send(A2UIEvent::toast(source, message, level, duration_ms))
    }

    /// Update status bar
    pub fn status(&self, source: &str, text: &str) -> Result<()> {
        self.send(A2UIEvent::status(source, text))
    }

    // ---- UI-side API ----

    /// Receive all pending events (non-blocking)
    pub fn receive_all(&self) -> Result<Vec<A2UIEvent>> {
        let mut events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;
        Ok(events.drain(..).collect())
    }

    /// Receive events up to a limit
    pub fn receive(&self, max_count: usize) -> Result<Vec<A2UIEvent>> {
        let mut events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;

        let count = max_count.min(events.len());
        Ok(events.drain(..count).collect())
    }

    /// Peek at events without removing them
    pub fn peek(&self) -> Result<Vec<A2UIEvent>> {
        let events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;
        Ok(events.iter().cloned().collect())
    }

    /// Check if there are pending events
    pub fn has_events(&self) -> Result<bool> {
        let events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;
        Ok(!events.is_empty())
    }

    /// Get event count
    pub fn event_count(&self) -> Result<usize> {
        let events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;
        Ok(events.len())
    }

    /// Submit a response to a prompt
    pub fn submit_response(&self, prompt_id: Uuid, response: PromptResponse) -> Result<()> {
        {
            let mut responses = self
                .responses
                .write()
                .map_err(|e| anyhow!("Failed to acquire responses lock: {}", e))?;
            responses.insert(prompt_id, response);
        }

        // Signal waiting threads
        let (lock, cvar) = &*self.response_signal;
        let mut signaled = lock
            .lock()
            .map_err(|e| anyhow!("Failed to acquire signal lock: {}", e))?;
        *signaled = true;
        cvar.notify_all();

        Ok(())
    }

    /// Check if a response exists for a prompt (for testing)
    pub fn has_response(&self, prompt_id: Uuid) -> Result<bool> {
        let responses = self
            .responses
            .read()
            .map_err(|e| anyhow!("Failed to acquire responses lock: {}", e))?;
        Ok(responses.contains_key(&prompt_id))
    }

    /// Wait for a response to a prompt (with timeout)
    fn wait_for_response(&self, prompt_id: Uuid) -> Result<PromptResponse> {
        let timeout = std::time::Duration::from_secs(300); // 5 minute timeout
        let start = std::time::Instant::now();

        loop {
            // Check if response is available
            {
                let mut responses = self
                    .responses
                    .write()
                    .map_err(|e| anyhow!("Failed to acquire responses lock: {}", e))?;
                if let Some(response) = responses.remove(&prompt_id) {
                    return Ok(response);
                }
            }

            // Check timeout
            if start.elapsed() > timeout {
                return Ok(PromptResponse::Cancelled);
            }

            // Wait for signal
            let (lock, cvar) = &*self.response_signal;
            let signaled = lock
                .lock()
                .map_err(|e| anyhow!("Failed to acquire signal lock: {}", e))?;

            let wait_time = std::time::Duration::from_millis(100);
            let (mut guard, _) = cvar
                .wait_timeout(signaled, wait_time)
                .map_err(|e| anyhow!("Failed to wait on condition: {}", e))?;
            *guard = false;
        }
    }

    /// Clear all pending events
    pub fn clear(&self) -> Result<()> {
        let mut events = self
            .events
            .lock()
            .map_err(|e| anyhow!("Failed to acquire events lock: {}", e))?;
        events.clear();
        Ok(())
    }
}

impl Default for A2UIChannel {
    fn default() -> Self {
        Self::new()
    }
}

// ===================== Global Instance =====================

lazy_static::lazy_static! {
    /// Global A2UI channel instance
    pub static ref A2UI_CHANNEL: A2UIChannel = A2UIChannel::new();
}

// ===================== Convenience Functions =====================

/// Send a notification to the global channel
pub fn notify(source: &str, message: &str) -> Result<()> {
    A2UI_CHANNEL.notify(source, message)
}

/// Send a notification with level to the global channel
pub fn notify_level(source: &str, message: &str, level: NotificationLevel) -> Result<()> {
    A2UI_CHANNEL.notify_level(source, message, level)
}

/// Send a progress update to the global channel
pub fn progress(source: &str, id: Uuid, label: &str, current: u64, total: u64) -> Result<()> {
    A2UI_CHANNEL.progress(source, id, label, current, total)
}

/// Complete a progress indicator
pub fn progress_complete(source: &str, id: Uuid) -> Result<()> {
    A2UI_CHANNEL.progress_complete(source, id)
}

/// Request text input (blocking)
pub fn prompt_text(source: &str, message: &str) -> Result<PromptResponse> {
    A2UI_CHANNEL.prompt_text(source, message)
}

/// Request confirmation (blocking)
pub fn prompt_confirm(source: &str, message: &str) -> Result<bool> {
    A2UI_CHANNEL.prompt_confirm(source, message)
}

/// Request selection (blocking)
pub fn prompt_select(source: &str, message: &str, options: Vec<String>) -> Result<PromptResponse> {
    A2UI_CHANNEL.prompt_select(source, message, options)
}

/// Render content
pub fn render(source: &str, content: RenderContent) -> Result<()> {
    A2UI_CHANNEL.render(source, content)
}

/// Show toast notification
pub fn toast(
    source: &str,
    message: &str,
    level: NotificationLevel,
    duration_ms: u64,
) -> Result<()> {
    A2UI_CHANNEL.toast(source, message, level, duration_ms)
}

/// Update status bar
pub fn status(source: &str, text: &str) -> Result<()> {
    A2UI_CHANNEL.status(source, text)
}

/// Receive all pending events from global channel
pub fn receive_all() -> Result<Vec<A2UIEvent>> {
    A2UI_CHANNEL.receive_all()
}

/// Submit response to a prompt on global channel
pub fn submit_response(prompt_id: Uuid, response: PromptResponse) -> Result<()> {
    A2UI_CHANNEL.submit_response(prompt_id, response)
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_level_parse() {
        assert_eq!(
            "info".parse::<NotificationLevel>().unwrap(),
            NotificationLevel::Info
        );
        assert_eq!(
            "success".parse::<NotificationLevel>().unwrap(),
            NotificationLevel::Success
        );
        assert_eq!(
            "warning".parse::<NotificationLevel>().unwrap(),
            NotificationLevel::Warning
        );
        assert_eq!(
            "warn".parse::<NotificationLevel>().unwrap(),
            NotificationLevel::Warning
        );
        assert_eq!(
            "error".parse::<NotificationLevel>().unwrap(),
            NotificationLevel::Error
        );
    }

    #[test]
    fn test_event_creation() {
        let event = A2UIEvent::notify("test_agent", "Hello World");
        assert_eq!(event.source, "test_agent");
        match event.event_type {
            A2UIEventType::Notify { message, level, .. } => {
                assert_eq!(message, "Hello World");
                assert_eq!(level, NotificationLevel::Info);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_channel_send_receive() {
        let channel = A2UIChannel::new();

        // Send some events
        channel.notify("agent1", "Test 1").unwrap();
        channel.notify("agent2", "Test 2").unwrap();

        assert_eq!(channel.event_count().unwrap(), 2);

        // Receive events
        let events = channel.receive_all().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(channel.event_count().unwrap(), 0);
    }

    #[test]
    fn test_channel_capacity() {
        let channel = A2UIChannel::with_capacity(3);

        // Send more than capacity
        for i in 0..5 {
            channel.notify("agent", &format!("Message {}", i)).unwrap();
        }

        // Should only have last 3
        assert_eq!(channel.event_count().unwrap(), 3);
    }

    #[test]
    fn test_progress_events() {
        let channel = A2UIChannel::new();
        let progress_id = Uuid::new_v4();

        channel
            .progress("agent", progress_id, "Downloading", 50, 100)
            .unwrap();

        let events = channel.receive_all().unwrap();
        assert_eq!(events.len(), 1);

        match &events[0].event_type {
            A2UIEventType::Progress {
                id, current, total, ..
            } => {
                assert_eq!(*id, progress_id);
                assert_eq!(*current, 50);
                assert_eq!(*total, 100);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_prompt_response() {
        let channel = A2UIChannel::new();
        let prompt_id = Uuid::new_v4();

        // Simulate submitting a response
        channel
            .submit_response(prompt_id, PromptResponse::Text("user input".to_string()))
            .unwrap();

        // Check response is stored
        let responses = channel.responses.read().unwrap();
        assert!(responses.contains_key(&prompt_id));
    }

    #[test]
    fn test_render_content_variants() {
        let channel = A2UIChannel::new();

        // Text
        channel
            .render("agent", RenderContent::Text("Plain text".to_string()))
            .unwrap();

        // Markdown
        channel
            .render("agent", RenderContent::Markdown("# Header".to_string()))
            .unwrap();

        // Table
        channel
            .render(
                "agent",
                RenderContent::Table {
                    headers: vec!["Name".to_string(), "Value".to_string()],
                    rows: vec![vec!["foo".to_string(), "bar".to_string()]],
                },
            )
            .unwrap();

        // Code
        channel
            .render(
                "agent",
                RenderContent::Code {
                    language: "rust".to_string(),
                    content: "fn main() {}".to_string(),
                },
            )
            .unwrap();

        assert_eq!(channel.event_count().unwrap(), 4);
    }

    #[test]
    fn test_toast_events() {
        let channel = A2UIChannel::new();

        channel
            .toast("agent", "Quick message", NotificationLevel::Success, 3000)
            .unwrap();

        let events = channel.receive_all().unwrap();
        match &events[0].event_type {
            A2UIEventType::Toast {
                message,
                level,
                duration_ms,
            } => {
                assert_eq!(message, "Quick message");
                assert_eq!(*level, NotificationLevel::Success);
                assert_eq!(*duration_ms, 3000);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_agent_lifecycle_events() {
        let channel = A2UIChannel::new();

        channel
            .send(A2UIEvent::agent_started("system", "agent1"))
            .unwrap();
        channel
            .send(A2UIEvent::agent_thinking(
                "system",
                "agent1",
                "Analyzing...",
                1,
            ))
            .unwrap();
        channel
            .send(A2UIEvent::agent_completed("system", "agent1", true))
            .unwrap();

        let events = channel.receive_all().unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_event_priority() {
        let event = A2UIEvent::notify("agent", "test").with_priority(EventPriority::Critical);
        assert_eq!(event.priority, EventPriority::Critical);
    }

    #[test]
    fn test_global_channel() {
        // Use global channel
        notify("test", "Global test").unwrap();

        // Clear to not affect other tests
        A2UI_CHANNEL.clear().unwrap();
    }
}
