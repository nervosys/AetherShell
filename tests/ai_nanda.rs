//! Comprehensive NANDA (Negotiation And Dynamic Agents) Tests
//! Tests for proposals, voting, consensus, task allocation, and coordination

use aethershell::ai::nanda::{
    NandaCoordinator, NandaProposal, NandaTaskAllocator, NandaVote, NegotiationStatus, Task,
};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

// ========== Proposal Tests ==========

#[test]
fn test_task_allocation_proposal() {
    let task_id = Uuid::new_v4();
    let proposal = NandaProposal::TaskAllocation {
        task_id,
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Best fit".to_string(),
    };

    match proposal {
        NandaProposal::TaskAllocation {
            agent_id, priority, ..
        } => {
            assert_eq!(agent_id, "agent1");
            assert_eq!(priority, 1.0);
        }
        _ => panic!("Wrong proposal type"),
    }
}

#[test]
fn test_resource_allocation_proposal() {
    let proposal = NandaProposal::ResourceAllocation {
        resource: "CPU".to_string(),
        agent_id: "agent1".to_string(),
        amount: 50.0,
    };

    match proposal {
        NandaProposal::ResourceAllocation {
            resource, amount, ..
        } => {
            assert_eq!(resource, "CPU");
            assert_eq!(amount, 50.0);
        }
        _ => panic!("Wrong proposal type"),
    }
}

#[test]
fn test_coordination_strategy_proposal() {
    let mut params = HashMap::new();
    params.insert("timeout".to_string(), json!(30));

    let proposal = NandaProposal::CoordinationStrategy {
        strategy: "RoundRobin".to_string(),
        parameters: params.clone(),
    };

    match proposal {
        NandaProposal::CoordinationStrategy {
            strategy,
            parameters,
        } => {
            assert_eq!(strategy, "RoundRobin");
            assert_eq!(parameters["timeout"], 30);
        }
        _ => panic!("Wrong proposal type"),
    }
}

#[test]
fn test_consensus_threshold_proposal() {
    let proposal = NandaProposal::ConsensusThreshold {
        threshold: 0.66,
        quorum: 5,
    };

    match proposal {
        NandaProposal::ConsensusThreshold { threshold, quorum } => {
            assert_eq!(threshold, 0.66);
            assert_eq!(quorum, 5);
        }
        _ => panic!("Wrong proposal type"),
    }
}

// ========== Vote Tests ==========

#[test]
fn test_accept_vote() {
    let vote = NandaVote::Accept;
    assert!(matches!(vote, NandaVote::Accept));
}

#[test]
fn test_reject_vote() {
    let vote = NandaVote::Reject {
        reason: "Insufficient resources".to_string(),
    };

    match vote {
        NandaVote::Reject { reason } => {
            assert_eq!(reason, "Insufficient resources");
        }
        _ => panic!("Wrong vote type"),
    }
}

#[test]
fn test_abstain_vote() {
    let vote = NandaVote::Abstain;
    assert!(matches!(vote, NandaVote::Abstain));
}

#[test]
fn test_counter_proposal_vote() {
    let counter = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent2".to_string(),
        priority: 2.0,
        rationale: "Better alternative".to_string(),
    };

    let vote = NandaVote::CounterProposal {
        proposal: Box::new(counter),
    };

    assert!(matches!(vote, NandaVote::CounterProposal { .. }));
}

// ========== Coordinator Tests ==========

#[test]
fn test_coordinator_creation() {
    let agents = vec!["agent1".to_string(), "agent2".to_string()];
    let coordinator = NandaCoordinator::new(agents.clone(), 0.66, 2);

    assert_eq!(coordinator.agents().len(), 2);
}

#[test]
fn test_propose_negotiation() {
    let agents = vec!["agent1".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.5, 1);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("agent1".to_string(), proposal);
    assert!(!neg_id.is_nil());
}

#[test]
fn test_propose_with_deadline() {
    let agents = vec!["agent1".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.5, 1);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let duration = chrono::Duration::seconds(60);
    let neg_id = coordinator.propose_with_deadline("agent1".to_string(), proposal, duration);

    let neg = coordinator.get_negotiation(neg_id).unwrap();
    assert!(neg.deadline.is_some());
}

#[test]
fn test_vote_on_negotiation() {
    let agents = vec!["agent1".to_string(), "agent2".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.5, 2);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("agent1".to_string(), proposal);

    let result = coordinator.vote(neg_id, "agent2".to_string(), NandaVote::Accept);
    assert!(result.is_ok());
}

#[test]
fn test_consensus_reached() {
    let agents = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.66, 3);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // 3 accepts should reach consensus (100% >= 66%)
    coordinator
        .vote(neg_id, "a1".to_string(), NandaVote::Accept)
        .unwrap();
    coordinator
        .vote(neg_id, "a2".to_string(), NandaVote::Accept)
        .unwrap();
    coordinator
        .vote(neg_id, "a3".to_string(), NandaVote::Accept)
        .unwrap();

    let status = coordinator.get_status(neg_id).unwrap();
    assert_eq!(status, NegotiationStatus::Accepted);
}

#[test]
fn test_consensus_rejected() {
    let agents = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.66, 3);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // All reject
    coordinator
        .vote(
            neg_id,
            "a1".to_string(),
            NandaVote::Reject {
                reason: "No".to_string(),
            },
        )
        .unwrap();
    coordinator
        .vote(
            neg_id,
            "a2".to_string(),
            NandaVote::Reject {
                reason: "No".to_string(),
            },
        )
        .unwrap();
    coordinator
        .vote(
            neg_id,
            "a3".to_string(),
            NandaVote::Reject {
                reason: "No".to_string(),
            },
        )
        .unwrap();

    let status = coordinator.get_status(neg_id).unwrap();
    assert_eq!(status, NegotiationStatus::Rejected);
}

#[test]
fn test_get_active_negotiations() {
    let agents = vec!["agent1".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.5, 1);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    coordinator.propose("agent1".to_string(), proposal);

    let active = coordinator.get_active_negotiations();
    assert_eq!(active.len(), 1);
}

#[test]
fn test_clear_completed_negotiations() {
    let agents = vec!["a1".to_string(), "a2".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.5, 2);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // Complete the negotiation
    coordinator
        .vote(neg_id, "a1".to_string(), NandaVote::Accept)
        .unwrap();
    coordinator
        .vote(neg_id, "a2".to_string(), NandaVote::Accept)
        .unwrap();

    coordinator.clear_completed();

    // Completed negotiation should be cleared
    assert_eq!(coordinator.get_all_negotiations().len(), 0);
}

#[test]
fn test_add_remove_agent() {
    let mut coordinator = NandaCoordinator::new(vec![], 0.5, 1);

    coordinator.add_agent("agent1".to_string());
    assert_eq!(coordinator.agents().len(), 1);

    coordinator.remove_agent("agent1");
    assert_eq!(coordinator.agents().len(), 0);
}

// ========== Negotiation Status Tests ==========

#[test]
fn test_negotiation_vote_counting() {
    use aethershell::ai::nanda::NandaNegotiation;

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let mut neg = NandaNegotiation::new("proposer".to_string(), proposal);

    neg.votes.insert("v1".to_string(), NandaVote::Accept);
    neg.votes.insert("v2".to_string(), NandaVote::Accept);
    neg.votes.insert(
        "v3".to_string(),
        NandaVote::Reject {
            reason: "No".to_string(),
        },
    );
    neg.votes.insert("v4".to_string(), NandaVote::Abstain);

    assert_eq!(neg.vote_count(), 4);
    assert_eq!(neg.accept_count(), 2);
    assert_eq!(neg.reject_count(), 1);
    assert_eq!(neg.abstain_count(), 1);
}

#[test]
fn test_negotiation_counter_proposals() {
    use aethershell::ai::nanda::NandaNegotiation;

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let mut neg = NandaNegotiation::new("proposer".to_string(), proposal);

    let counter = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent2".to_string(),
        priority: 2.0,
        rationale: "Better".to_string(),
    };

    neg.votes.insert(
        "v1".to_string(),
        NandaVote::CounterProposal {
            proposal: Box::new(counter),
        },
    );

    let counters = neg.counter_proposals();
    assert_eq!(counters.len(), 1);
}

#[test]
fn test_negotiation_expiration() {
    use aethershell::ai::nanda::NandaNegotiation;

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg = NandaNegotiation::new("proposer".to_string(), proposal)
        .with_deadline(chrono::Duration::milliseconds(-1)); // Already expired

    assert!(neg.is_expired());
}

// ========== Task Tests ==========

#[test]
fn test_task_creation() {
    let task = Task::new("Test task".to_string(), vec!["capability1".to_string()]);

    assert!(!task.id.is_nil());
    assert_eq!(task.description, "Test task");
    assert_eq!(task.priority, 1.0);
}

#[test]
fn test_task_with_priority() {
    let task = Task::new("Task".to_string(), vec![]).with_priority(5.0);

    assert_eq!(task.priority, 5.0);
}

#[test]
fn test_task_with_effort() {
    let task = Task::new("Task".to_string(), vec![]).with_effort(3.5);

    assert_eq!(task.estimated_effort, 3.5);
}

#[test]
fn test_task_builder_pattern() {
    let task = Task::new("Complex task".to_string(), vec!["compute".to_string()])
        .with_priority(10.0)
        .with_effort(5.0);

    assert_eq!(task.priority, 10.0);
    assert_eq!(task.estimated_effort, 5.0);
    assert_eq!(task.required_capabilities.len(), 1);
}

// ========== Task Allocator Tests ==========

#[test]
fn test_task_allocator_creation() {
    let coordinator = NandaCoordinator::new(vec![], 0.5, 1);
    let allocator = NandaTaskAllocator::new(coordinator);

    assert_eq!(allocator.get_all_allocations().len(), 0);
}

#[test]
fn test_add_task_to_queue() {
    let coordinator = NandaCoordinator::new(vec![], 0.5, 1);
    let mut allocator = NandaTaskAllocator::new(coordinator);

    let task = Task::new("Task 1".to_string(), vec!["compute".to_string()]);
    allocator.add_task(task);

    // Task should be in queue (we can't directly check, but allocation should work)
}

#[test]
fn test_allocate_tasks_with_capabilities() {
    let agents = vec!["agent1".to_string(), "agent2".to_string()];
    let coordinator = NandaCoordinator::new(agents, 0.5, 1);
    let mut allocator = NandaTaskAllocator::new(coordinator);

    let task = Task::new("Task".to_string(), vec!["compute".to_string()]);
    allocator.add_task(task);

    let mut capabilities = HashMap::new();
    capabilities.insert("agent1".to_string(), vec!["compute".to_string()]);
    capabilities.insert("agent2".to_string(), vec!["storage".to_string()]);

    let result = allocator.allocate_tasks(capabilities);
    assert!(result.is_ok());

    let negotiations = result.unwrap();
    // Should have proposed negotiations for capable agents
    assert!(!negotiations.is_empty());
}

#[test]
fn test_finalize_allocation() {
    let agents = vec!["agent1".to_string()];
    let coordinator = NandaCoordinator::new(agents, 0.5, 1);
    let mut allocator = NandaTaskAllocator::new(coordinator);

    let task_id = Uuid::new_v4();
    let proposal = NandaProposal::TaskAllocation {
        task_id,
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = allocator
        .coordinator_mut()
        .propose("agent1".to_string(), proposal);

    // Vote to accept
    allocator
        .coordinator_mut()
        .vote(neg_id, "agent1".to_string(), NandaVote::Accept)
        .unwrap();

    // Finalize
    let result = allocator.finalize_allocation(neg_id);
    assert!(result.is_ok());
}

#[test]
fn test_get_allocation() {
    let task_id = Uuid::new_v4();
    let agents = vec!["agent1".to_string()];
    let coordinator = NandaCoordinator::new(agents, 0.5, 1);
    let mut allocator = NandaTaskAllocator::new(coordinator);

    let proposal = NandaProposal::TaskAllocation {
        task_id,
        agent_id: "agent1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = allocator
        .coordinator_mut()
        .propose("agent1".to_string(), proposal);
    allocator
        .coordinator_mut()
        .vote(neg_id, "agent1".to_string(), NandaVote::Accept)
        .unwrap();
    allocator.finalize_allocation(neg_id).unwrap();

    let allocation = allocator.get_allocation(task_id);
    assert!(allocation.is_some());
    assert_eq!(allocation.unwrap(), "agent1");
}

// ========== Quorum Tests ==========

#[test]
fn test_quorum_not_met() {
    let agents = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.66, 3); // Need 3 votes

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // Only 2 votes - quorum not met
    coordinator
        .vote(neg_id, "a1".to_string(), NandaVote::Accept)
        .unwrap();
    coordinator
        .vote(neg_id, "a2".to_string(), NandaVote::Accept)
        .unwrap();

    // Should still be in Voting status
    let status = coordinator.get_status(neg_id).unwrap();
    assert!(matches!(
        status,
        NegotiationStatus::Voting | NegotiationStatus::Open
    ));
}

// ========== Edge Cases ==========

#[test]
fn test_vote_on_nonexistent_negotiation() {
    let mut coordinator = NandaCoordinator::new(vec![], 0.5, 1);

    let fake_id = Uuid::new_v4();
    let result = coordinator.vote(fake_id, "agent1".to_string(), NandaVote::Accept);

    assert!(result.is_err());
}

#[test]
fn test_empty_agent_list() {
    let coordinator = NandaCoordinator::new(vec![], 0.5, 0);
    assert_eq!(coordinator.agents().len(), 0);
}

#[test]
fn test_high_consensus_threshold() {
    let agents = vec!["a1".to_string(), "a2".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.99, 2); // 99% required

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Test".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // Both accept - should reach 99%
    coordinator
        .vote(neg_id, "a1".to_string(), NandaVote::Accept)
        .unwrap();
    coordinator
        .vote(neg_id, "a2".to_string(), NandaVote::Accept)
        .unwrap();

    let status = coordinator.get_status(neg_id).unwrap();
    assert_eq!(status, NegotiationStatus::Accepted);
}

#[test]
fn test_multiple_counter_proposals() {
    let agents = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
    let mut coordinator = NandaCoordinator::new(agents, 0.66, 3);

    let proposal = NandaProposal::TaskAllocation {
        task_id: Uuid::new_v4(),
        agent_id: "a1".to_string(),
        priority: 1.0,
        rationale: "Original".to_string(),
    };

    let neg_id = coordinator.propose("a1".to_string(), proposal);

    // Multiple agents counter-propose
    for i in 1..=3 {
        let counter = NandaProposal::TaskAllocation {
            task_id: Uuid::new_v4(),
            agent_id: format!("a{}", i),
            priority: i as f32,
            rationale: format!("Counter {}", i),
        };

        coordinator
            .vote(
                neg_id,
                format!("a{}", i),
                NandaVote::CounterProposal {
                    proposal: Box::new(counter),
                },
            )
            .unwrap();
    }

    let status = coordinator.get_status(neg_id).unwrap();
    assert_eq!(status, NegotiationStatus::Modified);
}
