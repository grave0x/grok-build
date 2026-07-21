//! Token-bucket rate limiter for MCP tool calls.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

/// Token-bucket rate limiter.
///
/// Tokens refill at a constant rate (`rate` per second) up to a maximum
/// burst capacity. Each call consumes one token. If no tokens are available,
/// the call is rejected.
#[derive(Debug)]
pub struct RateLimiter {
    /// Tokens refilled per second.
    rate: f64,
    /// Maximum burst (bucket capacity).
    burst: u32,
    /// Current available tokens.
    tokens: AtomicU32,
    /// Last refill timestamp (nanos from Instant).
    last_refill: AtomicU64,
}

impl RateLimiter {
    pub fn new(calls_per_second: u32, burst: u32) -> Self {
        Self {
            rate: calls_per_second as f64,
            burst,
            tokens: AtomicU32::new(burst),
            last_refill: AtomicU64::new(now_nanos()),
        }
    }

    /// Try to acquire a token. Returns true if allowed, false if rate-limited.
    pub fn try_acquire(&self) -> bool {
        self.refill();
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current == 0 {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Refill the bucket based on elapsed time.
    fn refill(&self) {
        let now = now_nanos();
        let last = self.last_refill.swap(now, Ordering::AcqRel);
        let elapsed_ns = now.saturating_sub(last);
        let elapsed_secs = elapsed_ns as f64 / 1_000_000_000.0;

        let new_tokens = (elapsed_secs * self.rate) as u32;
        if new_tokens > 0 {
            let mut current = self.tokens.load(Ordering::Relaxed);
            loop {
                let target = (current + new_tokens).min(self.burst);
                match self.tokens.compare_exchange_weak(
                    current,
                    target,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    /// Current available tokens (best-effort, not atomic with refill).
    pub fn available(&self) -> u32 {
        self.tokens.load(Ordering::Relaxed)
    }
}

fn now_nanos() -> u64 {
    // Best-effort monotonic nanos using Instant.
    // Instant doesn't provide raw nanos, so we use a base.
    use std::sync::OnceLock;
    static BASE: OnceLock<Instant> = OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    base.elapsed().as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_burst_allows_initial_calls() {
        let limiter = RateLimiter::new(100, 5);
        for _ in 0..5 {
            assert!(limiter.try_acquire(), "should allow within burst");
        }
        assert!(!limiter.try_acquire(), "should deny beyond burst");
    }

    #[test]
    fn test_refill() {
        let limiter = RateLimiter::new(10, 2);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire(), "empty bucket");
        // Tokens refill at 10/sec — 100ms should give ~1 token.
        std::thread::sleep(Duration::from_millis(150));
        assert!(limiter.try_acquire(), "should have refilled");
    }
}
