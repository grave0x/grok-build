# Spec 10 — API Local Sandbox (MITM Proxy)

- **Priority:** P2
- **Crate:** `xai-grok-api-sandbox` (new) + `grok proxy` CLI
- **Depends on:** `xai-grok-http`, `xai-grok-config`
- **Status:** Draft

---

## Overview

A local MITM reverse-proxy that intercepts all AI API calls from grok-build. Records request/response pairs for debugging, replay, and testing. Can inject modified responses to simulate edge cases without real API calls. Switchable between record, replay, and passthrough modes.

Builds on the existing `experiments/` directory's certificate infrastructure.

## Motivation

Debugging agent behavior is hard when every turn costs real API money and requires network access. An API sandbox lets developers:
1. **Record** real sessions → replay them offline for debugging.
2. **Inject** failure scenarios: rate limits, timeouts, error responses, hallucinations.
3. **Deterministic testing**: replay the same API responses → get the same agent behavior.
4. **Cost-free iteration**: iterate on agent prompts and tool definitions without spending API credits.
5. **Audit**: inspect every API call the agent makes (what model, what prompt, what response).

## Goals

- `grok proxy start` — start MITM proxy on `localhost:<port>`.
- `grok proxy stop` — stop proxy.
- `grok proxy record --session <id>` — record all API traffic to a file.
- `grok proxy replay --session <id>` — replay recorded traffic.
- `grok proxy inject <scenario>` — inject a pre-defined failure scenario.
- `grok proxy status` — show mode, request count, error count.
- Three modes: `passthrough` (default), `record`, `replay`.
- Scenario format: YAML/JSON describing request matchers and response overrides.

## Non-Goals

- Encrypted traffic inspection (HTTPS MITM requires a CA cert — already exists in `experiments/data/ca/`).
- Load testing or performance benchmarking (use dedicated tools like k6).
- UI for managing recorded sessions (CLI-only for MVP).

## Design

### Architecture

```
┌──────────────────────────────────┐
│  grok-build process               │
│  AI API calls → localhost:8443   │
└────────────────┬─────────────────┘
                 │ HTTP/1.1 or HTTP/2
                 ▼
┌──────────────────────────────────┐
│  xai-grok-api-sandbox (proxy)    │
│                                  │
│  ┌────────────────────────────┐  │
│  │ Mode Selector              │  │
│  │ passthrough | record |     │  │
│  │ replay | inject            │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Session Store              │  │
│  │ (recorded API traffic in   │  │
│  │  ~/.grok/proxy-sessions/)  │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Scenario Injector          │  │
│  │ (match → override          │  │
│  │  response)                 │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Request Logger             │  │
│  │ (ring buffer, last 1000)   │  │
│  └────────────────────────────┘  │
└──────────┬───────────────────────┘
           │ forwarded request
           ▼
┌──────────────────────────────────┐
│  Real API (grok.com / x.ai)      │
│  (or mock server)                │
└──────────────────────────────────┘
```

### Modes

**Passthrough:** Proxy without any modification. Records nothing. Useful for testing that the proxy doesn't break anything.

**Record:** Every request/response pair is saved to a session file:
```
~/.grok/proxy-sessions/<session-id>/
├── metadata.json      # session info, model used, token counts
├── requests/
│   ├── 0001_request.json
│   ├── 0001_response.json
│   ├── 0002_request.json
│   ├── 0002_response.json
│   └── ...
└── replay-index.json  # mapping from request hash → response file
```

**Replay:** Read a recorded session. Match incoming requests by method + URL + body hash. Return the recorded response. If no match found, either error or fall through to real API (configurable).

**Inject:** Apply scenario rules. Each rule has a `match` (request matcher) and `override` (response modifier).

### Scenario Format

```yaml
# scenarios/rate-limit.yaml
name: "Simulate rate limit on search"
description: "Returns 429 for web_search tool calls"
match:
  url_contains: "/v1/chat/completions"
  body_contains: "web_search"
override:
  status: 429
  body:
    error:
      message: "Rate limit exceeded"
      type: "rate_limit_error"
      retry_after: 30
```

```yaml
# scenarios/timeout-on-edit.yaml
name: "Simulate timeout on edit tool"
match:
  body_contains: "search_replace"
override:
  delay_ms: 30000    # introduce 30s delay
  status: 200
  body: |
    {"id":"mock","choices":[{"finish_reason":"stop"}]}
```

```yaml
# scenarios/fail-all.yaml
name: "Fail all API calls"
match:
  url_contains: "/v1/"
override:
  status: 500
  body:
    error:
      message: "Internal server error (injected)"
      type: "server_error"
```

### CLI Interface

```
# Start/Stop
grok proxy start [--port 8443]                    # start proxy
grok proxy start --mode record                     # start in record mode
grok proxy stop                                    # stop proxy

# Recording
grok proxy record start --session "debug-session"  # start recording
grok proxy record stop                             # stop recording
grok proxy record list                             # list recorded sessions
grok proxy record show <session-id>                # show session details

# Replay
grok proxy replay <session-id>                     # replay a recorded session

# Scenarios
grok proxy inject <scenario-file.yaml>             # inject a scenario
grok proxy inject list                             # list available scenarios
grok proxy inject disable                          # disable all scenarios

# Status
grok proxy status                                  # show proxy state
grok proxy status --verbose                        # recent requests, errors
```

### Config

```toml
[proxy]
enabled = false                    # disabled by default; enable for debugging
port = 8443
mode = "passthrough"               # passthrough | record | replay
ca_cert = "~/.grok/proxy-ca.crt"   # CA cert for HTTPS MITM
ca_key = "~/.grok/proxy-ca.key"    # CA key (keep secure)
record_dir = "~/.grok/proxy-sessions"
ring_buffer_size = 1000
replay_fallback = "error"          # error | passthrough
```

### Env Var Integration

When the proxy is running, grok-build automatically sets:
```
OPENAI_BASE_URL=http://localhost:8443/v1
GROK_BASE_URL=http://localhost:8443/v1
```

This redirects all API calls through the proxy without code changes.

## Implementation Plan

### Phase 1 — Passthrough + Record (2 weeks)
1. Create `xai-grok-api-sandbox` crate with Axum-based HTTP proxy.
2. Implement passthrough mode (forward requests, return responses).
3. Implement record mode (save request/response pairs to session directory).
4. Implement `grok proxy start`, `grok proxy stop`, `grok proxy status`.
5. Implement CA cert generation and installation (first-run setup).
6. Tests: proxy forwards correctly, record saves correct files, CA cert installs.

### Phase 2 — Replay + Inject (2-3 weeks)
1. Implement replay mode (match request → return recorded response).
2. Implement scenario injection (match rules → override response).
3. Implement `grok proxy replay`, `grok proxy inject`.
4. Add delay injection (simulate slow responses).
5. Tests: replay matches correctly, scenarios override as expected, delays work.

### Phase 3 — Polish (1 week)
1. Implement `grok proxy record list/show`.
2. Add ring buffer for recent request log.
3. Add mode switching without restart.
4. Add load scenario from `~/.grok/proxy-scenarios/` directory.
5. Tests: mode switching, ring buffer, directory-based scenario loading.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Request matching, response serialization, scenario parsing |
| Integration | Proxy → real API → recorded → replay matches |
| Scenario | Inject 429 → verify agent receives rate limit error and handles it |
| Performance | 1000 requests through proxy → <5ms overhead per request |
| Security | MITM cert is valid, no sensitive data leaked in logs |

## Open Questions

1. Should we support WebSocket traffic capture? *Decision:* Phase 3. MVP is HTTP/HTTPS only.
2. How to handle streaming responses (SSE)? *Decision:* Record as concatenated chunks; replay streams them at configurable speed.
3. Should the proxy support rewriting prompts (e.g., redact secrets)? *Decision:* Phase 2 via scenario rules with body transformation.
