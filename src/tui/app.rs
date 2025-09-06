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
        self.tab_index = (self.tab_index + 1) % 6; // 6 modes
        self.mode = match self.tab_index {
            0 => AppMode::Chat,
            1 => AppMode::AgentSwarm,
            2 => AppMode::MediaBrowser,
            3 => AppMode::Settings,
            4 => AppMode::DistributedAgents,
            5 => AppMode::AdvancedReasoning,
            _ => AppMode::Chat,
        };
    }

    pub fn previous_tab(&mut self) {
        if self.tab_index == 0 {
            self.tab_index = 5;
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
        let session_id = self
            .reasoning_coordinator
            .reasoning_engine
            .reason(&goal)
            .await?;
        self.active_planning_goal = Some(goal);
        Ok(Uuid::new_v4()) // Return a session ID
    }

    /// Get the status of distributed agents
    pub fn get_distributed_agent_status(&self) -> Vec<String> {
        if let Some(swarm) = &self.distributed_swarm {
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
}
