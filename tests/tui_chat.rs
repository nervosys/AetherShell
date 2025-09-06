//! Tests for TUI chat functionality

#[cfg(test)]
mod tui_chat_tests {
    use aether_shell::tui::agents::MultiModalAgent;
    use aether_shell::tui::app::{AgentInfo, AgentStatus, ChatMessage, MessageRole};
    use aether_shell::tui::chat::{
        ChatManager, ChatSession, ChatSettings, Participant, ParticipantType,
    };
    use aether_shell::tui::media::{MediaFile, MediaType};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_chat_session_creation() {
        let session = ChatSession::new("Test Session".to_string());

        assert_eq!(session.title, "Test Session");
        assert!(session.messages.is_empty());
        assert!(session.participants.is_empty());
        assert!(session.created_at <= Utc::now());

        // Allow for small timing difference between created_at and updated_at
        let time_diff = (session.updated_at - session.created_at)
            .num_milliseconds()
            .abs();
        assert!(
            time_diff < 1000,
            "Time difference between created_at and updated_at should be less than 1 second"
        );
    }

    #[test]
    fn test_chat_settings_defaults() {
        let settings = ChatSettings::default();

        assert!(!settings.auto_summarize);
        assert_eq!(settings.context_window_size, 4096);
        assert!(settings.enable_media_analysis);
        assert_eq!(settings.temperature, 0.7);
        assert_eq!(settings.max_tokens, None);
        assert_eq!(settings.system_prompt, None);
    }

    #[test]
    fn test_participant_management() {
        let mut session = ChatSession::new("Multi-participant Chat".to_string());

        let user_participant = Participant {
            id: Uuid::new_v4(),
            name: "User".to_string(),
            participant_type: ParticipantType::User,
            model: None,
            capabilities: vec![],
        };

        let agent_participant = Participant {
            id: Uuid::new_v4(),
            name: "AI Assistant".to_string(),
            participant_type: ParticipantType::Agent,
            model: Some("gpt-4".to_string()),
            capabilities: vec!["text_generation".to_string(), "image_analysis".to_string()],
        };

        session.add_participant(user_participant);
        session.add_participant(agent_participant);

        assert_eq!(session.participants.len(), 2);
        assert_eq!(
            session.participants[0].participant_type,
            ParticipantType::User
        );
        assert_eq!(
            session.participants[1].participant_type,
            ParticipantType::Agent
        );
        assert!(session.updated_at > session.created_at);
    }

    #[test]
    fn test_message_addition() {
        let mut session = ChatSession::new("Message Test".to_string());

        let message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            role: MessageRole::User,
            content: "Hello, world!".to_string(),
            media_attachments: vec![],
            model: None,
        };

        let original_updated = session.updated_at;
        session.add_message(message);

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "Hello, world!");
        assert!(session.updated_at > original_updated);
    }

    #[test]
    fn test_multimodal_message_processing() {
        let mut session = ChatSession::new("Multimodal Test".to_string());

        let media_file = MediaFile {
            path: "test_image.jpg".to_string(),
            media_type: MediaType::Image,
            size: Some((800, 600)),
            duration: None,
            thumbnail: None,
        };

        let result = session.process_multimodal_message(
            "What's in this image?".to_string(),
            vec![media_file],
            Uuid::new_v4(),
        );

        assert!(result.is_ok());

        let message = result.unwrap();
        assert_eq!(message.role, MessageRole::User);
        assert!(message.content.contains("What's in this image?"));
        assert_eq!(message.media_attachments.len(), 1);

        // Check that message was added to session
        assert_eq!(session.messages.len(), 1);
    }

    #[test]
    fn test_context_generation() {
        let mut session = ChatSession::new("Context Test".to_string());
        session.settings.context_window_size = 2;

        // Add multiple messages
        let messages = vec![
            ("First message", MessageRole::User),
            ("First response", MessageRole::Assistant),
            ("Second message", MessageRole::User),
            ("Second response", MessageRole::Assistant),
            ("Third message", MessageRole::User),
        ];

        for (content, role) in messages {
            let message = ChatMessage {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                role,
                content: content.to_string(),
                media_attachments: vec![],
                model: None,
            };
            session.add_message(message);
        }

        // Get context (should only include last 2 messages due to window size)
        let context = session.get_context_for_ai(false);
        assert!(context.contains("Second response"));
        assert!(context.contains("Third message"));
        assert!(!context.contains("First message"));
    }

    #[test]
    fn test_context_with_media() {
        let mut session = ChatSession::new("Media Context Test".to_string());

        let media_file = MediaFile {
            path: "context_image.png".to_string(),
            media_type: MediaType::Image,
            size: Some((1024, 768)),
            duration: None,
            thumbnail: None,
        };

        let message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            role: MessageRole::User,
            content: "Analyze this image".to_string(),
            media_attachments: vec![media_file],
            model: None,
        };

        session.add_message(message);

        // Test context with media
        let context_with_media = session.get_context_for_ai(true);
        assert!(context_with_media.contains("Analyze this image"));
        assert!(context_with_media.contains("Media:"));
        assert!(context_with_media.contains("context_image.png"));

        // Test context without media
        let context_without_media = session.get_context_for_ai(false);
        assert!(context_without_media.contains("Analyze this image"));
        assert!(!context_without_media.contains("Media:"));
    }

    #[test]
    fn test_markdown_export() {
        let mut session = ChatSession::new("Export Test".to_string());

        // Add a participant
        let participant = Participant {
            id: Uuid::new_v4(),
            name: "Test User".to_string(),
            participant_type: ParticipantType::User,
            model: None,
            capabilities: vec![],
        };
        session.add_participant(participant);

        // Add messages
        let user_message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            role: MessageRole::User,
            content: "Hello!".to_string(),
            media_attachments: vec![],
            model: None,
        };

        let assistant_message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            role: MessageRole::Assistant,
            content: "Hi there!".to_string(),
            media_attachments: vec![],
            model: None,
        };

        session.add_message(user_message);
        session.add_message(assistant_message);

        let markdown = session.export_to_markdown();

        assert!(markdown.contains("# Export Test"));
        assert!(markdown.contains("Test User"));
        assert!(markdown.contains("👤 User"));
        assert!(markdown.contains("🤖 Assistant"));
        assert!(markdown.contains("Hello!"));
        assert!(markdown.contains("Hi there!"));
    }

    #[test]
    fn test_chat_manager() {
        let manager = ChatManager::new();

        assert!(manager.sessions.is_empty());
        assert_eq!(manager.active_session_id, None);
        assert!(manager.agents.is_empty());
    }

    #[test]
    fn test_session_management() {
        let mut manager = ChatManager::new();

        // Create a session
        let session_id = manager.create_session("Test Session".to_string());

        assert_eq!(manager.sessions.len(), 1);
        assert_eq!(manager.active_session_id, Some(session_id));
        assert!(manager.sessions.contains_key(&session_id));

        // Get active session
        let active_session = manager.get_active_session();
        assert!(active_session.is_some());
        assert_eq!(active_session.unwrap().title, "Test Session");

        // Create another session
        let session_id2 = manager.create_session("Second Session".to_string());
        assert_eq!(manager.sessions.len(), 2);
        assert_eq!(manager.active_session_id, Some(session_id2));

        // Switch back to first session
        let switch_result = manager.switch_session(session_id);
        assert!(switch_result.is_ok());
        assert_eq!(manager.active_session_id, Some(session_id));
    }

    #[test]
    fn test_agent_integration() {
        let mut manager = ChatManager::new();
        let session_id = manager.create_session("Agent Test".to_string());

        // Create a multimodal agent
        let base_agent = AgentInfo {
            id: Uuid::new_v4(),
            name: "TestAgent".to_string(),
            model: "stub".to_string(),
            status: AgentStatus::Idle,
            current_task: None,
            tools: vec!["print".to_string()],
            created_at: Utc::now(),
            last_activity: Utc::now(),
        };

        let agent = MultiModalAgent::new(base_agent);
        let agent_id = agent.base_agent.id;

        // Add agent to session
        let result = manager.add_agent_to_session(session_id, agent);
        assert!(result.is_ok());

        // Check that agent was added to session and manager
        let session = manager.sessions.get(&session_id).unwrap();
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].id, agent_id);
        assert!(manager.agents.contains_key(&agent_id));
    }

    #[test]
    fn test_agent_response_processing() {
        let mut manager = ChatManager::new();
        let session_id = manager.create_session("Response Test".to_string());

        let base_agent = AgentInfo {
            id: Uuid::new_v4(),
            name: "ResponseAgent".to_string(),
            model: "stub".to_string(),
            status: AgentStatus::Idle,
            current_task: None,
            tools: vec![],
            created_at: Utc::now(),
            last_activity: Utc::now(),
        };

        let agent_id = base_agent.id;
        let agent = MultiModalAgent::new(base_agent);

        manager.add_agent_to_session(session_id, agent).unwrap();

        // Process agent response
        let result = manager.process_agent_response(
            session_id,
            agent_id,
            "Test input for agent".to_string(),
        );

        assert!(result.is_ok());

        // Check that response was added to session
        let session = manager.sessions.get(&session_id).unwrap();
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, MessageRole::Assistant);
    }

    #[test]
    fn test_auto_summarization() {
        let mut session = ChatSession::new("Summarization Test".to_string());
        session.settings.auto_summarize = true;
        session.settings.context_window_size = 2;

        // Add messages that exceed context window
        for i in 0..5 {
            let message = ChatMessage {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                role: if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                content: format!("Message {}", i),
                media_attachments: vec![],
                model: None,
            };
            session.add_message(message);
        }

        // Should have triggered summarization
        // Check that first message is now a system summary
        assert!(session.messages.len() <= session.settings.context_window_size + 1);
        if session.messages.len() > 2 {
            assert_eq!(session.messages[0].role, MessageRole::System);
            assert!(session.messages[0].content.contains("Conversation Summary"));
        }
    }

    #[test]
    fn test_system_prompt_context() {
        let mut session = ChatSession::new("System Prompt Test".to_string());
        session.settings.system_prompt = Some("You are a helpful assistant.".to_string());

        let context = session.get_context_for_ai(false);
        assert!(context.contains("System: You are a helpful assistant."));
    }
}
