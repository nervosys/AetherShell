//! NANDA: Negotiation And Dynamic Agent framework
//! Framework for dynamic agent negotiation, task allocation, and consensus

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// NANDA Negotiation Proposals
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NandaProposal {
    /// Propose task allocation
    TaskAllocation {
        task_id: Uuid,
        agent_id: String,
        priority: f32,
        rationale: String,
    },

    /// Propose resource allocation
    ResourceAllocation {
        resource: String,
        agent_id: String,
        amount: f32,
    },

    /// Propose coordination strategy
    CoordinationStrategy {
        strategy: String,
        parameters: HashMap<String, serde_json::Value>,
    },

    /// Propose consensus threshold
    ConsensusThreshold { threshold: f32, quorum: usize },
}

/// Voting options for proposals
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NandaVote {
    Accept,
    Reject { reason: String },
    Abstain,
    CounterProposal { proposal: Box<NandaProposal> },
}

/// Negotiation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NegotiationStatus {
    Open,
    Voting,
    Accepted,
    Rejected,
    Modified,
    Expired,
}

/// A negotiation with voting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NandaNegotiation {
    pub id: Uuid,
    pub proposal: NandaProposal,
    pub proposer: String,
    pub votes: HashMap<String, NandaVote>,
    pub status: NegotiationStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
}

impl NandaNegotiation {
    pub fn new(proposer: String, proposal: NandaProposal) -> Self {
        Self {
            id: Uuid::new_v4(),
            proposal,
            proposer,
            votes: HashMap::new(),
            status: NegotiationStatus::Open,
            timestamp: chrono::Utc::now(),
            deadline: None,
        }
    }

    pub fn with_deadline(mut self, duration: chrono::Duration) -> Self {
        self.deadline = Some(chrono::Utc::now() + duration);
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(deadline) = self.deadline {
            chrono::Utc::now() > deadline
        } else {
            false
        }
    }

    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }

    pub fn accept_count(&self) -> usize {
        self.votes
            .values()
            .filter(|v| matches!(v, NandaVote::Accept))
            .count()
    }

    pub fn reject_count(&self) -> usize {
        self.votes
            .values()
            .filter(|v| matches!(v, NandaVote::Reject { .. }))
            .count()
    }

    pub fn abstain_count(&self) -> usize {
        self.votes
            .values()
            .filter(|v| matches!(v, NandaVote::Abstain))
            .count()
    }

    pub fn counter_proposals(&self) -> Vec<NandaProposal> {
        self.votes
            .values()
            .filter_map(|v| {
                if let NandaVote::CounterProposal { proposal } = v {
                    Some((**proposal).clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

/// NANDA Coordinator for negotiations
pub struct NandaCoordinator {
    agents: Vec<String>,
    negotiations: Vec<NandaNegotiation>,
    consensus_threshold: f32,
    quorum: usize,
}

impl NandaCoordinator {
    pub fn new(agents: Vec<String>, consensus_threshold: f32, quorum: usize) -> Self {
        Self {
            agents,
            negotiations: Vec::new(),
            consensus_threshold,
            quorum,
        }
    }

    /// Propose new negotiation
    pub fn propose(&mut self, proposer: String, proposal: NandaProposal) -> Uuid {
        let negotiation = NandaNegotiation::new(proposer, proposal);
        let id = negotiation.id;
        self.negotiations.push(negotiation);
        id
    }

    /// Propose with deadline
    pub fn propose_with_deadline(
        &mut self,
        proposer: String,
        proposal: NandaProposal,
        duration: chrono::Duration,
    ) -> Uuid {
        let negotiation = NandaNegotiation::new(proposer, proposal).with_deadline(duration);
        let id = negotiation.id;
        self.negotiations.push(negotiation);
        id
    }

    /// Submit vote for negotiation
    pub fn vote(&mut self, negotiation_id: Uuid, agent_id: String, vote: NandaVote) -> Result<()> {
        if let Some(neg) = self
            .negotiations
            .iter_mut()
            .find(|n| n.id == negotiation_id)
        {
            // Check if expired
            if neg.is_expired() {
                neg.status = NegotiationStatus::Expired;
                return Err(anyhow::anyhow!(
                    "Negotiation {} has expired",
                    negotiation_id
                ));
            }

            // Record vote
            neg.votes.insert(agent_id, vote);
            neg.status = NegotiationStatus::Voting;

            // Check if we have quorum and can decide
            if neg.vote_count() >= self.quorum {
                self.evaluate_negotiation(negotiation_id)?;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Negotiation {} not found", negotiation_id))
        }
    }

    /// Evaluate negotiation status based on votes
    fn evaluate_negotiation(&mut self, id: Uuid) -> Result<()> {
        if let Some(neg) = self.negotiations.iter_mut().find(|n| n.id == id) {
            let total_votes = neg.vote_count() as f32;
            let accept_votes = neg.accept_count() as f32;
            let reject_votes = neg.reject_count() as f32;

            let accept_ratio = accept_votes / total_votes;
            let reject_ratio = reject_votes / total_votes;

            if accept_ratio >= self.consensus_threshold {
                neg.status = NegotiationStatus::Accepted;
            } else if reject_ratio > (1.0 - self.consensus_threshold) {
                neg.status = NegotiationStatus::Rejected;
            } else if !neg.counter_proposals().is_empty() {
                neg.status = NegotiationStatus::Modified;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Negotiation {} not found", id))
        }
    }

    /// Get negotiation status
    pub fn get_status(&self, id: Uuid) -> Option<NegotiationStatus> {
        self.negotiations
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.status.clone())
    }

    /// Get negotiation by ID
    pub fn get_negotiation(&self, id: Uuid) -> Option<&NandaNegotiation> {
        self.negotiations.iter().find(|n| n.id == id)
    }

    /// Get all active negotiations
    pub fn get_active_negotiations(&self) -> Vec<&NandaNegotiation> {
        self.negotiations
            .iter()
            .filter(|n| {
                !n.is_expired()
                    && matches!(
                        n.status,
                        NegotiationStatus::Open | NegotiationStatus::Voting
                    )
            })
            .collect()
    }

    /// Get all negotiations
    pub fn get_all_negotiations(&self) -> &[NandaNegotiation] {
        &self.negotiations
    }

    /// Resolve conflicts between proposals
    pub fn resolve_conflicts(&self, proposals: Vec<NandaProposal>) -> Result<NandaProposal> {
        proposals
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No proposals to resolve"))
    }

    /// Clear completed negotiations
    pub fn clear_completed(&mut self) {
        self.negotiations.retain(|n| {
            !matches!(
                n.status,
                NegotiationStatus::Accepted
                    | NegotiationStatus::Rejected
                    | NegotiationStatus::Expired
            )
        });
    }

    /// Get agent list
    pub fn agents(&self) -> &[String] {
        &self.agents
    }

    /// Add agent
    pub fn add_agent(&mut self, agent_id: String) {
        if !self.agents.contains(&agent_id) {
            self.agents.push(agent_id);
        }
    }

    /// Remove agent
    pub fn remove_agent(&mut self, agent_id: &str) {
        self.agents.retain(|a| a != agent_id);
    }
}

/// Task for allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub priority: f32,
    pub required_capabilities: Vec<String>,
    pub estimated_effort: f32,
}

impl Task {
    pub fn new(description: String, required_capabilities: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            description,
            priority: 1.0,
            required_capabilities,
            estimated_effort: 1.0,
        }
    }

    pub fn with_priority(mut self, priority: f32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_effort(mut self, effort: f32) -> Self {
        self.estimated_effort = effort;
        self
    }
}

/// Task Allocator using NANDA
pub struct NandaTaskAllocator {
    coordinator: NandaCoordinator,
    task_queue: Vec<Task>,
    allocations: HashMap<Uuid, String>, // task_id -> agent_id
}

impl NandaTaskAllocator {
    pub fn new(coordinator: NandaCoordinator) -> Self {
        Self {
            coordinator,
            task_queue: Vec::new(),
            allocations: HashMap::new(),
        }
    }

    /// Add task to queue
    pub fn add_task(&mut self, task: Task) {
        self.task_queue.push(task);
    }

    /// Allocate tasks via negotiation
    pub fn allocate_tasks(
        &mut self,
        agent_capabilities: HashMap<String, Vec<String>>,
    ) -> Result<Vec<Uuid>> {
        let mut proposed_negotiations = Vec::new();

        for task in &self.task_queue {
            // Find capable agents
            let capable_agents: Vec<String> = agent_capabilities
                .iter()
                .filter(|(_, caps)| {
                    task.required_capabilities
                        .iter()
                        .all(|req| caps.contains(req))
                })
                .map(|(id, _)| id.clone())
                .collect();

            if capable_agents.is_empty() {
                continue;
            }

            // Each capable agent proposes themselves
            for agent in capable_agents {
                let proposal = NandaProposal::TaskAllocation {
                    task_id: task.id,
                    agent_id: agent.clone(),
                    priority: task.priority,
                    rationale: format!("Agent {} has required capabilities", agent),
                };
                let neg_id = self.coordinator.propose(agent.clone(), proposal);
                proposed_negotiations.push(neg_id);
            }
        }

        Ok(proposed_negotiations)
    }

    /// Finalize allocation after voting
    pub fn finalize_allocation(&mut self, negotiation_id: Uuid) -> Result<()> {
        if let Some(neg) = self.coordinator.get_negotiation(negotiation_id) {
            if neg.status == NegotiationStatus::Accepted {
                if let NandaProposal::TaskAllocation {
                    task_id, agent_id, ..
                } = &neg.proposal
                {
                    self.allocations.insert(*task_id, agent_id.clone());
                }
            }
        }
        Ok(())
    }

    /// Get task allocation
    pub fn get_allocation(&self, task_id: Uuid) -> Option<&String> {
        self.allocations.get(&task_id)
    }

    /// Get all allocations
    pub fn get_all_allocations(&self) -> &HashMap<Uuid, String> {
        &self.allocations
    }

    /// Get coordinator
    pub fn coordinator(&self) -> &NandaCoordinator {
        &self.coordinator
    }

    /// Get mutable coordinator
    pub fn coordinator_mut(&mut self) -> &mut NandaCoordinator {
        &mut self.coordinator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiation_creation() {
        let proposal = NandaProposal::TaskAllocation {
            task_id: Uuid::new_v4(),
            agent_id: "agent1".to_string(),
            priority: 1.0,
            rationale: "Test".to_string(),
        };
        let neg = NandaNegotiation::new("agent1".to_string(), proposal);
        assert_eq!(neg.proposer, "agent1");
        assert_eq!(neg.status, NegotiationStatus::Open);
    }

    #[test]
    fn test_vote_counting() {
        let proposal = NandaProposal::TaskAllocation {
            task_id: Uuid::new_v4(),
            agent_id: "agent1".to_string(),
            priority: 1.0,
            rationale: "Test".to_string(),
        };
        let mut neg = NandaNegotiation::new("agent1".to_string(), proposal);

        neg.votes.insert("voter1".to_string(), NandaVote::Accept);
        neg.votes.insert(
            "voter2".to_string(),
            NandaVote::Reject {
                reason: "No".to_string(),
            },
        );

        assert_eq!(neg.accept_count(), 1);
        assert_eq!(neg.reject_count(), 1);
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new("Test task".to_string(), vec!["capability1".to_string()])
            .with_priority(2.0)
            .with_effort(3.0);

        assert_eq!(task.priority, 2.0);
        assert_eq!(task.estimated_effort, 3.0);
    }
}
