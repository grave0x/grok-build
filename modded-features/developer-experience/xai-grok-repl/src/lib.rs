//! Grok REPL Mode — interactive read-eval-print loop.
//!
//! Provides a fast, lightweight interactive mode for quick agent queries
//! without starting the full TUI. Supports multi-line input, history,
//! and streaming responses.
//!
//! Spec: modded-featureSpecs/developer-experience/04-repl-mode.md

/// REPL configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_history_size")]
    pub history_size: usize,
    #[serde(default = "default_prompt")]
    pub prompt: String,
    #[serde(default)]
    pub multiline: bool,
}

fn default_true() -> bool { true }
fn default_history_size() -> usize { 1000 }
fn default_prompt() -> String { "grok> ".into() }

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            history_size: 1000,
            prompt: "grok> ".into(),
            multiline: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ReplConfig::default();
        assert!(config.enabled);
        assert_eq!(config.prompt, "grok> ");
    }
}
