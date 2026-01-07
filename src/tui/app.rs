//! Main TUI Application State and Logic

use anyhow::Result;
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
use tui_input::Input;
use uuid::Uuid;

use super::distributed::DistributedSwarm;
use super::media::MediaFile;
use super::reasoning::{
    GoalSpecification, PlanningGoal, ReasoningCoordinator, ReasoningEngine, TaskPlanner,
};
use crate::{ai, env::Env};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    AgentSwarm,
    MediaBrowser,
    Settings,
    DistributedAgents,
    AdvancedReasoning,
    Search,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub role: MessageRole,
    pub content: String,
    pub media_attachments: Vec<MediaFile>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: Uuid,
    pub name: String,
    pub model: String,
    pub status: AgentStatus,
    pub current_task: Option<String>,
    pub tools: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Working,
    Waiting,
    Error(String),
}

pub struct App {
    pub should_quit: bool,
    pub mode: AppMode,
    pub input_mode: InputMode,

    // UI State
    pub input: Input,
    pub messages: Vec<ChatMessage>,
    pub agents: Vec<AgentInfo>,
    pub media_files: Vec<MediaFile>,

    // List states for navigation
    pub message_list_state: ListState,
    pub agent_list_state: ListState,
    pub media_list_state: ListState,
    pub tab_index: usize,

    // AetherShell environment
    pub env: Env,

    // Configuration
    pub config: AppConfig,

    // Current selections
    pub selected_media: Vec<usize>, // Indices into media_files
    pub current_model: String,

    // Search state
    pub search_query: String,
    pub search_results: Vec<usize>,
    pub search_result_index: usize,

    // Distributed agents and advanced reasoning
    pub distributed_swarm: Option<DistributedSwarm>,
    pub reasoning_coordinator: ReasoningCoordinator,
    pub active_planning_goal: Option<PlanningGoal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_model: String,
    pub max_messages: usize,
    pub auto_scroll: bool,
    pub show_timestamps: bool,
    pub enable_media_preview: bool,
    pub agent_update_interval: u64, // milliseconds
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_model: "stub".to_string(),
            max_messages: 1000,
            auto_scroll: true,
            show_timestamps: true,
            enable_media_preview: true,
            agent_update_interval: 1000,
        }
    }
}

impl App {
    pub fn new() -> Result<Self> {
        let mut message_list_state = ListState::default();
        message_list_state.select(Some(0));

        let mut agent_list_state = ListState::default();
        agent_list_state.select(Some(0));

        let mut media_list_state = ListState::default();
        media_list_state.select(Some(0));

        Ok(App {
            should_quit: false,
            mode: AppMode::Chat,
            input_mode: InputMode::Normal,
            input: Input::default(),
            messages: Vec::new(),
            agents: Vec::new(),
            media_files: Vec::new(),
            message_list_state,
            agent_list_state,
            media_list_state,
            tab_index: 0,
            env: Env::new(),
            config: AppConfig::default(),
            selected_media: Vec::new(),
            current_model: "stub".to_string(),
            search_query: String::new(),
            search_results: Vec::new(),
            search_result_index: 0,
            distributed_swarm: None,
            reasoning_coordinator: ReasoningCoordinator {
                reasoning_engine: ReasoningEngine::new(),
                task_planner: TaskPlanner {
                    goal: PlanningGoal {
                        description: "Default planning goal".to_string(),
                        input_data: ai::MultiModalMessage {
                            role: "user".to_string(),
                            content: vec![],
                        },
                        desired_output: GoalSpecification {
                            output_modalities: vec![],
                            quality_requirements: std::collections::HashMap::new(),
                            success_criteria: vec![],
                        },
                        constraints: vec![],
                        deadline: None,
                    },
                    available_agents: vec![],
                    planning_strategy: super::reasoning::PlanningStrategy::ForwardChaining,
                    execution_plan: None,
                },
                active_reasoning_sessions: std::collections::HashMap::new(),
            },
            active_planning_goal: None,
        })
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn switch_mode(&mut self, mode: AppMode) {
        self.mode = mode;
        self.input_mode = InputMode::Normal;
    }

    pub fn next_tab(&mut self) {
        self.tab_index = (self.tab_index + 1) % 7; // 7 modes
        self.mode = match self.tab_index {
            0 => AppMode::Chat,
            1 => AppMode::AgentSwarm,
            2 => AppMode::MediaBrowser,
            3 => AppMode::Settings,
            4 => AppMode::DistributedAgents,
            5 => AppMode::AdvancedReasoning,
            6 => AppMode::Search,
            _ => AppMode::Chat,
        };
    }

    pub fn previous_tab(&mut self) {
        if self.tab_index == 0 {
            self.tab_index = 6;
        } else {
            self.tab_index -= 1;
        }
        self.mode = match self.tab_index {
            0 => AppMode::Chat,
            1 => AppMode::AgentSwarm,
            2 => AppMode::MediaBrowser,
            3 => AppMode::Settings,
            4 => AppMode::DistributedAgents,
            5 => AppMode::AdvancedReasoning,
            6 => AppMode::Search,
            _ => AppMode::Chat,
        };
    }

    pub fn add_message(&mut self, role: MessageRole, content: String) {
        let message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            role,
            content,
            media_attachments: self.get_selected_media_files(),
            model: Some(self.current_model.clone()),
        };

        self.messages.push(message);

        // Auto-scroll to latest message
        if self.config.auto_scroll {
            let index = self.messages.len().saturating_sub(1);
            self.message_list_state.select(Some(index));
        }

        // Limit message history
        if self.messages.len() > self.config.max_messages {
            self.messages.remove(0);
        }
    }

    pub fn send_message(&mut self) -> Result<()> {
        if self.input.value().trim().is_empty() {
            return Ok(());
        }

        let user_input = self.input.value().to_string();
        self.input.reset();

        // Add user message
        self.add_message(MessageRole::User, user_input.clone());

        // Process with AI
        let response = match self.current_model.as_str() {
            "stub" => ai::stub::complete_sync(&user_input)?,
            _ => ai::complete_sync_router(&user_input)?,
        };

        // Add AI response
        self.add_message(MessageRole::Assistant, response);

        // Clear selected media after sending
        self.selected_media.clear();

        Ok(())
    }

    pub fn add_agent(&mut self, name: String, model: String, tools: Vec<String>) {
        let agent = AgentInfo {
            id: Uuid::new_v4(),
            name,
            model,
            status: AgentStatus::Idle,
            current_task: None,
            tools,
            created_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
        };

        self.agents.push(agent);
    }

    pub fn remove_selected_agent(&mut self) {
        if let Some(selected) = self.agent_list_state.selected() {
            if selected < self.agents.len() {
                self.agents.remove(selected);

                // Adjust selection
                if self.agents.is_empty() {
                    self.agent_list_state.select(None);
                } else if selected >= self.agents.len() {
                    self.agent_list_state.select(Some(self.agents.len() - 1));
                }
            }
        }
    }

    pub fn add_media_file(&mut self, file: MediaFile) {
        self.media_files.push(file);
    }

    pub fn toggle_media_selection(&mut self) {
        if let Some(selected) = self.media_list_state.selected() {
            if selected < self.media_files.len() {
                if let Some(pos) = self.selected_media.iter().position(|&x| x == selected) {
                    self.selected_media.remove(pos);
                } else {
                    self.selected_media.push(selected);
                }
            }
        }
    }

    pub fn get_selected_media_files(&self) -> Vec<MediaFile> {
        self.selected_media
            .iter()
            .filter_map(|&idx| self.media_files.get(idx).cloned())
            .collect()
    }

    pub fn clear_media_selection(&mut self) {
        self.selected_media.clear();
    }

    pub fn start_agent_task(&mut self, task: String) -> Result<()> {
        if let Some(selected) = self.agent_list_state.selected() {
            if let Some(agent) = self.agents.get_mut(selected) {
                let agent_name = agent.name.clone();
                agent.status = AgentStatus::Working;
                agent.current_task = Some(task.clone());
                agent.last_activity = chrono::Utc::now();

                // Add system message
                self.add_message(
                    MessageRole::System,
                    format!("Agent '{}' started task: {}", agent_name, task),
                );
            }
        }
        Ok(())
    }

    pub fn get_tab_titles(&self) -> Vec<&'static str> {
        vec![
            "Chat",
            "Agents",
            "Media",
            "Settings",
            "Distributed",
            "Reasoning",
            "Search",
        ]
    }

    pub fn move_list_up(&mut self) {
        match self.mode {
            AppMode::Chat => {
                let i = match self.message_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.messages.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.message_list_state.select(Some(i));
            }
            AppMode::AgentSwarm => {
                let i = match self.agent_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.agents.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.agent_list_state.select(Some(i));
            }
            AppMode::MediaBrowser => {
                let i = match self.media_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.media_files.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.media_list_state.select(Some(i));
            }
            _ => {}
        }
    }

    pub fn move_list_down(&mut self) {
        match self.mode {
            AppMode::Chat => {
                let i = match self.message_list_state.selected() {
                    Some(i) => {
                        if i >= self.messages.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.message_list_state.select(Some(i));
            }
            AppMode::AgentSwarm => {
                let i = match self.agent_list_state.selected() {
                    Some(i) => {
                        if i >= self.agents.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.agent_list_state.select(Some(i));
            }
            AppMode::MediaBrowser => {
                let i = match self.media_list_state.selected() {
                    Some(i) => {
                        if i >= self.media_files.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.media_list_state.select(Some(i));
            }
            _ => {}
        }
    }

    /// Start a distributed agent swarm
    pub async fn start_distributed_swarm(&mut self, listen_addr: &str) -> Result<()> {
        let addr: std::net::SocketAddr = listen_addr.parse()?;
        let swarm = DistributedSwarm::new(addr).await?;
        self.distributed_swarm = Some(swarm);
        Ok(())
    }

    /// Stop the distributed agent swarm
    pub async fn stop_distributed_swarm(&mut self) -> Result<()> {
        if let Some(mut swarm) = self.distributed_swarm.take() {
            swarm.shutdown().await?;
        }
        Ok(())
    }

    /// Start an advanced reasoning session
    pub async fn start_reasoning_session(&mut self, goal: PlanningGoal) -> Result<Uuid> {
        let _session_result = self
            .reasoning_coordinator
            .reasoning_engine
            .reason(&goal)
            .await?;
        self.active_planning_goal = Some(goal);
        Ok(Uuid::new_v4()) // Return a session ID
    }

    /// Get the status of distributed agents
    pub fn get_distributed_agent_status(&self) -> Vec<String> {
        if let Some(_swarm) = &self.distributed_swarm {
            vec!["Distributed swarm active".to_string()]
        } else {
            vec!["No distributed swarm running".to_string()]
        }
    }

    /// Get active reasoning sessions
    pub fn get_active_reasoning_sessions(&self) -> Vec<String> {
        self.reasoning_coordinator
            .active_reasoning_sessions
            .iter()
            .map(|(id, session)| format!("{}: {}", id, session.goal.description))
            .collect()
    }

    /// Export conversation to markdown format
    pub fn export_to_markdown(&self) -> String {
        let mut output = String::new();
        output.push_str("# AetherShell Conversation Export\n\n");
        output.push_str(&format!(
            "**Exported:** {}\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));
        output.push_str(&format!("**Model:** {}\n", self.current_model));
        output.push_str(&format!("**Total Messages:** {}\n\n", self.messages.len()));
        output.push_str("---\n\n");

        for msg in &self.messages {
            let role = match msg.role {
                MessageRole::User => "👤 User",
                MessageRole::Assistant => "🤖 Assistant",
                MessageRole::System => "⚙️ System",
            };

            output.push_str(&format!(
                "## {} ({})\n\n",
                role,
                msg.timestamp.format("%H:%M:%S")
            ));

            if let Some(model) = &msg.model {
                output.push_str(&format!("*Model: {}*\n\n", model));
            }

            output.push_str(&msg.content);
            output.push_str("\n\n");

            if !msg.media_attachments.is_empty() {
                output.push_str("**Attachments:**\n");
                for media in &msg.media_attachments {
                    output.push_str(&format!("- {} ({:?})\n", media.path, media.media_type));
                }
                output.push_str("\n");
            }

            output.push_str("---\n\n");
        }

        output
    }

    /// Export conversation to JSON format
    pub fn export_to_json(&self) -> Result<String> {
        #[derive(Serialize)]
        struct ExportData {
            exported_at: String,
            model: String,
            total_messages: usize,
            messages: Vec<ExportMessage>,
        }

        #[derive(Serialize)]
        struct ExportMessage {
            timestamp: String,
            role: String,
            content: String,
            model: Option<String>,
            media_count: usize,
        }

        let export = ExportData {
            exported_at: chrono::Utc::now().to_rfc3339(),
            model: self.current_model.clone(),
            total_messages: self.messages.len(),
            messages: self
                .messages
                .iter()
                .map(|msg| ExportMessage {
                    timestamp: msg.timestamp.to_rfc3339(),
                    role: format!("{:?}", msg.role),
                    content: msg.content.clone(),
                    model: msg.model.clone(),
                    media_count: msg.media_attachments.len(),
                })
                .collect(),
        };

        serde_json::to_string_pretty(&export)
            .map_err(|e| anyhow::anyhow!("JSON export failed: {}", e))
    }

    /// Search messages by content
    pub fn search_messages(&self, query: &str) -> Vec<usize> {
        let query_lower = query.to_lowercase();
        self.messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.content.to_lowercase().contains(&query_lower))
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Filter messages by role
    pub fn filter_by_role(&self, role: MessageRole) -> Vec<usize> {
        self.messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.role == role)
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Execute search and update search state
    pub fn execute_search(&mut self) {
        if self.search_query.is_empty() {
            self.search_results.clear();
            self.search_result_index = 0;
        } else {
            self.search_results = self.search_messages(&self.search_query.clone());
            self.search_result_index = 0;
        }
    }

    /// Navigate to next search result
    pub fn next_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.search_result_index = (self.search_result_index + 1) % self.search_results.len();
        }
    }

    /// Navigate to previous search result
    pub fn previous_search_result(&mut self) {
        if !self.search_results.is_empty() {
            if self.search_result_index == 0 {
                self.search_result_index = self.search_results.len() - 1;
            } else {
                self.search_result_index -= 1;
            }
        }
    }

    /// Clear search and return to chat mode
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_results.clear();
        self.search_result_index = 0;
        self.mode = AppMode::Chat;
    }

    /// Get conversation statistics
    pub fn get_stats(&self) -> ConversationStats {
        let user_msgs = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count();
        let assistant_msgs = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();
        let system_msgs = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .count();

        let total_chars: usize = self.messages.iter().map(|m| m.content.len()).sum();
        let avg_msg_length = if !self.messages.is_empty() {
            total_chars / self.messages.len()
        } else {
            0
        };

        let total_media = self
            .messages
            .iter()
            .map(|m| m.media_attachments.len())
            .sum();

        ConversationStats {
            total_messages: self.messages.len(),
            user_messages: user_msgs,
            assistant_messages: assistant_msgs,
            system_messages: system_msgs,
            total_characters: total_chars,
            avg_message_length: avg_msg_length,
            total_media_attachments: total_media,
            active_agents: self.agents.len(),
        }
    }

    /// Clear all messages
    pub fn clear_conversation(&mut self) {
        self.messages.clear();
        self.message_list_state.select(Some(0));
    }

    /// Get context window (last N messages)
    pub fn get_context_window(&self, window_size: usize) -> Vec<&ChatMessage> {
        let start = if self.messages.len() > window_size {
            self.messages.len() - window_size
        } else {
            0
        };
        self.messages[start..].iter().collect()
    }

    /// Calculate estimated tokens (rough approximation)
    pub fn estimate_tokens(&self) -> usize {
        // Rough estimate: ~4 characters per token
        self.messages.iter().map(|m| m.content.len() / 4).sum()
    }

    /// Get agent performance metrics
    pub fn get_agent_metrics(&self) -> Vec<AgentMetrics> {
        self.agents
            .iter()
            .map(|agent| {
                let uptime = chrono::Utc::now()
                    .signed_duration_since(agent.created_at)
                    .num_seconds();
                let idle_time = chrono::Utc::now()
                    .signed_duration_since(agent.last_activity)
                    .num_seconds();

                AgentMetrics {
                    name: agent.name.clone(),
                    status: agent.status.clone(),
                    uptime_seconds: uptime,
                    idle_seconds: idle_time,
                    tool_count: agent.tools.len(),
                }
            })
            .collect()
    }

    /// Toggle auto-scroll setting
    pub fn toggle_auto_scroll(&mut self) {
        self.config.auto_scroll = !self.config.auto_scroll;
    }

    /// Toggle timestamp display
    pub fn toggle_timestamps(&mut self) {
        self.config.show_timestamps = !self.config.show_timestamps;
    }

    /// Toggle media preview
    pub fn toggle_media_preview(&mut self) {
        self.config.enable_media_preview = !self.config.enable_media_preview;
    }

    /// Get current mode as string
    pub fn get_mode_string(&self) -> &'static str {
        match self.mode {
            AppMode::Chat => "Chat",
            AppMode::AgentSwarm => "Agent Swarm",
            AppMode::MediaBrowser => "Media Browser",
            AppMode::Settings => "Settings",
            AppMode::DistributedAgents => "Distributed Agents",
            AppMode::AdvancedReasoning => "Advanced Reasoning",
            AppMode::Search => "Search",
        }
    }

    /// Get help text for current mode
    pub fn get_help_text(&self) -> Vec<String> {
        match self.mode {
            AppMode::Chat => vec![
                "Enter: Send message".to_string(),
                "Ctrl+C: Copy selected".to_string(),
                "Ctrl+E: Export conversation".to_string(),
                "Ctrl+L: Clear conversation".to_string(),
                "Ctrl+F: Search messages".to_string(),
                "Tab: Switch mode".to_string(),
                "Ctrl+Q: Quit".to_string(),
            ],
            AppMode::AgentSwarm => vec![
                "Enter: Start selected agent".to_string(),
                "Space: Pause/Resume".to_string(),
                "D: Delete selected agent".to_string(),
                "N: New agent".to_string(),
                "M: View metrics".to_string(),
                "Tab: Switch mode".to_string(),
            ],
            AppMode::MediaBrowser => vec![
                "Enter: Select/Deselect media".to_string(),
                "D: Delete selected".to_string(),
                "A: Add media file".to_string(),
                "P: Preview".to_string(),
                "C: Clear selection".to_string(),
                "Tab: Switch mode".to_string(),
            ],
            AppMode::Settings => vec![
                "1: Toggle auto-scroll".to_string(),
                "2: Toggle timestamps".to_string(),
                "3: Toggle media preview".to_string(),
                "↑/↓: Navigate settings".to_string(),
                "Enter: Change value".to_string(),
                "Tab: Switch mode".to_string(),
            ],
            AppMode::DistributedAgents => vec![
                "Enter: Deploy agent".to_string(),
                "S: View swarm status".to_string(),
                "C: Coordinate agents".to_string(),
                "H: Health check".to_string(),
                "Tab: Switch mode".to_string(),
            ],
            AppMode::AdvancedReasoning => vec![
                "Enter: Start reasoning".to_string(),
                "G: Set goal".to_string(),
                "P: Plan steps".to_string(),
                "E: Execute plan".to_string(),
                "V: Visualize reasoning".to_string(),
                "Tab: Switch mode".to_string(),
            ],
            AppMode::Search => vec![
                "Type: Enter search query".to_string(),
                "Enter: Execute search".to_string(),
                "↑/↓: Navigate results".to_string(),
                "Esc: Clear search / Return to Chat".to_string(),
                "Ctrl+C: Copy selected result".to_string(),
                "Tab: Switch mode".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConversationStats {
    pub total_messages: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub system_messages: usize,
    pub total_characters: usize,
    pub avg_message_length: usize,
    pub total_media_attachments: usize,
    pub active_agents: usize,
}

#[derive(Debug, Clone)]
pub struct AgentMetrics {
    pub name: String,
    pub status: AgentStatus,
    pub uptime_seconds: i64,
    pub idle_seconds: i64,
    pub tool_count: usize,
}
