use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::Widget;

const NAV_ITEMS: &[(&str, &str)] = &[
    ("⌂", "Projects"),
    ("✎", "Chat"),
    ("☰", "Files"),
    ("⚙", "Settings"),
];

pub struct Sidebar {
    pub active: usize,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self { active: 0 }
    }
}

impl Widget for Sidebar {
    fn name(&self) -> &'static str {
        "sidebar"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::RIGHT)
            .style(Style::new().bg(Color::Black));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(NAV_ITEMS.len() as u16 + 2),
                Constraint::Min(0),
            ])
            .split(inner);

        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            "  NAVIGATION",
            Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));

        for (i, (icon, label)) in NAV_ITEMS.iter().enumerate() {
            let active = i == self.active;
            let style = if active {
                Style::new()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::White)
            };
            let prefix = if active { "▸ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(format!("{prefix}{icon} {label}"), style),
            ]));
        }

        frame.render_widget(Paragraph::new(lines).style(Style::new().bg(Color::Black)), chunks[0]);
    }
}
