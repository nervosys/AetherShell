//! Comprehensive A2A (Agent-to-Agent) Communication Tests
//! Tests for message bus, routing, agent communication, and protocols

use aethershell::ai::a2a::{A2AAgent, A2AMessage, A2AMessageBus, A2AMessageType};
use serde_json::json;
use std::sync::Arc;

// ========== Message Bus Tests ==========

#[test]
fn test_message_bus_creation() {
    let bus = A2AMessageBus::new();
    assert_eq!(bus.get_all_messages().unwrap().len(), 0);
}

#[test]
fn test_message_bus_default() {
    let bus = A2AMessageBus::default();
    assert_eq!(bus.registered_agents().unwrap().len(), 0);
}

#[test]
fn test_register_agent() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();
    assert_eq!(bus.registered_agents().unwrap().len(), 1);
    assert!(
        bus.registered_agents()
            .unwrap()
            .contains(&"agent1".to_string())
    );
}

#[test]
fn test_register_multiple_agents() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();
    bus.register_agent("agent2").unwrap();
    bus.register_agent("agent3").unwrap();
    assert_eq!(bus.registered_agents().unwrap().len(), 3);
}

// ========== Direct Messaging Tests ==========

#[test]
fn test_send_direct_message() {
    let bus = A2AMessageBus::new();
    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Hello".to_string(),
        },
    );

    let result = bus.send(msg);
    assert!(result.is_ok());
    assert_eq!(bus.get_all_messages().unwrap().len(), 1);
}

#[test]
fn test_receive_direct_message() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent2").unwrap();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Test message".to_string(),
        },
    );

    bus.send(msg).unwrap();
    let messages = bus.receive("agent2").unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].from, "agent1");
}

#[test]
fn test_receive_consumes_messages() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();

    let msg = A2AMessage::new(
        "agent2".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent1".to_string(),
            content: "Test".to_string(),
        },
    );

    bus.send(msg).unwrap();

    let messages1 = bus.receive("agent1").unwrap();
    assert_eq!(messages1.len(), 1);

    // Second receive should be empty
    let messages2 = bus.receive("agent1").unwrap();
    assert_eq!(messages2.len(), 0);
}

#[test]
fn test_peek_does_not_consume() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();

    let msg = A2AMessage::new(
        "agent2".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent1".to_string(),
            content: "Test".to_string(),
        },
    );

    bus.send(msg).unwrap();

    // Peek should show the message
    let peeked = bus.peek("agent1").unwrap();
    assert_eq!(peeked.len(), 1);

    // Message should still be there
    let received = bus.receive("agent1").unwrap();
    assert_eq!(received.len(), 1);
}

// ========== Broadcast Tests ==========

#[test]
fn test_broadcast_message() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();
    bus.register_agent("agent2").unwrap();
    bus.register_agent("agent3").unwrap();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::Broadcast {
            content: "Broadcast to all".to_string(),
        },
    );

    bus.send(msg).unwrap();

    // All agents should receive the broadcast
    assert_eq!(bus.message_count("agent1").unwrap(), 1);
    assert_eq!(bus.message_count("agent2").unwrap(), 1);
    assert_eq!(bus.message_count("agent3").unwrap(), 1);
}

#[test]
fn test_broadcast_to_empty_bus() {
    let bus = A2AMessageBus::new();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::Broadcast {
            content: "Nobody listening".to_string(),
        },
    );

    // Should not error, just no recipients
    let result = bus.send(msg);
    assert!(result.is_ok());
}

// ========== Delegation Tests ==========

#[test]
fn test_delegate_task() {
    let bus = A2AMessageBus::new();
    bus.register_agent("worker").unwrap();

    let msg = A2AMessage::new(
        "coordinator".to_string(),
        A2AMessageType::Delegate {
            to: "worker".to_string(),
            task: "Process data".to_string(),
            context: json!({"priority": "high"}),
        },
    );

    bus.send(msg).unwrap();

    let messages = bus.receive("worker").unwrap();
    assert_eq!(messages.len(), 1);

    if let A2AMessageType::Delegate { task, context, .. } = &messages[0].msg_type {
        assert_eq!(task, "Process data");
        assert_eq!(context["priority"], "high");
    } else {
        panic!("Expected Delegate message");
    }
}

#[test]
fn test_delegation_response() {
    let bus = A2AMessageBus::new();

    let msg = A2AMessage::new(
        "worker".to_string(),
        A2AMessageType::DelegateResponse {
            from: "worker".to_string(),
            result: "Task completed".to_string(),
            success: true,
        },
    );

    bus.send(msg).unwrap();
    assert_eq!(bus.get_all_messages().unwrap().len(), 1);
}

// ========== Capability Query Tests ==========

#[test]
fn test_query_capabilities() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent2").unwrap();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::QueryCapabilities {
            to: "agent2".to_string(),
        },
    );

    bus.send(msg).unwrap();
    let messages = bus.receive("agent2").unwrap();
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_capabilities_response() {
    let bus = A2AMessageBus::new();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::CapabilitiesResponse {
            from: "agent1".to_string(),
            capabilities: vec!["compute".to_string(), "storage".to_string()],
        },
    );

    bus.send(msg).unwrap();

    let messages = bus.get_all_messages().unwrap();
    if let A2AMessageType::CapabilitiesResponse { capabilities, .. } = &messages[0].msg_type {
        assert_eq!(capabilities.len(), 2);
    }
}

// ========== Agent Tests ==========

#[test]
fn test_a2a_agent_creation() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent = A2AAgent::new(
        "agent1".to_string(),
        vec!["capability1".to_string()],
        Arc::clone(&bus),
    )
    .unwrap();

    assert_eq!(agent.id, "agent1");
    assert_eq!(agent.capabilities.len(), 1);
}

#[test]
fn test_agent_send_message() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent1 = A2AAgent::new("agent1".to_string(), vec![], Arc::clone(&bus)).unwrap();

    bus.register_agent("agent2").unwrap();

    let result = agent1.send_message("agent2", "Hello agent2".to_string());
    assert!(result.is_ok());

    assert_eq!(bus.message_count("agent2").unwrap(), 1);
}

#[test]
fn test_agent_broadcast() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent1 = A2AAgent::new("agent1".to_string(), vec![], Arc::clone(&bus)).unwrap();

    bus.register_agent("agent2").unwrap();
    bus.register_agent("agent3").unwrap();

    agent1
        .broadcast("Important announcement".to_string())
        .unwrap();

    assert!(bus.message_count("agent2").unwrap() > 0);
    assert!(bus.message_count("agent3").unwrap() > 0);
}

#[test]
fn test_agent_delegate_task() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent1 = A2AAgent::new("agent1".to_string(), vec![], Arc::clone(&bus)).unwrap();

    bus.register_agent("agent2").unwrap();

    agent1
        .delegate_task("agent2", "Task description".to_string(), json!({}))
        .unwrap();

    assert_eq!(bus.message_count("agent2").unwrap(), 1);
}

#[test]
fn test_agent_query_capabilities() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent1 = A2AAgent::new("agent1".to_string(), vec![], Arc::clone(&bus)).unwrap();

    bus.register_agent("agent2").unwrap();

    agent1.query_capabilities("agent2").unwrap();

    assert_eq!(bus.message_count("agent2").unwrap(), 1);
}

#[test]
fn test_agent_respond_capabilities() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent = A2AAgent::new(
        "agent1".to_string(),
        vec!["compute".to_string(), "storage".to_string()],
        Arc::clone(&bus),
    )
    .unwrap();

    agent.respond_capabilities().unwrap();

    let messages = bus.get_all_messages().unwrap();
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_agent_receive_messages() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent = A2AAgent::new("receiver".to_string(), vec![], Arc::clone(&bus)).unwrap();

    let msg = A2AMessage::new(
        "sender".to_string(),
        A2AMessageType::DirectMessage {
            to: "receiver".to_string(),
            content: "Test".to_string(),
        },
    );

    bus.send(msg).unwrap();

    let messages = agent.receive_messages().unwrap();
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_agent_pending_count() {
    let bus = Arc::new(A2AMessageBus::new());
    let agent = A2AAgent::new("agent1".to_string(), vec![], Arc::clone(&bus)).unwrap();

    bus.send(A2AMessage::new(
        "sender".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent1".to_string(),
            content: "Message 1".to_string(),
        },
    ))
    .unwrap();

    bus.send(A2AMessage::new(
        "sender".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent1".to_string(),
            content: "Message 2".to_string(),
        },
    ))
    .unwrap();

    assert_eq!(agent.pending_message_count().unwrap(), 2);
}

// ========== Message Routing Tests ==========

#[test]
fn test_routing_to_correct_recipient() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();
    bus.register_agent("agent2").unwrap();
    bus.register_agent("agent3").unwrap();

    let msg = A2AMessage::new(
        "sender".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "For agent2 only".to_string(),
        },
    );

    bus.send(msg).unwrap();

    assert_eq!(bus.message_count("agent1").unwrap(), 0);
    assert_eq!(bus.message_count("agent2").unwrap(), 1);
    assert_eq!(bus.message_count("agent3").unwrap(), 0);
}

#[test]
fn test_multiple_messages_to_same_agent() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();

    for i in 0..5 {
        let msg = A2AMessage::new(
            "sender".to_string(),
            A2AMessageType::DirectMessage {
                to: "agent1".to_string(),
                content: format!("Message {}", i),
            },
        );
        bus.send(msg).unwrap();
    }

    assert_eq!(bus.message_count("agent1").unwrap(), 5);
}

// ========== Clear and Reset Tests ==========

#[test]
fn test_clear_messages() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent1").unwrap();

    let msg = A2AMessage::new(
        "sender".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent1".to_string(),
            content: "Test".to_string(),
        },
    );

    bus.send(msg).unwrap();
    assert!(bus.get_all_messages().unwrap().len() > 0);

    bus.clear().unwrap();
    assert_eq!(bus.get_all_messages().unwrap().len(), 0);
    assert_eq!(bus.message_count("agent1").unwrap(), 0);
}

// ========== Message Metadata Tests ==========

#[test]
fn test_message_has_id() {
    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Test".to_string(),
        },
    );

    assert!(!msg.id.is_nil());
}

#[test]
fn test_message_has_timestamp() {
    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Test".to_string(),
        },
    );

    let now = chrono::Utc::now();
    let diff = (now - msg.timestamp).num_seconds().abs();
    assert!(diff < 2); // Within 2 seconds
}

#[test]
fn test_message_recipients_helper() {
    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Test".to_string(),
        },
    );

    let recipients = msg.recipients();
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0], "agent2");
}

#[test]
fn test_is_response_helper() {
    let response_msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DelegateResponse {
            from: "agent1".to_string(),
            result: "Done".to_string(),
            success: true,
        },
    );

    assert!(response_msg.is_response());

    let request_msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Test".to_string(),
        },
    );

    assert!(!request_msg.is_response());
}

// ========== Concurrent Access Tests ==========

#[test]
fn test_concurrent_sends() {
    use std::thread;

    let bus = Arc::new(A2AMessageBus::new());
    bus.register_agent("receiver").unwrap();

    let mut handles = vec![];

    for i in 0..5 {
        let bus = Arc::clone(&bus);
        let handle = thread::spawn(move || {
            let msg = A2AMessage::new(
                format!("sender{}", i),
                A2AMessageType::DirectMessage {
                    to: "receiver".to_string(),
                    content: format!("Message from thread {}", i),
                },
            );
            bus.send(msg).unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(bus.message_count("receiver").unwrap(), 5);
}

// ========== Edge Cases ==========

#[test]
fn test_empty_message_content() {
    let bus = A2AMessageBus::new();
    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "".to_string(),
        },
    );

    let result = bus.send(msg);
    assert!(result.is_ok());
}

#[test]
fn test_long_message_content() {
    let bus = A2AMessageBus::new();
    let long_content = "a".repeat(10000);

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: long_content.clone(),
        },
    );

    bus.send(msg).unwrap();

    let messages = bus.get_all_messages().unwrap();
    if let A2AMessageType::DirectMessage { content, .. } = &messages[0].msg_type {
        assert_eq!(content.len(), 10000);
    }
}

#[test]
fn test_unicode_in_messages() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent2").unwrap();

    let msg = A2AMessage::new(
        "agent1".to_string(),
        A2AMessageType::DirectMessage {
            to: "agent2".to_string(),
            content: "Hello 世界 🌍".to_string(),
        },
    );

    bus.send(msg).unwrap();
    let messages = bus.receive("agent2").unwrap();

    if let A2AMessageType::DirectMessage { content, .. } = &messages[0].msg_type {
        assert!(content.contains("世界"));
        assert!(content.contains("🌍"));
    }
}

#[test]
fn test_special_characters_in_agent_ids() {
    let bus = A2AMessageBus::new();
    bus.register_agent("agent-with-dash").unwrap();
    bus.register_agent("agent_with_underscore").unwrap();
    bus.register_agent("agent.with.dot").unwrap();

    assert_eq!(bus.registered_agents().unwrap().len(), 3);
}
