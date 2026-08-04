//! Evolutionary Algorithm Primitives for AetherShell
//!
//! Provides genetic algorithms, neuroevolution, and population-based optimization
//! for evolving agent behaviors, communication protocols, and consensus mechanisms.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::neural::{seed_rng, ConsensusNetwork, NeuralNetwork};

/// Fitness function result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessResult {
    pub score: f64,
    pub metrics: HashMap<String, f64>,
}

impl FitnessResult {
    pub fn new(score: f64) -> Self {
        Self {
            score,
            metrics: HashMap::new(),
        }
    }

    pub fn with_metric(mut self, name: &str, value: f64) -> Self {
        self.metrics.insert(name.to_string(), value);
        self
    }
}

/// Selection strategies for evolutionary algorithms
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SelectionStrategy {
    /// Tournament selection with given size
    Tournament(usize),
    /// Roulette wheel (fitness proportionate)
    Roulette,
    /// Rank-based selection
    Rank,
    /// Truncation selection (top N%)
    Truncation(f64),
    /// Elitism (keep top N)
    Elite(usize),
}

/// Crossover strategies
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CrossoverStrategy {
    /// Single point crossover
    SinglePoint,
    /// Two point crossover
    TwoPoint,
    /// Uniform crossover with given probability
    Uniform(f64),
    /// Blend crossover (BLX-α)
    Blend(f64),
    /// Simulated binary crossover
    SBX(f64),
}

/// An individual in the population
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Individual<G: Clone> {
    pub genome: G,
    pub fitness: Option<FitnessResult>,
    pub generation: usize,
    pub id: usize,
}

impl<G: Clone> Individual<G> {
    pub fn new(genome: G, id: usize) -> Self {
        Self {
            genome,
            fitness: None,
            generation: 0,
            id,
        }
    }

    pub fn score(&self) -> f64 {
        self.fitness
            .as_ref()
            .map(|f| f.score)
            .unwrap_or(f64::NEG_INFINITY)
    }
}

/// A genome that can be evolved
pub trait Evolvable: Clone + Send + Sync {
    /// Create a random instance
    fn random() -> Self;

    /// Mutate with given rate and strength
    fn mutate(&self, rate: f64, strength: f64) -> Self;

    /// Crossover with another genome
    fn crossover(&self, other: &Self) -> Self;

    /// Get parameter count for statistics
    fn param_count(&self) -> usize;
}

impl Evolvable for NeuralNetwork {
    fn random() -> Self {
        // Default architecture - should be overridden
        NeuralNetwork::feedforward(
            "random",
            &[4, 8, 4],
            crate::neural::Activation::ReLU,
            crate::neural::Activation::Tanh,
        )
    }

    fn mutate(&self, rate: f64, strength: f64) -> Self {
        self.mutate(rate, strength)
    }

    fn crossover(&self, other: &Self) -> Self {
        NeuralNetwork::crossover(self, other)
    }

    fn param_count(&self) -> usize {
        self.param_count()
    }
}

impl Evolvable for Vec<f64> {
    fn random() -> Self {
        vec![rand_f64(); 10]
    }

    fn mutate(&self, rate: f64, strength: f64) -> Self {
        self.iter()
            .map(|x| {
                if rand_f64() < rate {
                    x + (rand_f64() * 2.0 - 1.0) * strength
                } else {
                    *x
                }
            })
            .collect()
    }

    fn crossover(&self, other: &Self) -> Self {
        self.iter()
            .zip(other.iter())
            .map(|(a, b)| if rand_f64() < 0.5 { *a } else { *b })
            .collect()
    }

    fn param_count(&self) -> usize {
        self.len()
    }
}

/// Configuration for evolutionary algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionConfig {
    pub population_size: usize,
    pub generations: usize,
    pub mutation_rate: f64,
    pub mutation_strength: f64,
    pub crossover_rate: f64,
    pub selection: SelectionStrategy,
    pub crossover: CrossoverStrategy,
    pub elitism: usize,
    pub seed: Option<u64>,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            population_size: 50,
            generations: 100,
            mutation_rate: 0.1,
            mutation_strength: 0.3,
            crossover_rate: 0.7,
            selection: SelectionStrategy::Tournament(3),
            crossover: CrossoverStrategy::Uniform(0.5),
            elitism: 2,
            seed: None,
        }
    }
}

/// Statistics from evolution run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionStats {
    pub generation: usize,
    pub best_fitness: f64,
    pub avg_fitness: f64,
    pub worst_fitness: f64,
    pub diversity: f64,
    pub fitness_history: Vec<f64>,
}

impl EvolutionStats {
    pub fn new() -> Self {
        Self {
            generation: 0,
            best_fitness: f64::NEG_INFINITY,
            avg_fitness: 0.0,
            worst_fitness: f64::INFINITY,
            diversity: 0.0,
            fitness_history: Vec::new(),
        }
    }
}

impl Default for EvolutionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Population of individuals for evolution
#[derive(Debug, Clone)]
pub struct Population<G: Evolvable> {
    pub individuals: Vec<Individual<G>>,
    pub generation: usize,
    pub config: EvolutionConfig,
    pub stats: EvolutionStats,
    pub next_id: usize,
}

impl<G: Evolvable> Population<G> {
    pub fn new(config: EvolutionConfig) -> Self {
        if let Some(seed) = config.seed {
            seed_rng(seed);
        }

        let mut individuals = Vec::with_capacity(config.population_size);
        for i in 0..config.population_size {
            individuals.push(Individual::new(G::random(), i));
        }

        let next_id = config.population_size;

        Self {
            individuals,
            generation: 0,
            config,
            stats: EvolutionStats::new(),
            next_id,
        }
    }

    /// Initialize with custom genomes
    pub fn with_genomes(genomes: Vec<G>, config: EvolutionConfig) -> Self {
        if let Some(seed) = config.seed {
            seed_rng(seed);
        }

        let individuals = genomes
            .into_iter()
            .enumerate()
            .map(|(i, g)| Individual::new(g, i))
            .collect();

        let next_id = config.population_size;

        Self {
            individuals,
            generation: 0,
            config,
            stats: EvolutionStats::new(),
            next_id,
        }
    }

    /// Evaluate all individuals with a fitness function
    pub fn evaluate<F>(&mut self, mut fitness_fn: F)
    where
        F: FnMut(&G) -> FitnessResult,
    {
        for ind in &mut self.individuals {
            if ind.fitness.is_none() {
                ind.fitness = Some(fitness_fn(&ind.genome));
            }
        }
        self.update_stats();
    }

    /// Perform one generation of evolution
    pub fn evolve<F>(&mut self, mut fitness_fn: F)
    where
        F: FnMut(&G) -> FitnessResult,
    {
        // Evaluate current population
        self.evaluate(&mut fitness_fn);

        // Sort by fitness (descending)
        self.individuals
            .sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));

        // Create new population
        let mut new_population = Vec::with_capacity(self.config.population_size);

        // Elitism: keep best individuals
        for i in 0..self.config.elitism.min(self.individuals.len()) {
            let mut elite = self.individuals[i].clone();
            elite.generation = self.generation + 1;
            new_population.push(elite);
        }

        // Generate rest through selection, crossover, mutation
        while new_population.len() < self.config.population_size {
            let parent1 = self.select();

            let offspring = if rand_f64() < self.config.crossover_rate {
                let parent2 = self.select();
                self.crossover(parent1, parent2)
            } else {
                parent1.genome.clone()
            };

            let mutated =
                offspring.mutate(self.config.mutation_rate, self.config.mutation_strength);

            new_population.push(Individual {
                genome: mutated,
                fitness: None,
                generation: self.generation + 1,
                id: self.next_id,
            });
            self.next_id += 1;
        }

        self.individuals = new_population;
        self.generation += 1;
        self.stats.generation = self.generation;
    }

    fn select(&self) -> &Individual<G> {
        match self.config.selection {
            SelectionStrategy::Tournament(size) => self.tournament_select(size),
            SelectionStrategy::Roulette => self.roulette_select(),
            SelectionStrategy::Rank => self.rank_select(),
            SelectionStrategy::Truncation(ratio) => self.truncation_select(ratio),
            SelectionStrategy::Elite(n) => self.elite_select(n),
        }
    }

    fn tournament_select(&self, size: usize) -> &Individual<G> {
        let mut best: Option<&Individual<G>> = None;
        let mut best_fitness = f64::NEG_INFINITY;

        for _ in 0..size {
            let idx = (rand_f64() * self.individuals.len() as f64) as usize;
            let ind = &self.individuals[idx.min(self.individuals.len() - 1)];
            if ind.score() > best_fitness {
                best_fitness = ind.score();
                best = Some(ind);
            }
        }

        best.unwrap_or(&self.individuals[0])
    }

    fn roulette_select(&self) -> &Individual<G> {
        let total: f64 = self.individuals.iter().map(|i| i.score().max(0.0)).sum();
        if total <= 0.0 {
            return &self.individuals[0];
        }

        let mut spin = rand_f64() * total;
        for ind in &self.individuals {
            spin -= ind.score().max(0.0);
            if spin <= 0.0 {
                return ind;
            }
        }

        &self.individuals[0]
    }

    fn rank_select(&self) -> &Individual<G> {
        let n = self.individuals.len();
        let total_rank: usize = (n * (n + 1)) / 2;
        let mut spin = rand_f64() * total_rank as f64;

        for (i, ind) in self.individuals.iter().enumerate() {
            let rank = n - i; // Higher rank for better fitness
            spin -= rank as f64;
            if spin <= 0.0 {
                return ind;
            }
        }

        &self.individuals[0]
    }

    fn truncation_select(&self, ratio: f64) -> &Individual<G> {
        let cutoff = ((self.individuals.len() as f64) * ratio) as usize;
        let idx = (rand_f64() * cutoff.max(1) as f64) as usize;
        &self.individuals[idx.min(self.individuals.len() - 1)]
    }

    fn elite_select(&self, n: usize) -> &Individual<G> {
        let idx = (rand_f64() * n.min(self.individuals.len()) as f64) as usize;
        &self.individuals[idx.min(self.individuals.len() - 1)]
    }

    fn crossover(&self, parent1: &Individual<G>, parent2: &Individual<G>) -> G {
        match self.config.crossover {
            CrossoverStrategy::SinglePoint
            | CrossoverStrategy::TwoPoint
            | CrossoverStrategy::Uniform(_)
            | CrossoverStrategy::Blend(_)
            | CrossoverStrategy::SBX(_) => parent1.genome.crossover(&parent2.genome),
        }
    }

    fn update_stats(&mut self) {
        if self.individuals.is_empty() {
            return;
        }

        let scores: Vec<f64> = self.individuals.iter().map(|i| i.score()).collect();

        self.stats.best_fitness = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        self.stats.worst_fitness = scores.iter().cloned().fold(f64::INFINITY, f64::min);
        self.stats.avg_fitness = scores.iter().sum::<f64>() / scores.len() as f64;

        // Diversity: standard deviation of fitness
        let variance: f64 = scores
            .iter()
            .map(|s| (s - self.stats.avg_fitness).powi(2))
            .sum::<f64>()
            / scores.len() as f64;
        self.stats.diversity = variance.sqrt();

        self.stats.fitness_history.push(self.stats.best_fitness);
    }

    pub fn best(&self) -> Option<&Individual<G>> {
        self.individuals
            .iter()
            .max_by(|a, b| a.score().partial_cmp(&b.score()).unwrap_or(Ordering::Equal))
    }

    pub fn best_n(&self, n: usize) -> Vec<&Individual<G>> {
        let mut sorted: Vec<_> = self.individuals.iter().collect();
        sorted.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));
        sorted.into_iter().take(n).collect()
    }
}

/// Neuroevolution of Augmenting Topologies (NEAT) - simplified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeatGenome {
    pub nodes: Vec<NeatNode>,
    pub connections: Vec<NeatConnection>,
    pub input_count: usize,
    pub output_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeatNode {
    pub id: usize,
    pub node_type: NeatNodeType,
    pub bias: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NeatNodeType {
    Input,
    Hidden,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeatConnection {
    pub from: usize,
    pub to: usize,
    pub weight: f64,
    pub enabled: bool,
    pub innovation: usize,
}

impl NeatGenome {
    pub fn new(input_count: usize, output_count: usize) -> Self {
        let mut nodes = Vec::new();

        // Input nodes
        for i in 0..input_count {
            nodes.push(NeatNode {
                id: i,
                node_type: NeatNodeType::Input,
                bias: 0.0,
            });
        }

        // Output nodes
        for i in 0..output_count {
            nodes.push(NeatNode {
                id: input_count + i,
                node_type: NeatNodeType::Output,
                bias: rand_f64() * 2.0 - 1.0,
            });
        }

        // Initial connections (all inputs to all outputs)
        let mut connections = Vec::new();
        let mut innovation = 0;
        for i in 0..input_count {
            for o in 0..output_count {
                connections.push(NeatConnection {
                    from: i,
                    to: input_count + o,
                    weight: rand_f64() * 2.0 - 1.0,
                    enabled: true,
                    innovation,
                });
                innovation += 1;
            }
        }

        Self {
            nodes,
            connections,
            input_count,
            output_count,
        }
    }

    pub fn forward(&self, input: &[f64]) -> Vec<f64> {
        let mut node_values: HashMap<usize, f64> = HashMap::new();

        // Set input values
        for (i, &val) in input.iter().enumerate() {
            if i < self.input_count {
                node_values.insert(i, val);
            }
        }

        // Topological sort and compute (simplified)
        for _ in 0..self.nodes.len() {
            for conn in &self.connections {
                if conn.enabled {
                    if let Some(&from_val) = node_values.get(&conn.from) {
                        let to_node = &self.nodes.iter().find(|n| n.id == conn.to);
                        if let Some(node) = to_node {
                            let current = node_values.get(&conn.to).unwrap_or(&node.bias);
                            node_values.insert(conn.to, current + from_val * conn.weight);
                        }
                    }
                }
            }
        }

        // Extract output values with activation
        let mut output = Vec::with_capacity(self.output_count);
        for i in 0..self.output_count {
            let id = self.input_count + i;
            let val = node_values.get(&id).unwrap_or(&0.0);
            output.push(val.tanh());
        }

        output
    }

    pub fn add_node(&mut self, connection_idx: usize, new_innovation: usize) {
        if connection_idx >= self.connections.len() {
            return;
        }

        let conn = &self.connections[connection_idx];
        if !conn.enabled {
            return;
        }

        let from = conn.from;
        let to = conn.to;
        let weight = conn.weight;

        // Disable old connection
        self.connections[connection_idx].enabled = false;

        // Add new hidden node
        let new_id = self.nodes.len();
        self.nodes.push(NeatNode {
            id: new_id,
            node_type: NeatNodeType::Hidden,
            bias: 0.0,
        });

        // Add two new connections
        self.connections.push(NeatConnection {
            from,
            to: new_id,
            weight: 1.0,
            enabled: true,
            innovation: new_innovation,
        });

        self.connections.push(NeatConnection {
            from: new_id,
            to,
            weight,
            enabled: true,
            innovation: new_innovation + 1,
        });
    }

    pub fn add_connection(&mut self, from: usize, to: usize, innovation: usize) {
        // Check if connection already exists
        for conn in &self.connections {
            if conn.from == from && conn.to == to {
                return;
            }
        }

        self.connections.push(NeatConnection {
            from,
            to,
            weight: rand_f64() * 2.0 - 1.0,
            enabled: true,
            innovation,
        });
    }
}

impl Evolvable for NeatGenome {
    fn random() -> Self {
        NeatGenome::new(4, 2)
    }

    fn mutate(&self, rate: f64, strength: f64) -> Self {
        let mut genome = self.clone();

        // Mutate weights
        for conn in &mut genome.connections {
            if rand_f64() < rate {
                conn.weight += (rand_f64() * 2.0 - 1.0) * strength;
            }
        }

        // Mutate biases
        for node in &mut genome.nodes {
            if rand_f64() < rate {
                node.bias += (rand_f64() * 2.0 - 1.0) * strength;
            }
        }

        // Structural mutations (lower probability)
        if rand_f64() < rate * 0.1 && !genome.connections.is_empty() {
            let idx = (rand_f64() * genome.connections.len() as f64) as usize;
            let innovation = genome.connections.len() * 2;
            genome.add_node(idx, innovation);
        }

        genome
    }

    fn crossover(&self, other: &Self) -> Self {
        let mut genome = self.clone();

        // Align by innovation number and take from fitter parent
        for conn in &mut genome.connections {
            if let Some(other_conn) = other
                .connections
                .iter()
                .find(|c| c.innovation == conn.innovation)
            {
                if rand_f64() < 0.5 {
                    conn.weight = other_conn.weight;
                }
            }
        }

        genome
    }

    fn param_count(&self) -> usize {
        self.connections.len() + self.nodes.len()
    }
}

/// Coevolution for multi-agent scenarios
#[derive(Debug)]
pub struct CoevolutionManager<G: Evolvable> {
    pub populations: Vec<Population<G>>,
    pub interaction_matrix: Vec<Vec<f64>>,
}

impl<G: Evolvable> CoevolutionManager<G> {
    pub fn new(num_populations: usize, config: EvolutionConfig) -> Self {
        let mut populations = Vec::with_capacity(num_populations);
        for _ in 0..num_populations {
            populations.push(Population::new(config.clone()));
        }

        let interaction_matrix = vec![vec![0.0; num_populations]; num_populations];

        Self {
            populations,
            interaction_matrix,
        }
    }

    /// Evaluate populations through interaction
    pub fn evaluate_interaction<F>(&mut self, interaction_fn: F)
    where
        F: Fn(&G, &G) -> (f64, f64),
    {
        let num_pops = self.populations.len();

        // Collect interaction scores separately to avoid double mutable borrow
        let mut scores: Vec<Vec<f64>> = self
            .populations
            .iter()
            .map(|p| vec![0.0; p.individuals.len()])
            .collect();

        for i in 0..num_pops {
            for j in (i + 1)..num_pops {
                let len_i = self.populations[i].individuals.len();
                let len_j = self.populations[j].individuals.len();

                for ii in 0..len_i {
                    for jj in 0..len_j {
                        let (score_i, score_j) = interaction_fn(
                            &self.populations[i].individuals[ii].genome,
                            &self.populations[j].individuals[jj].genome,
                        );
                        scores[i][ii] += score_i;
                        scores[j][jj] += score_j;
                    }
                }
            }
        }

        // Apply scores to individuals
        for (pop_idx, pop) in self.populations.iter_mut().enumerate() {
            let n = pop.individuals.len();
            for (ind_idx, ind) in pop.individuals.iter_mut().enumerate() {
                let score = scores[pop_idx][ind_idx] / (num_pops * n).max(1) as f64;
                ind.fitness = Some(FitnessResult::new(score));
            }
        }
    }

    /// Evolve all populations
    pub fn evolve_all(&mut self) {
        for pop in &mut self.populations {
            // Sort and select
            pop.individuals
                .sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));

            let mut new_pop = Vec::with_capacity(pop.config.population_size);

            // Elitism
            for i in 0..pop.config.elitism.min(pop.individuals.len()) {
                let mut elite = pop.individuals[i].clone();
                elite.fitness = None;
                elite.generation = pop.generation + 1;
                new_pop.push(elite);
            }

            // Generate offspring
            while new_pop.len() < pop.config.population_size {
                let parent1 = pop.tournament_select(3);
                let parent2 = pop.tournament_select(3);

                let offspring = if rand_f64() < pop.config.crossover_rate {
                    parent1.genome.crossover(&parent2.genome)
                } else {
                    parent1.genome.clone()
                };

                let mutated =
                    offspring.mutate(pop.config.mutation_rate, pop.config.mutation_strength);

                new_pop.push(Individual {
                    genome: mutated,
                    fitness: None,
                    generation: pop.generation + 1,
                    id: pop.next_id,
                });
                pop.next_id += 1;
            }

            pop.individuals = new_pop;
            pop.generation += 1;
        }
    }
}

/// Evolution for communication protocols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolGenome {
    /// Message encoding network
    pub encoder: NeuralNetwork,
    /// Message decoding network
    pub decoder: NeuralNetwork,
    /// Response generation network
    pub responder: NeuralNetwork,
    /// Message dimension
    pub message_dim: usize,
}

impl ProtocolGenome {
    pub fn new(state_dim: usize, message_dim: usize, action_dim: usize) -> Self {
        use crate::neural::Activation;

        Self {
            encoder: NeuralNetwork::feedforward(
                "encoder",
                &[state_dim, (state_dim + message_dim) / 2, message_dim],
                Activation::ReLU,
                Activation::Tanh,
            ),
            decoder: NeuralNetwork::feedforward(
                "decoder",
                &[message_dim, message_dim * 2, state_dim],
                Activation::ReLU,
                Activation::Tanh,
            ),
            responder: NeuralNetwork::feedforward(
                "responder",
                &[
                    state_dim + message_dim,
                    (state_dim + action_dim) / 2,
                    action_dim,
                ],
                Activation::ReLU,
                Activation::Softmax,
            ),
            message_dim,
        }
    }

    pub fn encode(&self, state: &[f64]) -> Vec<f64> {
        self.encoder.forward(state)
    }

    pub fn decode(&self, message: &[f64]) -> Vec<f64> {
        self.decoder.forward(message)
    }

    pub fn respond(&self, state: &[f64], message: &[f64]) -> Vec<f64> {
        let mut input = state.to_vec();
        input.extend(message);
        self.responder.forward(&input)
    }
}

impl Evolvable for ProtocolGenome {
    fn random() -> Self {
        ProtocolGenome::new(8, 4, 4)
    }

    fn mutate(&self, rate: f64, strength: f64) -> Self {
        Self {
            encoder: self.encoder.mutate(rate, strength),
            decoder: self.decoder.mutate(rate, strength),
            responder: self.responder.mutate(rate, strength),
            message_dim: self.message_dim,
        }
    }

    fn crossover(&self, other: &Self) -> Self {
        Self {
            encoder: self.encoder.crossover(&other.encoder),
            decoder: self.decoder.crossover(&other.decoder),
            responder: self.responder.crossover(&other.responder),
            message_dim: self.message_dim,
        }
    }

    fn param_count(&self) -> usize {
        self.encoder.param_count() + self.decoder.param_count() + self.responder.param_count()
    }
}

/// Consensus genome for evolving distributed consensus
impl Evolvable for ConsensusNetwork {
    fn random() -> Self {
        ConsensusNetwork::new("evolved", 4, 8, 16, 4)
    }

    fn mutate(&self, rate: f64, strength: f64) -> Self {
        let mut new = self.clone();
        let mut params = new.get_params();

        for p in &mut params {
            if rand_f64() < rate {
                *p += (rand_f64() * 2.0 - 1.0) * strength;
            }
        }

        new.set_params(&params);
        new
    }

    fn crossover(&self, other: &Self) -> Self {
        let mut new = self.clone();
        let params1 = self.get_params();
        let params2 = other.get_params();

        let new_params: Vec<f64> = params1
            .iter()
            .zip(params2.iter())
            .map(|(a, b)| if rand_f64() < 0.5 { *a } else { *b })
            .collect();

        new.set_params(&new_params);
        new
    }

    fn param_count(&self) -> usize {
        self.param_count()
    }
}

// Simple pseudo-random number generator. Not cryptographic — it drives mutation
// and selection, never anything security-bearing. See `neural::rand_f64` for why
// this is an atomic rather than a `static mut`.
fn rand_f64() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(54321);
    let next = STATE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| {
            Some(s.wrapping_mul(6364136223846793005).wrapping_add(1))
        })
        .map(|prev| prev.wrapping_mul(6364136223846793005).wrapping_add(1))
        .unwrap_or(0);
    (next >> 33) as f64 / (1u64 << 31) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_population_creation() {
        let config = EvolutionConfig::default();
        let pop: Population<Vec<f64>> = Population::new(config);
        assert_eq!(pop.individuals.len(), 50);
    }

    #[test]
    fn test_evolution_step() {
        let config = EvolutionConfig {
            population_size: 20,
            generations: 5,
            ..Default::default()
        };

        let mut pop: Population<Vec<f64>> = Population::new(config);

        // Simple fitness: sum of elements
        for _ in 0..5 {
            pop.evolve(|genome| FitnessResult::new(genome.iter().sum()));
        }

        assert_eq!(pop.generation, 5);
        assert!(pop.stats.best_fitness > f64::NEG_INFINITY);
    }

    #[test]
    fn test_neat_genome() {
        let genome = NeatGenome::new(3, 2);
        let output = genome.forward(&[1.0, 0.5, 0.0]);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn test_protocol_genome() {
        let protocol = ProtocolGenome::new(4, 3, 2);
        let state = vec![1.0, 0.0, 0.5, 0.5];
        let message = protocol.encode(&state);
        assert_eq!(message.len(), 3);

        let response = protocol.respond(&state, &message);
        assert_eq!(response.len(), 2);
    }

    #[test]
    fn test_coevolution() {
        let config = EvolutionConfig {
            population_size: 10,
            ..Default::default()
        };

        let mut coevo: CoevolutionManager<Vec<f64>> = CoevolutionManager::new(2, config);

        coevo.evaluate_interaction(|a, b| {
            let diff: f64 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum();
            (1.0 / (1.0 + diff), 1.0 / (1.0 + diff))
        });

        coevo.evolve_all();
        assert_eq!(coevo.populations[0].generation, 1);
    }
}
