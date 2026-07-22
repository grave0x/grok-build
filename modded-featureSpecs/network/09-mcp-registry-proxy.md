# Spec 09 — MCP Registry Proxy

- **Priority:** P0
- **Crate:** `xai-grok-mcp-proxy` (new, extends `xai-grok-mcp`)
- **Depends on:** `xai-grok-mcp`, `xai-circuit-breaker`, `xai-grok-config`, `xai-grok-hooks`
- **Status:** Draft

---

## Overview

A local smart proxy for MCP (Model Context Protocol) servers that adds health checking, automatic restart, request/response logging, rate limiting, circuit breaking, and offline caching. Wraps every MCP server connection with resilience and observability.

## Motivation

MCP servers are external processes that crash, hang, get rate-limited, or return errors. Currently, a failing MCP server fails the entire agent turn. This is unacceptable for production use — MCP integrations with databases, web APIs, and internal tools must be reliable.

The existing `xai-grok-mcp` crate handles protocol and connection lifecycle but has no resilience layer. The existing `xai-circuit-breaker` crate provides the circuit-breaker primitive. This spec bridges them.

## Goals

- Auto-restart crashed MCP servers (with configurable max retries).
- Circuit breaker: after N consecutive failures, stop calling a server for a cooldown period.
- Rate limiting: configurable calls/second per MCP server.
- Request/response logging to file (debuggability).
- Health check pings: periodic `ping` to detect hung servers before they're needed.
- Offline caching: cache MCP tool results for a configurable TTL.
- `grok mcp status` — show all MCP servers with health, uptime, error count.
- `grok mcp restart <server>` — manually restart a server.
- `grok mcp logs <server> [-n 50] [--follow]` — view server logs.
- Graceful degradation: when a server is down, tools from that server are hidden from the model (not presented as available).

## Non-Goals

- MCP server discovery (already handled by `xai-grok-mcp` and config `[mcp_servers]`).
- Authentication tokens for MCP servers (handled by config/env).
- Cross-machine MCP routing (all MCP is local to the agent process).

## Design

### Architecture

```
┌──────────────────────────────────────────────┐
│  xai-grok-shell / agent loop                  │
│  Tool call: mcp_server.my_tool(...)            │
└──────────┬───────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────┐
│  xai-grok-mcp-proxy                           │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Per-Server Proxy Instance               │  │
│  │                                        │  │
│  │  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │ Circuit  │  │ Rate Limiter      │   │  │
│  │  │ Breaker  │  │ (token bucket)    │   │  │
│  │  └──────────┘  └──────────────────┘   │  │
│  │  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │ Health   │  │ Cache            │   │  │
│  │  │ Checker  │  │ (TTL + LRU)      │   │  │
│  │  └──────────┘  └──────────────────┘   │  │
│  │  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │ Logger   │  │ Restart          │   │  │
│  │  │ (ring    │  │ Controller       │   │  │
│  │  │  buffer) │  │ (max retries)    │   │  │
│  │  └──────────┘  └──────────────────┘   │  │
│  └────────────────────────────────────────┘  │
└──────────┬───────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────┐
│  MCP Server (subprocess)                      │
│  stdio transport                              │
└──────────────────────────────────────────────┘
```

### Config

```toml
[mcp_servers.my-db]
command = "npx @my/db-mcp"
args = ["--port", "5432"]

[mcp_servers.my-db.proxy]
# Circuit breaker
circuit_breaker_enabled = true
circuit_breaker_threshold = 5       # failures before open
circuit_breaker_cooldown_secs = 30  # seconds before half-open
circuit_breaker_half_open_max = 3   # probes before reset or reopen

# Rate limiting
rate_limit_calls_per_second = 10
rate_limit_burst = 20

# Health checking
health_check_enabled = true
health_check_interval_secs = 30
health_check_tool = "ping"          # optional: tool name to call for health

# Caching
cache_enabled = true
cache_ttl_secs = 300
cache_max_entries = 100
cache_key_tools = ["list_tables", "get_schema"]  # only cache read-only tools

# Logging
log_dir = "~/.grok/mcp-logs/my-db"
log_retention_days = 7

# Restart
restart_max_retries = 3
restart_backoff_secs = 5            # exponential: 5, 10, 20...
```

### Data Model

```rust
struct McpServerProxy {
    config: McpServerConfig,
    state: ProxyState,
    circuit_breaker: CircuitBreaker,
    rate_limiter: RateLimiter,
    cache: Option<ToolCache>,
    logger: RingBufferLogger,
    health: HealthStatus,
    start_time: Instant,
    error_count: u64,
}

enum ProxyState {
    Starting,
    Running,
    Degraded,      // circuit breaker half-open
    Stopped,
    Failed(String),
}

struct HealthStatus {
    last_ping: Option<Instant>,
    last_pong: Option<Instant>,
    is_healthy: bool,
    consecutive_failures: u32,
}

struct ToolCache {
    ttl: Duration,
    max_entries: usize,
    store: LruCache<CacheKey, CachedResult>,
}

struct CacheKey {
    tool_name: String,
    args_hash: String,  // BLAKE3 of serialized arguments
}
```

### Circuit Breaker States

```
CLOSED (normal operation)
  │  failures >= threshold
  ▼
OPEN (reject all calls)
  │  cooldown expires
  ▼
HALF_OPEN (probe with 1 call)
  │  success → CLOSED
  │  failure → OPEN (reset cooldown)
  ▼
CLOSED or OPEN (loop)
```

### CLI Interface

```
grok mcp status                    # show all servers with health, uptime, errors
grok mcp status my-db              # show single server details
grok mcp restart my-db             # restart server process
grok mcp logs my-db                # show recent logs (ring buffer)
grok mcp logs my-db --follow       # tail -f style
grok mcp flush-cache my-db         # clear server's result cache
grok mcp stats my-db               # show rate limit stats, cache hit rate
```

### Hook Integration

```json
{
  "hooks": {
    "McpServerStatus": [
      {
        "hooks": [
          { "type": "command", "command": "bin/notify-mcp-down.sh" }
        ]
      }
    ],
    "McpCircuitBreaker": [
      {
        "hooks": [
          { "type": "command", "command": "bin/log-circuit-break.sh" }
        ]
      }
    ]
  }
}
```

Events: `McpServerCrash`, `McpServerRestart`, `McpServerHealthy`, `McpCircuitBreakerOpen`, `McpRateLimitHit`.

## Implementation Plan

### Phase 1 — Circuit Breaker + Restart (2-3 weeks)
1. Create `xai-grok-mcp-proxy` crate with `McpServerProxy` struct.
2. Integrate `xai-circuit-breaker` into the proxy.
3. Implement auto-restart with exponential backoff.
4. Implement `grok mcp status` and `grok mcp restart`.
5. Tests: circuit breaker opens/recovers correctly, restart retries with backoff.

### Phase 2 — Rate Limiting + Health Checks (2 weeks)
1. Implement token-bucket rate limiter.
2. Implement periodic health check pings.
3. Implement graceful degradation (hide tools from unavailable servers).
4. Implement `grok mcp stats`.
5. Tests: rate limiting blocks excess calls, health check detects hung server.

### Phase 3 — Caching + Logging (1-2 weeks)
1. Implement `ToolCache` with TTL and LRU eviction.
2. Implement ring-buffer logger.
3. Implement `grok mcp logs`.
4. Implement hook events (crash, restart, circuit breaker).
5. Tests: cache hit rate, log rotation, hook events fire correctly.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Circuit breaker state machine, rate limiter token bucket, cache TTL |
| Integration | Mock MCP server that crashes → auto-restart; mock server that hangs → health check detects |
| Resilience | Network partition → circuit opens → recovers on reconnect |
| Performance | 1000 cached calls → <1ms per cache hit; 100 concurrent calls → rate limiter correctly sequences |

## Open Questions

1. Should caching be opt-in per tool (only safe read-only tools)? *Decision:* Yes. Cache only tools marked `readonly: true` in their schema or explicitly listed in `cache_key_tools`.
2. How to handle MCP servers that are also process managers (DB connection pools)? Restarting them loses state. *Decision:* Marked as `restart_max_retries = 0` — never restart, only alert.
3. Should the proxy support MCP over HTTP/SSE (not just stdio)? *Decision:* Phase 2. Stdio-only in MVP.
