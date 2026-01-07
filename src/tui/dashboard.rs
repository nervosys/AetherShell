//! TUI Performance Dashboard and Statistics

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
};

use super::app::App;

/// Draw the statistics dashboard
pub fn draw_stats_panel(f: &mut Frame, app: &App, area: Rect) {
    let stats = app.get_stats();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(8), // Message stats
            Constraint::Length(6), // Token estimation
            Constraint::Min(5),    // Agent stats
        ])
        .split(area);

    // Title
    let title = Paragraph::new("📊 Conversation Statistics")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Message statistics
    let msg_stats_text = vec![
        Line::from(vec![
            Span::styled("Total Messages: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.total_messages.to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  👤 User: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.user_messages.to_string(),
                Style::default().fg(Color::Blue),
            ),
            Span::raw("  "),
            Span::styled("🤖 Assistant: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.assistant_messages.to_string(),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("  "),
            Span::styled("⚙️  System: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.system_messages.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Total Characters: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.total_characters.to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Avg Message Length: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} chars", stats.avg_message_length),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Media Attachments: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.total_media_attachments.to_string(),
                Style::default().fg(Color::Magenta),
            ),
        ]),
    ];

    let msg_stats = Paragraph::new(msg_stats_text)
        .block(Block::default().borders(Borders::ALL).title("Messages"))
        .wrap(Wrap { trim: false });
    f.render_widget(msg_stats, chunks[1]);

    // Token estimation
    let tokens = app.estimate_tokens();
    let token_percentage = (tokens as f64 / 4096.0).min(1.0); // Assuming 4K context

    let token_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Estimated Tokens"),
        )
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent((token_percentage * 100.0) as u16)
        .label(format!("{} / 4096", tokens));
    f.render_widget(token_gauge, chunks[2]);

    // Agent statistics
    let _agent_items: Vec<ListItem> = stats
        .active_agents
        .to_string()
        .lines()
        .map(|line| ListItem::new(Line::from(line)))
        .collect();

    let agent_stats = List::new(vec![ListItem::new(Line::from(vec![
        Span::styled("Active Agents: ", Style::default().fg(Color::Gray)),
        Span::styled(
            stats.active_agents.to_string(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]))])
    .block(Block::default().borders(Borders::ALL).title("Agents"));

    f.render_widget(agent_stats, chunks[3]);
}

/// Draw help panel with keyboard shortcuts
pub fn draw_help_panel(f: &mut Frame, app: &App, area: Rect) {
    let help_text = app.get_help_text();

    let help_items: Vec<ListItem> = help_text
        .iter()
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:15}", parts[0]),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(parts[1], Style::default().fg(Color::Gray)),
                ]))
            } else {
                ListItem::new(Line::from(Span::raw(line)))
            }
        })
        .collect();

    let help_list = List::new(help_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "⌨️  {} Mode - Keyboard Shortcuts",
                    app.get_mode_string()
                ))
                .title_alignment(Alignment::Center),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(help_list, area);
}

/// Draw agent metrics dashboard
pub fn draw_agent_metrics(f: &mut Frame, app: &App, area: Rect) {
    let metrics = app.get_agent_metrics();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(5),    // Metrics list
        ])
        .split(area);

    // Title
    let title = Paragraph::new("🤖 Agent Performance Metrics")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Metrics list
    let metric_items: Vec<ListItem> = metrics
        .iter()
        .map(|m| {
            let status_color = match m.status {
                super::app::AgentStatus::Idle => Color::Gray,
                super::app::AgentStatus::Working => Color::Green,
                super::app::AgentStatus::Waiting => Color::Yellow,
                super::app::AgentStatus::Error(_) => Color::Red,
            };

            let uptime_display = format_duration(m.uptime_seconds);
            let idle_display = format_duration(m.idle_seconds);

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled("📛 ", Style::default()),
                    Span::styled(
                        format!("{:20}", m.name),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" ● ", Style::default().fg(status_color)),
                    Span::styled(format!("{:?}", m.status), Style::default().fg(status_color)),
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled("⏱  Uptime: ", Style::default().fg(Color::Gray)),
                    Span::styled(uptime_display, Style::default().fg(Color::Cyan)),
                    Span::raw("   "),
                    Span::styled("💤 Idle: ", Style::default().fg(Color::Gray)),
                    Span::styled(idle_display, Style::default().fg(Color::Yellow)),
                    Span::raw("   "),
                    Span::styled("🔧 Tools: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        m.tool_count.to_string(),
                        Style::default().fg(Color::Magenta),
                    ),
                ]),
                Line::from(""),
            ])
        })
        .collect();

    let metrics_list = List::new(metric_items)
        .block(Block::default().borders(Borders::ALL).title("Metrics"))
        .style(Style::default().fg(Color::White));

    f.render_widget(metrics_list, chunks[1]);
}

/// Format duration in seconds to human-readable format
fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Draw context window indicator
pub fn draw_context_window(f: &mut Frame, app: &App, area: Rect) {
    let window_size = 10; // Last 10 messages
    let context = app.get_context_window(window_size);

    let context_text = vec![
        Line::from(vec![
            Span::styled("Context Window: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("Last {} messages", context.len()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    let context_para = Paragraph::new(context_text)
        .block(Block::default().borders(Borders::ALL).title("📝 Context"))
        .wrap(Wrap { trim: false });

    f.render_widget(context_para, area);
}

/// Draw export confirmation
pub fn draw_export_notification(f: &mut Frame, area: Rect, format: &str) {
    let notification = Paragraph::new(format!("✅ Conversation exported to {}", format))
        .style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green)),
        );

    f.render_widget(notification, area);
}
