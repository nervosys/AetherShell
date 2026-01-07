// Tests for TUI Search Mode

use aether_shell::tui::app::{App, AppMode, MessageRole};

#[test]
fn test_search_mode_initialization() {
    let app = App::new().unwrap();

    // Verify search state is initialized
    assert_eq!(app.search_query, "");
    assert_eq!(app.search_results.len(), 0);
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_execute_search() {
    let mut app = App::new().unwrap();

    // Add test messages
    app.add_message(MessageRole::User, "How do pipelines work?".to_string());
    app.add_message(
        MessageRole::Assistant,
        "Pipelines use the | operator".to_string(),
    );
    app.add_message(MessageRole::User, "What about functions?".to_string());

    // Execute search
    app.search_query = "pipeline".to_string();
    app.execute_search();

    // Verify results
    assert_eq!(app.search_results.len(), 2);
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_execute_search_empty_query() {
    let mut app = App::new().unwrap();

    // Add test messages
    app.add_message(MessageRole::User, "Test message".to_string());

    // Execute search with empty query
    app.search_query = "".to_string();
    app.execute_search();

    // Verify no results
    assert_eq!(app.search_results.len(), 0);
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_next_search_result() {
    let mut app = App::new().unwrap();

    // Add test messages
    app.add_message(MessageRole::User, "Hello world".to_string());
    app.add_message(MessageRole::User, "Hello again".to_string());
    app.add_message(MessageRole::User, "Hello there".to_string());

    // Execute search
    app.search_query = "hello".to_string();
    app.execute_search();

    assert_eq!(app.search_results.len(), 3);
    assert_eq!(app.search_result_index, 0);

    // Navigate to next result
    app.next_search_result();
    assert_eq!(app.search_result_index, 1);

    app.next_search_result();
    assert_eq!(app.search_result_index, 2);

    // Should wrap around
    app.next_search_result();
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_previous_search_result() {
    let mut app = App::new().unwrap();

    // Add test messages
    app.add_message(MessageRole::User, "Test 1".to_string());
    app.add_message(MessageRole::User, "Test 2".to_string());
    app.add_message(MessageRole::User, "Test 3".to_string());

    // Execute search
    app.search_query = "test".to_string();
    app.execute_search();

    assert_eq!(app.search_result_index, 0);

    // Navigate backward from first result (should wrap to last)
    app.previous_search_result();
    assert_eq!(app.search_result_index, 2);

    app.previous_search_result();
    assert_eq!(app.search_result_index, 1);

    app.previous_search_result();
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_clear_search() {
    let mut app = App::new().unwrap();

    // Add test messages and execute search
    app.add_message(MessageRole::User, "Test message".to_string());
    app.search_query = "test".to_string();
    app.execute_search();
    app.mode = AppMode::Search;

    // Verify search is active
    assert!(!app.search_query.is_empty());
    assert!(!app.search_results.is_empty());
    assert_eq!(app.mode, AppMode::Search);

    // Clear search
    app.clear_search();

    // Verify search is cleared
    assert_eq!(app.search_query, "");
    assert_eq!(app.search_results.len(), 0);
    assert_eq!(app.search_result_index, 0);
    assert_eq!(app.mode, AppMode::Chat);
}

#[test]
fn test_search_navigation_empty_results() {
    let mut app = App::new().unwrap();

    // No messages, no results
    app.search_query = "nonexistent".to_string();
    app.execute_search();

    // Navigation should not crash
    app.next_search_result();
    assert_eq!(app.search_result_index, 0);

    app.previous_search_result();
    assert_eq!(app.search_result_index, 0);
}

#[test]
fn test_search_with_special_characters() {
    let mut app = App::new().unwrap();

    // Add messages with special characters
    app.add_message(
        MessageRole::User,
        "What's the syntax for fn(x) => x * 2?".to_string(),
    );
    app.add_message(
        MessageRole::Assistant,
        "The fn(x) => syntax defines lambdas".to_string(),
    );

    // Search for special characters
    app.search_query = "fn(x)".to_string();
    app.execute_search();

    assert_eq!(app.search_results.len(), 2);
}

#[test]
fn test_search_mode_tab_navigation() {
    let mut app = App::new().unwrap();

    // Verify Search is the 7th tab (index 6)
    app.tab_index = 6;
    app.next_tab();
    assert_eq!(app.mode, AppMode::Chat); // Should wrap to first tab

    // Navigate to Search mode
    app.tab_index = 5;
    app.next_tab();
    assert_eq!(app.mode, AppMode::Search);
    assert_eq!(app.tab_index, 6);
}

#[test]
fn test_get_mode_string_search() {
    let mut app = App::new().unwrap();
    app.mode = AppMode::Search;

    assert_eq!(app.get_mode_string(), "Search");
}

#[test]
fn test_get_help_text_search() {
    let mut app = App::new().unwrap();
    app.mode = AppMode::Search;

    let help_text = app.get_help_text();
    assert!(!help_text.is_empty());

    let all_text = help_text.join("\n");
    assert!(all_text.contains("search"));
    assert!(all_text.contains("Navigate"));
}

#[test]
fn test_search_case_insensitive() {
    let mut app = App::new().unwrap();

    // Add messages with mixed case
    app.add_message(MessageRole::User, "HELLO WORLD".to_string());
    app.add_message(MessageRole::User, "hello world".to_string());
    app.add_message(MessageRole::User, "HeLLo WoRLd".to_string());

    // Search with lowercase
    app.search_query = "hello".to_string();
    app.execute_search();

    // Should find all three
    assert_eq!(app.search_results.len(), 3);
}

#[test]
fn test_search_partial_match() {
    let mut app = App::new().unwrap();

    // Add messages
    app.add_message(MessageRole::User, "Understanding pipelines".to_string());
    app.add_message(MessageRole::User, "Pipeline syntax".to_string());
    app.add_message(MessageRole::User, "Data processing".to_string());

    // Search for partial word
    app.search_query = "pipe".to_string();
    app.execute_search();

    // Should find both "pipelines" and "Pipeline"
    assert_eq!(app.search_results.len(), 2);
}

#[test]
fn test_search_with_media_attachments() {
    let mut app = App::new().unwrap();

    // Add message with content that should be searchable
    app.add_message(MessageRole::User, "Check this image".to_string());
    app.add_message(MessageRole::Assistant, "I see an image file".to_string());

    // Search should find messages regardless of media attachments
    app.search_query = "image".to_string();
    app.execute_search();

    assert_eq!(app.search_results.len(), 2);
}

#[test]
fn test_tab_titles_include_search() {
    let app = App::new().unwrap();
    let titles = app.get_tab_titles();

    assert_eq!(titles.len(), 7);
    assert_eq!(titles[6], "Search");
}

#[test]
fn test_search_result_indices_validity() {
    let mut app = App::new().unwrap();

    // Add messages
    for i in 0..5 {
        app.add_message(MessageRole::User, format!("Message {}", i));
    }

    // Search for "Message"
    app.search_query = "Message".to_string();
    app.execute_search();

    // Verify all result indices are valid
    for &idx in &app.search_results {
        assert!(idx < app.messages.len());
    }
}
