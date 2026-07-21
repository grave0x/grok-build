//! Health checker — periodic ping to detect hung MCP servers.

use std::time::{Duration, Instant};

/// Health status for an MCP server.
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub last_ping: Option<Instant>,
    pub last_pong: Option<Instant>,
    pub is_healthy: bool,
    pub consecutive_failures: u32,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            last_ping: None,
            last_pong: None,
            is_healthy: true,
            consecutive_failures: 0,
        }
    }
}

impl HealthStatus {
    /// How long since the last successful pong.
    pub fn time_since_last_pong(&self) -> Option<Duration> {
        self.last_pong.map(|t| t.elapsed())
    }

    /// Mark a successful ping/pong cycle.
    pub fn record_success(&mut self) {
        let now = Instant::now();
        self.last_ping = Some(now);
        self.last_pong = Some(now);
        self.is_healthy = true;
        self.consecutive_failures = 0;
    }

    /// Mark a failed health check.
    pub fn record_failure(&mut self) {
        self.last_ping = Some(Instant::now());
        self.consecutive_failures += 1;
        if self.consecutive_failures >= 3 {
            self.is_healthy = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_default_healthy() {
        let status = HealthStatus::default();
        assert!(status.is_healthy);
        assert_eq!(status.consecutive_failures, 0);
    }

    #[test]
    fn test_health_failures() {
        let mut status = HealthStatus::default();
        status.record_failure();
        status.record_failure();
        assert!(status.is_healthy); // Not yet 3 failures
        status.record_failure();
        assert!(!status.is_healthy); // 3 failures = unhealthy
    }

    #[test]
    fn test_health_recovery() {
        let mut status = HealthStatus::default();
        for _ in 0..4 {
            status.record_failure();
        }
        assert!(!status.is_healthy);
        status.record_success();
        assert!(status.is_healthy);
        assert_eq!(status.consecutive_failures, 0);
    }
}
