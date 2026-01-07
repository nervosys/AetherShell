// Integration test to verify dashboard UI components render correctly

use aether_shell::tui::app::{App, AppMode, MessageRole};

#[test]
fn test_dashboard_integration_with_app() {
    let mut app = App::new().unwrap();

    // Add some test data
    app.add_message(MessageRole::User, "Test message 1".to_string());
    app.add_message(MessageRole::Assistant, "Response 1".to_string());
    app.add_message(MessageRole::User, "Test message 2".to_string());

    // Verify stats can be retrieved
    let stats = app.get_stats();
    assert_eq!(stats.total_messages, 3);
    assert_eq!(stats.user_messages, 2);
    assert_eq!(stats.assistant_messages, 1);

    // Verify token estimation
    let tokens = app.estimate_tokens();
    assert!(tokens > 0);

    // Verify mode string generation
    let mode_str = app.get_mode_string();
    assert_eq!(mode_str, "Chat");

    // Switch modes and verify
    app.mode = AppMode::AgentSwarm;
    let mode_str = app.get_mode_string();
    assert_eq!(mode_str, "Agent Swarm");
}

#[test]
fn test_dashboard_help_text_generation() {
    let mut app = App::new().unwrap();

    // Test help text for each mode
    let modes = vec![
        AppMode::Chat,
        AppMode::AgentSwarm,
        AppMode::MediaBrowser,
        AppMode::Settings,
        AppMode::DistributedAgents,
        AppMode::AdvancedReasoning,
    ];

    for mode in modes {
        app.mode = mode.clone();
        let help_text = app.get_help_text();
        assert!(
            !help_text.is_empty(),
            "Help text should exist for {:?}",
            mode
        );

        // Verify common shortcuts exist
        let all_text = help_text.join("\n");
        assert!(all_text.contains("Tab"), "Should mention Tab navigation");
    }
}

#[test]
fn test_dashboard_context_window_integration() {
    let mut app = App::new().unwrap();

    // Add many messages
    for i in 0..20 {
        app.add_message(MessageRole::User, format!("Message {}", i));
    }

    // Get context window
    let context = app.get_context_window(5);
    assert_eq!(context.len(), 5);

    // Verify it's the last 5 messages
    assert_eq!(context[0].content, "Message 15");
    assert_eq!(context[4].content, "Message 19");
}

#[test]
fn test_dashboard_export_integration() {
    let mut app = App::new().unwrap();

    app.add_message(MessageRole::User, "Hello".to_string());
    app.add_message(MessageRole::Assistant, "Hi there".to_string());

    // Test markdown export
    let markdown = app.export_to_markdown();
    assert!(markdown.contains("# AetherShell Conversation Export"));
    assert!(markdown.contains("**Total Messages:** 2"));
    assert!(markdown.contains("Hello"));
    assert!(markdown.contains("Hi there"));

    // Test JSON export
    let json = app.export_to_json().unwrap();
    println!("JSON output: {}", json); // Debug output
    assert!(json.contains("total_messages"));
    assert!(json.contains("Hello"));
    assert!(json.contains("Hi there"));
}

#[test]
fn test_dashboard_search_integration() {
    let mut app = App::new().unwrap();

    app.add_message(MessageRole::User, "How do I use pipelines?".to_string());
    app.add_message(
        MessageRole::Assistant,
        "Pipelines in AetherShell use the | operator".to_string(),
    );
    app.add_message(MessageRole::User, "What about functions?".to_string());

    // Search for "pipeline"
    let results = app.search_messages("pipeline");
    assert_eq!(results.len(), 2); // Should find both "pipelines" and "Pipelines"

    // Search for "function"
    let results = app.search_messages("function");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_dashboard_toggle_settings_integration() {
    let mut app = App::new().unwrap();

    // Test auto-scroll toggle
    let initial = app.config.auto_scroll;
    app.toggle_auto_scroll();
    assert_eq!(app.config.auto_scroll, !initial);
    app.toggle_auto_scroll();
    assert_eq!(app.config.auto_scroll, initial);

    // Test timestamps toggle
    let initial = app.config.show_timestamps;
    app.toggle_timestamps();
    assert_eq!(app.config.show_timestamps, !initial);

    // Test media preview toggle
    let initial = app.config.enable_media_preview;
    app.toggle_media_preview();
    assert_eq!(app.config.enable_media_preview, !initial);
}

#[test]
fn test_dashboard_stats_with_agents() {
    let mut app = App::new().unwrap();

    // Add messages
    app.add_message(MessageRole::User, "Task 1".to_string());
    app.add_message(MessageRole::Assistant, "Working on it".to_string());

    // Note: We can't easily add agents in this test without the full agent system
    // but we can verify stats work with the agent_count field
    let stats = app.get_stats();
    assert_eq!(stats.active_agents, app.agents.len());
}
