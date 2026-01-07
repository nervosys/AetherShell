// Tests for TUI dashboard and statistics features

#[cfg(test)]
mod tui_dashboard_tests {
    use aether_shell::tui::app::{App, AppMode, MessageRole};

    #[test]
    fn test_export_to_markdown() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Hello".to_string());
        app.add_message(MessageRole::Assistant, "Hi there!".to_string());

        let markdown = app.export_to_markdown();

        assert!(markdown.contains("# AetherShell Conversation Export"));
        assert!(markdown.contains("👤 User"));
        assert!(markdown.contains("🤖 Assistant"));
        assert!(markdown.contains("Hello"));
        assert!(markdown.contains("Hi there!"));
    }

    #[test]
    fn test_export_to_json() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Test message".to_string());

        let json_result = app.export_to_json();
        assert!(json_result.is_ok());

        let json = json_result.unwrap();
        assert!(json.contains("exported_at"));
        assert!(json.contains("total_messages"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_search_messages() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Hello world".to_string());
        app.add_message(MessageRole::Assistant, "How can I help?".to_string());
        app.add_message(MessageRole::User, "Tell me about Rust".to_string());

        let results = app.search_messages("rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 2);

        let results = app.search_messages("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0);

        let results = app.search_messages("nonexistent");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_filter_by_role() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Message 1".to_string());
        app.add_message(MessageRole::Assistant, "Response 1".to_string());
        app.add_message(MessageRole::User, "Message 2".to_string());
        app.add_message(MessageRole::Assistant, "Response 2".to_string());
        app.add_message(MessageRole::System, "System message".to_string());

        let user_msgs = app.filter_by_role(MessageRole::User);
        assert_eq!(user_msgs.len(), 2);
        assert_eq!(user_msgs, vec![0, 2]);

        let assistant_msgs = app.filter_by_role(MessageRole::Assistant);
        assert_eq!(assistant_msgs.len(), 2);
        assert_eq!(assistant_msgs, vec![1, 3]);

        let system_msgs = app.filter_by_role(MessageRole::System);
        assert_eq!(system_msgs.len(), 1);
        assert_eq!(system_msgs, vec![4]);
    }

    #[test]
    fn test_conversation_stats() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Hello".to_string());
        app.add_message(MessageRole::Assistant, "Hi there!".to_string());
        app.add_message(MessageRole::System, "System ready".to_string());

        let stats = app.get_stats();

        assert_eq!(stats.total_messages, 3);
        assert_eq!(stats.user_messages, 1);
        assert_eq!(stats.assistant_messages, 1);
        assert_eq!(stats.system_messages, 1);
        assert_eq!(
            stats.total_characters,
            "Hello".len() + "Hi there!".len() + "System ready".len()
        );
        assert!(stats.avg_message_length > 0);
    }

    #[test]
    fn test_clear_conversation() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Message 1".to_string());
        app.add_message(MessageRole::Assistant, "Response 1".to_string());

        assert_eq!(app.messages.len(), 2);

        app.clear_conversation();

        assert_eq!(app.messages.len(), 0);
        assert_eq!(app.message_list_state.selected(), Some(0));
    }

    #[test]
    fn test_context_window() {
        let mut app = App::new().unwrap();

        // Add 15 messages
        for i in 0..15 {
            app.add_message(MessageRole::User, format!("Message {}", i));
        }

        // Get last 10 messages
        let context = app.get_context_window(10);
        assert_eq!(context.len(), 10);

        // Verify it's the last 10
        assert!(context[0].content.contains("Message 5"));
        assert!(context[9].content.contains("Message 14"));

        // Test when window size > total messages
        let app2 = App::new().unwrap();
        let context2 = app2.get_context_window(10);
        assert_eq!(context2.len(), 0);
    }

    #[test]
    fn test_estimate_tokens() {
        let mut app = App::new().unwrap();

        // Empty conversation
        assert_eq!(app.estimate_tokens(), 0);

        // Add message with 100 characters (roughly 25 tokens at 4 chars/token)
        let msg = "a".repeat(100);
        app.add_message(MessageRole::User, msg);

        let tokens = app.estimate_tokens();
        assert_eq!(tokens, 25); // 100 / 4

        // Add another 200 character message
        let msg2 = "b".repeat(200);
        app.add_message(MessageRole::Assistant, msg2);

        let tokens2 = app.estimate_tokens();
        assert_eq!(tokens2, 75); // (100 + 200) / 4
    }

    #[test]
    fn test_agent_metrics() {
        let mut app = App::new().unwrap();

        // No agents
        let metrics = app.get_agent_metrics();
        assert_eq!(metrics.len(), 0);

        // Add agent
        app.add_agent(
            "TestAgent".to_string(),
            "stub".to_string(),
            vec!["tool1".to_string()],
        );

        let metrics = app.get_agent_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "TestAgent");
        assert_eq!(metrics[0].tool_count, 1);
        assert!(metrics[0].uptime_seconds >= 0);
    }

    #[test]
    fn test_toggle_settings() {
        let mut app = App::new().unwrap();

        // Default values
        assert!(app.config.auto_scroll);
        assert!(app.config.show_timestamps);
        assert!(app.config.enable_media_preview);

        // Toggle auto-scroll
        app.toggle_auto_scroll();
        assert!(!app.config.auto_scroll);
        app.toggle_auto_scroll();
        assert!(app.config.auto_scroll);

        // Toggle timestamps
        app.toggle_timestamps();
        assert!(!app.config.show_timestamps);
        app.toggle_timestamps();
        assert!(app.config.show_timestamps);

        // Toggle media preview
        app.toggle_media_preview();
        assert!(!app.config.enable_media_preview);
        app.toggle_media_preview();
        assert!(app.config.enable_media_preview);
    }

    #[test]
    fn test_get_mode_string() {
        let mut app = App::new().unwrap();

        assert_eq!(app.get_mode_string(), "Chat");

        app.switch_mode(AppMode::AgentSwarm);
        assert_eq!(app.get_mode_string(), "Agent Swarm");

        app.switch_mode(AppMode::MediaBrowser);
        assert_eq!(app.get_mode_string(), "Media Browser");

        app.switch_mode(AppMode::Settings);
        assert_eq!(app.get_mode_string(), "Settings");

        app.switch_mode(AppMode::DistributedAgents);
        assert_eq!(app.get_mode_string(), "Distributed Agents");

        app.switch_mode(AppMode::AdvancedReasoning);
        assert_eq!(app.get_mode_string(), "Advanced Reasoning");
    }

    #[test]
    fn test_get_help_text() {
        let mut app = App::new().unwrap();

        // Chat mode help
        let help = app.get_help_text();
        assert!(!help.is_empty());
        assert!(help.iter().any(|s| s.contains("Send message")));

        // Agent mode help
        app.switch_mode(AppMode::AgentSwarm);
        let help = app.get_help_text();
        assert!(help.iter().any(|s| s.contains("agent")));

        // Media mode help
        app.switch_mode(AppMode::MediaBrowser);
        let help = app.get_help_text();
        assert!(help.iter().any(|s| s.contains("media")));
    }

    #[test]
    fn test_stats_with_media() {
        let mut app = App::new().unwrap();

        // Add message with media attachment
        let msg_with_media = "Test message".to_string();
        app.add_message(MessageRole::User, msg_with_media);

        // Simulate adding media (would need MediaFile implementation)
        // For now, just test that stats work
        let stats = app.get_stats();
        assert_eq!(stats.total_messages, 1);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut app = App::new().unwrap();
        app.add_message(MessageRole::User, "Hello WORLD".to_string());
        app.add_message(MessageRole::User, "hello world".to_string());
        app.add_message(MessageRole::User, "HeLLo WoRLd".to_string());

        // Should find all three regardless of case
        let results = app.search_messages("hello");
        assert_eq!(results.len(), 3);

        let results = app.search_messages("HELLO");
        assert_eq!(results.len(), 3);

        let results = app.search_messages("WoRlD");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_export_empty_conversation() {
        let app = App::new().unwrap();

        let markdown = app.export_to_markdown();
        assert!(markdown.contains("**Total Messages:** 0"));

        let json = app.export_to_json().unwrap();
        assert!(json.contains("\"total_messages\": 0"));
    }

    #[test]
    fn test_context_window_edge_cases() {
        let mut app = App::new().unwrap();

        // Window size 0
        let context = app.get_context_window(0);
        assert_eq!(context.len(), 0);

        // Add 5 messages, request window of 10
        for i in 0..5 {
            app.add_message(MessageRole::User, format!("Msg {}", i));
        }
        let context = app.get_context_window(10);
        assert_eq!(context.len(), 5); // Should return all available

        // Request exact size
        let context = app.get_context_window(5);
        assert_eq!(context.len(), 5);

        // Request smaller window
        let context = app.get_context_window(3);
        assert_eq!(context.len(), 3);
        assert!(context[0].content.contains("Msg 2")); // Last 3 are 2, 3, 4
    }
}
