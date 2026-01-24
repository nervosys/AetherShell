//! Tests for TUI application state and logic

#[cfg(test)]
mod tui_app_tests {
    use aether_shell::tui::app::{AgentStatus, App, AppMode, InputMode, MessageRole};
    use aether_shell::tui::media::{MediaFile, MediaType};

    #[test]
    fn test_app_initialization() {
        let app = App::new().unwrap();

        assert_eq!(app.mode, AppMode::Chat);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(!app.should_quit);
        assert!(app.messages.is_empty());
        assert!(app.agents.is_empty());
        assert!(app.media_files.is_empty());
        assert!(app.selected_media.is_empty());
        assert_eq!(app.tab_index, 0);
    }

    #[test]
    fn test_mode_switching() {
        let mut app = App::new().unwrap();

        // Test direct mode switching
        app.switch_mode(AppMode::AgentSwarm);
        assert_eq!(app.mode, AppMode::AgentSwarm);
        assert_eq!(app.input_mode, InputMode::Normal);

        app.switch_mode(AppMode::MediaBrowser);
        assert_eq!(app.mode, AppMode::MediaBrowser);

        // Test tab navigation
        app.tab_index = 0;
        app.next_tab();
        assert_eq!(app.tab_index, 1);
        assert_eq!(app.mode, AppMode::AgentSwarm);

        app.next_tab();
        assert_eq!(app.tab_index, 2);
        assert_eq!(app.mode, AppMode::MediaBrowser);

        // Test wrap around
        app.tab_index = 6;
        app.next_tab();
        assert_eq!(app.tab_index, 0);
        assert_eq!(app.mode, AppMode::Chat);

        // Test previous tab
        app.previous_tab();
        assert_eq!(app.tab_index, 6);
        assert_eq!(app.mode, AppMode::Search);
    }

    #[test]
    fn test_message_management() {
        let mut app = App::new().unwrap();

        // Test adding messages
        app.add_message(MessageRole::User, "Hello".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Hello");
        assert_eq!(app.messages[0].role, MessageRole::User);

        app.add_message(MessageRole::Assistant, "Hi there!".to_string());
        assert_eq!(app.messages.len(), 2);

        // Test message limit (set config to small value for testing)
        app.config.max_messages = 2;
        app.add_message(MessageRole::System, "Third message".to_string());
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].content, "Hi there!"); // First message should be removed
        assert_eq!(app.messages[1].content, "Third message");
    }

    #[test]
    fn test_agent_management() {
        let mut app = App::new().unwrap();

        // Test adding agents
        app.add_agent(
            "TestAgent".to_string(),
            "gpt-4".to_string(),
            vec!["print".to_string(), "ls".to_string()],
        );

        assert_eq!(app.agents.len(), 1);
        assert_eq!(app.agents[0].name, "TestAgent");
        assert_eq!(app.agents[0].model, "gpt-4");
        assert_eq!(app.agents[0].status, AgentStatus::Idle);
        assert_eq!(app.agents[0].tools.len(), 2);

        // Test agent removal
        app.agent_list_state.select(Some(0));
        app.remove_selected_agent();
        assert_eq!(app.agents.len(), 0);
        assert_eq!(app.agent_list_state.selected(), None);
    }

    #[test]
    fn test_media_selection() {
        let mut app = App::new().unwrap();

        // Add some media files
        let media1 = MediaFile {
            path: "image1.jpg".to_string(),
            media_type: MediaType::Image,
            size: Some((100, 100)),
            duration: None,
            thumbnail: None,
        };
        let media2 = MediaFile {
            path: "video1.mp4".to_string(),
            media_type: MediaType::Video,
            size: None,
            duration: Some(60.0),
            thumbnail: None,
        };

        app.add_media_file(media1);
        app.add_media_file(media2);
        assert_eq!(app.media_files.len(), 2);

        // Test media selection
        app.media_list_state.select(Some(0));
        app.toggle_media_selection();
        assert_eq!(app.selected_media.len(), 1);
        assert!(app.selected_media.contains(&0));

        // Test deselection
        app.toggle_media_selection();
        assert_eq!(app.selected_media.len(), 0);

        // Test multiple selections
        app.toggle_media_selection(); // Select index 0
        app.media_list_state.select(Some(1));
        app.toggle_media_selection(); // Select index 1
        assert_eq!(app.selected_media.len(), 2);

        // Test getting selected media files
        let selected_files = app.get_selected_media_files();
        assert_eq!(selected_files.len(), 2);
        assert_eq!(selected_files[0].path, "image1.jpg");
        assert_eq!(selected_files[1].path, "video1.mp4");

        // Test clearing selection
        app.clear_media_selection();
        assert_eq!(app.selected_media.len(), 0);
    }

    #[test]
    fn test_send_message_functionality() {
        let mut app = App::new().unwrap();

        // Set up input
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
        };
        use tui_input::backend::crossterm::to_input_request;

        app.input.reset();

        let h_event = KeyEvent {
            code: KeyCode::Char('H'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let e_event = KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let l_event = KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let o_event = KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };

        if let Some(req) = to_input_request(&Event::Key(h_event)) {
            app.input.handle(req);
        }
        if let Some(req) = to_input_request(&Event::Key(e_event)) {
            app.input.handle(req);
        }
        if let Some(req) = to_input_request(&Event::Key(l_event)) {
            app.input.handle(req);
        }
        if let Some(req) = to_input_request(&Event::Key(l_event)) {
            app.input.handle(req);
        }
        if let Some(req) = to_input_request(&Event::Key(o_event)) {
            app.input.handle(req);
        }

        // Send message
        let result = app.send_message();
        assert!(result.is_ok());

        // Check that message was added and input was cleared
        assert_eq!(app.messages.len(), 2); // User message + AI response
        assert_eq!(app.messages[0].role, MessageRole::User);
        assert_eq!(app.messages[0].content, "Hello");
        assert_eq!(app.messages[1].role, MessageRole::Assistant);
        assert!(app.input.value().is_empty());
    }

    #[test]
    fn test_agent_task_assignment() {
        let mut app = App::new().unwrap();

        // Add an agent
        app.add_agent(
            "TaskAgent".to_string(),
            "gpt-4".to_string(),
            vec!["print".to_string()],
        );

        // Select the agent and assign a task
        app.agent_list_state.select(Some(0));
        let task = "Analyze the codebase".to_string();
        let result = app.start_agent_task(task.clone());

        assert!(result.is_ok());
        assert_eq!(app.agents[0].status, AgentStatus::Working);
        assert_eq!(app.agents[0].current_task, Some(task));

        // Check that a system message was added
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, MessageRole::System);
        assert!(app.messages[0].content.contains("TaskAgent"));
        assert!(app.messages[0].content.contains("started task"));
    }

    #[test]
    fn test_list_navigation() {
        let mut app = App::new().unwrap();

        // Add some messages for navigation testing
        app.add_message(MessageRole::User, "Message 1".to_string());
        app.add_message(MessageRole::Assistant, "Response 1".to_string());
        app.add_message(MessageRole::User, "Message 2".to_string());

        // Test navigation in chat mode
        app.mode = AppMode::Chat;
        app.message_list_state.select(Some(0));

        app.move_list_down();
        assert_eq!(app.message_list_state.selected(), Some(1));

        app.move_list_down();
        assert_eq!(app.message_list_state.selected(), Some(2));

        // Test wrap around
        app.move_list_down();
        assert_eq!(app.message_list_state.selected(), Some(0));

        app.move_list_up();
        assert_eq!(app.message_list_state.selected(), Some(2));
    }

    #[test]
    fn test_config_defaults() {
        let app = App::new().unwrap();

        // default_model depends on environment, may be "stub" or empty
        assert!(
            app.config.default_model == "stub" || app.config.default_model.is_empty(),
            "default_model should be 'stub' or empty, got: '{}'",
            app.config.default_model
        );
        assert_eq!(app.config.max_messages, 1000);
        assert!(app.config.auto_scroll);
        assert!(app.config.show_timestamps);
        assert!(app.config.enable_media_preview);
        assert_eq!(app.config.agent_update_interval, 1000);
    }

    #[test]
    fn test_tab_titles() {
        let app = App::new().unwrap();
        let titles = app.get_tab_titles();

        assert_eq!(titles.len(), 7);
        assert_eq!(titles[0], "Chat");
        assert_eq!(titles[1], "Agents");
        assert_eq!(titles[2], "Media");
        assert_eq!(titles[3], "Settings");
        assert_eq!(titles[4], "Distributed");
        assert_eq!(titles[5], "Reasoning");
        assert_eq!(titles[6], "Search");
    }

    #[test]
    fn test_quit_functionality() {
        let mut app = App::new().unwrap();
        assert!(!app.should_quit);

        app.quit();
        assert!(app.should_quit);
    }
}
