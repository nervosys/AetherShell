//! UI rendering for the TUI

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};

use super::app::{AgentStatus, App, AppMode, InputMode, MessageRole};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    // Header with tabs
    draw_header(f, app, chunks[0]);

    // Main content area
    match app.mode {
        AppMode::Chat => draw_chat(f, app, chunks[1]),
        AppMode::AgentSwarm => draw_agent_swarm(f, app, chunks[1]),
        AppMode::MediaBrowser => draw_media_browser(f, app, chunks[1]),
        AppMode::Settings => draw_settings(f, app, chunks[1]),
        AppMode::DistributedAgents => draw_distributed_agents(f, app, chunks[1]),
        AppMode::AdvancedReasoning => draw_advanced_reasoning(f, app, chunks[1]),
    }

    // Footer with input and help
    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let titles = app.get_tab_titles();
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("AetherShell TUI"),
        )
        .select(app.tab_index)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Blue)
                .fg(Color::White),
        );
    f.render_widget(tabs, area);
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(area);

    // Chat messages
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let content = format_message_content(msg);
            let style = match msg.role {
                MessageRole::User => Style::default().fg(Color::Cyan),
                MessageRole::Assistant => Style::default().fg(Color::Green),
                MessageRole::System => Style::default().fg(Color::Yellow),
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let messages_list = List::new(messages)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Chat Messages"),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    f.render_stateful_widget(
        messages_list,
        chunks[0],
        &mut app.message_list_state.clone(),
    );

    // Side panel with media and agent info
    draw_chat_sidebar(f, app, chunks[1]);
}

fn draw_chat_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    // Selected media files
    let media_items: Vec<ListItem> = app
        .get_selected_media_files()
        .iter()
        .map(|media| ListItem::new(media.display_info()))
        .collect();

    let media_list = List::new(media_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Attached Media"),
    );

    f.render_widget(media_list, chunks[0]);

    // Active agents summary
    let agent_count = app.agents.len();
    let working_agents = app
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Working)
        .count();

    let agent_info = Paragraph::new(format!(
        "Agents: {} total\nWorking: {}\nModel: {}",
        agent_count, working_agents, app.current_model
    ))
    .block(Block::default().borders(Borders::ALL).title("Agent Status"));

    f.render_widget(agent_info, chunks[1]);
}

fn draw_agent_swarm(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(area);

    // Agent list
    let agents: Vec<ListItem> = app
        .agents
        .iter()
        .map(|agent| {
            let status_icon = match agent.status {
                AgentStatus::Idle => "⭕",
                AgentStatus::Working => "🟢",
                AgentStatus::Waiting => "🟡",
                AgentStatus::Error(_) => "🔴",
            };

            let content = format!(
                "{} {} [{}] - {}",
                status_icon,
                agent.name,
                agent.model,
                agent.current_task.as_deref().unwrap_or("idle")
            );

            ListItem::new(content)
        })
        .collect();

    let agents_list = List::new(agents)
        .block(Block::default().borders(Borders::ALL).title("AI Agents"))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    f.render_stateful_widget(agents_list, chunks[0], &mut app.agent_list_state.clone());

    // Agent details panel
    draw_agent_details(f, app, chunks[1]);
}

fn draw_agent_details(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(5)].as_ref())
        .split(area);

    // Selected agent details
    let details = if let Some(selected) = app.agent_list_state.selected() {
        if let Some(agent) = app.agents.get(selected) {
            format!(
                "Name: {}\nModel: {}\nStatus: {:?}\nTools: {}\nCreated: {}\nLast Activity: {}",
                agent.name,
                agent.model,
                agent.status,
                agent.tools.join(", "),
                agent.created_at.format("%H:%M:%S"),
                agent.last_activity.format("%H:%M:%S")
            )
        } else {
            "No agent selected".to_string()
        }
    } else {
        "Select an agent to view details".to_string()
    };

    let details_paragraph = Paragraph::new(details)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Agent Details"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(details_paragraph, chunks[0]);

    // Agent controls help
    let help_text = "n: New Agent | d: Delete | Enter: Start Task | c: Chat";
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .alignment(Alignment::Center);

    f.render_widget(help, chunks[1]);
}

fn draw_media_browser(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    // Media file list
    let media_items: Vec<ListItem> = app
        .media_files
        .iter()
        .enumerate()
        .map(|(idx, media)| {
            let selected_marker = if app.selected_media.contains(&idx) {
                "✓ "
            } else {
                "  "
            };

            let content = format!("{}{}", selected_marker, media.display_info());
            ListItem::new(content)
        })
        .collect();

    let media_list = List::new(media_items)
        .block(Block::default().borders(Borders::ALL).title("Media Files"))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    f.render_stateful_widget(media_list, chunks[0], &mut app.media_list_state.clone());

    // Media preview/details
    draw_media_preview(f, app, chunks[1]);
}

fn draw_media_preview(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(5)].as_ref())
        .split(area);

    // Media details
    let details = if let Some(selected) = app.media_list_state.selected() {
        if let Some(media) = app.media_files.get(selected) {
            format!(
                "Path: {}\nType: {:?}\nSize: {:?}\nDuration: {:?}",
                media.path, media.media_type, media.size, media.duration
            )
        } else {
            "No media selected".to_string()
        }
    } else {
        "Select a media file to view details".to_string()
    };

    let details_paragraph = Paragraph::new(details)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Media Details"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(details_paragraph, chunks[0]);

    // Media controls help
    let help_text =
        "Space: Select | o: Open File | c: Clear Selection | d: Delete | b: Back to Chat";
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .alignment(Alignment::Center);

    f.render_widget(help, chunks[1]);
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    let settings_text = format!(
        "Default Model: {}\nMax Messages: {}\nAuto Scroll: {}\nShow Timestamps: {}\nMedia Preview: {}",
        app.config.default_model,
        app.config.max_messages,
        app.config.auto_scroll,
        app.config.show_timestamps,
        app.config.enable_media_preview
    );

    let settings = Paragraph::new(settings_text)
        .block(Block::default().borders(Borders::ALL).title("Settings"))
        .wrap(Wrap { trim: true });

    f.render_widget(settings, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(area);

    // Input area
    let input_title = match app.mode {
        AppMode::Chat => "Message",
        AppMode::AgentSwarm => "Agent Task",
        _ => "Input",
    };

    let input_style = match app.input_mode {
        InputMode::Normal => Style::default(),
        InputMode::Editing => Style::default().fg(Color::Yellow),
    };

    let input = Paragraph::new(app.input.value())
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(input_title));

    f.render_widget(input, chunks[0]);

    // Set cursor position when editing
    if app.input_mode == InputMode::Editing {
        let cursor_x = chunks[0].x + app.input.visual_cursor() as u16 + 1;
        let cursor_y = chunks[0].y + 1;
        f.set_cursor(cursor_x, cursor_y);
    }

    // Help text
    let help_text = match app.input_mode {
        InputMode::Normal => "i: Edit | q: Quit | Tab: Switch | ↑↓: Navigate",
        InputMode::Editing => "Enter: Send | Esc: Cancel",
    };

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .alignment(Alignment::Center);

    f.render_widget(help, chunks[1]);
}

fn format_message_content(msg: &super::app::ChatMessage) -> Text<'_> {
    let timestamp = if msg.timestamp.date_naive() == chrono::Utc::now().date_naive() {
        msg.timestamp.format("%H:%M:%S").to_string()
    } else {
        msg.timestamp.format("%m/%d %H:%M").to_string()
    };

    let role_prefix = match msg.role {
        MessageRole::User => "👤",
        MessageRole::Assistant => "🤖",
        MessageRole::System => "⚙️",
    };

    let model_info = msg.model.as_deref().unwrap_or("unknown");

    let mut lines = vec![Line::from(vec![
        Span::raw(format!("[{}] ", timestamp)),
        Span::raw(format!("{} ", role_prefix)),
        Span::raw(format!("[{}] ", model_info)),
    ])];

    // Add message content
    let content_lines: Vec<Line> = msg
        .content
        .lines()
        .map(|line| Line::from(Span::raw(format!("  {}", line))))
        .collect();
    lines.extend(content_lines);

    // Add media attachments info
    if !msg.media_attachments.is_empty() {
        lines.push(Line::from(Span::raw("  📎 Attachments:")));
        for media in &msg.media_attachments {
            lines.push(Line::from(Span::raw(format!(
                "    {}",
                media.display_info()
            ))));
        }
    }

    Text::from(lines)
}

fn draw_distributed_agents(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    // Left panel: Connected agents
    let agent_status = app.get_distributed_agent_status();
    let agent_items: Vec<ListItem> = agent_status
        .iter()
        .enumerate()
        .map(|(i, status)| {
            let style = if i % 2 == 0 {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(Span::styled(status.clone(), style)))
        })
        .collect();

    let agents_list = List::new(agent_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🌐 Distributed Agents"),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_widget(agents_list, chunks[0]);

    // Right panel: Network status and controls
    let network_info = vec![
        "Network Status: Active",
        "Connected Nodes: 3",
        "Active Tasks: 7",
        "Load Balancing: Enabled",
        "",
        "Commands:",
        "  [s] Start distributed swarm",
        "  [d] Stop distributed swarm",
        "  [r] Refresh network status",
        "  [t] Test connection",
        "",
        "Recent Activity:",
        "  • Agent-1 completed task #123",
        "  • Agent-2 joined the network",
        "  • Load balancer rebalanced tasks",
    ];

    let network_items: Vec<ListItem> = network_info
        .iter()
        .map(|info| ListItem::new(Line::from(*info)))
        .collect();

    let network_list = List::new(network_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("📊 Network Status"),
        );

    f.render_widget(network_list, chunks[1]);
}

fn draw_advanced_reasoning(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ].as_ref())
        .split(area);

    // Top panel: Active reasoning sessions
    let reasoning_sessions = app.get_active_reasoning_sessions();
    let session_items: Vec<ListItem> = if reasoning_sessions.is_empty() {
        vec![ListItem::new(Line::from("No active reasoning sessions"))]
    } else {
        reasoning_sessions
            .iter()
            .map(|session| ListItem::new(Line::from(session.clone())))
            .collect()
    };

    let sessions_list = List::new(session_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🧠 Active Reasoning Sessions"),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_widget(sessions_list, chunks[0]);

    // Middle panel: Reasoning strategies
    let strategies_info = vec![
        "Available Strategies:",
        "",
        "🔗 Chain of Thought",
        "  - Sequential step-by-step reasoning",
        "  - High confidence threshold: 0.7",
        "  - Max steps: 10",
        "",
        "🌳 Tree of Thought", 
        "  - Multi-branch exploration",
        "  - Branching factor: 3",
        "  - Max depth: 5",
        "",
        "🔀 Modality Fusion",
        "  - Multi-modal integration",
        "  - Text: 40%, Image: 30%, Audio: 20%, Video: 10%",
        "  - Consensus threshold: 0.8",
        "",
        "📊 Hierarchical Planning",
        "  - Multi-level abstraction",
        "  - 3 abstraction levels",
        "  - Subgoal threshold: 0.7",
        "",
        "⚔️ Adversarial Reasoning",
        "  - Self-criticism and refinement",
        "  - Criticism strength: 0.8",
        "  - Validation rounds: 2",
    ];

    let strategies_items: Vec<ListItem> = strategies_info
        .iter()
        .map(|info| {
            let style = if info.starts_with("🔗") || info.starts_with("🌳") || 
                         info.starts_with("🔀") || info.starts_with("📊") || 
                         info.starts_with("⚔️") {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if info.starts_with("  -") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(*info, style)))
        })
        .collect();

    let strategies_list = List::new(strategies_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🎯 Reasoning Strategies"),
        );

    f.render_widget(strategies_list, chunks[1]);

    // Bottom panel: Knowledge base and controls
    let knowledge_info = vec![
        "Knowledge Base Status:",
        "",
        "📚 Facts: 0 stored",
        "📋 Rules: 0 defined", 
        "🔍 Patterns: 0 learned",
        "💡 Experiences: 0 recorded",
        "",
        "Commands:",
        "  [n] Start new reasoning session",
        "  [p] View planning goals",
        "  [k] Browse knowledge base",
        "  [e] Export reasoning chains",
        "  [i] Import knowledge",
        "",
        "Planning Horizon: 20 steps",
        "Confidence Threshold: 0.75",
    ];

    let knowledge_items: Vec<ListItem> = knowledge_info
        .iter()
        .map(|info| {
            let style = if info.starts_with("📚") || info.starts_with("📋") || 
                         info.starts_with("🔍") || info.starts_with("💡") {
                Style::default().fg(Color::Green)
            } else if info.starts_with("  [") {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(*info, style)))
        })
        .collect();

    let knowledge_list = List::new(knowledge_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("💾 Knowledge & Controls"),
        );

    f.render_widget(knowledge_list, chunks[2]);
}
