//! Distributed agent networking and coordination

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};
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
    #[serde(default)]
    pub location: Option<GeoLocation>,
    #[serde(default)]
    pub cost_per_task: f64,
    #[serde(default)]
    pub latency_ms: f64,
}

/// Geographic location for proximity-based selection
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub region: String,
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
    pub coordinator_location: Option<GeoLocation>,
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
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
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

/// Measure network latency to a specific address
async fn measure_latency(addr: SocketAddr) -> f64 {
    let start = Instant::now();
    
    match TcpStream::connect(addr).await {
        Ok(mut stream) => {
            // Send a ping message
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            
            let ping = NetworkMessage::Ping { timestamp };
            if let Ok(bytes) = serde_json::to_vec(&ping) {
                let len = bytes.len() as u32;
                let _ = stream.write_all(&len.to_be_bytes()).await;
                let _ = stream.write_all(&bytes).await;
            }
            
            start.elapsed().as_secs_f64() * 1000.0 // Convert to milliseconds
        }
        Err(_) => {
            f64::MAX // Connection failed
        }
    }
}

/// Calculate geographic distance using Haversine formula
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();
    
    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    
    EARTH_RADIUS_KM * c
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

        self.message_tx
            .send(NetworkMessage::AgentRegistration { agent })?;

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
        let active_tasks = tasks
            .iter()
            .filter(|t| matches!(t.status, DistributedTaskStatus::Running))
            .count();
        let completed_tasks = completed.len();
        let average_load = if total_agents > 0 {
            agents.values().map(|a| a.load).sum::<f32>() / total_agents as f32
        } else {
            0.0
        };

        // Calculate average network latency from all agents
        let network_latency = if total_agents > 0 {
            let latency_sum: f64 = agents.values().map(|a| a.latency_ms).sum();
            latency_sum / total_agents as f64
        } else {
            0.0
        };

        SwarmStatus {
            total_agents,
            active_tasks,
            completed_tasks,
            average_load,
            network_latency,
        }
    }

    /// Update latency measurements for all agents
    pub async fn update_latencies(&self) {
        let agents_read = self.network_agents.read().await;
        let addresses: Vec<_> = agents_read.values().map(|a| (a.id, a.address)).collect();
        drop(agents_read);

        for (agent_id, addr) in addresses {
            let latency = measure_latency(addr).await;
            if let Some(agent) = self.network_agents.write().await.get_mut(&agent_id) {
                agent.latency_ms = latency;
            }
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
                        if let Ok(Some(agent_id)) =
                            coordinator.select_best_agent(&agents_read, task).await
                        {
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
            coordinator_location: None,
        }
    }

    /// Set the coordinator's geographic location for proximity calculations
    pub fn set_location(&mut self, location: GeoLocation) {
        self.coordinator_location = Some(location);
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
                matches!(agent.status, NetworkAgentStatus::Available)
                    && agent.load < self.load_balancer.max_load_per_agent
                    && self.agent_has_capabilities(agent, &task.required_capabilities)
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
                    .min_by(|a, b| {
                        a.load
                            .partial_cmp(&b.load)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
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
                // Select agent closest to coordinator
                if let Some(ref coord_loc) = self.coordinator_location {
                    available_agents
                        .iter()
                        .filter(|a| a.location.is_some())
                        .min_by(|a, b| {
                            let a_loc = a.location.as_ref().unwrap();
                            let b_loc = b.location.as_ref().unwrap();
                            let dist_a = haversine_distance(
                                coord_loc.latitude, coord_loc.longitude,
                                a_loc.latitude, a_loc.longitude
                            );
                            let dist_b = haversine_distance(
                                coord_loc.latitude, coord_loc.longitude,
                                b_loc.latitude, b_loc.longitude
                            );
                            dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|agent| agent.id)
                } else {
                    // Fallback to load-balanced if no location
                    available_agents
                        .iter()
                        .min_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|agent| agent.id)
                }
            }
            DistributionStrategy::CostOptimized => {
                // Select agent with lowest cost per task
                available_agents
                    .iter()
                    .min_by(|a, b| {
                        a.cost_per_task
                            .partial_cmp(&b.cost_per_task)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|agent| agent.id)
            }
            DistributionStrategy::LatencyOptimized => {
                // Select agent with lowest network latency
                available_agents
                    .iter()
                    .min_by(|a, b| {
                        a.latency_ms
                            .partial_cmp(&b.latency_ms)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|agent| agent.id)
            }
        };

        Ok(selected)
    }

    fn agent_has_capabilities(&self, agent: &NetworkAgent, required: &[String]) -> bool {
        required.iter().all(|cap| agent.capabilities.contains(cap))
    }

    fn calculate_capability_score(&self, agent: &NetworkAgent, required: &[String]) -> usize {
        required
            .iter()
            .filter(|cap| agent.capabilities.contains(cap))
            .count()
    }
}

/// Handle incoming network connections
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    agents: Arc<RwLock<HashMap<Uuid, NetworkAgent>>>,
    message_tx: mpsc::UnboundedSender<NetworkMessage>,
) {
    tracing::info!("New connection from: {}", addr);
    
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    
    loop {
        // Read message length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        if reader.read_exact(&mut len_buf).await.is_err() {
            break; // Connection closed
        }
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        
        // Read message body
        let mut msg_buf = vec![0u8; msg_len];
        if reader.read_exact(&mut msg_buf).await.is_err() {
            break;
        }
        
        // Deserialize and handle message
        match serde_json::from_slice::<NetworkMessage>(&msg_buf) {
            Ok(message) => {
                match &message {
                    NetworkMessage::AgentRegistration { agent } => {
                        let mut agents_write = agents.write().await;
                        agents_write.insert(agent.id, agent.clone());
                        let _ = message_tx.send(message.clone());
                        
                        // Send acknowledgment
                        let ack = serde_json::json!({"status": "registered", "agent_id": agent.id.to_string()});
                        let ack_bytes = serde_json::to_vec(&ack).unwrap();
                        let _ = writer.write_all(&(ack_bytes.len() as u32).to_be_bytes()).await;
                        let _ = writer.write_all(&ack_bytes).await;
                    }
                    NetworkMessage::Heartbeat { agent_id, load, status } => {
                        let mut agents_write = agents.write().await;
                        if let Some(agent) = agents_write.get_mut(agent_id) {
                            agent.load = *load;
                            agent.status = status.clone();
                            agent.last_heartbeat = chrono::Utc::now();
                        }
                        let _ = message_tx.send(message.clone());
                    }
                    NetworkMessage::TaskResult { task_id, result } => {
                        let _ = message_tx.send(NetworkMessage::TaskResult {
                            task_id: *task_id,
                            result: result.clone(),
                        });
                    }
                    NetworkMessage::AgentShutdown { agent_id } => {
                        let mut agents_write = agents.write().await;
                        agents_write.remove(agent_id);
                        let _ = message_tx.send(message.clone());
                    }
                    NetworkMessage::Ping { timestamp } => {
                        // Respond with Pong
                        let pong = NetworkMessage::Pong { timestamp: *timestamp };
                        if let Ok(pong_bytes) = serde_json::to_vec(&pong) {
                            let _ = writer.write_all(&(pong_bytes.len() as u32).to_be_bytes()).await;
                            let _ = writer.write_all(&pong_bytes).await;
                        }
                    }
                    _ => {
                        // Forward other messages to the channel
                        let _ = message_tx.send(message);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse message from {}: {}", addr, e);
            }
        }
    }
    
    tracing::info!("Connection closed from: {}", addr);
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

    /// Send a message to the swarm coordinator
    async fn send_message(&mut self, message: &NetworkMessage) -> Result<()> {
        if let Some(ref mut stream) = self.connection {
            let msg_bytes = serde_json::to_vec(message)?;
            stream.write_all(&(msg_bytes.len() as u32).to_be_bytes()).await?;
            stream.write_all(&msg_bytes).await?;
            stream.flush().await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Not connected to swarm"))
        }
    }

    /// Read a response from the swarm
    async fn read_response(&mut self) -> Result<serde_json::Value> {
        if let Some(ref mut stream) = self.connection {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let msg_len = u32::from_be_bytes(len_buf) as usize;
            
            let mut msg_buf = vec![0u8; msg_len];
            stream.read_exact(&mut msg_buf).await?;
            
            Ok(serde_json::from_slice(&msg_buf)?)
        } else {
            Err(anyhow::anyhow!("Not connected to swarm"))
        }
    }

    pub async fn register_agent(&mut self, agent: NetworkAgent) -> Result<()> {
        let message = NetworkMessage::AgentRegistration { agent };
        self.send_message(&message).await?;
        
        // Wait for acknowledgment
        let response = self.read_response().await?;
        if response.get("status").and_then(|s| s.as_str()) == Some("registered") {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Registration failed"))
        }
    }

    pub async fn send_heartbeat(&mut self, load: f32, status: NetworkAgentStatus) -> Result<()> {
        let message = NetworkMessage::Heartbeat {
            agent_id: self.agent_id,
            load,
            status,
        };
        self.send_message(&message).await
    }

    pub async fn send_task_result(&mut self, task_id: Uuid, result: TaskResult) -> Result<()> {
        let message = NetworkMessage::TaskResult { task_id, result };
        self.send_message(&message).await
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        let message = NetworkMessage::AgentShutdown { agent_id: self.agent_id };
        self.send_message(&message).await?;
        self.connection = None;
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
            location: None,
            cost_per_task: 0.01,
            latency_ms: 0.0,
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
            location: None,
            cost_per_task: 0.0,
            latency_ms: 0.0,
        };

        let required = vec!["text".to_string(), "image".to_string()];
        assert!(coordinator.agent_has_capabilities(&agent, &required));

        let score = coordinator.calculate_capability_score(&agent, &required);
        assert_eq!(score, 2);
    }

    #[test]
    fn test_haversine_distance() {
        // New York to London approximately 5570 km
        let dist = haversine_distance(40.7128, -74.0060, 51.5074, -0.1278);
        assert!(dist > 5500.0 && dist < 5700.0);
        
        // Same point should be 0
        let dist = haversine_distance(0.0, 0.0, 0.0, 0.0);
        assert!(dist < 0.001);
    }
}
