use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::Widget;

pub struct StatusBar {
    pub message: String,
    pub connected: bool,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            message: "⏳ initialising…".into(),
            connected: false,
        }
    }
}

impl Widget for StatusBar {
    fn name(&self) -> &'static str {
        "status_bar"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let connected_span = if self.connected {
            Span::styled(" ● CONNECTED", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" ● DISCONNECTED", Style::new().fg(Color::Red).add_modifier(Modifier::BOLD))
        };

        let text = Line::from(vec![
            Span::raw(" "),
            connected_span,
            Span::raw(" │ "),
            Span::raw(&self.message),
        ]);

        let block = Block::default()
            .borders(Borders::TOP)
            .style(Style::new().bg(Color::Black));

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(text).style(Style::new().bg(Color::Black)), inner);
    }
}
