//! Model Router — intelligent model selection.
//!
//! Routes agent tasks to the appropriate model based on:
//! - Task complexity (simple Q&A vs. multi-file refactor)
//! - Cost budget (cheap model for simple, expensive for complex)
//! - Latency requirements (streaming vs. batch)
//! - Tool availability (some models can't use certain tools)
//!
//! Spec: modded-featureSpecs/model/07-model-router.md

/// Routing tier for model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::Display)]
pub enum RoutingTier {
    /// Fast, cheap model for simple queries.
    Fast,
    /// Balanced model for typical development tasks.
    Balanced,
    /// Most capable model for complex reasoning.
    Max,
}

/// Routing configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouterConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_tier: RoutingTier,
    #[serde(default)]
    pub auto_upgrade: bool,
    /// Tool names that trigger an automatic upgrade to Max tier.
    #[serde(default)]
    pub complex_tools: Vec<String>,
}

fn default_true() -> bool { true }

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_tier: RoutingTier::Balanced,
            auto_upgrade: true,
            complex_tools: vec!["Edit".into(), "Bash".into()],
        }
    }
}

/// Routing decision with rationale.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub tier: RoutingTier,
    pub model: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RouterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.default_tier, RoutingTier::Balanced);
        assert!(config.complex_tools.contains(&"Edit".to_string()));
    }
}
