use std::collections::VecDeque;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::Widget;

const MAX_LINES: usize = 100;

pub struct Log {
    lines: VecDeque<String>,
}

impl Default for Log {
    fn default() -> Self {
        let mut s = Self { lines: VecDeque::new() };
        s.push("log ready");
        s
    }
}

impl Log {
    pub fn push(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(msg);
    }
}

impl Widget for Log {
    fn name(&self) -> &'static str {
        "log"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Log ")
            .borders(Borders::ALL)
            .style(Style::new().fg(Color::DarkGray));

        let inner = block.inner(area);

        let lines: Vec<Line> = self.lines.iter().rev().take(inner.height as usize).rev()
            .map(|l| Line::from(l.as_str()))
            .collect();

        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).style(Style::new().bg(Color::Black)),
            inner,
        );
    }
}
