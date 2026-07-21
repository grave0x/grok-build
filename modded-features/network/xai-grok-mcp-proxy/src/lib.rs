//! MCP Registry Proxy — resilience layer for MCP servers.
//!
//! Wraps every MCP server connection with:
//! - Circuit breaking (via `xai-circuit-breaker`)
//! - Rate limiting (token bucket)
//! - Health checking (periodic pings)
//! - Auto-restart (exponential backoff)
//! - Request/response logging (ring buffer)
//! - Result caching (TTL + LRU)
//!
//! # Architecture
//!
//! ```text
//! Agent → McpServerProxy → CircuitBreaker → RateLimiter → MCP Server
//!                           ↑ HealthChecker   ↑ Logger     ↑ Cache
//! ```
//!
//! Spec: modded-featureSpecs/network/09-mcp-registry-proxy.md

pub mod cache;
pub mod config;
pub mod health;
pub mod limiter;
pub mod proxy;

pub use cache::ToolCache;
pub use config::ProxyConfig;
pub use proxy::{McpServerProxy, ProxyState};
