//! Agent management and swarm coordination for the TUI

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::app::{AgentInfo, AgentStatus};
use crate::ai;

#[derive(Debug, Clone)]
pub struct AgentSwarm {
    pub agents: HashMap<Uuid, AgentInfo>,
    pub coordinator: SwarmCoordinator,
    pub shared_memory: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SwarmCoordinator {
    pub strategy: CoordinationStrategy,
    pub task_queue: Vec<SwarmTask>,
    pub completed_tasks: Vec<SwarmTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoordinationStrategy {
    RoundRobin,
    LoadBalanced,
    Specialized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: Uuid,
    pub description: String,
    pub assigned_agent: Option<Uuid>,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<String>,
    pub dependencies: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed(String),
}

impl AgentSwarm {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            coordinator: SwarmCoordinator::new(),
            shared_memory: HashMap::new(),
        }
    }

    pub fn add_agent(&mut self, agent: AgentInfo) {
        self.agents.insert(agent.id, agent);
    }

    pub fn remove_agent(&mut self, agent_id: Uuid) -> Option<AgentInfo> {
        self.agents.remove(&agent_id)
    }

    pub fn assign_task(&mut self, task_description: String) -> Result<Uuid> {
        let task = SwarmTask {
            id: Uuid::new_v4(),
            description: task_description,
            assigned_agent: None,
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now(),
            completed_at: None,
            result: None,
            dependencies: Vec::new(),
        };

        let task_id = task.id;
        self.coordinator.task_queue.push(task);
        self.coordinator.assign_next_task(&mut self.agents)?;

        Ok(task_id)
    }

    pub fn update_agent_status(&mut self, agent_id: Uuid, status: AgentStatus) {
        if let Some(agent) = self.agents.get_mut(&agent_id) {
            agent.status = status;
            agent.last_activity = chrono::Utc::now();
        }
    }

    pub fn get_swarm_status(&self) -> SwarmStatus {
        let total_agents = self.agents.len();
        let idle_agents = self
            .agents
            .values()
            .filter(|a| a.status == AgentStatus::Idle)
            .count();
        let working_agents = self
            .agents
            .values()
            .filter(|a| a.status == AgentStatus::Working)
            .count();
        let pending_tasks = self.coordinator.task_queue.len();
        let completed_tasks = self.coordinator.completed_tasks.len();

        SwarmStatus {
            total_agents,
            idle_agents,
            working_agents,
            pending_tasks,
            completed_tasks,
        }
    }

    pub fn broadcast_message(&mut self, message: &str) {
        self.shared_memory.insert(
            "broadcast".to_string(),
            format!("{}: {}", chrono::Utc::now().format("%H:%M:%S"), message),
        );
    }
}

impl SwarmCoordinator {
    pub fn new() -> Self {
        Self {
            strategy: CoordinationStrategy::RoundRobin,
            task_queue: Vec::new(),
            completed_tasks: Vec::new(),
        }
    }

    pub fn assign_next_task(&mut self, agents: &mut HashMap<Uuid, AgentInfo>) -> Result<()> {
        // Find the next pending task
        let pending_task_idx = self
            .task_queue
            .iter()
            .position(|task| task.status == TaskStatus::Pending);

        if let Some(task_idx) = pending_task_idx {
            // Find an available agent based on strategy
            let available_agent = self.find_available_agent(agents)?;

            if let Some(agent_id) = available_agent {
                // Assign the task
                self.task_queue[task_idx].assigned_agent = Some(agent_id);
                self.task_queue[task_idx].status = TaskStatus::Assigned;

                // Update agent status
                if let Some(agent) = agents.get_mut(&agent_id) {
                    agent.status = AgentStatus::Working;
                    agent.current_task = Some(self.task_queue[task_idx].description.clone());
                    agent.last_activity = chrono::Utc::now();
                }
            }
        }

        Ok(())
    }

    fn find_available_agent(&self, agents: &HashMap<Uuid, AgentInfo>) -> Result<Option<Uuid>> {
        match self.strategy {
            CoordinationStrategy::RoundRobin => {
                // Simple round-robin: find first idle agent
                Ok(agents
                    .values()
                    .find(|agent| agent.status == AgentStatus::Idle)
                    .map(|agent| agent.id))
            }
            CoordinationStrategy::LoadBalanced => {
                // Find agent with least current workload
                Ok(agents
                    .values()
                    .filter(|agent| agent.status == AgentStatus::Idle)
                    .min_by_key(|agent| agent.last_activity)
                    .map(|agent| agent.id))
            }
            CoordinationStrategy::Specialized => {
                // Find agent best suited for the task (placeholder logic)
                Ok(agents
                    .values()
                    .find(|agent| agent.status == AgentStatus::Idle)
                    .map(|agent| agent.id))
            }
        }
    }

    pub fn complete_task(&mut self, task_id: Uuid, result: String) -> Result<()> {
        if let Some(task_idx) = self.task_queue.iter().position(|task| task.id == task_id) {
            let mut task = self.task_queue.remove(task_idx);
            task.status = TaskStatus::Completed;
            task.result = Some(result);
            task.completed_at = Some(chrono::Utc::now());

            self.completed_tasks.push(task);
        }

        Ok(())
    }

    pub fn fail_task(&mut self, task_id: Uuid, error: String) -> Result<()> {
        if let Some(task) = self.task_queue.iter_mut().find(|task| task.id == task_id) {
            task.status = TaskStatus::Failed(error);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SwarmStatus {
    pub total_agents: usize,
    pub idle_agents: usize,
    pub working_agents: usize,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
}

/// Multi-modal agent that can process text, images, audio, and video
#[derive(Debug, Clone)]
pub struct MultiModalAgent {
    pub base_agent: AgentInfo,
    pub supported_modalities: Vec<Modality>,
    pub context_window: Vec<MultiModalMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
}

#[derive(Debug, Clone)]
pub struct MultiModalMessage {
    pub content: String,
    pub modality: Modality,
    pub metadata: HashMap<String, String>,
}

impl MultiModalAgent {
    pub fn new(base_agent: AgentInfo) -> Self {
        Self {
            base_agent,
            supported_modalities: vec![Modality::Text, Modality::Image], // Default capabilities
            context_window: Vec::new(),
        }
    }

    pub fn process_multimodal_input(&mut self, message: MultiModalMessage) -> Result<String> {
        if !self.supported_modalities.contains(&message.modality) {
            return Err(anyhow::anyhow!(
                "Agent does not support {:?} modality",
                message.modality
            ));
        }

        self.context_window.push(message.clone());

        // Process based on modality
        match message.modality {
            Modality::Text => {
                // Standard text processing
                ai::complete_sync_router(&message.content)
            }
            Modality::Image => {
                // For multi-modal LLMs like GPT-4V, we'd encode the image
                // and send it along with the text prompt
                let prompt = format!("Analyze this image: {}", message.content);
                ai::complete_sync_router(&prompt)
            }
            Modality::Audio => {
                // Audio transcription + processing
                let prompt = format!("Process this audio content: {}", message.content);
                ai::complete_sync_router(&prompt)
            }
            Modality::Video => {
                // Video analysis (frame extraction + description)
                let prompt = format!("Analyze this video: {}", message.content);
                ai::complete_sync_router(&prompt)
            }
        }
    }

    pub fn supports_modality(&self, modality: &Modality) -> bool {
        self.supported_modalities.contains(modality)
    }

    pub fn add_modality_support(&mut self, modality: Modality) {
        if !self.supported_modalities.contains(&modality) {
            self.supported_modalities.push(modality);
        }
    }
}
