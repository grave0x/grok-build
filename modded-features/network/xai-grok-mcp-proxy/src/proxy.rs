//! MCP Server Proxy — wraps an MCP server with resilience features.

use crate::cache::ToolCache;
use crate::config::ProxyConfig;
use crate::health::HealthStatus;
use crate::limiter::RateLimiter;
use std::time::{Duration, Instant};

/// The MCP server proxy — one instance per configured MCP server.
#[derive(Debug)]
pub struct McpServerProxy {
    pub name: String,
    pub config: ProxyConfig,
    pub state: ProxyState,
    pub health: HealthStatus,
    pub start_time: Instant,
    pub error_count: u64,
    pub cache: Option<ToolCache>,
    pub rate_limiter: RateLimiter,
}

/// Lifecycle state of a proxied MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyState {
    /// Initial configuration applied, not yet started.
    Starting,
    /// Server is running and healthy.
    Running,
    /// Circuit breaker is half-open — probing with limited traffic.
    Degraded,
    /// Server has been manually stopped.
    Stopped,
    /// Server has failed with an unrecoverable error.
    Failed,
}

impl std::fmt::Display for ProxyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Degraded => write!(f, "degraded"),
            Self::Stopped => write!(f, "stopped"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl McpServerProxy {
    pub fn new(name: impl Into<String>, config: ProxyConfig) -> Self {
        let rate_limit = RateLimiter::new(
            config.rate_limit.calls_per_second,
            config.rate_limit.burst,
        );

        let cache = if config.cache.enabled && !config.cache.cache_key_tools.is_empty() {
            Some(ToolCache::new(
                Duration::from_secs(config.cache.ttl_secs),
                config.cache.max_entries,
                config.cache.cache_key_tools.clone(),
            ))
        } else {
            None
        };

        Self {
            name: name.into(),
            config,
            state: ProxyState::Starting,
            health: HealthStatus::default(),
            start_time: Instant::now(),
            error_count: 0,
            cache,
            rate_limiter: rate_limit,
        }
    }

    /// Uptime since the proxy was created.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check if rate-limited.
    pub fn check_rate_limit(&self) -> bool {
        self.rate_limiter.try_acquire()
    }

    /// Transition to a new state, logging for observability.
    pub fn set_state(&mut self, state: ProxyState) {
        if self.state != state {
            tracing::info!(
                server = %self.name,
                from = %self.state,
                to = %state,
                "MCP proxy state transition"
            );
            self.state = state;
        }
    }

    /// Record a successful call.
    pub fn record_success(&mut self) {
        self.health.record_success();
        if self.state == ProxyState::Degraded {
            self.set_state(ProxyState::Running);
        }
    }

    /// Record a failed call.
    pub fn record_failure(&mut self) {
        self.error_count += 1;
        self.health.record_failure();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_creation() {
        let proxy = McpServerProxy::new("test-db", ProxyConfig::default());
        assert_eq!(proxy.name, "test-db");
        assert!(matches!(proxy.state, ProxyState::Starting));
        assert!(proxy.health.is_healthy);
        assert_eq!(proxy.error_count, 0);
    }

    #[test]
    fn test_rate_limit_enforcement() {
        let mut config = ProxyConfig::default();
        config.rate_limit.calls_per_second = 1000;
        config.rate_limit.burst = 3;
        let proxy = McpServerProxy::new("test", config);

        assert!(proxy.check_rate_limit());
        assert!(proxy.check_rate_limit());
        assert!(proxy.check_rate_limit());
        assert!(!proxy.check_rate_limit(), "should be rate-limited after burst");
    }

    #[test]
    fn test_state_transitions() {
        let mut proxy = McpServerProxy::new("test", ProxyConfig::default());
        proxy.set_state(ProxyState::Running);
        assert!(matches!(proxy.state, ProxyState::Running));

        proxy.set_state(ProxyState::Degraded);
        assert!(matches!(proxy.state, ProxyState::Degraded));

        proxy.record_success();
        assert!(matches!(proxy.state, ProxyState::Running));

        proxy.set_state(ProxyState::Failed);
        assert!(matches!(proxy.state, ProxyState::Failed));
    }
}
