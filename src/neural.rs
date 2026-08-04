//! Neural Network Primitives for AetherShell
//!
//! Provides lightweight neural network building blocks for in-shell learning,
//! particularly useful for evolving multi-agent communication protocols and
//! consensus mechanisms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Activation functions for neural network layers
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Activation {
    ReLU,
    Sigmoid,
    Tanh,
    Softmax,
    Linear,
    LeakyReLU(f64),
    Swish,
}

impl Activation {
    pub fn apply(&self, x: f64) -> f64 {
        match self {
            Activation::ReLU => x.max(0.0),
            Activation::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            Activation::Tanh => x.tanh(),
            Activation::Softmax => x.exp(), // Normalized in layer
            Activation::Linear => x,
            Activation::LeakyReLU(alpha) => {
                if x > 0.0 {
                    x
                } else {
                    alpha * x
                }
            }
            Activation::Swish => x * (1.0 / (1.0 + (-x).exp())),
        }
    }

    pub fn derivative(&self, x: f64) -> f64 {
        match self {
            Activation::ReLU => {
                if x > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Activation::Sigmoid => {
                let s = self.apply(x);
                s * (1.0 - s)
            }
            Activation::Tanh => 1.0 - x.tanh().powi(2),
            Activation::Softmax => 1.0, // Handled specially in backprop
            Activation::Linear => 1.0,
            Activation::LeakyReLU(alpha) => {
                if x > 0.0 {
                    1.0
                } else {
                    *alpha
                }
            }
            Activation::Swish => {
                let sig = 1.0 / (1.0 + (-x).exp());
                sig + x * sig * (1.0 - sig)
            }
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "relu" => Some(Activation::ReLU),
            "sigmoid" => Some(Activation::Sigmoid),
            "tanh" => Some(Activation::Tanh),
            "softmax" => Some(Activation::Softmax),
            "linear" => Some(Activation::Linear),
            "leaky_relu" => Some(Activation::LeakyReLU(0.01)),
            "swish" => Some(Activation::Swish),
            _ => None,
        }
    }
}

/// A dense (fully connected) layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenseLayer {
    pub weights: Vec<Vec<f64>>,
    pub biases: Vec<f64>,
    pub activation: Activation,
    pub input_size: usize,
    pub output_size: usize,
}

impl DenseLayer {
    pub fn new(input_size: usize, output_size: usize, activation: Activation) -> Self {
        // Xavier initialization
        let scale = (2.0 / (input_size + output_size) as f64).sqrt();
        let mut weights = vec![vec![0.0; input_size]; output_size];
        let mut biases = vec![0.0; output_size];

        for i in 0..output_size {
            for j in 0..input_size {
                weights[i][j] = (rand_f64() * 2.0 - 1.0) * scale;
            }
            biases[i] = 0.0;
        }

        Self {
            weights,
            biases,
            activation,
            input_size,
            output_size,
        }
    }

    pub fn forward(&self, input: &[f64]) -> Vec<f64> {
        let mut output = vec![0.0; self.output_size];

        for i in 0..self.output_size {
            let mut sum = self.biases[i];
            for j in 0..self.input_size {
                sum += self.weights[i][j] * input.get(j).unwrap_or(&0.0);
            }
            output[i] = self.activation.apply(sum);
        }

        // Handle softmax normalization
        if matches!(self.activation, Activation::Softmax) {
            let max = output.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let exp_sum: f64 = output.iter().map(|x| (x - max).exp()).sum();
            for x in &mut output {
                *x = (*x - max).exp() / exp_sum;
            }
        }

        output
    }

    pub fn param_count(&self) -> usize {
        self.input_size * self.output_size + self.output_size
    }

    /// Get flattened parameters (weights + biases)
    pub fn get_params(&self) -> Vec<f64> {
        let mut params = Vec::with_capacity(self.param_count());
        for row in &self.weights {
            params.extend(row);
        }
        params.extend(&self.biases);
        params
    }

    /// Set parameters from flattened vector
    pub fn set_params(&mut self, params: &[f64]) {
        let mut idx = 0;
        for i in 0..self.output_size {
            for j in 0..self.input_size {
                self.weights[i][j] = params[idx];
                idx += 1;
            }
        }
        for i in 0..self.output_size {
            self.biases[i] = params[idx];
            idx += 1;
        }
    }
}

/// A complete neural network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralNetwork {
    pub layers: Vec<DenseLayer>,
    pub name: String,
}

impl NeuralNetwork {
    pub fn new(name: &str) -> Self {
        Self {
            layers: Vec::new(),
            name: name.to_string(),
        }
    }

    /// Create a network from layer specifications
    /// Format: [(input, output, activation), ...]
    pub fn from_spec(name: &str, spec: &[(usize, usize, Activation)]) -> Self {
        let mut net = Self::new(name);
        for (input, output, activation) in spec {
            net.layers
                .push(DenseLayer::new(*input, *output, *activation));
        }
        net
    }

    /// Create a simple feedforward network
    pub fn feedforward(
        name: &str,
        layer_sizes: &[usize],
        hidden_activation: Activation,
        output_activation: Activation,
    ) -> Self {
        let mut net = Self::new(name);
        for i in 0..layer_sizes.len() - 1 {
            let activation = if i == layer_sizes.len() - 2 {
                output_activation
            } else {
                hidden_activation
            };
            net.layers.push(DenseLayer::new(
                layer_sizes[i],
                layer_sizes[i + 1],
                activation,
            ));
        }
        net
    }

    pub fn forward(&self, input: &[f64]) -> Vec<f64> {
        let mut current = input.to_vec();
        for layer in &self.layers {
            current = layer.forward(&current);
        }
        current
    }

    pub fn param_count(&self) -> usize {
        self.layers.iter().map(|l| l.param_count()).sum()
    }

    /// Get all parameters as a flat vector
    pub fn get_params(&self) -> Vec<f64> {
        let mut params = Vec::with_capacity(self.param_count());
        for layer in &self.layers {
            params.extend(layer.get_params());
        }
        params
    }

    /// Set all parameters from a flat vector
    pub fn set_params(&mut self, params: &[f64]) {
        let mut idx = 0;
        for layer in &mut self.layers {
            let count = layer.param_count();
            layer.set_params(&params[idx..idx + count]);
            idx += count;
        }
    }

    /// Clone with mutated parameters
    pub fn mutate(&self, mutation_rate: f64, mutation_strength: f64) -> Self {
        let mut new_net = self.clone();
        let mut params = new_net.get_params();

        for p in &mut params {
            if rand_f64() < mutation_rate {
                *p += (rand_f64() * 2.0 - 1.0) * mutation_strength;
            }
        }

        new_net.set_params(&params);
        new_net
    }

    /// Crossover with another network
    pub fn crossover(&self, other: &NeuralNetwork) -> Self {
        let mut new_net = self.clone();
        let params1 = self.get_params();
        let params2 = other.get_params();

        let mut new_params = Vec::with_capacity(params1.len());
        for (p1, p2) in params1.iter().zip(params2.iter()) {
            new_params.push(if rand_f64() < 0.5 { *p1 } else { *p2 });
        }

        new_net.set_params(&new_params);
        new_net
    }
}

/// A recurrent neural network cell (simple RNN)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RNNCell {
    pub input_weights: Vec<Vec<f64>>,
    pub hidden_weights: Vec<Vec<f64>>,
    pub biases: Vec<f64>,
    pub hidden_size: usize,
    pub input_size: usize,
    pub hidden_state: Vec<f64>,
}

impl RNNCell {
    pub fn new(input_size: usize, hidden_size: usize) -> Self {
        let scale = (2.0 / (input_size + hidden_size) as f64).sqrt();

        let mut input_weights = vec![vec![0.0; input_size]; hidden_size];
        let mut hidden_weights = vec![vec![0.0; hidden_size]; hidden_size];
        let biases = vec![0.0; hidden_size];

        for i in 0..hidden_size {
            for j in 0..input_size {
                input_weights[i][j] = (rand_f64() * 2.0 - 1.0) * scale;
            }
            for j in 0..hidden_size {
                hidden_weights[i][j] = (rand_f64() * 2.0 - 1.0) * scale;
            }
        }

        Self {
            input_weights,
            hidden_weights,
            biases,
            hidden_size,
            input_size,
            hidden_state: vec![0.0; hidden_size],
        }
    }

    pub fn forward(&mut self, input: &[f64]) -> Vec<f64> {
        let mut new_hidden = vec![0.0; self.hidden_size];

        for i in 0..self.hidden_size {
            let mut sum = self.biases[i];

            // Input contribution
            for j in 0..self.input_size {
                sum += self.input_weights[i][j] * input.get(j).unwrap_or(&0.0);
            }

            // Hidden state contribution
            for j in 0..self.hidden_size {
                sum += self.hidden_weights[i][j] * self.hidden_state[j];
            }

            new_hidden[i] = sum.tanh();
        }

        self.hidden_state = new_hidden.clone();
        new_hidden
    }

    pub fn reset(&mut self) {
        self.hidden_state = vec![0.0; self.hidden_size];
    }

    pub fn param_count(&self) -> usize {
        self.input_size * self.hidden_size + self.hidden_size * self.hidden_size + self.hidden_size
    }

    pub fn get_params(&self) -> Vec<f64> {
        let mut params = Vec::with_capacity(self.param_count());
        for row in &self.input_weights {
            params.extend(row);
        }
        for row in &self.hidden_weights {
            params.extend(row);
        }
        params.extend(&self.biases);
        params
    }

    pub fn set_params(&mut self, params: &[f64]) {
        let mut idx = 0;
        for i in 0..self.hidden_size {
            for j in 0..self.input_size {
                self.input_weights[i][j] = params[idx];
                idx += 1;
            }
        }
        for i in 0..self.hidden_size {
            for j in 0..self.hidden_size {
                self.hidden_weights[i][j] = params[idx];
                idx += 1;
            }
        }
        for i in 0..self.hidden_size {
            self.biases[i] = params[idx];
            idx += 1;
        }
    }
}

/// Attention mechanism for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionLayer {
    pub query_weights: Vec<Vec<f64>>,
    pub key_weights: Vec<Vec<f64>>,
    pub value_weights: Vec<Vec<f64>>,
    pub dim: usize,
}

impl AttentionLayer {
    pub fn new(dim: usize) -> Self {
        let scale = (1.0 / dim as f64).sqrt();

        let init_weights = || {
            let mut w = vec![vec![0.0; dim]; dim];
            for i in 0..dim {
                for j in 0..dim {
                    w[i][j] = (rand_f64() * 2.0 - 1.0) * scale;
                }
            }
            w
        };

        Self {
            query_weights: init_weights(),
            key_weights: init_weights(),
            value_weights: init_weights(),
            dim,
        }
    }

    /// Compute attention over a set of messages
    pub fn attend(&self, query: &[f64], keys: &[Vec<f64>], values: &[Vec<f64>]) -> Vec<f64> {
        if keys.is_empty() {
            return vec![0.0; self.dim];
        }

        // Transform query
        let q = self.transform(query, &self.query_weights);

        // Compute attention scores
        let mut scores: Vec<f64> = keys
            .iter()
            .map(|k| {
                let k_transformed = self.transform(k, &self.key_weights);
                let dot: f64 = q.iter().zip(k_transformed.iter()).map(|(a, b)| a * b).sum();
                dot / (self.dim as f64).sqrt()
            })
            .collect();

        // Softmax
        let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exp_sum: f64 = scores.iter().map(|s| (s - max).exp()).sum();
        for s in &mut scores {
            *s = (*s - max).exp() / exp_sum;
        }

        // Weighted sum of values
        let mut output = vec![0.0; self.dim];
        for (i, v) in values.iter().enumerate() {
            let v_transformed = self.transform(v, &self.value_weights);
            for (j, val) in v_transformed.iter().enumerate() {
                output[j] += scores[i] * val;
            }
        }

        output
    }

    fn transform(&self, input: &[f64], weights: &[Vec<f64>]) -> Vec<f64> {
        let mut output = vec![0.0; self.dim];
        for i in 0..self.dim {
            for j in 0..self.dim.min(input.len()) {
                output[i] += weights[i][j] * input[j];
            }
        }
        output
    }
}

/// Message encoder/decoder for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCodec {
    pub encoder: NeuralNetwork,
    pub decoder: NeuralNetwork,
    pub latent_dim: usize,
}

impl MessageCodec {
    pub fn new(message_dim: usize, latent_dim: usize) -> Self {
        let encoder = NeuralNetwork::feedforward(
            "encoder",
            &[message_dim, (message_dim + latent_dim) / 2, latent_dim],
            Activation::ReLU,
            Activation::Tanh,
        );

        let decoder = NeuralNetwork::feedforward(
            "decoder",
            &[latent_dim, (message_dim + latent_dim) / 2, message_dim],
            Activation::ReLU,
            Activation::Sigmoid,
        );

        Self {
            encoder,
            decoder,
            latent_dim,
        }
    }

    pub fn encode(&self, message: &[f64]) -> Vec<f64> {
        self.encoder.forward(message)
    }

    pub fn decode(&self, latent: &[f64]) -> Vec<f64> {
        self.decoder.forward(latent)
    }
}

/// Consensus network for distributed decision making
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusNetwork {
    pub name: String,
    pub local_encoder: NeuralNetwork,
    pub message_processor: NeuralNetwork,
    pub decision_network: NeuralNetwork,
    pub state_dim: usize,
    pub message_dim: usize,
    pub num_rounds: usize,
    pub agent_networks: Vec<NeuralNetwork>,
}

impl ConsensusNetwork {
    pub fn new(
        name: &str,
        agent_count: usize,
        state_dim: usize,
        message_dim: usize,
        decision_dim: usize,
    ) -> Self {
        let local_encoder = NeuralNetwork::feedforward(
            "local_encoder",
            &[state_dim, state_dim * 2, message_dim],
            Activation::ReLU,
            Activation::Tanh,
        );

        let message_processor = NeuralNetwork::feedforward(
            "message_processor",
            &[message_dim * 2, message_dim * 2, message_dim],
            Activation::ReLU,
            Activation::Tanh,
        );

        let decision_network = NeuralNetwork::feedforward(
            "decision_network",
            &[
                message_dim + state_dim,
                (message_dim + decision_dim) / 2,
                decision_dim,
            ],
            Activation::ReLU,
            Activation::Softmax,
        );

        // Create per-agent networks for individual processing
        let agent_networks: Vec<NeuralNetwork> = (0..agent_count)
            .map(|i| {
                NeuralNetwork::feedforward(
                    &format!("agent_{}", i),
                    &[state_dim, state_dim, message_dim],
                    Activation::ReLU,
                    Activation::Tanh,
                )
            })
            .collect();

        Self {
            name: name.to_string(),
            local_encoder,
            message_processor,
            decision_network,
            state_dim,
            message_dim,
            num_rounds: 3,
            agent_networks,
        }
    }

    /// Run consensus across all agents
    /// Returns (individual_decisions, consensus_output)
    pub fn consensus(&mut self, agent_inputs: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<f64>) {
        let num_agents = agent_inputs.len().min(self.agent_networks.len());

        // Encode each agent's state
        let mut messages: Vec<Vec<f64>> = agent_inputs
            .iter()
            .take(num_agents)
            .map(|input| self.local_encoder.forward(input))
            .collect();

        // Run message passing rounds
        for _ in 0..self.num_rounds {
            let mut new_messages = Vec::with_capacity(num_agents);
            for i in 0..num_agents {
                // Aggregate other agents' messages (simple mean)
                let mut aggregated = vec![0.0; self.message_dim];
                let mut count = 0;
                for (j, msg) in messages.iter().enumerate() {
                    if j != i {
                        for (k, v) in msg.iter().enumerate() {
                            if k < aggregated.len() {
                                aggregated[k] += v;
                            }
                        }
                        count += 1;
                    }
                }
                if count > 0 {
                    for v in &mut aggregated {
                        *v /= count as f64;
                    }
                }
                new_messages.push(self.process_messages(&messages[i], &aggregated));
            }
            messages = new_messages;
        }

        // Compute individual decisions
        let decisions: Vec<Vec<f64>> = messages
            .iter()
            .zip(agent_inputs.iter().take(num_agents))
            .map(|(msg, state)| self.decide(msg, state))
            .collect();

        // Compute consensus as mean of all decisions
        let decision_dim = decisions.first().map(|d| d.len()).unwrap_or(0);
        let mut consensus = vec![0.0; decision_dim];
        for decision in &decisions {
            for (i, v) in decision.iter().enumerate() {
                if i < consensus.len() {
                    consensus[i] += v;
                }
            }
        }
        let num_decisions = decisions.len() as f64;
        if num_decisions > 0.0 {
            for v in &mut consensus {
                *v /= num_decisions;
            }
        }

        (decisions, consensus)
    }

    /// Generate initial message from local state
    pub fn encode_state(&self, state: &[f64]) -> Vec<f64> {
        self.local_encoder.forward(state)
    }

    /// Process incoming messages and own state to produce new message
    pub fn process_messages(&self, own_message: &[f64], aggregated_messages: &[f64]) -> Vec<f64> {
        let mut input = own_message.to_vec();
        input.extend(aggregated_messages);
        self.message_processor.forward(&input)
    }

    /// Make final decision based on converged messages and local state
    pub fn decide(&self, final_message: &[f64], local_state: &[f64]) -> Vec<f64> {
        let mut input = final_message.to_vec();
        input.extend(local_state);
        self.decision_network.forward(&input)
    }

    pub fn param_count(&self) -> usize {
        self.local_encoder.param_count()
            + self.message_processor.param_count()
            + self.decision_network.param_count()
    }

    pub fn get_params(&self) -> Vec<f64> {
        let mut params = self.local_encoder.get_params();
        params.extend(self.message_processor.get_params());
        params.extend(self.decision_network.get_params());
        params
    }

    pub fn set_params(&mut self, params: &[f64]) {
        let mut idx = 0;

        let enc_count = self.local_encoder.param_count();
        self.local_encoder.set_params(&params[idx..idx + enc_count]);
        idx += enc_count;

        let proc_count = self.message_processor.param_count();
        self.message_processor
            .set_params(&params[idx..idx + proc_count]);
        idx += proc_count;

        let dec_count = self.decision_network.param_count();
        self.decision_network
            .set_params(&params[idx..idx + dec_count]);
    }
}

/// Registry for managing neural networks in the shell
#[derive(Debug, Default)]
pub struct NetworkRegistry {
    pub networks: HashMap<String, NeuralNetwork>,
    pub consensus_networks: HashMap<String, ConsensusNetwork>,
    pub rnn_cells: HashMap<String, RNNCell>,
}

impl NetworkRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_network(&mut self, net: NeuralNetwork) {
        self.networks.insert(net.name.clone(), net);
    }

    pub fn get_network(&self, name: &str) -> Option<&NeuralNetwork> {
        self.networks.get(name)
    }

    pub fn get_network_mut(&mut self, name: &str) -> Option<&mut NeuralNetwork> {
        self.networks.get_mut(name)
    }
}

// Simple pseudo-random number generator for reproducibility.
//
// Not cryptographic, and deliberately so — it seeds network weights, where
// reproducibility from `seed_rng` is the point. Never use it for tokens, salts
// or keys.
//
// An `AtomicU64` rather than a `static mut`: builtins are reachable from the
// multi-threaded agent API server, so two concurrent requests mutating a plain
// `static mut` would be a data race, which is undefined behaviour regardless of
// how benign the values are. `Relaxed` is sufficient because the state is not
// ordering anything else, and single-threaded callers see the identical
// sequence they did before.
static RNG_STATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(12345);

fn rand_f64() -> f64 {
    use std::sync::atomic::Ordering;
    let next = RNG_STATE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| {
            Some(s.wrapping_mul(6364136223846793005).wrapping_add(1))
        })
        // The closure always returns `Some`, so this cannot fail.
        .map(|prev| prev.wrapping_mul(6364136223846793005).wrapping_add(1))
        .unwrap_or(0);
    (next >> 33) as f64 / (1u64 << 31) as f64
}

pub fn seed_rng(seed: u64) {
    RNG_STATE.store(seed, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dense_layer_forward() {
        seed_rng(42);
        let layer = DenseLayer::new(3, 2, Activation::ReLU);
        let output = layer.forward(&[1.0, 2.0, 3.0]);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn test_neural_network_forward() {
        seed_rng(42);
        let net =
            NeuralNetwork::feedforward("test", &[4, 8, 2], Activation::ReLU, Activation::Softmax);
        let output = net.forward(&[1.0, 0.0, 1.0, 0.0]);
        assert_eq!(output.len(), 2);
        let sum: f64 = output.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6); // Softmax sums to 1
    }

    #[test]
    fn test_network_mutation() {
        seed_rng(42);
        let net =
            NeuralNetwork::feedforward("test", &[2, 4, 2], Activation::ReLU, Activation::Linear);
        let mutated = net.mutate(0.5, 0.1);
        assert_ne!(net.get_params(), mutated.get_params());
    }

    #[test]
    fn test_rnn_cell() {
        seed_rng(42);
        let mut rnn = RNNCell::new(4, 8);
        let out1 = rnn.forward(&[1.0, 0.0, 0.0, 0.0]);
        let out2 = rnn.forward(&[0.0, 1.0, 0.0, 0.0]);
        assert_eq!(out1.len(), 8);
        assert_ne!(out1, out2); // Different due to hidden state
    }

    #[test]
    fn test_consensus_network() {
        seed_rng(42);
        let mut consensus = ConsensusNetwork::new("test", 3, 4, 8, 3);
        let agent_inputs = vec![
            vec![1.0, 0.5, 0.0, 0.5],
            vec![0.5, 1.0, 0.5, 0.0],
            vec![0.0, 0.5, 1.0, 0.5],
        ];

        // Test individual encoding
        let message = consensus.encode_state(&agent_inputs[0]);
        assert_eq!(message.len(), 8);

        // Test consensus
        let (decisions, consensus_output) = consensus.consensus(&agent_inputs);
        assert_eq!(decisions.len(), 3);
        for decision in &decisions {
            assert_eq!(decision.len(), 3);
        }
        assert_eq!(consensus_output.len(), 3);
    }

    #[test]
    fn test_attention_layer() {
        seed_rng(42);
        let attention = AttentionLayer::new(4);
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let keys = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0, 1.0, 0.0, 0.0]];
        let values = vec![vec![1.0, 1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0, 1.0]];
        let output = attention.attend(&query, &keys, &values);
        assert_eq!(output.len(), 4);
    }
}
