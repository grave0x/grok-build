//! Widget trait + registry for the TUI layout engine.

use ratatui::layout::Rect;
use ratatui::Frame;

use xai_grok_build_config::WidgetInstance;

/// Where a widget sits in the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Left,
    Main,
    Right,
    Bottom,
}

impl Position {
    /// Compute area for this position given full screen and config dimensions.
    /// `left_w`, `right_w`, `bottom_h` come from enabled widget configs (0 if not present).
    pub fn area_with(&self, full: Rect, left_w: u16, right_w: u16, bottom_h: u16) -> Rect {
        match self {
            Position::Left => Rect {
                x: full.x,
                y: full.y,
                width: left_w.min(full.width),
                height: full.height.saturating_sub(bottom_h),
            },
            Position::Main => Rect {
                x: full.x + left_w,
                y: full.y,
                width: full.width.saturating_sub(left_w + right_w),
                height: full.height.saturating_sub(bottom_h),
            },
            Position::Right => Rect {
                x: full.x + full.width.saturating_sub(right_w),
                y: full.y,
                width: right_w.min(full.width),
                height: full.height.saturating_sub(bottom_h),
            },
            Position::Bottom => Rect {
                x: full.x,
                y: full.y + full.height.saturating_sub(bottom_h),
                width: full.width,
                height: bottom_h.min(full.height),
            },
        }
    }
}

/// A single renderable TUI widget.
pub trait Widget: Send {
    fn name(&self) -> &'static str;
    fn render(&mut self, frame: &mut Frame, area: Rect);

    /// Handle a key event. Return true if consumed.
    fn handle_key(&mut self, _key: crossterm::event::KeyEvent) -> bool {
        false
    }
}

/// Compute pixel dimensions from enabled widget configs.
pub fn compute_dimensions(config_widgets: &[WidgetInstance]) -> (u16, u16, u16) {
    let mut left_w = 0u16;
    let mut right_w = 0u16;
    let mut bottom_h = 0u16;
    for w in config_widgets {
        if !w.enabled {
            continue;
        }
        match w.position {
            xai_grok_build_config::WidgetPosition::Left => {
                left_w = left_w.max(w.width.unwrap_or(28));
            }
            xai_grok_build_config::WidgetPosition::Right => {
                right_w = right_w.max(w.width.unwrap_or(24));
            }
            xai_grok_build_config::WidgetPosition::Bottom => {
                bottom_h += w.height.unwrap_or(3);
            }
            xai_grok_build_config::WidgetPosition::Main => {}
        }
    }
    (left_w, right_w, bottom_h)
}

pub mod log;
pub mod sidebar;
pub mod status_bar;
