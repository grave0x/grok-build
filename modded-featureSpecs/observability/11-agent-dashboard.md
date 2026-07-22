# Spec 11 — Agent Dashboard

- **Priority:** P2
- **Crate:** `xai-grok-dashboard` (new, optional web UI)
- **Depends on:** `xai-grok-shell`, `xai-grok-pager`, `xai-grok-tools` (for live events)
- **Status:** Draft

---

## Overview

A lightweight web UI dashboard that shows live agent activity, session metrics, tool usage heatmaps, and cost analytics. Runs as an optional sidecar HTTP server that grok-build can expose on localhost.

## Motivation

Agent sessions are invisible black boxes — you only see what appears in the TUI scrollback. A dashboard provides:
- **At a glance:** what is the agent doing *right now*?
- **Historical:** what did the agent do in the last session?
- **Analytics:** which tools are used most? How long do bash commands take?
- **Debugging:** inspect individual tool calls with full params and results.
- **Management:** kill hung commands, view logs, adjust config.

For power users and teams, a web dashboard makes agent observability accessible without digging through raw logs.

## Goals

- Live agent activity feed (WebSocket from agent process → browser).
- Session timeline with expandable tool call cards.
- Tool usage heatmap (which tools, how often, duration).
- Token usage charts (input vs output over time).
- Error rate dashboard.
- Recent file edits timeline.
- Agent goals display (current plan mode goal, progress).
- Config viewer (current effective config, model, sandbox state).

## Non-Goals

- Full multi-user auth (single-user localhost only).
- Persistent database (dashboards are ephemeral, derived from session state).
- Mobile app (responsive web UI only).

## Design

### Architecture

```
┌──────────────────────────────────┐
│  grok-build process               │
│  ┌────────────────────────────┐  │
│  │ Agent Loop                  │  │
│  │   ↓ events (tokio channel)  │  │
│  │  ┌──────────────────────┐  │  │
│  │  │ Event Bus             │  │  │
│  │  │ (broadcast channel)   │  │  │
│  │  └──────────────────────┘  │  │
│  │       │ subscribed          │  │
│  │       ▼                     │  │
│  │  ┌──────────────────────┐  │  │
│  │  │ Dashboard HTTP Server │  │  │
│  │  │ (Axum, optional)      │  │  │
│  │  └──────────────────────┘  │  │
│  └────────────────────────────┘  │
└──────────────────┬───────────────┘
                   │ localhost:9100
                   ▼
┌──────────────────────────────────┐
│  Browser / curl                   │
│  http://localhost:9100            │
│                                   │
│  ┌──────────────┐ ┌────────────┐ │
│  │ Live Feed     │ │ Charts     │ │
│  │ (WebSocket)   │ │ (Chart.js) │ │
│  └──────────────┘ └────────────┘ │
│  ┌──────────────┐ ┌────────────┐ │
│  │ Tool Cards    │ │ Config     │ │
│  └──────────────┘ └────────────┘ │
└──────────────────────────────────┘
```

### Event Bus

```rust
/// Events emitted by the agent loop and consumed by the dashboard.
#[derive(Clone, Serialize)]
enum DashboardEvent {
    SessionStarted { id: Uuid, model: String, cwd: String },
    ToolCallStarted { id: u64, tool: String, params: JsonValue, timestamp: DateTime<Utc> },
    ToolCallCompleted { id: u64, success: bool, duration: Duration, result_summary: String },
    ModelInteraction { prompt_tokens: u64, response_tokens: u64, model: String, duration: Duration },
    Error { tool: String, error: String, timestamp: DateTime<Utc> },
    GoalUpdated { goal: String, status: GoalStatus },
    SandboxViolation { command: String, rule: String },
    SessionEnded { id: Uuid, total_duration: Duration, total_tokens: u64 },
}
```

The event bus is a `tokio::sync::broadcast` channel. The dashboard server subscribes to it. Other subscribers (audit log, cost tracker, hooks) can also subscribe.

### API Endpoints

```
GET  /                          → SPA HTML (React built with esbuild)
GET  /api/events                → WebSocket (live event stream)
GET  /api/session               → current session metadata
GET  /api/session/timeline      → all tool calls in current session
GET  /api/session/stats         → aggregated stats (tool counts, avg duration, error rate)
GET  /api/session/tokens        → token usage over time (time series)
GET  /api/session/goals         → current and completed goals
GET  /api/config                → effective config (redacted secrets)
GET  /api/history?limit=10      → recent sessions summary
```

### Web UI Pages

**Live Feed** (default):
```
┌─── Agent Live ──────────────────────────────────┐
│ 🟢 Running | Model: grok-3 | Session: 23m 14s  │
│                                                  │
│ ⚡ Bash: cargo build --release                   │
│    compiling... 2.3s                              │
│ ✓ Read: Cargo.toml (42 lines) 0.1s               │
│ ✗ Bash: ./deploy.sh (exit 1) 15.2s               │
│    Error: auth failed                             │
│ ✓ Edit: src/main.rs:255-270 0.8s                 │
│                                                  │
│ [Pause] [Filter ▾]                              │
└──────────────────────────────────────────────────┘
```

**Stats Dashboard:**
```
┌─── Session Stats ──────────────────────────────┐
│ Tool Calls: 142                                 │
│ Total Time: 23m 14s                             │
│ Errors: 5 (3.5%)                                │
│ Total Tokens: 1,234,567                         │
│ Est. Cost: $0.42                                │
│                                                  │
│ ┌─── Tool Usage ─────┐  ┌─── Duration ──────┐  │
│ │ Bash ████████ 87   │  │ Avg: 2.3s          │  │
│ │ Edit ████ 32       │  │ Median: 0.8s       │  │
│ │ Read ██ 18         │  │ P99: 15.2s         │  │
│ │ ...                │  │                    │  │
│ └────────────────────┘  └────────────────────┘  │
│ ┌─── Token Usage ────┐                          │
│ │ ╱╲╱╲╱╲╱╲╱╲╱╲╱╲    │  input                  │
│ │ ╱╲╱╲╱╲╱╲╱╲╱╲╱╲    │  output                 │
│ │ ╱╲╱╲╱╲╱╲╱╲╱╲╱╲    │                          │
│ └────────────────────┘                          │
└──────────────────────────────────────────────────┘
```

### Config

```toml
[dashboard]
enabled = false                  # disabled by default
port = 9100
host = "127.0.0.1"
open_browser = true              # auto-open browser on start
event_buffer_size = 1000         # ring buffer for live feed
```

### CLI Interface

```
grok dashboard                   # open dashboard in browser (start server if needed)
grok dashboard start             # start dashboard server
grok dashboard stop              # stop dashboard server
grok dashboard status            # show dashboard server status
```

## Implementation Plan

### Phase 1 — HTTP API + Events (2 weeks)
1. Create `xai-grok-dashboard` crate with Axum HTTP server.
2. Implement event bus subscription (`tokio::sync::broadcast`).
3. Wire agent loop to emit `DashboardEvent`s.
4. Implement WebSocket endpoint for live event stream.
5. Implement REST endpoints: `/api/session`, `/api/session/timeline`, `/api/session/stats`.
6. Tests: events emitted correctly, WebSocket delivers events, REST endpoints return correct data.

### Phase 2 — Web UI (2-3 weeks)
1. Build React SPA with esbuild (single HTML+JS file, no bundler).
2. Implement Live Feed page with auto-scrolling event cards.
3. Implement Stats Dashboard page with Chart.js charts.
4. Implement Config Viewer page.
5. Add `grok dashboard` CLI command.
6. Tests: UI renders correctly, WebSocket reconnects on disconnect.

### Phase 3 — Polish (1 week)
1. Add error rate dashboard.
2. Add tool usage heatmap.
3. Add dark mode / light mode (follow system preference).
4. Add `open_browser` auto-launch.
5. Performance: virtual scrolling for large timelines.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Event serialization, API response correctness |
| Integration | Start dashboard → agent runs → WebSocket receives events |
| UI | Dashboard renders in headless browser (Playwright) |
| Performance | 10,000 events → WebSocket delivery < 100ms total |

## Open Questions

1. Should the dashboard be a separate process or embedded? *Decision:* Embedded optional HTTP server (started on demand, zero-cost when disabled).
2. React SPA or simple HTML with htmx? *Decision:* Simple HTML + vanilla JS + Chart.js for MVP. No build step.
3. Should we support multiple concurrent browser tabs? *Decision:* Yes, WebSocket per connection (broadcast channel supports multiple subscribers).
