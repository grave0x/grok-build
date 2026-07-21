//! TTL + LRU result cache for MCP tool calls.
//!
//! Only caches read-only tools (explicitly listed in `cache_key_tools`).
//! Cache keys are (tool_name, BLAKE3(arguments)).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A cached MCP tool result with expiry.
#[derive(Debug, Clone)]
struct CachedResult {
    value: serde_json::Value,
    created_at: Instant,
}

/// In-memory cache for MCP tool call results.
#[derive(Debug)]
pub struct ToolCache {
    ttl: Duration,
    max_entries: usize,
    allowed_tools: Vec<String>,
    store: HashMap<CacheKey, CachedResult>,
}

/// Compound cache key: tool name + hash of arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub tool_name: String,
    pub args_hash: String,
}

impl CacheKey {
    pub fn new(tool_name: &str, args: &serde_json::Value) -> Self {
        let args_str = serde_json::to_string(args).unwrap_or_default();
        let args_hash = blake3::hash(args_str.as_bytes()).to_hex().to_string();
        Self {
            tool_name: tool_name.to_string(),
            args_hash,
        }
    }
}

impl ToolCache {
    pub fn new(ttl: Duration, max_entries: usize, allowed_tools: Vec<String>) -> Self {
        Self {
            ttl,
            max_entries,
            allowed_tools,
            store: HashMap::new(),
        }
    }

    /// Check if a tool is eligible for caching.
    pub fn can_cache(&self, tool_name: &str) -> bool {
        self.allowed_tools.iter().any(|t| t == tool_name)
    }

    /// Look up a cached result. Returns None if expired or not found.
    pub fn get(&self, key: &CacheKey) -> Option<serde_json::Value> {
        let entry = self.store.get(key)?;
        if entry.created_at.elapsed() > self.ttl {
            return None;
        }
        Some(entry.value.clone())
    }

    /// Insert a result into the cache.
    pub fn insert(&mut self, key: CacheKey, value: serde_json::Value) {
        // Evict oldest if at capacity.
        if self.store.len() >= self.max_entries {
            if let Some(oldest) = self.store.iter().min_by_key(|(_, v)| v.created_at) {
                let oldest_key = oldest.0.clone();
                self.store.remove(&oldest_key);
            }
        }
        self.store.insert(
            key,
            CachedResult {
                value,
                created_at: Instant::now(),
            },
        );
    }

    /// Evict all expired entries.
    pub fn evict_expired(&mut self) -> usize {
        let before = self.store.len();
        self.store.retain(|_, v| v.created_at.elapsed() <= self.ttl);
        before - self.store.len()
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Clear all entries.
    pub fn flush(&mut self) {
        self.store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit() {
        let mut cache = ToolCache::new(
            Duration::from_secs(300),
            100,
            vec!["list_tables".to_string()],
        );
        let args = serde_json::json!({"schema": "public"});
        let key = CacheKey::new("list_tables", &args);

        cache.insert(key.clone(), serde_json::json!({"tables": ["users", "posts"]}));
        let result = cache.get(&key);
        assert!(result.is_some());
        assert_eq!(result.unwrap()["tables"][0], "users");
    }

    #[test]
    fn test_cache_miss_uncacheable_tool() {
        let cache = ToolCache::new(
            Duration::from_secs(300),
            100,
            vec!["list_tables".to_string()],
        );
        assert!(!cache.can_cache("drop_table"));
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = ToolCache::new(Duration::from_secs(300), 2, vec!["t1".to_string()]);

        cache.insert(CacheKey::new("t1", &serde_json::json!(1)), serde_json::json!(1));
        std::thread::sleep(Duration::from_millis(10));
        cache.insert(CacheKey::new("t1", &serde_json::json!(2)), serde_json::json!(2));
        std::thread::sleep(Duration::from_millis(10));
        cache.insert(CacheKey::new("t1", &serde_json::json!(3)), serde_json::json!(3));

        assert_eq!(cache.len(), 2);
        // The oldest entry (args=1) should be evicted.
        let key1 = CacheKey::new("t1", &serde_json::json!(1));
        assert!(cache.get(&key1).is_none());
    }
}
