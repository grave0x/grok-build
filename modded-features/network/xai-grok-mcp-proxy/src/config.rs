//! Proxy configuration loaded from `.grok/config.toml` `[mcp_servers.<name>.proxy]`.

use serde::{Deserialize, Serialize};

/// Per-MCP-server proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Circuit breaker settings.
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    /// Rate limiting settings.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// Health check settings.
    #[serde(default)]
    pub health_check: HealthCheckConfig,
    /// Result caching settings.
    #[serde(default)]
    pub cache: CacheConfig,
    /// Logging settings.
    #[serde(default)]
    pub logging: LogConfig,
    /// Restart settings.
    #[serde(default)]
    pub restart: RestartConfig,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            circuit_breaker: CircuitBreakerConfig::default(),
            rate_limit: RateLimitConfig::default(),
            health_check: HealthCheckConfig::default(),
            cache: CacheConfig::default(),
            logging: LogConfig::default(),
            restart: RestartConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Number of consecutive failures before opening the circuit.
    #[serde(default = "default_threshold")]
    pub threshold: u32,
    /// Seconds to wait before transitioning from Open to HalfOpen.
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
    /// Maximum probes allowed in HalfOpen state before re-opening.
    #[serde(default = "default_half_open_max")]
    pub half_open_max: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 5,
            cooldown_secs: 30,
            half_open_max: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum calls per second.
    #[serde(default = "default_calls_per_second")]
    pub calls_per_second: u32,
    /// Maximum burst size (tokens that can be consumed instantly).
    #[serde(default = "default_burst")]
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            calls_per_second: 10,
            burst: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Interval in seconds between health pings.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Optional tool name to invoke for health check (defaults to "ping").
    #[serde(default)]
    pub tool: Option<String>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 30,
            tool: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub enabled: bool,
    /// TTL in seconds for cached results.
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
    /// Maximum number of entries in the cache.
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Tool names whose results are safe to cache (read-only tools only).
    #[serde(default)]
    pub cache_key_tools: Vec<String>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_secs: 300,
            max_entries: 100,
            cache_key_tools: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Directory for MCP server logs.
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    /// Retention in days for log files.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_dir: String::new(), // filled at runtime per server
            retention_days: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartConfig {
    /// Maximum number of restart attempts.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial backoff in seconds (doubles each retry).
    #[serde(default = "default_backoff_secs")]
    pub backoff_secs: u64,
}

impl Default for RestartConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_secs: 5,
        }
    }
}

fn default_true() -> bool { true }
fn default_threshold() -> u32 { 5 }
fn default_cooldown_secs() -> u64 { 30 }
fn default_half_open_max() -> u32 { 3 }
fn default_calls_per_second() -> u32 { 10 }
fn default_burst() -> u32 { 20 }
fn default_interval() -> u64 { 30 }
fn default_ttl() -> u64 { 300 }
fn default_max_entries() -> usize { 100 }
fn default_log_dir() -> String { String::new() }
fn default_retention_days() -> u32 { 7 }
fn default_max_retries() -> u32 { 3 }
fn default_backoff_secs() -> u64 { 5 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_proxy_config() {
        let config = ProxyConfig::default();
        assert!(config.circuit_breaker.enabled);
        assert_eq!(config.circuit_breaker.threshold, 5);
        assert_eq!(config.rate_limit.calls_per_second, 10);
        assert!(config.health_check.enabled);
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_str = r#"
[circuit_breaker]
enabled = true
threshold = 10
cooldown_secs = 60
half_open_max = 5

[rate_limit]
calls_per_second = 50
burst = 100

[health_check]
enabled = true
interval_secs = 15

[cache]
enabled = true
ttl_secs = 600
max_entries = 500
cache_key_tools = ["list_tables", "get_schema"]

[logging]
log_dir = "~/.grok/mcp-logs/my-db"
retention_days = 14

[restart]
max_retries = 5
backoff_secs = 10
"#;
        let config: ProxyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.circuit_breaker.threshold, 10);
        assert_eq!(config.rate_limit.calls_per_second, 50);
        assert_eq!(config.health_check.interval_secs, 15);
        assert!(config.cache.enabled);
        assert_eq!(config.cache.cache_key_tools.len(), 2);
        assert_eq!(config.restart.max_retries, 5);
    }
}
