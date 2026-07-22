//! `grok-build-tui` — config-driven dashboard.

#![deny(unused)]
#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::io::stdout;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use xai_grok_build_config::load_config;

mod layout;
mod widgets;

use layout::LayoutEngine;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cfg = load_config()?;
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut engine = LayoutEngine::new();
    register_widgets(&mut engine, &cfg);

    let res = run(&mut terminal, &mut engine, &cfg).await;

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    engine: &mut LayoutEngine,
    cfg: &xai_grok_build_config::GrokBuildConfig,
) -> Result<()> {
    loop {
        terminal.draw(|frame| {
            engine.render(frame, &cfg.tui.layout.widgets);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Engine gets first chance to consume
                    if engine.handle_key(key) {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Down => {
                            // Forward sidebar navigation
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

/// Register all built-in widgets.
fn register_widgets(engine: &mut LayoutEngine, cfg: &xai_grok_build_config::GrokBuildConfig) {
    engine.register(Box::new(widgets::sidebar::Sidebar::default()));
    engine.register(Box::new(widgets::status_bar::StatusBar::new(cfg)));
    engine.register(Box::new(widgets::log::Log::default()));
    engine.register(Box::new(widgets::projects::Projects::new(cfg)));
    engine.register(Box::new(widgets::chat_view::ChatView::new(cfg)));
    engine.register(Box::new(widgets::usage::Usage::new(cfg)));
    engine.register(Box::new(widgets::help_bar::HelpBar::default()));
}
