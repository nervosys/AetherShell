//! Terminal User Interface for AetherShell
//!
//! This module provides a rich TUI experience for interacting with multi-modal LLMs
//! and managing AI agent swarms. Features include:
//! - Multi-pane interface with command input, media preview, and agent status
//! - Support for images, videos, and audio in terminal
//! - Real-time agent swarm management and monitoring
//! - Interactive chat interface with multi-modal inputs

pub mod agents;
pub mod app;
pub mod chat;
pub mod distributed;
pub mod events;
pub mod media;
pub mod reasoning;
pub mod ui;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use app::App;

/// Main entry point for the TUI
pub fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new()?;
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if events::handle_events(app)? {
            break;
        }
    }
    Ok(())
}
