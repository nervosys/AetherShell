//! Event handling for the TUI

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;
use tui_input::backend::crossterm::EventHandler;

use super::app::{App, AppMode, InputMode};
use super::media::MediaFile;

pub fn handle_events(app: &mut App) -> Result<bool> {
    // Check if app should quit first
    if app.should_quit {
        return Ok(true);
    }

    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return handle_key_event(app, key);
            }
        }
    }

    // Check again after processing events
    Ok(app.should_quit)
}

fn handle_key_event(app: &mut App, key: crossterm::event::KeyEvent) -> Result<bool> {
    match app.input_mode {
        InputMode::Normal => handle_normal_mode(app, key),
        InputMode::Editing => handle_editing_mode(app, key),
    }
}

fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) -> Result<bool> {
    match key.code {
        // Global commands
        KeyCode::Char('q') => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.quit();
                return Ok(true);
            } else {
                // Just 'q' also quits in normal mode for convenience
                app.quit();
                return Ok(true);
            }
        }
        KeyCode::Esc => {
            app.quit();
            return Ok(true);
        }
        KeyCode::Char('c') => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.quit();
                return Ok(true);
            }
        }

        // Tab navigation
        KeyCode::Tab => app.next_tab(),
        KeyCode::BackTab => app.previous_tab(),

        // Mode-specific number keys
        KeyCode::Char('1') => app.switch_mode(AppMode::Chat),
        KeyCode::Char('2') => app.switch_mode(AppMode::AgentSwarm),
        KeyCode::Char('3') => app.switch_mode(AppMode::MediaBrowser),
        KeyCode::Char('4') => app.switch_mode(AppMode::Settings),
        KeyCode::Char('5') => app.switch_mode(AppMode::DistributedAgents),
        KeyCode::Char('6') => app.switch_mode(AppMode::AdvancedReasoning),

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => app.move_list_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_list_down(),

        // Mode-specific commands
        _ => handle_mode_specific_normal(app, key)?,
    }
    Ok(false)
}

fn handle_mode_specific_normal(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match app.mode {
        AppMode::Chat => handle_chat_normal(app, key),
        AppMode::AgentSwarm => handle_agent_normal(app, key),
        AppMode::MediaBrowser => handle_media_normal(app, key),
        AppMode::Settings => handle_settings_normal(app, key),
        AppMode::DistributedAgents => handle_distributed_normal(app, key),
        AppMode::AdvancedReasoning => handle_reasoning_normal(app, key),
        AppMode::Search => handle_search_normal(app, key),
    }
}

fn handle_chat_normal(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter | KeyCode::Char('i') => {
            app.input_mode = InputMode::Editing;
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Export conversation to markdown
            let markdown = app.export_to_markdown();
            // In real implementation, save to file
            std::fs::write("conversation_export.md", markdown)
                .unwrap_or_else(|_| eprintln!("Failed to export conversation"));
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Export conversation to JSON
            if let Ok(json) = app.export_to_json() {
                std::fs::write("conversation_export.json", json)
                    .unwrap_or_else(|_| eprintln!("Failed to export JSON"));
            }
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Clear chat history
            app.clear_conversation();
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Switch to search mode
            app.mode = AppMode::Search;
            app.input_mode = InputMode::Editing;
        }
        KeyCode::Char('1') => {
            // Toggle auto-scroll
            app.toggle_auto_scroll();
        }
        KeyCode::Char('2') => {
            // Toggle timestamps
            app.toggle_timestamps();
        }
        KeyCode::Char('3') => {
            // Toggle media preview
            app.toggle_media_preview();
        }
        KeyCode::Char('c') => {
            // Clear chat history
            app.messages.clear();
            app.message_list_state.select(None);
        }
        KeyCode::Char('m') => {
            // Switch to media browser to attach files
            app.switch_mode(AppMode::MediaBrowser);
        }
        KeyCode::Char('a') => {
            // Switch to agent mode
            app.switch_mode(AppMode::AgentSwarm);
        }
        _ => {}
    }
    Ok(())
}

fn handle_agent_normal(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('n') => {
            // Create new agent
            app.add_agent(
                format!("Agent-{}", app.agents.len() + 1),
                app.current_model.clone(),
                vec!["print".to_string(), "ls".to_string()],
            );
        }
        KeyCode::Delete | KeyCode::Char('d') => {
            // Delete selected agent
            app.remove_selected_agent();
        }
        KeyCode::Enter | KeyCode::Char('s') => {
            // Start task for selected agent
            app.input_mode = InputMode::Editing;
        }
        KeyCode::Char('m') => {
            // View metrics for all agents
            let metrics = app.get_agent_metrics();
            // In real implementation, this would show a metrics panel
            for metric in metrics {
                eprintln!(
                    "Agent: {} | Status: {:?} | Uptime: {}s | Tools: {}",
                    metric.name, metric.status, metric.uptime_seconds, metric.tool_count
                );
            }
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Pause/resume all agents (placeholder)
            // Would toggle agent execution state
        }
        KeyCode::Char('r') => {
            // Restart selected agent (placeholder)
            // Would reset agent state
        }
        KeyCode::Char('c') => {
            // Switch to chat mode
            app.switch_mode(AppMode::Chat);
        }
        _ => {}
    }
    Ok(())
}

fn handle_media_normal(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter | KeyCode::Char(' ') => {
            // Toggle selection of current media file
            app.toggle_media_selection();
        }
        KeyCode::Char('o') => {
            // Open file dialog (placeholder)
            // In a real implementation, you'd use a file picker
            if let Ok(file) = MediaFile::from_path("example.jpg") {
                app.add_media_file(file);
            }
        }
        KeyCode::Char('c') => {
            // Clear selection
            app.clear_media_selection();
        }
        KeyCode::Delete | KeyCode::Char('d') => {
            // Remove selected media file
            if let Some(selected) = app.media_list_state.selected() {
                if selected < app.media_files.len() {
                    app.media_files.remove(selected);

                    // Update selection indices
                    app.selected_media.retain(|&idx| idx != selected);
                    for idx in &mut app.selected_media {
                        if *idx > selected {
                            *idx -= 1;
                        }
                    }

                    // Adjust list selection
                    if app.media_files.is_empty() {
                        app.media_list_state.select(None);
                    } else if selected >= app.media_files.len() {
                        app.media_list_state.select(Some(app.media_files.len() - 1));
                    }
                }
            }
        }
        KeyCode::Char('b') => {
            // Go back to chat with selected media
            app.switch_mode(AppMode::Chat);
        }
        _ => {}
    }
    Ok(())
}

fn handle_settings_normal(_app: &mut App, _key: crossterm::event::KeyEvent) -> Result<()> {
    // Settings mode navigation would go here
    Ok(())
}

fn handle_editing_mode(app: &mut App, key: crossterm::event::KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            match app.mode {
                AppMode::Chat => {
                    app.send_message()?;
                    app.input_mode = InputMode::Normal;
                }
                AppMode::AgentSwarm => {
                    // Start agent task
                    let task = app.input.value().to_string();
                    if !task.trim().is_empty() {
                        app.start_agent_task(task)?;
                        app.input.reset();
                    }
                    app.input_mode = InputMode::Normal;
                }
                AppMode::Search => {
                    // Execute search
                    app.search_query = app.input.value().to_string();
                    app.execute_search();
                    app.input.reset();
                    app.input_mode = InputMode::Normal;
                }
                _ => {
                    app.input_mode = InputMode::Normal;
                }
            }
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
        }
        _ => {
            app.input.handle_event(&Event::Key(key));
        }
    }
    Ok(false)
}

fn handle_distributed_normal(_app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('s') => {
            // Start distributed swarm (placeholder - would be async in real implementation)
            // app.start_distributed_swarm("127.0.0.1:8080").await?;
        }
        KeyCode::Char('d') => {
            // Stop distributed swarm (placeholder - would be async in real implementation)
            // app.stop_distributed_swarm().await?;
        }
        KeyCode::Char('r') => {
            // Refresh network status
            // In a real implementation, this would update the agent status
        }
        KeyCode::Char('t') => {
            // Test connection
            // In a real implementation, this would ping all connected agents
        }
        _ => {}
    }
    Ok(())
}

fn handle_reasoning_normal(_app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('n') => {
            // Start new reasoning session (placeholder - would be async in real implementation)
            // let goal = create_planning_goal_from_input(&app.input.value());
            // app.start_reasoning_session(goal).await?;
        }
        KeyCode::Char('p') => {
            // View planning goals
            // In a real implementation, this would show a dialog with active goals
        }
        KeyCode::Char('k') => {
            // Browse knowledge base
            // In a real implementation, this would show knowledge base contents
        }
        KeyCode::Char('e') => {
            // Export reasoning chains
            // In a real implementation, this would save reasoning chains to file
        }
        KeyCode::Char('i') => {
            // Import knowledge
            // In a real implementation, this would load knowledge from file
        }
        _ => {}
    }
    Ok(())
}

fn handle_search_normal(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('i') | KeyCode::Char('/') => {
            // Enter search input mode
            app.input_mode = InputMode::Editing;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // Navigate to next search result
            app.next_search_result();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            // Navigate to previous search result
            app.previous_search_result();
        }
        KeyCode::Esc => {
            // Clear search and return to chat
            app.clear_search();
        }
        KeyCode::Char('c')
            if key.modifiers.contains(KeyModifiers::CONTROL)
            // Copy selected search result to clipboard (future enhancement)
            && !app.search_results.is_empty() =>
        {
            let result_idx = app.search_results[app.search_result_index];
            if let Some(msg) = app.messages.get(result_idx) {
                eprintln!("Would copy: {}", msg.content);
            }
        }
        _ => {}
    }
    Ok(())
}
