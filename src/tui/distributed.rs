//! Distributed agent networking and coordination

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::ai::MultiModalMessage;

/// Task priority levels for distributed processing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Network-connected agent that can communicate across the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAgent {
    pub id: Uuid,
    pub name: String,
    pub address: SocketAddr,
    pub capabilities: Vec<String>,
    pub load: f32,
    pub status: NetworkAgentStatus,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkAgentStatus {
    Available,
    Busy,
    Offline,
    Error(String),
}

/// Distributed swarm coordinator managing agents across the network
#[derive(Debug)]
pub struct DistributedSwarm {
    pub id: Uuid,
    pub network_agents: Arc<RwLock<HashMap<Uuid, NetworkAgent>>>,
    pub task_queue: Arc<RwLock<Vec<DistributedTask>>>,
    pub completed_tasks: Arc<RwLock<Vec<DistributedTask>>>,
    pub coordinator: DistributedCoordinator,
    pub listener: Option<TcpListener>,
    pub message_tx: mpsc::UnboundedSender<NetworkMessage>,
    pub message_rx: Arc<RwLock<mpsc::UnboundedReceiver<NetworkMessage>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedTask {
    pub id: Uuid,
    pub description: String,
    pub assigned_agent: Option<Uuid>,
    pub priority: TaskPriority,
    pub required_capabilities: Vec<String>,
    pub input_data: MultiModalMessage,
    pub result: Option<TaskResult>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub dependencies: Vec<Uuid>,
    pub status: DistributedTaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributedTaskStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub output: MultiModalMessage,
    pub metadata: HashMap<String, String>,
    pub execution_time: f64,
    pub confidence_score: f32,
}

/// Advanced coordinator with multiple distribution strategies
#[derive(Debug, Clone)]
pub struct DistributedCoordinator {
    pub strategy: DistributionStrategy,
    pub load_balancer: LoadBalancer,
    pub failure_handler: FailureHandler,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributionStrategy {
    RoundRobin,
    LoadBalanced,
    CapabilityBased,
    GeographicProximity,
    CostOptimized,
    LatencyOptimized,
}

#[derive(Debug, Clone)]
pub struct LoadBalancer {
    pub max_load_per_agent: f32,
    pub rebalance_threshold: f32,
    pub health_check_interval: u64,
}

#[derive(Debug, Clone)]
pub struct FailureHandler {
    pub max_retries: u32,
    pub retry_delay: u64,
    pub fallback_strategy: FallbackStrategy,
}

#[derive(Debug, Clone)]
pub enum FallbackStrategy {
    ReassignToOther,
    RetryLocally,
    SkipTask,
    EscalateToHuman,
}

/// Network messages for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    AgentRegistration {
        agent: NetworkAgent,
    },
    TaskAssignment {
        task: DistributedTask,
        target_agent: Uuid,
    },
    TaskResult {
        task_id: Uuid,
        result: TaskResult,
    },
    Heartbeat {
        agent_id: Uuid,
        load: f32,
        status: NetworkAgentStatus,
    },
    AgentShutdown {
        agent_id: Uuid,
    },
    SwarmUpdate {
        swarm_status: SwarmStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmStatus {
    pub total_agents: usize,
    pub active_tasks: usize,
    pub completed_tasks: usize,
    pub average_load: f32,
    pub network_latency: f64,
}

impl DistributedSwarm {
    /// Create a new distributed swarm
    pub async fn new(listen_addr: SocketAddr) -> Result<Self> {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let listener = TcpListener::bind(listen_addr).await?;
        
        Ok(Self {
            id: Uuid::new_v4(),
            network_agents: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(RwLock::new(Vec::new())),
            completed_tasks: Arc::new(RwLock::new(Vec::new())),
            coordinator: DistributedCoordinator::new(),
            listener: Some(listener),
            message_tx,
            message_rx: Arc::new(RwLock::new(message_rx)),
        })
    }

    /// Start the distributed swarm coordinator
    pub async fn start(&mut self) -> Result<()> {
        if let Some(listener) = self.listener.take() {
            let agents = Arc::clone(&self.network_agents);
            let message_tx = self.message_tx.clone();
            
            tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((stream, addr)) => {
                            let agents_clone = Arc::clone(&agents);
                            let tx_clone = message_tx.clone();
                            tokio::spawn(handle_connection(stream, addr, agents_clone, tx_clone));
                        }
                        Err(e) => {
                            eprintln!("Failed to accept connection: {}", e);
                        }
                    }
                }
            });
        }
        
        // Start task distribution loop
        self.start_task_distribution().await?;
        
        Ok(())
    }

    /// Register a new network agent
    pub async fn register_agent(&self, agent: NetworkAgent) -> Result<()> {
        let mut agents = self.network_agents.write().await;
        agents.insert(agent.id, agent.clone());
        
        self.message_tx.send(NetworkMessage::AgentRegistration { agent })?;
        
        Ok(())
    }

    /// Assign task to best available agent based on strategy
    pub async fn assign_task(&self, task: DistributedTask) -> Result<()> {
        let agents = self.network_agents.read().await;
        let best_agent = self.coordinator.select_best_agent(&agents, &task).await?;
        
        if let Some(agent_id) = best_agent {
            let mut updated_task = task.clone();
            updated_task.assigned_agent = Some(agent_id);
            updated_task.status = DistributedTaskStatus::Assigned;
            
            self.message_tx.send(NetworkMessage::TaskAssignment {
                task: updated_task.clone(),
                target_agent: agent_id,
            })?;
            
            let mut queue = self.task_queue.write().await;
            queue.push(updated_task);
        }
        
        Ok(())
    }

    /// Get swarm performance metrics
    pub async fn get_metrics(&self) -> SwarmStatus {
        let agents = self.network_agents.read().await;
        let tasks = self.task_queue.read().await;
        let completed = self.completed_tasks.read().await;
        
        let total_agents = agents.len();
        let active_tasks = tasks.iter().filter(|t| matches!(t.status, DistributedTaskStatus::Running)).count();
        let completed_tasks = completed.len();
        let average_load = if total_agents > 0 {
            agents.values().map(|a| a.load).sum::<f32>() / total_agents as f32
        } else {
            0.0
        };
        
        SwarmStatus {
            total_agents,
            active_tasks,
            completed_tasks,
            average_load,
            network_latency: 0.0, // TODO: Implement latency measurement
        }
    }

    async fn start_task_distribution(&self) -> Result<()> {
        let queue = Arc::clone(&self.task_queue);
        let agents = Arc::clone(&self.network_agents);
        let coordinator = self.coordinator.clone();
        let message_tx = self.message_tx.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                let mut task_queue = queue.write().await;
                let agents_read = agents.read().await;
                
                // Find pending tasks and assign them
                for task in task_queue.iter_mut() {
                    if matches!(task.status, DistributedTaskStatus::Pending) {
                        if let Ok(Some(agent_id)) = coordinator.select_best_agent(&agents_read, task).await {
                            task.assigned_agent = Some(agent_id);
                            task.status = DistributedTaskStatus::Assigned;
                            
                            let _ = message_tx.send(NetworkMessage::TaskAssignment {
                                task: task.clone(),
                                target_agent: agent_id,
                            });
                        }
                    }
                }
            }
        });
        
        Ok(())
    }

    /// Shutdown the distributed swarm and cleanup resources
    pub async fn shutdown(&mut self) -> Result<()> {
        // Clean up resources, close connections, etc.
        self.network_agents.write().await.clear();
        self.task_queue.write().await.clear();
        self.completed_tasks.write().await.clear();
        
        // Close the TCP listener
        if let Some(_listener) = self.listener.take() {
            // Listener will be dropped and closed automatically
        }
        
        Ok(())
    }
}

impl DistributedCoordinator {
    pub fn new() -> Self {
        Self {
            strategy: DistributionStrategy::LoadBalanced,
            load_balancer: LoadBalancer {
                max_load_per_agent: 0.8,
                rebalance_threshold: 0.6,
                health_check_interval: 30,
            },
            failure_handler: FailureHandler {
                max_retries: 3,
                retry_delay: 5000,
                fallback_strategy: FallbackStrategy::ReassignToOther,
            },
        }
    }

    /// Select the best agent for a task based on the current strategy
    pub async fn select_best_agent(
        &self,
        agents: &HashMap<Uuid, NetworkAgent>,
        task: &DistributedTask,
    ) -> Result<Option<Uuid>> {
        let available_agents: Vec<_> = agents
            .values()
            .filter(|agent| {
                matches!(agent.status, NetworkAgentStatus::Available) &&
                agent.load < self.load_balancer.max_load_per_agent &&
                self.agent_has_capabilities(agent, &task.required_capabilities)
            })
            .collect();

        if available_agents.is_empty() {
            return Ok(None);
        }

        let selected = match self.strategy {
            DistributionStrategy::RoundRobin => {
                // Simple round-robin selection
                available_agents.first().map(|agent| agent.id)
            }
            DistributionStrategy::LoadBalanced => {
                // Select agent with lowest load
                available_agents
                    .iter()
                    .min_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|agent| agent.id)
            }
            DistributionStrategy::CapabilityBased => {
                // Select agent with best capability match
                available_agents
                    .iter()
                    .max_by_key(|agent| {
                        self.calculate_capability_score(agent, &task.required_capabilities)
                    })
                    .map(|agent| agent.id)
            }
            DistributionStrategy::GeographicProximity => {
                // TODO: Implement geographic proximity selection
                available_agents.first().map(|agent| agent.id)
            }
            DistributionStrategy::CostOptimized => {
                // TODO: Implement cost-based selection
                available_agents.first().map(|agent| agent.id)
            }
            DistributionStrategy::LatencyOptimized => {
                // TODO: Implement latency-based selection
                available_agents.first().map(|agent| agent.id)
            }
        };

        Ok(selected)
    }

    fn agent_has_capabilities(&self, agent: &NetworkAgent, required: &[String]) -> bool {
        required.iter().all(|cap| agent.capabilities.contains(cap))
    }

    fn calculate_capability_score(&self, agent: &NetworkAgent, required: &[String]) -> usize {
        required.iter().filter(|cap| agent.capabilities.contains(cap)).count()
    }
}

/// Handle incoming network connections
async fn handle_connection(
    _stream: TcpStream,
    addr: SocketAddr,
    _agents: Arc<RwLock<HashMap<Uuid, NetworkAgent>>>,
    _message_tx: mpsc::UnboundedSender<NetworkMessage>,
) {
    println!("New connection from: {}", addr);
    
    // TODO: Implement message protocol for agent communication
    // This would handle serialization/deserialization of NetworkMessage
    // and maintain persistent connections with agents
}

/// Client for connecting to a distributed swarm
#[derive(Debug)]
pub struct SwarmClient {
    pub agent_id: Uuid,
    pub swarm_address: SocketAddr,
    pub connection: Option<TcpStream>,
}

impl SwarmClient {
    pub fn new(agent_id: Uuid, swarm_address: SocketAddr) -> Self {
        Self {
            agent_id,
            swarm_address,
            connection: None,
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let stream = TcpStream::connect(self.swarm_address).await?;
        self.connection = Some(stream);
        Ok(())
    }

    pub async fn register_agent(&self, _agent: NetworkAgent) -> Result<()> {
        // TODO: Send registration message to swarm coordinator
        Ok(())
    }

    pub async fn send_heartbeat(&self, _load: f32, _status: NetworkAgentStatus) -> Result<()> {
        // TODO: Send heartbeat message
        Ok(())
    }

    pub async fn send_task_result(&self, _task_id: Uuid, _result: TaskResult) -> Result<()> {
        // TODO: Send task completion result
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_distributed_swarm_creation() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let swarm = DistributedSwarm::new(addr).await.unwrap();
        
        assert!(!swarm.id.is_nil());
        assert_eq!(swarm.network_agents.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_agent_registration() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let swarm = DistributedSwarm::new(addr).await.unwrap();
        
        let agent = NetworkAgent {
            id: Uuid::new_v4(),
            name: "Test Agent".to_string(),
            address: "127.0.0.1:8081".parse().unwrap(),
            capabilities: vec!["text".to_string(), "image".to_string()],
            load: 0.0,
            status: NetworkAgentStatus::Available,
            last_heartbeat: chrono::Utc::now(),
        };
        
        swarm.register_agent(agent.clone()).await.unwrap();
        
        let agents = swarm.network_agents.read().await;
        assert_eq!(agents.len(), 1);
        assert!(agents.contains_key(&agent.id));
    }

    #[test]
    fn test_capability_matching() {
        let coordinator = DistributedCoordinator::new();
        
        let agent = NetworkAgent {
            id: Uuid::new_v4(),
            name: "Test Agent".to_string(),
            address: "127.0.0.1:8081".parse().unwrap(),
            capabilities: vec!["text".to_string(), "image".to_string(), "audio".to_string()],
            load: 0.0,
            status: NetworkAgentStatus::Available,
            last_heartbeat: chrono::Utc::now(),
        };
        
        let required = vec!["text".to_string(), "image".to_string()];
        assert!(coordinator.agent_has_capabilities(&agent, &required));
        
        let score = coordinator.calculate_capability_score(&agent, &required);
        assert_eq!(score, 2);
    }
}
