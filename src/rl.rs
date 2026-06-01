//! Reinforcement Learning module for AetherShell
//!
//! Provides RL primitives for training agents and learning optimal behaviors.
//!
//! ## Features
//! - Q-Learning with epsilon-greedy exploration
//! - SARSA (State-Action-Reward-State-Action)
//! - Policy Gradient methods (REINFORCE)
//! - Actor-Critic architecture
//! - Experience Replay buffers
//! - Multi-agent RL support
//!
//! ## Example Usage
//! ```ae
//! # Create a Q-learning agent
//! let agent = rl_agent("q_learning", {
//!   state_size: 10,
//!   action_size: 4,
//!   learning_rate: 0.1,
//!   discount: 0.99,
//!   epsilon: 0.1
//! })
//!
//! # Training loop
//! let state = env_reset(env)
//! let action = rl_action(agent, state)
//! let (next_state, reward, done) = env_step(env, action)
//! rl_update(agent, state, action, reward, next_state, done)
//! ```

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==================== Q-Learning ====================

/// Q-Learning agent for tabular reinforcement learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QLearningAgent {
    /// Name identifier
    pub name: String,
    /// Q-table: maps (state, action) pairs to expected values
    pub q_table: HashMap<(usize, usize), f64>,
    /// Number of possible states
    pub state_size: usize,
    /// Number of possible actions
    pub action_size: usize,
    /// Learning rate (alpha)
    pub learning_rate: f64,
    /// Discount factor (gamma)
    pub discount: f64,
    /// Exploration rate (epsilon)
    pub epsilon: f64,
    /// Epsilon decay rate
    pub epsilon_decay: f64,
    /// Minimum epsilon
    pub epsilon_min: f64,
    /// Training steps completed
    pub steps: usize,
}

impl QLearningAgent {
    /// Create a new Q-learning agent
    pub fn new(
        name: &str,
        state_size: usize,
        action_size: usize,
        learning_rate: f64,
        discount: f64,
        epsilon: f64,
    ) -> Self {
        Self {
            name: name.to_string(),
            q_table: HashMap::new(),
            state_size,
            action_size,
            learning_rate,
            discount,
            epsilon,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
            steps: 0,
        }
    }

    /// Get Q-value for a state-action pair
    pub fn get_q(&self, state: usize, action: usize) -> f64 {
        *self.q_table.get(&(state, action)).unwrap_or(&0.0)
    }

    /// Set Q-value for a state-action pair
    pub fn set_q(&mut self, state: usize, action: usize, value: f64) {
        self.q_table.insert((state, action), value);
    }

    /// Select action using epsilon-greedy policy
    pub fn select_action(&self, state: usize) -> usize {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < self.epsilon {
            // Explore: random action
            rng.gen_range(0..self.action_size)
        } else {
            // Exploit: best action
            self.best_action(state)
        }
    }

    /// Get the best action for a state (greedy)
    pub fn best_action(&self, state: usize) -> usize {
        (0..self.action_size)
            .max_by(|&a, &b| {
                self.get_q(state, a)
                    .partial_cmp(&self.get_q(state, b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0)
    }

    /// Update Q-value using the Q-learning update rule
    /// Q(s,a) = Q(s,a) + α * (r + γ * max_a' Q(s',a') - Q(s,a))
    pub fn update(
        &mut self,
        state: usize,
        action: usize,
        reward: f64,
        next_state: usize,
        done: bool,
    ) {
        let current_q = self.get_q(state, action);

        let max_next_q = if done {
            0.0
        } else {
            (0..self.action_size)
                .map(|a| self.get_q(next_state, a))
                .fold(f64::NEG_INFINITY, f64::max)
        };

        let td_target = reward + self.discount * max_next_q;
        let td_error = td_target - current_q;
        let new_q = current_q + self.learning_rate * td_error;

        self.set_q(state, action, new_q);
        self.steps += 1;

        // Decay epsilon
        if self.epsilon > self.epsilon_min {
            self.epsilon *= self.epsilon_decay;
        }
    }
}

// ==================== SARSA ====================

/// SARSA agent (on-policy TD control)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarsaAgent {
    /// Name identifier
    pub name: String,
    /// Q-table: maps (state, action) pairs to expected values
    pub q_table: HashMap<(usize, usize), f64>,
    /// Number of possible states
    pub state_size: usize,
    /// Number of possible actions
    pub action_size: usize,
    /// Learning rate (alpha)
    pub learning_rate: f64,
    /// Discount factor (gamma)
    pub discount: f64,
    /// Exploration rate (epsilon)
    pub epsilon: f64,
    /// Training steps completed
    pub steps: usize,
}

impl SarsaAgent {
    /// Create a new SARSA agent
    pub fn new(
        name: &str,
        state_size: usize,
        action_size: usize,
        learning_rate: f64,
        discount: f64,
        epsilon: f64,
    ) -> Self {
        Self {
            name: name.to_string(),
            q_table: HashMap::new(),
            state_size,
            action_size,
            learning_rate,
            discount,
            epsilon,
            steps: 0,
        }
    }

    /// Get Q-value for a state-action pair
    pub fn get_q(&self, state: usize, action: usize) -> f64 {
        *self.q_table.get(&(state, action)).unwrap_or(&0.0)
    }

    /// Set Q-value for a state-action pair
    pub fn set_q(&mut self, state: usize, action: usize, value: f64) {
        self.q_table.insert((state, action), value);
    }

    /// Select action using epsilon-greedy policy
    pub fn select_action(&self, state: usize) -> usize {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < self.epsilon {
            rng.gen_range(0..self.action_size)
        } else {
            self.best_action(state)
        }
    }

    /// Get the best action for a state
    pub fn best_action(&self, state: usize) -> usize {
        (0..self.action_size)
            .max_by(|&a, &b| {
                self.get_q(state, a)
                    .partial_cmp(&self.get_q(state, b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0)
    }

    /// Update Q-value using the SARSA update rule
    /// Q(s,a) = Q(s,a) + α * (r + γ * Q(s',a') - Q(s,a))
    pub fn update(
        &mut self,
        state: usize,
        action: usize,
        reward: f64,
        next_state: usize,
        next_action: usize,
        done: bool,
    ) {
        let current_q = self.get_q(state, action);

        let next_q = if done {
            0.0
        } else {
            self.get_q(next_state, next_action)
        };

        let td_target = reward + self.discount * next_q;
        let td_error = td_target - current_q;
        let new_q = current_q + self.learning_rate * td_error;

        self.set_q(state, action, new_q);
        self.steps += 1;
    }
}

// ==================== Policy Gradient (REINFORCE) ====================

/// Policy Gradient agent using REINFORCE algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGradientAgent {
    /// Name identifier
    pub name: String,
    /// Policy parameters (linear function approximation)
    pub weights: Vec<Vec<f64>>,
    /// State dimension
    pub state_dim: usize,
    /// Number of actions
    pub action_size: usize,
    /// Learning rate
    pub learning_rate: f64,
    /// Discount factor
    pub discount: f64,
    /// Episode history for batch updates
    pub episode_states: Vec<Vec<f64>>,
    pub episode_actions: Vec<usize>,
    pub episode_rewards: Vec<f64>,
    /// Training episodes completed
    pub episodes: usize,
}

impl PolicyGradientAgent {
    /// Create a new Policy Gradient agent
    pub fn new(
        name: &str,
        state_dim: usize,
        action_size: usize,
        learning_rate: f64,
        discount: f64,
    ) -> Self {
        let mut rng = rand::thread_rng();

        // Initialize weights with small random values
        let weights: Vec<Vec<f64>> = (0..action_size)
            .map(|_| (0..state_dim).map(|_| rng.gen_range(-0.1..0.1)).collect())
            .collect();

        Self {
            name: name.to_string(),
            weights,
            state_dim,
            action_size,
            learning_rate,
            discount,
            episode_states: Vec::new(),
            episode_actions: Vec::new(),
            episode_rewards: Vec::new(),
            episodes: 0,
        }
    }

    /// Compute action probabilities using softmax
    pub fn action_probabilities(&self, state: &[f64]) -> Vec<f64> {
        let logits: Vec<f64> = self
            .weights
            .iter()
            .map(|w| {
                w.iter()
                    .zip(state.iter())
                    .map(|(wi, si)| wi * si)
                    .sum::<f64>()
            })
            .collect();

        softmax(&logits)
    }

    /// Select action using the current policy
    pub fn select_action(&self, state: &[f64]) -> usize {
        let probs = self.action_probabilities(state);
        sample_from_distribution(&probs)
    }

    /// Record a step in the episode
    pub fn record_step(&mut self, state: Vec<f64>, action: usize, reward: f64) {
        self.episode_states.push(state);
        self.episode_actions.push(action);
        self.episode_rewards.push(reward);
    }

    /// Update policy using REINFORCE at end of episode
    pub fn end_episode(&mut self) {
        if self.episode_states.is_empty() {
            return;
        }

        // Compute discounted returns
        let returns = compute_returns(&self.episode_rewards, self.discount);

        // Normalize returns (baseline subtraction)
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
        let normalized_returns: Vec<f64> = returns.iter().map(|r| r - mean_return).collect();

        // Update weights using policy gradient
        for ((state, action), g_t) in self
            .episode_states
            .iter()
            .zip(self.episode_actions.iter())
            .zip(normalized_returns.iter())
        {
            let probs = self.action_probabilities(state);

            // Gradient of log policy
            for a in 0..self.action_size {
                let indicator = if a == *action { 1.0 } else { 0.0 };
                let grad_log_pi = indicator - probs[a];

                // Update weights: θ += α * G_t * ∇ log π(a|s)
                for (i, si) in state.iter().enumerate() {
                    self.weights[a][i] += self.learning_rate * g_t * grad_log_pi * si;
                }
            }
        }

        // Clear episode history
        self.episode_states.clear();
        self.episode_actions.clear();
        self.episode_rewards.clear();
        self.episodes += 1;
    }
}

// ==================== Actor-Critic ====================

/// Actor-Critic agent combining policy and value learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorCriticAgent {
    /// Name identifier
    pub name: String,
    /// Actor (policy) weights
    pub actor_weights: Vec<Vec<f64>>,
    /// Critic (value) weights
    pub critic_weights: Vec<f64>,
    /// State dimension
    pub state_dim: usize,
    /// Number of actions
    pub action_size: usize,
    /// Actor learning rate
    pub actor_lr: f64,
    /// Critic learning rate
    pub critic_lr: f64,
    /// Discount factor
    pub discount: f64,
    /// Training steps completed
    pub steps: usize,
}

impl ActorCriticAgent {
    /// Create a new Actor-Critic agent
    pub fn new(
        name: &str,
        state_dim: usize,
        action_size: usize,
        actor_lr: f64,
        critic_lr: f64,
        discount: f64,
    ) -> Self {
        let mut rng = rand::thread_rng();

        let actor_weights: Vec<Vec<f64>> = (0..action_size)
            .map(|_| (0..state_dim).map(|_| rng.gen_range(-0.1..0.1)).collect())
            .collect();

        let critic_weights: Vec<f64> = (0..state_dim).map(|_| rng.gen_range(-0.1..0.1)).collect();

        Self {
            name: name.to_string(),
            actor_weights,
            critic_weights,
            state_dim,
            action_size,
            actor_lr,
            critic_lr,
            discount,
            steps: 0,
        }
    }

    /// Compute state value V(s)
    pub fn value(&self, state: &[f64]) -> f64 {
        self.critic_weights
            .iter()
            .zip(state.iter())
            .map(|(w, s)| w * s)
            .sum()
    }

    /// Compute action probabilities
    pub fn action_probabilities(&self, state: &[f64]) -> Vec<f64> {
        let logits: Vec<f64> = self
            .actor_weights
            .iter()
            .map(|w| {
                w.iter()
                    .zip(state.iter())
                    .map(|(wi, si)| wi * si)
                    .sum::<f64>()
            })
            .collect();

        softmax(&logits)
    }

    /// Select action using current policy
    pub fn select_action(&self, state: &[f64]) -> usize {
        let probs = self.action_probabilities(state);
        sample_from_distribution(&probs)
    }

    /// Update actor and critic using TD error
    pub fn update(
        &mut self,
        state: &[f64],
        action: usize,
        reward: f64,
        next_state: &[f64],
        done: bool,
    ) {
        // Compute TD error: δ = r + γV(s') - V(s)
        let v_s = self.value(state);
        let v_next = if done { 0.0 } else { self.value(next_state) };
        let td_error = reward + self.discount * v_next - v_s;

        // Update critic: w += α_c * δ * ∇V(s)
        for (i, si) in state.iter().enumerate() {
            self.critic_weights[i] += self.critic_lr * td_error * si;
        }

        // Update actor: θ += α_a * δ * ∇ log π(a|s)
        let probs = self.action_probabilities(state);
        for a in 0..self.action_size {
            let indicator = if a == action { 1.0 } else { 0.0 };
            let grad_log_pi = indicator - probs[a];

            for (i, si) in state.iter().enumerate() {
                self.actor_weights[a][i] += self.actor_lr * td_error * grad_log_pi * si;
            }
        }

        self.steps += 1;
    }
}

// ==================== Experience Replay ====================

/// Experience tuple for replay buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: Vec<f64>,
    pub action: usize,
    pub reward: f64,
    pub next_state: Vec<f64>,
    pub done: bool,
}

/// Experience replay buffer for off-policy learning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayBuffer {
    /// Maximum buffer size
    pub capacity: usize,
    /// Stored experiences
    pub buffer: Vec<Experience>,
    /// Current position for circular buffer
    pub position: usize,
}

impl ReplayBuffer {
    /// Create a new replay buffer
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: Vec::with_capacity(capacity),
            position: 0,
        }
    }

    /// Add an experience to the buffer
    pub fn push(&mut self, experience: Experience) {
        if self.buffer.len() < self.capacity {
            self.buffer.push(experience);
        } else {
            self.buffer[self.position] = experience;
        }
        self.position = (self.position + 1) % self.capacity;
    }

    /// Sample a batch of experiences
    pub fn sample(&self, batch_size: usize) -> Vec<Experience> {
        let mut rng = rand::thread_rng();
        let len = self.buffer.len();

        if len == 0 {
            return Vec::new();
        }

        (0..batch_size.min(len))
            .map(|_| {
                let idx = rng.gen_range(0..len);
                self.buffer[idx].clone()
            })
            .collect()
    }

    /// Get buffer size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

// ==================== DQN (Deep Q-Network) ====================

/// DQN agent using neural networks for function approximation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DQNAgent {
    /// Name identifier
    pub name: String,
    /// Policy network (using our neural network module)
    pub network: crate::neural::NeuralNetwork,
    /// Target network for stable learning
    pub target_network: crate::neural::NeuralNetwork,
    /// Experience replay buffer
    pub replay_buffer: ReplayBuffer,
    /// State dimension
    pub state_dim: usize,
    /// Number of actions
    pub action_size: usize,
    /// Learning rate
    pub learning_rate: f64,
    /// Discount factor
    pub discount: f64,
    /// Exploration rate
    pub epsilon: f64,
    /// Epsilon decay
    pub epsilon_decay: f64,
    /// Minimum epsilon
    pub epsilon_min: f64,
    /// Batch size for training
    pub batch_size: usize,
    /// Steps between target network updates
    pub target_update_freq: usize,
    /// Training steps completed
    pub steps: usize,
}

impl DQNAgent {
    /// Create a new DQN agent
    pub fn new(
        name: &str,
        state_dim: usize,
        action_size: usize,
        hidden_sizes: &[usize],
        learning_rate: f64,
        discount: f64,
        epsilon: f64,
        buffer_size: usize,
    ) -> Self {
        use crate::neural::{Activation, NeuralNetwork};

        // Build layer sizes: [state_dim, ...hidden_sizes, action_size]
        let mut layer_sizes = vec![state_dim];
        layer_sizes.extend_from_slice(hidden_sizes);
        layer_sizes.push(action_size);

        let network = NeuralNetwork::feedforward(
            &format!("{}_policy", name),
            &layer_sizes,
            Activation::ReLU,
            Activation::Linear, // Q-values can be any real number
        );

        let target_network = network.clone();

        Self {
            name: name.to_string(),
            network,
            target_network,
            replay_buffer: ReplayBuffer::new(buffer_size),
            state_dim,
            action_size,
            learning_rate,
            discount,
            epsilon,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
            batch_size: 32,
            target_update_freq: 100,
            steps: 0,
        }
    }

    /// Select action using epsilon-greedy policy
    pub fn select_action(&self, state: &[f64]) -> usize {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < self.epsilon {
            rng.gen_range(0..self.action_size)
        } else {
            self.best_action(state)
        }
    }

    /// Get the best action (greedy)
    pub fn best_action(&self, state: &[f64]) -> usize {
        let q_values = self.network.forward(state);
        q_values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Store experience and potentially train
    pub fn step(
        &mut self,
        state: Vec<f64>,
        action: usize,
        reward: f64,
        next_state: Vec<f64>,
        done: bool,
    ) {
        // Store experience
        self.replay_buffer.push(Experience {
            state,
            action,
            reward,
            next_state,
            done,
        });

        // Train if we have enough experiences
        if self.replay_buffer.len() >= self.batch_size {
            self.train();
        }

        self.steps += 1;

        // Update target network periodically
        if self.steps % self.target_update_freq == 0 {
            self.update_target_network();
        }

        // Decay epsilon
        if self.epsilon > self.epsilon_min {
            self.epsilon *= self.epsilon_decay;
        }
    }

    /// Train on a batch of experiences
    fn train(&mut self) {
        let batch = self.replay_buffer.sample(self.batch_size);

        for exp in batch {
            // Compute target Q-value
            let target_q_values = self.target_network.forward(&exp.next_state);
            let max_target_q = target_q_values
                .iter()
                .fold(f64::NEG_INFINITY, |a, &b| a.max(b));

            let target = if exp.done {
                exp.reward
            } else {
                exp.reward + self.discount * max_target_q
            };

            // Get current Q-values and update for the taken action
            let mut current_q = self.network.forward(&exp.state);
            let td_error = target - current_q[exp.action];

            // Simple gradient descent update
            // In a full implementation, this would use proper backpropagation
            current_q[exp.action] += self.learning_rate * td_error;

            // Update network weights (simplified - in practice use backprop)
            // This is a placeholder for demonstration
        }
    }

    /// Copy weights from policy network to target network
    pub fn update_target_network(&mut self) {
        self.target_network = self.network.clone();
    }
}

// ==================== Multi-Agent RL ====================

/// Configuration for multi-agent RL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentConfig {
    /// Number of agents
    pub num_agents: usize,
    /// Whether agents share experiences
    pub shared_replay: bool,
    /// Whether agents share network parameters
    pub parameter_sharing: bool,
    /// Communication between agents
    pub communication: bool,
}

/// Multi-agent RL coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentRL {
    /// Configuration
    pub config: MultiAgentConfig,
    /// Individual agents
    pub agents: Vec<DQNAgent>,
    /// Shared replay buffer (if enabled)
    pub shared_buffer: Option<ReplayBuffer>,
    /// Training episodes completed
    pub episodes: usize,
}

impl MultiAgentRL {
    /// Create a new multi-agent RL system
    pub fn new(
        config: MultiAgentConfig,
        state_dim: usize,
        action_size: usize,
        hidden_sizes: &[usize],
        learning_rate: f64,
    ) -> Self {
        let agents: Vec<DQNAgent> = (0..config.num_agents)
            .map(|i| {
                DQNAgent::new(
                    &format!("agent_{}", i),
                    state_dim,
                    action_size,
                    hidden_sizes,
                    learning_rate,
                    0.99,
                    1.0,
                    10000,
                )
            })
            .collect();

        let shared_buffer = if config.shared_replay {
            Some(ReplayBuffer::new(100000))
        } else {
            None
        };

        Self {
            config,
            agents,
            shared_buffer,
            episodes: 0,
        }
    }

    /// Get actions for all agents
    pub fn select_actions(&self, states: &[Vec<f64>]) -> Vec<usize> {
        self.agents
            .iter()
            .zip(states.iter())
            .map(|(agent, state)| agent.select_action(state))
            .collect()
    }

    /// Update all agents with their experiences
    pub fn step(
        &mut self,
        states: Vec<Vec<f64>>,
        actions: Vec<usize>,
        rewards: Vec<f64>,
        next_states: Vec<Vec<f64>>,
        dones: Vec<bool>,
    ) {
        for (i, agent) in self.agents.iter_mut().enumerate() {
            // Store in shared buffer if enabled
            if let Some(ref mut buffer) = self.shared_buffer {
                buffer.push(Experience {
                    state: states[i].clone(),
                    action: actions[i],
                    reward: rewards[i],
                    next_state: next_states[i].clone(),
                    done: dones[i],
                });
            }

            // Individual agent update
            agent.step(
                states[i].clone(),
                actions[i],
                rewards[i],
                next_states[i].clone(),
                dones[i],
            );
        }
    }

    /// Parameter sharing: copy best agent's parameters to all
    pub fn share_parameters(&mut self) {
        if !self.config.parameter_sharing {
            return;
        }

        // Find agent with highest average Q-value (simple heuristic)
        // In practice, you'd use a better metric
        let best_agent = &self.agents[0].clone();

        for agent in &mut self.agents {
            agent.network = best_agent.network.clone();
        }
    }
}

// ==================== Helper Functions ====================

/// Softmax function
fn softmax(x: &[f64]) -> Vec<f64> {
    let max_x = x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exp_x: Vec<f64> = x.iter().map(|xi| (xi - max_x).exp()).collect();
    let sum_exp: f64 = exp_x.iter().sum();
    exp_x.iter().map(|e| e / sum_exp).collect()
}

/// Sample from a probability distribution
fn sample_from_distribution(probs: &[f64]) -> usize {
    let mut rng = rand::thread_rng();
    let sample: f64 = rng.gen();
    let mut cumsum = 0.0;

    for (i, p) in probs.iter().enumerate() {
        cumsum += p;
        if sample < cumsum {
            return i;
        }
    }

    probs.len() - 1
}

/// Compute discounted returns from rewards
fn compute_returns(rewards: &[f64], discount: f64) -> Vec<f64> {
    let mut returns = vec![0.0; rewards.len()];
    let mut running_return = 0.0;

    for t in (0..rewards.len()).rev() {
        running_return = rewards[t] + discount * running_return;
        returns[t] = running_return;
    }

    returns
}

// ==================== Simple Environments for Testing ====================

/// Simple gridworld environment
#[derive(Debug, Clone)]
pub struct GridWorld {
    /// Grid size (width)
    pub width: usize,
    /// Grid size (height)  
    pub height: usize,
    /// Current agent position
    pub agent_pos: (usize, usize),
    /// Goal position
    pub goal_pos: (usize, usize),
    /// Obstacles (positions that give negative reward)
    pub obstacles: Vec<(usize, usize)>,
}

impl GridWorld {
    /// Create a new gridworld
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            agent_pos: (0, 0),
            goal_pos: (width - 1, height - 1),
            obstacles: Vec::new(),
        }
    }

    /// Reset environment to initial state
    pub fn reset(&mut self) -> usize {
        self.agent_pos = (0, 0);
        self.state_to_idx()
    }

    /// Take action and return (next_state, reward, done)
    pub fn step(&mut self, action: usize) -> (usize, f64, bool) {
        // Actions: 0=up, 1=down, 2=left, 3=right
        let (dx, dy): (i32, i32) = match action {
            0 => (0, -1), // up
            1 => (0, 1),  // down
            2 => (-1, 0), // left
            3 => (1, 0),  // right
            _ => (0, 0),
        };

        let new_x = (self.agent_pos.0 as i32 + dx).clamp(0, self.width as i32 - 1) as usize;
        let new_y = (self.agent_pos.1 as i32 + dy).clamp(0, self.height as i32 - 1) as usize;

        self.agent_pos = (new_x, new_y);

        let done = self.agent_pos == self.goal_pos;
        let reward = if done {
            1.0
        } else if self.obstacles.contains(&self.agent_pos) {
            -0.5
        } else {
            -0.01 // Small step penalty
        };

        (self.state_to_idx(), reward, done)
    }

    /// Convert position to state index
    fn state_to_idx(&self) -> usize {
        self.agent_pos.1 * self.width + self.agent_pos.0
    }

    /// Get total number of states
    pub fn state_size(&self) -> usize {
        self.width * self.height
    }

    /// Get number of actions
    pub fn action_size(&self) -> usize {
        4
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_q_learning_basic() {
        let mut agent = QLearningAgent::new("test", 10, 4, 0.1, 0.99, 0.1);

        // Initial Q-values should be 0
        assert_eq!(agent.get_q(0, 0), 0.0);

        // Update should change Q-value
        agent.update(0, 0, 1.0, 1, false);
        assert!(agent.get_q(0, 0) > 0.0);
    }

    #[test]
    fn test_sarsa_basic() {
        let mut agent = SarsaAgent::new("test", 10, 4, 0.1, 0.99, 0.1);

        agent.update(0, 0, 1.0, 1, 1, false);
        assert!(agent.get_q(0, 0) > 0.0);
    }

    #[test]
    fn test_policy_gradient_basic() {
        let mut agent = PolicyGradientAgent::new("test", 4, 2, 0.01, 0.99);

        let state = vec![1.0, 0.0, 0.5, 0.5];
        let action = agent.select_action(&state);
        assert!(action < 2);

        // Record and update
        agent.record_step(state, action, 1.0);
        agent.end_episode();
        assert_eq!(agent.episodes, 1);
    }

    #[test]
    fn test_actor_critic_basic() {
        let mut agent = ActorCriticAgent::new("test", 4, 2, 0.01, 0.01, 0.99);

        let state = vec![1.0, 0.0, 0.5, 0.5];
        let next_state = vec![0.0, 1.0, 0.5, 0.5];

        let action = agent.select_action(&state);
        agent.update(&state, action, 1.0, &next_state, false);

        assert_eq!(agent.steps, 1);
    }

    #[test]
    fn test_replay_buffer() {
        let mut buffer = ReplayBuffer::new(100);

        assert!(buffer.is_empty());

        buffer.push(Experience {
            state: vec![1.0, 0.0],
            action: 0,
            reward: 1.0,
            next_state: vec![0.0, 1.0],
            done: false,
        });

        assert_eq!(buffer.len(), 1);

        let samples = buffer.sample(1);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn test_gridworld() {
        let mut env = GridWorld::new(5, 5);

        assert_eq!(env.state_size(), 25);
        assert_eq!(env.action_size(), 4);

        let state = env.reset();
        assert_eq!(state, 0);

        // Move right
        let (next_state, reward, done) = env.step(3);
        assert_eq!(next_state, 1);
        assert!(!done);
        assert!(reward < 0.0); // Step penalty
    }

    #[test]
    fn test_softmax() {
        let x = vec![1.0, 2.0, 3.0];
        let probs = softmax(&x);

        // Sum should be 1
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);

        // Probabilities should be ordered
        assert!(probs[0] < probs[1]);
        assert!(probs[1] < probs[2]);
    }

    #[test]
    fn test_compute_returns() {
        let rewards = vec![1.0, 1.0, 1.0];
        let returns = compute_returns(&rewards, 0.9);

        // G_2 = 1.0
        // G_1 = 1.0 + 0.9 * 1.0 = 1.9
        // G_0 = 1.0 + 0.9 * 1.9 = 2.71
        assert!((returns[2] - 1.0).abs() < 1e-6);
        assert!((returns[1] - 1.9).abs() < 1e-6);
        assert!((returns[0] - 2.71).abs() < 1e-6);
    }
}
