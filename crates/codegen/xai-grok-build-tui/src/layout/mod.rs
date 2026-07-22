//! Config-driven layout engine. Reads `[[tui.layout.widgets]]` from config,
//! allocates screen area per `Position`, and dispatches rendering.
//! Supports tabbed Main area — only one Main widget renders at a time.

use std::collections::HashMap;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use xai_grok_build_config::WidgetInstance;

use crate::widgets::{compute_dimensions, Position, Widget};

/// Manages the set of active widgets and their positions.
pub struct LayoutEngine {
    /// Widgets keyed by name.
    widgets: HashMap<&'static str, Box<dyn Widget>>,
    /// Index into Main-positioned widgets for tab switching.
    active_tab: usize,
    /// Cached list of Main-positioned widget names from config.
    main_widgets: Vec<String>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            active_tab: 0,
            main_widgets: Vec::new(),
        }
    }

    /// Register a widget. Silently skips if a widget with the same name exists.
    pub fn register(&mut self, widget: Box<dyn Widget>) {
        let name = widget.name();
        self.widgets.entry(name).or_insert(widget);
    }

    /// Handle a key event. Returns true if consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Tab => {
                self.next_tab();
                true
            }
            KeyCode::BackTab => {
                self.prev_tab();
                true
            }
            _ => {
                // Forward to active main widget
                if let Some(name) = self.main_widgets.get(self.active_tab) {
                    if let Some(w) = self.widgets.get_mut(name.as_str()) {
                        return w.handle_key(key);
                    }
                }
                false
            }
        }
    }

    fn next_tab(&mut self) {
        if !self.main_widgets.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.main_widgets.len();
        }
    }

    fn prev_tab(&mut self) {
        if !self.main_widgets.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.main_widgets.len().saturating_sub(1)
            } else {
                self.active_tab - 1
            };
        }
    }

    /// Return the name of the active main widget, if any.
    pub fn active_main_widget(&self) -> Option<&str> {
        self.main_widgets.get(self.active_tab).map(|s| s.as_str())
    }

    /// Render all enabled widgets according to config.
    pub fn render(&mut self, frame: &mut Frame, config_widgets: &[WidgetInstance]) {
        let full = frame.area();
        let (left_w, right_w, bottom_h) = compute_dimensions(config_widgets);

        // Rebuild main widget order from config
        self.main_widgets = config_widgets
            .iter()
            .filter(|w| w.enabled && w.position == xai_grok_build_config::WidgetPosition::Main)
            .map(|w| w.name.clone())
            .collect();
        if self.active_tab >= self.main_widgets.len() {
            self.active_tab = self.main_widgets.len().saturating_sub(1);
        }

        for cfg in config_widgets {
            if !cfg.enabled {
                continue;
            }

            let pos = match &cfg.position {
                xai_grok_build_config::WidgetPosition::Left => Position::Left,
                xai_grok_build_config::WidgetPosition::Main => Position::Main,
                xai_grok_build_config::WidgetPosition::Right => Position::Right,
                xai_grok_build_config::WidgetPosition::Bottom => Position::Bottom,
            };

            // For Main-positioned widgets, only render the active tab
            if pos == Position::Main {
                let Some(active) = self.active_main_widget() else {
                    continue;
                };
                if cfg.name != active {
                    continue;
                }
            }

            let Some(widget) = self.widgets.get_mut(cfg.name.as_str()) else {
                continue;
            };

            let area = pos.area_with(full, left_w, right_w, bottom_h);
            widget.render(frame, area);
        }
    }
}
